//! Durable tool-loop continuations carried by ordinary opaque tool-call IDs.
use super::*;
use crate::execution_clock::{ExecutionStart, Snapshot};
use rusqlite::{params, OptionalExtension, TransactionBehavior};
use serde::{de::DeserializeOwned, Deserialize, Deserializer, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct State {
    version: u8,
    configuration: String,
    registration: String,
    effort: Option<String>,
    temperature: f64,
    pub mode: String,
    clock: Snapshot,
    wall_seconds: u64,
    objective_index: usize,
    pub references: String,
    reference_pins: serde_json::Value,
    pub internal_tools: u32,
    pub call_limit: usize,
    pub cycle_limit: usize,
    pub output_limit: usize,
    pub input_limit: usize,
    pub tool_limit: u32,
    pub calls: usize,
    pub cycles: usize,
    pub reserved: usize,
    pub prompt_bytes: usize,
    pub proposals: u32,
    pub advice: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    expert_advice: Option<super::expert_advice::Record>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) typed_evidence_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    typed_evidence: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    core_memory: Option<super::core_tools::Memory>,
    /// Model-call namespaces reserved for in-flight typed core batches. These
    /// are intentionally not replayed after a crash or failed provider call.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    #[serde(deserialize_with = "deserialize_pending_map")]
    core_pending: BTreeMap<usize, Vec<String>>,
    /// Projected core-memory bytes held by each durable pending batch.
    /// The key set must match `core_pending`; legacy snapshots with pending
    /// fingerprints but no byte ledger are rejected by validation.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    #[serde(deserialize_with = "deserialize_pending_map")]
    core_pending_bytes: BTreeMap<usize, usize>,
    /// Expert model-call namespaces consumed by exact reuse of checked work.
    /// Reuse is task-local and never creates a new operation record.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    core_reused_calls: BTreeSet<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    typed_routes_sha256: Option<String>,
    history_len: usize,
    history_sha256: String,
    pending: Vec<String>,
}

fn deserialize_pending_map<'de, D, V>(
    deserializer: D,
) -> std::result::Result<BTreeMap<usize, V>, D::Error>
where
    D: Deserializer<'de>,
    V: DeserializeOwned,
{
    let entries = BTreeMap::<String, V>::deserialize(deserializer)?;
    let mut parsed = BTreeMap::new();
    for (key, value) in entries {
        let call = key.parse::<usize>().map_err(serde::de::Error::custom)?;
        if !(1..=65_535).contains(&call) || call.to_string() != key {
            return Err(serde::de::Error::custom(
                "pending call keys must be canonical decimal indices",
            ));
        }
        if parsed.insert(call, value).is_some() {
            return Err(serde::de::Error::custom("duplicate pending call index"));
        }
    }
    Ok(parsed)
}

pub(super) struct Lease {
    store: Arc<Store>,
    pub task: String,
    id: String,
    generation: usize,
    claim: String,
    checksum: String,
    pub state: State,
    pub deadline: Instant,
    expert_catalog: Option<super::session_context::ExpertCatalog>,
    typed_experts: Option<Arc<super::typed_experts::Snapshot>>,
    core_typed_experts: Option<Arc<super::typed_experts::Snapshot>>,
    done: bool,
}

/// Owned, irreversibly charged typed operation. No model or registry lookup
/// occurs after construction; concurrent language work cannot change its input.
pub(super) struct PreparedTypedWork {
    store: Arc<Store>,
    task: String,
    clock: Snapshot,
    snapshot: Arc<super::typed_experts::Snapshot>,
    identity: super::typed_experts::PlanIdentity,
    entries: Vec<super::typed_experts::Entry>,
    deadline: Instant,
    concurrency: usize,
    shared_slots: Option<super::network_slots::Budget>,
    reasoning_arguments: bool,
    core_call: Option<usize>,
}

/// Host evidence binding an expert's completed advisory call to its core batch.
#[derive(Clone)]
pub(super) struct ExpertCoreCall {
    pub(super) call: usize,
    pub(super) request_sha256: String,
    pub(super) content_sha256: String,
    pub(super) model: String,
}

/// Checked task-local records returned for an expert batch without dispatching
/// another provider call or manufacturing new memory records.
pub(super) struct CoreReuse {
    pub(super) references: Vec<serde_json::Value>,
    pub(super) context: String,
}

struct TypedPart {
    id: String,
    value: String,
    work: super::typed_experts::Work,
    provider_identity: serde_json::Value,
}

pub(super) struct TypedResult {
    value: String,
    identity: super::typed_experts::PlanIdentity,
    parts: Vec<TypedPart>,
    reasoning_arguments: bool,
    core_call: Option<usize>,
}

impl TypedResult {
    pub fn value(&self) -> &str {
        &self.value
    }
    pub fn output_contract(&self) -> &'static str {
        self.identity.output_contract()
    }
    pub fn translated(&self) -> bool {
        self.identity.translated()
    }

    pub(super) fn reasoning_arguments(&self) -> bool {
        self.reasoning_arguments
    }

    pub(super) fn core_references(&self) -> Vec<serde_json::Value> {
        self.parts
            .iter()
            .map(|part| {
                json!({
                    "operation_id": part.id,
                    "result_sha256": sha256(part.value.as_bytes())
                })
            })
            .collect()
    }
}

type TypedOperation =
    std::pin::Pin<Box<dyn Future<Output = (super::typed_experts::Entry, Result<String>)> + Send>>;

/// Poll every owned operation on each wake. Completed work is removed so the
/// caller can release dependent nodes while slower independent work continues.
async fn ready_typed_operations(
    operations: &mut Vec<(String, TypedOperation)>,
) -> Vec<(String, super::typed_experts::Entry, Result<String>)> {
    use std::future::poll_fn;
    use std::task::Poll;
    let ready = poll_fn(|context| {
        let mut ready = Vec::new();
        for (index, (id, operation)) in operations.iter_mut().enumerate() {
            if let Poll::Ready((entry, result)) = operation.as_mut().poll(context) {
                ready.push((index, id.clone(), entry, result));
            }
        }
        if ready.is_empty() {
            Poll::Pending
        } else {
            Poll::Ready(ready)
        }
    })
    .await;
    let mut indices = ready
        .iter()
        .map(|(index, _, _, _)| *index)
        .collect::<Vec<_>>();
    indices.sort_unstable_by(|left, right| right.cmp(left));
    for index in indices {
        drop(operations.swap_remove(index));
    }
    ready
        .into_iter()
        .map(|(_, id, entry, result)| (id, entry, result))
        .collect()
}

impl PreparedTypedWork {
    /// Create a latest-value progress channel for this immutable operation set.
    /// The initial snapshot contains only trusted host plan metadata.
    pub(super) fn progress_channel(
        &self,
    ) -> Result<(
        tokio::sync::watch::Sender<super::typed_progress::Snapshot>,
        tokio::sync::watch::Receiver<super::typed_progress::Snapshot>,
    )> {
        let expected_ids = self
            .identity
            .entry_ids()
            .map(str::to_owned)
            .collect::<Vec<_>>();
        let initial =
            super::typed_progress::Snapshot::pending(self.identity.query_digest(), &expected_ids)?;
        Ok(tokio::sync::watch::channel(initial))
    }

    /// Share the task-wide execution slots with concurrently running model roles.
    /// The semaphore is acquired inside owned operation futures so this scheduler
    /// keeps polling all already-admitted work while some nodes wait for capacity.
    pub(super) fn share_slots(&mut self, budget: super::network_slots::Budget) -> Result<()> {
        let limit = budget.limit();
        if !(1..=4).contains(&limit) || self.concurrency > limit {
            bail!("Shared task concurrency is outside its bound")
        }
        self.shared_slots = Some(budget);
        Ok(())
    }

    pub async fn execute(self) -> Result<TypedResult> {
        self.execute_inner(None).await
    }

    /// Execute the same admitted work while publishing cumulative checked
    /// operation progress to a trusted host watch receiver.
    pub(super) async fn execute_with_progress(
        self,
        sender: tokio::sync::watch::Sender<super::typed_progress::Snapshot>,
    ) -> Result<TypedResult> {
        self.execute_inner(Some(sender)).await
    }

    async fn execute_inner(
        self,
        progress: Option<tokio::sync::watch::Sender<super::typed_progress::Snapshot>>,
    ) -> Result<TypedResult> {
        use super::graph_schedule::{self, Dependency, Evidence, Node};
        let expected_ids = self
            .identity
            .entry_ids()
            .map(str::to_owned)
            .collect::<Vec<_>>();
        let mut progress_snapshot = if progress.is_some() {
            Some(super::typed_progress::Snapshot::pending(
                self.identity.query_digest(),
                &expected_ids,
            )?)
        } else {
            None
        };
        if let Err(error) = remaining(&self.clock) {
            publish_typed_failure(&progress, &mut progress_snapshot);
            return Err(error);
        }
        let mut pending = self.entries;
        let mut parts = Vec::<TypedPart>::new();
        let mut checked_outputs = std::collections::BTreeMap::<String, String>::new();
        let mut in_flight = Vec::<(String, TypedOperation)>::new();
        let mut failure = None;

        while !pending.is_empty() || !in_flight.is_empty() {
            if failure.is_none() && !pending.is_empty() {
                if let Err(error) = remaining(&self.clock) {
                    failure = Some(error);
                } else {
                    let ids = pending
                        .iter()
                        .map(|entry| entry.id.as_str())
                        .chain(in_flight.iter().map(|(id, _)| id.as_str()))
                        .chain(parts.iter().map(|part| part.id.as_str()))
                        .collect::<Vec<_>>();
                    let built_nodes = (|| -> Result<Vec<Node>> {
                        let mut nodes = pending
                            .iter()
                            .map(|entry| {
                                let dependencies = entry
                                    .template
                                    .as_ref()
                                    .map(|template| template.dependencies())
                                    .unwrap_or_default()
                                    .iter()
                                    .map(|id| {
                                        let node = ids
                                            .iter()
                                            .position(|candidate| *candidate == id)
                                            .ok_or_else(|| {
                                                anyhow::anyhow!("Typed graph dependency is missing")
                                            })?;
                                        Ok(Dependency {
                                            node,
                                            evidence: Evidence::Checked,
                                        })
                                    })
                                    .collect::<Result<Vec<_>>>()?;
                                Ok(Node {
                                    group: 0,
                                    dependencies,
                                    completed: None,
                                    in_flight: false,
                                    blocked: false,
                                })
                            })
                            .collect::<Result<Vec<_>>>()?;
                        nodes.extend(in_flight.iter().map(|_| Node {
                            group: 0,
                            dependencies: Vec::new(),
                            completed: None,
                            in_flight: true,
                            blocked: false,
                        }));
                        nodes.extend(parts.iter().map(|_| Node {
                            group: 0,
                            dependencies: Vec::new(),
                            completed: Some(Evidence::Checked),
                            in_flight: false,
                            blocked: false,
                        }));
                        Ok(nodes)
                    })();
                    match built_nodes {
                        Err(error) => failure = Some(error),
                        Ok(nodes) => {
                            let available = self.concurrency.saturating_sub(in_flight.len());
                            let ready = if available == 0 {
                                Vec::new()
                            } else {
                                match graph_schedule::ready_wave(
                                    &nodes,
                                    1,
                                    self.concurrency,
                                    self.concurrency,
                                ) {
                                    Ok(ready) => ready,
                                    Err(error) => {
                                        failure = Some(error);
                                        Vec::new()
                                    }
                                }
                            };
                            if ready.is_empty() && in_flight.is_empty() && !pending.is_empty() {
                                failure.get_or_insert_with(|| {
                                    anyhow::anyhow!("Typed graph has no dependency-ready operation")
                                });
                            }

                            // Remove selected entries in reverse index order, then
                            // restore stable graph order for preparation and dispatch.
                            let mut selected = ready
                                .into_iter()
                                .rev()
                                .map(|index| pending.remove(index))
                                .collect::<Vec<_>>();
                            selected.reverse();
                            for mut entry in selected {
                                if failure.is_some() {
                                    break;
                                }
                                let preparation = (|| -> Result<()> {
                                    if let Some(template) = &entry.template {
                                        let capability = entry.work.capability();
                                        let (resolved, binding_audit) =
                                            template.resolve(&checked_outputs)?;
                                        if resolved.capability() != capability
                                            || resolved.translated() != entry.work.translated()
                                        {
                                            bail!("Typed graph binding changed its admitted capability or origin")
                                        }
                                        entry.work = resolved;
                                        let mut source_evidence = std::collections::BTreeMap::new();
                                        for source in template.dependencies() {
                                            let part = parts
                                                .iter()
                                                .find(|part| part.id == *source)
                                                .ok_or_else(|| {
                                                    anyhow::anyhow!(
                                                    "Typed graph prerequisite evidence is missing"
                                                )
                                                })?;
                                            source_evidence.insert(
                                                source,
                                                json!({
                                                    "result_sha256":sha256(part.value.as_bytes()),
                                                    "criterion_sha256":part.work.criterion_digest(),
                                                    "compiled_criterion_match":true,
                                                    "provider_identity":part.provider_identity
                                                }),
                                            );
                                        }
                                        self.store.event(&self.task, "agent_typed_graph_binding", "jev-plus", json!({
                                            "format_version":1, "operation_id":entry.id,
                                            "root_query_sha256":self.identity.query_digest(), "snapshot_sha256":self.snapshot.digest(),
                                            "resolved_criterion_sha256":entry.work.criterion_digest(),
                                            "source_evidence":source_evidence,
                                            "binding":binding_audit, "permissions_granted":false, "execution_started":false
                                        }))?;
                                    }
                                    Ok(())
                                })();
                                if let Err(error) = preparation {
                                    failure = Some(error);
                                    break;
                                }
                                let id = entry.id.clone();
                                let snapshot = self.snapshot.clone();
                                let task = self.task.clone();
                                let deadline = self.deadline;
                                let store = self.store.clone();
                                let root_query_sha256 = self.identity.query_digest().to_owned();
                                let shared_slots = self.shared_slots.clone();
                                let operation = Box::pin(async move {
                                    let result = match TaskPolicy::client_collective(&task) {
                                        Ok(policy) => {
                                            let permit = match shared_slots {
                                                Some(slots) => {
                                                    slots.acquire(deadline).await.map(Some)
                                                }
                                                None => Ok(None),
                                            };
                                            match permit {
                                                Ok(_permit) => {
                                                    snapshot
                                                        .evaluate(
                                                            &task,
                                                            &entry,
                                                            &store,
                                                            &root_query_sha256,
                                                            deadline,
                                                            &policy,
                                                        )
                                                        .await
                                                }
                                                Err(error) => Err(error),
                                            }
                                        }
                                        Err(error) => Err(error),
                                    };
                                    (entry, result)
                                })
                                    as TypedOperation;
                                in_flight.push((id, operation));
                            }
                        }
                    }
                }
            }

            if in_flight.is_empty() {
                break;
            }
            if failure.is_some() {
                publish_typed_failure(&progress, &mut progress_snapshot);
            }
            // Poll every owned sibling on each wake and account for every
            // completion before considering newly unblocked dependent work.
            let completed = ready_typed_operations(&mut in_flight).await;
            for (id, entry, result) in completed {
                if id != entry.id {
                    failure.get_or_insert_with(|| {
                        anyhow::anyhow!(
                            "Typed operation completion did not match its in-flight identity"
                        )
                    });
                }
                let event_result = self.store.event(&self.task, "agent_typed_expert_result", "jev-plus", json!({
                    "operation_id":entry.id, "root_query_sha256":self.identity.query_digest(),
                    "capability":entry.work.capability().as_str(),
                    "snapshot_sha256":self.snapshot.digest(),
                    "original_query_sha256":entry.work.query_digest(),
                    "criterion_sha256":entry.work.criterion_digest(),
                    "result_sha256":result.as_ref().ok().map(|value|sha256(value.as_bytes())),
                    "compiled_criterion_match":result.is_ok(),
                    "original_goal_semantics_verified":result.is_ok() && !self.reasoning_arguments && !self.identity.translated(),
                    "criterion_origin":if self.reasoning_arguments {"reasoning_model_arguments"} else if self.identity.translated() {"language_translation"} else {"original_exact_goal"},
                    "language_translated":self.identity.translated() && !self.reasoning_arguments, "checker":entry.work.checker(),
                    "presented_answer_verified":false, "automatic_retry":false,
                    "prepared_provider_route":entry.provider_route.as_ref().map(|proof|proof.audit_metadata())
                }));
                if let Err(error) = event_result {
                    failure.get_or_insert(error);
                }
                match result {
                    Ok(value) => match entry.provider_route.as_ref() {
                        Some(proof) => {
                            let metadata = proof.audit_metadata();
                            let mut provider_identity = serde_json::Map::new();
                            for key in [
                                "route",
                                "catalog_sha256",
                                "expert_id",
                                "expert_version",
                                "canonical_manifest_sha256",
                                "provider_module_id",
                                "provider_module_version",
                                "capability",
                                "capability_version",
                            ] {
                                if let Some(value) = metadata.get(key) {
                                    provider_identity.insert(key.to_owned(), value.clone());
                                }
                            }
                            checked_outputs.insert(entry.id.clone(), value.clone());
                            parts.push(TypedPart {
                                id: entry.id,
                                value,
                                work: entry.work,
                                provider_identity: serde_json::Value::Object(provider_identity),
                            });
                            if failure.is_none() {
                                if let (true, Some(sender), Some(snapshot)) = (
                                    failure.is_none(),
                                    progress.as_ref(),
                                    progress_snapshot.as_mut(),
                                ) {
                                    if let Some(part) = parts.last() {
                                        match super::typed_progress::Part::checked(
                                            part.id.clone(),
                                            part.value.clone(),
                                            part.work.criterion_digest().to_owned(),
                                            part.work.checker().to_owned(),
                                            part.provider_identity.clone(),
                                            part.work.query_digest().to_owned(),
                                            part.work.translated() && !self.reasoning_arguments,
                                        )
                                        .and_then(|progress_part| snapshot.record(progress_part))
                                        {
                                            Ok(()) => {
                                                sender.send_replace(snapshot.clone());
                                            }
                                            Err(error) => {
                                                failure.get_or_insert(error);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        None => {
                            failure.get_or_insert_with(|| {
                                anyhow::anyhow!(
                                    "Checked typed result has no prepared provider route"
                                )
                            });
                        }
                    },
                    Err(_) => {
                        failure.get_or_insert_with(|| {
                            anyhow::anyhow!(
                                "Typed expert operation failed; no operation will be replayed"
                            )
                        });
                    }
                }
                if let Err(error) = remaining(&self.clock) {
                    failure.get_or_insert(error);
                }
            }
            if failure.is_some() {
                publish_typed_failure(&progress, &mut progress_snapshot);
            }
        }
        if let Some(error) = failure {
            publish_typed_failure(&progress, &mut progress_snapshot);
            return Err(error);
        }
        // Final checking uses the original declared operation order, even when
        // dependencies caused execution to complete in a different order.
        let mut ordered = Vec::with_capacity(parts.len());
        for id in self.identity.entry_ids() {
            let Some(index) = parts.iter().position(|part| part.id == id) else {
                publish_typed_failure(&progress, &mut progress_snapshot);
                bail!("Typed graph result is missing a declared operation")
            };
            ordered.push(parts.remove(index));
        }
        let parts = ordered;
        let values = parts
            .iter()
            .map(|part| (part.id.clone(), part.value.clone()))
            .collect::<Vec<_>>();
        let value = match self.identity.assemble(&values) {
            Ok(value) => value,
            Err(error) => {
                publish_typed_failure(&progress, &mut progress_snapshot);
                return Err(error);
            }
        };
        if let (Some(sender), Some(snapshot)) = (&progress, &mut progress_snapshot) {
            if let Err(error) = snapshot.mark_complete() {
                snapshot.mark_failed();
                sender.send_replace(snapshot.clone());
                return Err(error);
            }
            sender.send_replace(snapshot.clone());
        }
        Ok(TypedResult {
            value,
            identity: self.identity,
            parts,
            reasoning_arguments: self.reasoning_arguments,
            core_call: self.core_call,
        })
    }
}

fn publish_typed_failure(
    sender: &Option<tokio::sync::watch::Sender<super::typed_progress::Snapshot>>,
    snapshot: &mut Option<super::typed_progress::Snapshot>,
) {
    if let (Some(sender), Some(snapshot)) = (sender, snapshot) {
        snapshot.mark_failed();
        sender.send_replace(snapshot.clone());
    }
}

pub(super) struct NativeScopeSessionSnapshot {
    pub clock: Snapshot,
    pub generation: usize,
    pub calls: usize,
    pub state: String,
}

struct SessionSqlProgressReset<'a>(&'a rusqlite::Connection);

impl Drop for SessionSqlProgressReset<'_> {
    fn drop(&mut self) {
        self.0.progress_handler(0, None::<fn() -> bool>);
    }
}

/// Load the already-authenticated durable client snapshot for the native
/// lifecycle bridge. Keep validation here so that bridge cannot accept a
/// weaker copy of the AgentRuntime continuation invariants.
pub(super) fn native_scope_session_snapshot(
    store: &Store,
    task: &str,
    cfg: &Config,
) -> Result<NativeScopeSessionSnapshot> {
    uuid::Uuid::parse_str(task)?;
    let db = store.lock()?;
    let started = Instant::now();
    db.progress_handler(
        1000,
        Some(move || started.elapsed() > Duration::from_secs(1)),
    );
    let _scan_deadline = SessionSqlProgressReset(&db);
    let (session_state, generation, raw, checksum): (String, usize, String, String) = db.query_row(
        "SELECT CASE WHEN length(CAST(state AS BLOB))<=32 THEN state END,generation,CASE WHEN length(CAST(snapshot AS BLOB))<=32768 THEN snapshot END,CASE WHEN length(CAST(snapshot_sha256 AS BLOB))=64 THEN snapshot_sha256 END FROM agent_sessions WHERE task_id=?1 LIMIT 1",
        [task],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
    )?;
    if raw.len() > 32768 || sha256(raw.as_bytes()) != checksum {
        bail!("Native scope client session snapshot is missing or has changed")
    }
    let state: State = crate::strict_json::value(raw.as_bytes(), 32768)
        .and_then(|value| Ok(serde_json::from_value(value)?))?;
    state.validate(cfg)?;
    if state.configuration != sha256(&serde_json::to_vec(cfg)?) {
        bail!("Native scope client session configuration differs from its original snapshot")
    }
    let task_state: String = db.query_row(
        "SELECT CASE WHEN length(CAST(status AS BLOB))<=64 THEN status END FROM tasks WHERE id=?1",
        [task],
        |row| row.get(0),
    )?;
    let matches = matches!(
        (session_state.as_str(), task_state.as_str()),
        ("active", "running") | ("waiting", "waiting_client")
    );
    if !matches {
        bail!("Native scope client session and root task lifecycle differ")
    }
    drop(_scan_deadline);
    Ok(NativeScopeSessionSnapshot {
        clock: state.clock,
        generation,
        calls: state.calls,
        state: session_state,
    })
}

fn history_hash(messages: &[Message]) -> Result<String> {
    let mut normalized = messages.to_vec();
    for message in &mut normalized {
        if message.content.as_deref() == Some("") {
            message.content = None;
        }
        for call in &mut message.tool_calls {
            call.function.arguments = serde_json::to_string(&crate::strict_json::value(
                call.function.arguments.as_bytes(),
                32000,
            )?)?;
        }
    }
    Ok(sha256(&serde_json::to_vec(&normalized)?))
}
fn effective_effort(cfg: &Config, turn: &Turn) -> Option<String> {
    turn.reasoning_effort
        .clone()
        .or_else(|| cfg.reasoning_effort.clone())
}
fn valid_id(id: &str) -> bool {
    id.strip_prefix("cgf_").is_some_and(|id| {
        id.len() == 32
            && id
                .bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
    })
}
fn valid_core_digest(digest: &str) -> bool {
    digest.len() == 64
        && digest
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
fn remaining(clock: &Snapshot) -> Result<Instant> {
    let before = Instant::now();
    let observation = clock.remaining()?;
    let ns = observation["remaining_ns"]
        .as_u64()
        .filter(|n| *n > 0)
        .ok_or_else(|| {
            anyhow::anyhow!("Collective session expired or its same-boot clock is unavailable")
        })?;
    Ok(before + Duration::from_nanos(ns))
}
fn raw_state(state: &State) -> Result<(String, String)> {
    let raw = serde_json::to_string(state)?;
    if raw.len() > 32768 {
        bail!("Collective checkpoint exceeds bounds")
    }
    let checksum = sha256(raw.as_bytes());
    Ok((raw, checksum))
}

fn core_verifier_capability(capability: crate::fabric::modules::Capability) -> &'static str {
    match capability {
        crate::fabric::modules::Capability::ConstraintsSolve => "constraints.verify",
        other => other.as_str(),
    }
}

fn core_record(
    proposal: &super::core_tools::Proposal,
    operation_id: String,
    model_call_index: usize,
    criterion_sha256: String,
    provider_identity: String,
    result_json: String,
) -> super::core_tools::Record {
    super::core_tools::Record {
        operation_id,
        model_call_index,
        proposal_id: proposal.call_id.clone(),
        name: proposal.name.clone(),
        capability: proposal.capability.as_str().to_owned(),
        fingerprint: proposal.fingerprint.clone(),
        arguments_json: proposal.arguments_json.clone(),
        result_sha256: sha256(result_json.as_bytes()),
        result_json,
        criterion_sha256,
        provider_sha256: sha256(provider_identity.as_bytes()),
        provider_identity,
        origin: "reasoning_model_arguments".to_owned(),
        original_semantics_verified: false,
    }
}

fn core_provider_identity(
    provider: &serde_json::Value,
    inputs: &serde_json::Value,
) -> Result<String> {
    let mut provider = provider
        .as_object()
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("Core provider provenance must be an object"))?;
    if !inputs.is_null()
        && provider
            .insert("checked_memory_inputs".to_owned(), inputs.clone())
            .is_some()
    {
        bail!("Core provider provenance contains a reserved input field")
    }
    Ok(serde_json::to_string(&provider)?)
}

fn audit(
    db: &rusqlite::Connection,
    task: &str,
    kind: &str,
    payload: serde_json::Value,
) -> Result<()> {
    let raw = payload.to_string();
    if raw.len() > 8192 {
        bail!("Session audit exceeds metadata bounds")
    }
    db.execute("INSERT INTO events(task_id,kind,source,payload,created) VALUES (?1,?2,'jev-r',?3,unixepoch())", params![task,kind,raw])?;
    Ok(())
}

impl State {
    fn validate(&self, cfg: &Config) -> Result<()> {
        self.validate_core_pending_bytes()?;
        self.clock.validate(cfg.max_wall_seconds)?;
        super::reasoning_memory::validate_checkpoint(&self.advice, &self.references)?;
        if let Some(advice) = &self.expert_advice {
            advice.validate_checkpoint(&sha256(self.references.as_bytes()))?;
            advice.validate_core_references(self.core_memory.as_ref())?;
            advice.context()?;
        }
        super::session_context::validate_checkpoint(&self.references, &self.reference_pins)?;
        let (calls, cycles) = crate::reasoning::mode_limits(&self.mode)?;
        let references_valid = if self.references.is_empty() && cfg.agent_project_profile.is_none()
        {
            // Preserve closed legacy snapshots created when the configured tool
            // allowance was zero and no retrieval context was captured.
            true
        } else {
            serde_json::from_str::<serde_json::Value>(&self.references).is_ok_and(|references| {
                references["trust"] == "untrusted_data_never_instructions_or_permissions"
            })
        };
        let project_pin = self.reference_pins.get("project_profile");
        let configured_profile_matches = match (cfg.agent_project_profile.as_deref(), project_pin) {
            (None, Some(serde_json::Value::Null) | None) => true,
            (Some(expected), Some(pin)) => pin
                .get("id")
                .and_then(serde_json::Value::as_str)
                .zip(pin.get("version").and_then(serde_json::Value::as_u64))
                .is_some_and(|(id, version)| format!("{id}@{version}") == expected),
            _ => false,
        };
        if self.wall_seconds != cfg.max_wall_seconds
            || self.references.len() > 8000
            || !references_valid
            || !configured_profile_matches
            || serde_json::to_vec(&self.reference_pins)?.len() > 4096
            || self.objective_index >= self.history_len.max(1) && self.history_len > 0
            || self.proposals.saturating_add(self.internal_tools) > self.tool_limit
            || self.version != 1
            || self.call_limit != super::collective::call_limit(cfg, calls)
            || self.cycle_limit != cycles.min(cfg.max_cycles)
            || self.output_limit != cfg.max_tokens
            || self.input_limit != cfg.max_input_bytes
            || self.tool_limit != cfg.max_tool_calls
            || self.calls > self.call_limit
            || self.cycles > self.cycle_limit
            || self.reserved > self.output_limit
            || self.prompt_bytes > self.input_limit
            || self.proposals > self.tool_limit
            || self.advice.len() > 2000
            || self.typed_evidence.as_ref().is_some_and(|raw| {
                raw.len() > 16_384
                    || self.typed_evidence_sha256.as_deref()
                        != Some(sha256(raw.as_bytes()).as_str())
            })
            || self.typed_evidence_sha256.as_ref().is_some_and(|digest| {
                digest.len() != 64
                    || !digest
                        .bytes()
                        .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
            })
            || self
                .core_memory
                .as_ref()
                .is_some_and(|memory| memory.validate_checkpoint().is_err())
            || self.core_pending.len() > 4
            || self.core_pending.iter().any(|(call, fingerprints)| {
                *call == 0
                    || *call > self.calls
                    || fingerprints.is_empty()
                    || fingerprints.len() > 4
                    || fingerprints.iter().any(|digest| {
                        digest.len() != 64
                            || !digest
                                .bytes()
                                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
                    })
            })
            || self.core_pending.values().map(Vec::len).sum::<usize>() > 16
            || self.core_reused_calls.len() > self.calls
            || self.core_reused_calls.iter().any(|call| {
                *call == 0
                    || *call > self.calls
                    || *call > self.call_limit
                    || *call > 65_535
                    || self.core_pending.contains_key(call)
                    || self
                        .core_memory
                        .as_ref()
                        .is_some_and(|memory| memory.contains_call(*call))
            })
            || self.core_memory.as_ref().map_or(0, |memory| memory.len())
                + self.core_pending.values().map(Vec::len).sum::<usize>()
                > 16
            || self.core_memory.as_ref().is_some_and(|memory| {
                self.core_pending
                    .keys()
                    .any(|call| memory.contains_call(*call))
            })
            || {
                let mut pending_fingerprints = std::collections::BTreeSet::new();
                self.core_pending.values().flatten().any(|fingerprint| {
                    !pending_fingerprints.insert(fingerprint)
                        || self
                            .core_memory
                            .as_ref()
                            .is_some_and(|memory| memory.contains_fingerprint(fingerprint))
                })
            }
            || self.pending.len() > 16
            || self.typed_routes_sha256.as_ref().is_some_and(|digest| {
                digest.len() != 64
                    || !digest
                        .bytes()
                        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            })
            || self.history_len > 255
            || self.pending.iter().any(|id| !valid_id(id))
        {
            bail!("Invalid collective session checkpoint")
        }
        Ok(())
    }

    fn validate_core_pending_bytes(&self) -> Result<()> {
        self.validate_core_reused_calls()?;
        if self.core_pending.len() != self.core_pending_bytes.len()
            || self.core_pending.keys().ne(self.core_pending_bytes.keys())
            || self.core_pending_bytes.len() > 4
            || self.core_pending_bytes.iter().any(|(call, bytes)| {
                *call == 0
                    || *call > 65_535
                    || *call > self.calls
                    || *bytes == 0
                    || *bytes > super::core_tools::MEMORY_BYTE_LIMIT
            })
        {
            bail!("Core pending fingerprints and byte reservations are inconsistent")
        }
        let committed_bytes = match &self.core_memory {
            Some(memory) => memory.encoded_bytes()?,
            None => super::core_tools::Memory::empty_encoded_bytes()?,
        };
        let reserved_bytes = self
            .core_pending_bytes
            .values()
            .try_fold(0usize, |sum, bytes| {
                sum.checked_add(*bytes)
                    .ok_or_else(|| anyhow::anyhow!("Core pending byte reservation overflow"))
            })?;
        if committed_bytes
            .checked_add(reserved_bytes)
            .is_none_or(|total| total > super::core_tools::MEMORY_BYTE_LIMIT)
        {
            bail!("Core memory checkpoint byte budget exceeded")
        }
        Ok(())
    }

    fn validate_core_reused_calls(&self) -> Result<()> {
        if self.core_reused_calls.len() > self.calls
            || self.core_reused_calls.iter().any(|call| {
                *call == 0
                    || *call > self.calls
                    || *call > self.call_limit
                    || *call > 65_535
                    || self.core_pending.contains_key(call)
                    || self
                        .core_memory
                        .as_ref()
                        .is_some_and(|memory| memory.contains_call(*call))
            })
        {
            bail!("Consumed core reuse namespaces are inconsistent")
        }
        Ok(())
    }
}

type SessionRow = (String, String, String, usize, String, String, String, usize);

impl Lease {
    fn begin(
        runtime: &AgentRuntime,
        request: &Turn,
        registration: &serde_json::Value,
    ) -> Result<Self> {
        let configuration = sha256(&serde_json::to_vec(&runtime.cfg)?);
        let registration = sha256(&serde_json::to_vec(registration)?);
        let effort = effective_effort(&runtime.cfg, request);
        let temperature = request.temperature.unwrap_or(runtime.cfg.temperature);
        // Only server-issued IDs signal continuation. Unknown IDs with our prefix
        // never fall back to a new budget, including after retention removed them.
        let latest = request
            .messages
            .iter()
            .rev()
            .flat_map(|m| m.tool_calls.iter().rev())
            .find(|c| c.id.starts_with("cgf_"))
            .map(|c| c.id.clone());
        {
            let mut db = runtime.store.lock()?;
            let cleanup = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
            super::session_cleanup::expire_waiting(&cleanup)?;
            cleanup.commit()?;
        }
        let mut db = runtime.store.lock()?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(call) = latest {
            if !valid_id(&call) {
                bail!("Malformed collective continuation handle")
            }
            let row: Option<SessionRow> = tx.query_row(
                "SELECT s.id,s.task_id,s.state,s.generation,s.claim,CASE WHEN length(CAST(s.snapshot AS BLOB))<=32768 THEN s.snapshot END,s.snapshot_sha256,c.generation FROM agent_session_calls c JOIN agent_sessions s ON s.id=c.session_id WHERE c.id=?1",
                [&call], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?,r.get(7)?))).optional()?;
            let (id, task, status, generation, _, raw, checksum, call_generation) =
                row.ok_or_else(|| anyhow::anyhow!("Unknown collective continuation handle"))?;
            if sha256(raw.as_bytes()) != checksum {
                bail!("Collective checkpoint checksum mismatch")
            }
            let mut state: State =
                serde_json::from_value(crate::strict_json::value(raw.as_bytes(), 32768)?)?;
            if status != "closed" {
                state.validate(&runtime.cfg)?;
                let mut scopes = tx.prepare(
                    "SELECT CASE WHEN length(CAST(payload AS BLOB))<=131072 THEN payload END FROM events WHERE task_id=?1 AND kind='agent_native_scope' AND source='security-kernel' ORDER BY id LIMIT 2",
                )?;
                let scopes = scopes
                    .query_map([&task], |row| row.get::<_, String>(0))?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                match (&runtime.native_policy, scopes.as_slice()) {
                    (None, []) => (),
                    (Some(policy), [raw]) => {
                        let scope = crate::strict_json::value(raw.as_bytes(), 131072)?;
                        let version = if runtime.native_on_demand { 3 } else { 2 };
                        if scope["format_version"] != version
                            || (scope["on_demand"] == true) != runtime.native_on_demand
                            || scope["root_task_id"] != task
                            || scope["configuration_sha256"] != state.configuration
                            || scope["policy_sha256"]
                                != sha256(&serde_json::to_vec(policy.as_ref())?)
                            || scope["evidence_class"] != "model_key_and_inventory"
                            || scope["explicit_operator_approval"] != true
                            || scope["response_instance_check_required"] != false
                        {
                            bail!("Collective continuation native policy or evidence class changed")
                        }
                    }
                    _ => bail!(
                        "Collective continuation requires its single original native model scope"
                    ),
                }
            }
            if status != "closed"
                && (state.configuration != configuration
                    || state.registration != registration
                    || state.effort != effort
                    || state.temperature != temperature)
            {
                bail!("Collective continuation configuration, model or effort changed")
            }
            if request.messages.len() < state.history_len
                || history_hash(&request.messages[..state.history_len])? != state.history_sha256
            {
                bail!("Collective continuation history differs from the committed checkpoint")
            }
            if status == "closed" {
                // A genuine new user turn after an echoed final answer starts a
                // new task. An old tool request without that answer is a replay.
                let suffix = &request.messages[state.history_len..];
                if suffix.is_empty()
                    || suffix
                        .iter()
                        .any(|m| !matches!(m.role.as_str(), "user" | "system" | "developer"))
                    || !suffix.iter().any(|m| m.role == "user")
                {
                    bail!("Closed collective session cannot be replayed")
                }
            } else {
                if status != "waiting"
                    || generation != call_generation
                    || !state.pending.contains(&call)
                {
                    bail!(
                        "Collective session is active, interrupted or its checkpoint was consumed"
                    )
                }
                let suffix = &request.messages[state.history_len..];
                if suffix.len() < state.pending.len() {
                    bail!("Missing collective client tool results")
                }
                let ids: std::collections::BTreeSet<_> = suffix[..state.pending.len()]
                    .iter()
                    .filter(|m| m.role == "tool")
                    .filter_map(|m| m.tool_call_id.as_deref())
                    .collect();
                if ids.len() != state.pending.len()
                    || state.pending.iter().any(|id| !ids.contains(id.as_str()))
                    || suffix[state.pending.len()..]
                        .iter()
                        .any(|m| !matches!(m.role.as_str(), "user" | "system" | "developer"))
                {
                    bail!("Collective tool results do not exactly match the pending checkpoint")
                }
                let count: usize = tx.query_row("SELECT count(*) FROM agent_session_calls WHERE session_id=?1 AND generation=?2",params![id,generation],|r|r.get(0))?;
                if count != state.pending.len() {
                    bail!("Collective pending call ledger is incomplete")
                }
                for message in &request.messages[..state.history_len] {
                    for pending in message
                        .tool_calls
                        .iter()
                        .filter(|c| state.pending.contains(&c.id))
                    {
                        let saved: String = tx.query_row("SELECT call_sha256 FROM agent_session_calls WHERE id=?1 AND session_id=?2 AND generation=?3",params![pending.id,id,generation],|r|r.get(0))?;
                        if saved
                            != history_hash(&[Message {
                                role: "assistant".into(),
                                content: None,
                                tool_calls: vec![pending.clone()],
                                tool_call_id: None,
                            }])?
                        {
                            bail!("Collective pending proposal binding changed")
                        }
                    }
                }
                let deadline = remaining(&state.clock)?;
                let claim = uuid::Uuid::new_v4().to_string();
                let task_state: String =
                    tx.query_row("SELECT status FROM tasks WHERE id=?1", [&task], |r| {
                        r.get(0)
                    })?;
                if task_state != "waiting_client" {
                    bail!("Collective task is not awaiting client results")
                }
                let core_typed_experts = match &state.typed_routes_sha256 {
                    Some(digest) => {
                        let raw = super::typed_snapshot_storage::restore(&tx, &task, digest)?;
                        let broker = runtime
                            .retained_typed_modules
                            .as_ref()
                            .or(runtime.typed_modules.as_ref())
                            .ok_or_else(|| {
                                anyhow::anyhow!(
                                    "Original typed implementation registry is unavailable"
                                )
                            })?;
                        broker.require_store(&runtime.store)?;
                        Some(Arc::new(super::typed_experts::Snapshot::restore_metadata(
                            broker.clone(),
                            &task,
                            &raw,
                            runtime.native_owner.clone(),
                        )?))
                    }
                    None => None,
                };
                // All pending handles and their immutable ledger were checked above.
                // Clear only the local working copy; the next write still uses
                // the original stored checksum for its compare-and-swap.
                state.pending.clear();
                if tx.execute("UPDATE agent_sessions SET state='active',claim=?2,updated=unixepoch() WHERE id=?1 AND state='waiting' AND generation=?3",params![id,claim,generation])? != 1 { bail!("Collective checkpoint was concurrently claimed") }
                tx.execute(
                    "UPDATE tasks SET status='running',updated=unixepoch() WHERE id=?1",
                    [&task],
                )?;
                audit(
                    &tx,
                    &task,
                    "agent_session_resume",
                    json!({"generation":generation,"request_sha256":sha256(&serde_json::to_vec(request)?),"calls":state.calls,"reserved_output_tokens":state.reserved,"client_results_verified":false}),
                )?;
                tx.commit()?;
                return Ok(Self {
                    store: runtime.store.clone(),
                    task,
                    id,
                    generation,
                    claim,
                    checksum,
                    state,
                    deadline,
                    expert_catalog: None,
                    typed_experts: None,
                    core_typed_experts,
                    done: false,
                });
            }
        }
        crate::storage::admit(&tx)?;
        let mode = collective::mode(&runtime.cfg, request).to_string();
        let (calls, cycles) = crate::reasoning::mode_limits(&mode)?;
        let start = ExecutionStart::new(runtime.cfg.max_wall_seconds);
        let clock = start.clock.ok_or_else(|| {
            anyhow::anyhow!("Durable collective sessions require a same-boot execution clock")
        })?;
        let id = uuid::Uuid::new_v4().to_string();
        let task = uuid::Uuid::new_v4().to_string();
        let claim = uuid::Uuid::new_v4().to_string();
        let objective_index = request
            .messages
            .iter()
            .rposition(|m| m.role == "user")
            .ok_or_else(|| {
                anyhow::anyhow!("Collective session requires an original user objective")
            })?;
        let objective = request.messages[objective_index]
            .content
            .as_deref()
            .unwrap_or("");
        let mut query_end = objective.len().min(12000);
        while !objective.is_char_boundary(query_end) {
            query_end -= 1;
        }
        let include_retrieval = runtime.cfg.max_tool_calls > 0;
        if include_retrieval {
            TaskPolicy::client_collective(&task)?.authorize(
                &task,
                "jev-plus",
                "knowledge.search",
            )?;
        }
        let (references, reference_pins, expert_catalog) =
            super::session_context::capture_network_context(
                &tx,
                &objective[..query_end],
                runtime.cfg.agent_project_profile.as_deref(),
                include_retrieval,
                runtime.cfg.language_model().is_some(),
            )?;
        let internal_tools = u32::from(include_retrieval);
        let typed_snapshot = runtime
            .typed_modules
            .as_ref()
            .filter(|_| include_retrieval)
            .map(|broker| super::typed_experts::Snapshot::capture(broker.clone(), &task, &tx))
            .transpose()?;
        let typed_experts = typed_snapshot.map(Arc::new);
        let typed_routes_metadata = typed_experts
            .as_ref()
            .map(|snapshot| snapshot.export_metadata())
            .transpose()?;
        let state = State {
            version: 1,
            wall_seconds: runtime.cfg.max_wall_seconds,
            objective_index,
            references,
            reference_pins,
            internal_tools,
            configuration,
            registration,
            effort,
            temperature,
            mode: mode.clone(),
            clock,
            call_limit: super::collective::call_limit(&runtime.cfg, calls),
            cycle_limit: cycles.min(runtime.cfg.max_cycles),
            output_limit: runtime.cfg.max_tokens,
            input_limit: runtime.cfg.max_input_bytes,
            tool_limit: runtime.cfg.max_tool_calls,
            calls: 0,
            cycles: 0,
            reserved: 0,
            prompt_bytes: 0,
            proposals: 0,
            advice: String::new(),
            expert_advice: None,
            typed_evidence_sha256: None,
            typed_evidence: None,
            core_memory: None,
            core_pending: BTreeMap::new(),
            core_pending_bytes: BTreeMap::new(),
            core_reused_calls: BTreeSet::new(),
            typed_routes_sha256: typed_routes_metadata.as_ref().map(|raw| sha256(raw)),
            history_len: 0,
            history_sha256: sha256(b"[]"),
            pending: vec![],
        };
        let (raw, checksum) = raw_state(&state)?;
        tx.execute("INSERT INTO tasks(id,query,mode,status,answer,created,updated) VALUES (?1,?2,?3,'running','',unixepoch(),unixepoch())",params![task,format!("Agent turn {}",sha256(&serde_json::to_vec(request)?)),mode])?;
        if let Some(raw) = &typed_routes_metadata {
            super::typed_snapshot_storage::persist(&tx, &task, raw)?;
        }
        tx.execute("INSERT INTO agent_sessions(id,task_id,state,generation,claim,snapshot,snapshot_sha256,updated) VALUES (?1,?2,'active',0,?3,?4,?5,unixepoch())",params![id,task,claim,raw,checksum])?;
        audit(
            &tx,
            &task,
            "agent_session_start",
            json!({"format_version":1,"mode":state.mode,"call_limit":state.call_limit,"cycle_limit":state.cycle_limit,"output_limit":state.output_limit,"input_limit":state.input_limit,"tool_limit":state.tool_limit,"session_budget_scope":"tool_loop","raw_history_stored":false,"sensitive_derived_advice_stored":true,"reference_pins":state.reference_pins,"internal_tools":state.internal_tools}),
        )?;
        if let Some(snapshot) = &typed_experts {
            audit(
                &tx,
                &task,
                "agent_typed_expert_snapshot",
                snapshot.audit_metadata(),
            )?;
        }
        tx.commit()?;
        Ok(Self {
            store: runtime.store.clone(),
            task,
            id,
            generation: 0,
            claim,
            checksum,
            state,
            deadline: start.deadline,
            expert_catalog,
            typed_experts: typed_experts.clone(),
            core_typed_experts: typed_experts,
            done: false,
        })
    }

    pub fn original_objective<'a>(&self, request: &'a Turn) -> Result<&'a str> {
        request
            .messages
            .get(self.state.objective_index)
            .and_then(|message| message.content.as_deref())
            .ok_or_else(|| anyhow::anyhow!("Original collective objective is missing"))
    }

    pub fn typed_translation_contract(&self, request: &Turn) -> Option<String> {
        if self.generation != 0
            || self.state.calls != 0
            || self.state.cycles != 0
            || self
                .state
                .proposals
                .saturating_add(self.state.internal_tools)
                .saturating_add(2)
                > self.state.tool_limit
            || request
                .tool_choice
                .as_ref()
                .is_some_and(|choice| choice == "required" || choice.is_object())
        {
            return None;
        }
        let query = self.original_objective(request).ok()?;
        if super::typed_experts::Work::has_explicit_prefix(query) {
            return None;
        }
        let snapshot = self.typed_experts.as_ref()?;
        use crate::fabric::modules::Capability;
        let capabilities = [
            Capability::MathEvaluate,
            Capability::SqlQuery,
            Capability::SortingVerify,
            Capability::OrderingVerify,
            Capability::PathVerify,
            Capability::ConstraintsSolve,
        ]
        .into_iter()
        .filter(|capability| snapshot.available(*capability))
        .map(Capability::as_str)
        .collect::<Vec<_>>();
        if capabilities.is_empty() {
            return None;
        }
        Some(format!("{}\nHost-available typed capabilities: {}. Maximum typed operations: {}. For final usage include exactly the chosen typed capability set in required_capabilities. For evidence usage preserve the full request capability set, including remaining reasoning capabilities; typed capabilities must be a subset. Do not request capability versions for typed routes; preserve versions for other reasoning capabilities.",
            super::typed_experts::translation::INTERFACE_TRANSLATION_CONTRACT,
            json!(capabilities),
            self.state.tool_limit.saturating_sub(self.state.proposals.saturating_add(self.state.internal_tools)).saturating_div(2).min(super::typed_experts::MAX_TYPED_OPERATIONS as u32)))
    }

    fn typed_plan_eligible(
        &self,
        request: &Turn,
        plan: &mut super::typed_experts::Plan,
        calls: usize,
    ) -> Result<bool> {
        if self.generation != 0
            || self.state.calls != calls
            || self.state.cycles != 0
            || !self.state.pending.is_empty()
            || self
                .state
                .proposals
                .saturating_add(self.state.internal_tools)
                .saturating_add((plan.len() as u32).saturating_mul(2))
                > self.state.tool_limit
            || request
                .tool_choice
                .as_ref()
                .is_some_and(|choice| choice == "required" || choice.is_object())
        {
            return Ok(false);
        }
        if plan.query_digest() != sha256(self.original_objective(request)?.as_bytes()) {
            bail!("Typed plan differs from the original request")
        }
        if plan
            .works()
            .any(|work| work.translated() != plan.translated())
        {
            bail!("Typed plan contains inconsistent criterion origins")
        }
        let Some(snapshot) = &self.typed_experts else {
            return Ok(false);
        };
        if !snapshot.bind_plan(plan)? {
            return Ok(false);
        }
        let policy = TaskPolicy::client_collective(&self.task)?;
        for work in plan.works() {
            policy.authorize(&self.task, "jev-plus", work.capability().as_str())?;
        }
        Ok(true)
    }

    pub fn prepare_translated_work(
        &self,
        request: &Turn,
        translation: &super::typed_experts::translation::Translation,
    ) -> Result<Option<super::typed_experts::Plan>> {
        if self.typed_experts.is_none()
            || super::typed_experts::Work::has_explicit_prefix(self.original_objective(request)?)
        {
            return Ok(None);
        }
        let mut plan = super::typed_experts::Plan::from_translation(
            self.original_objective(request)?,
            translation,
        )?;
        if self.typed_plan_eligible(request, &mut plan, 1)? {
            Ok(Some(plan))
        } else {
            Ok(None)
        }
    }

    pub fn prepare_translated_batch(
        &self,
        request: &Turn,
        translations: Vec<(String, super::typed_experts::translation::Translation)>,
    ) -> Result<Option<super::typed_experts::Plan>> {
        if self.typed_experts.is_none()
            || super::typed_experts::Work::has_explicit_prefix(self.original_objective(request)?)
        {
            return Ok(None);
        }
        let mut plan = super::typed_experts::Plan::from_translated_entries(
            self.original_objective(request)?,
            translations,
        )?;
        if self.typed_plan_eligible(request, &mut plan, 1)? {
            Ok(Some(plan))
        } else {
            Ok(None)
        }
    }

    pub fn prepare_translated_graph(
        &self,
        request: &Turn,
        translation: &super::typed_experts::GraphTranslation,
    ) -> Result<Option<super::typed_experts::Plan>> {
        if self.typed_experts.is_none()
            || super::typed_experts::Work::has_explicit_prefix(self.original_objective(request)?)
        {
            return Ok(None);
        }
        let mut plan = super::typed_experts::Plan::from_translated_graph(
            self.original_objective(request)?,
            translation,
        )?;
        if self.typed_plan_eligible(request, &mut plan, 1)? {
            Ok(Some(plan))
        } else {
            Ok(None)
        }
    }

    pub fn prepare_typed_work(&self, request: &Turn) -> Result<Option<super::typed_experts::Plan>> {
        if self.typed_experts.is_none() {
            return Ok(None);
        }
        let query = self.original_objective(request)?;
        if !query.starts_with("typed-batch:")
            && !query.starts_with("typed-graph:")
            && super::typed_experts::Work::capability_hint(query).is_none()
        {
            return Ok(None);
        }
        let Some(mut plan) = super::typed_experts::Plan::from_query(query)? else {
            return Ok(None);
        };
        if self.typed_plan_eligible(request, &mut plan, 0)? {
            Ok(Some(plan))
        } else {
            Ok(None)
        }
    }

    /// Advertise task-opening or exactly restored host routes only when the
    /// independent task policy and remaining budgets admit their capabilities.
    pub(super) fn core_catalog(
        &self,
        runtime: &AgentRuntime,
        request: &Turn,
    ) -> Result<Option<super::core_tools::Catalog>> {
        let root = self.original_objective(request)?;
        if self.state.calls == 0
            || self.state.calls >= self.state.call_limit
            || runtime.cfg.language_model().is_none()
            || self.state.configuration != sha256(&serde_json::to_vec(&runtime.cfg)?)
            || !Arc::ptr_eq(&self.store, &runtime.store)
            || request
                .tool_choice
                .as_ref()
                .is_some_and(|choice| choice != "auto")
            || self
                .state
                .proposals
                .saturating_add(self.state.internal_tools)
                .saturating_add(2)
                > self.state.tool_limit
        {
            return Ok(None);
        }
        let Some(snapshot) = self.core_typed_experts.as_ref() else {
            return Ok(None);
        };
        if let Some(memory) = &self.state.core_memory {
            memory.validate(root)?;
            if memory.len() >= 16 {
                return Ok(None);
            }
        }
        let policy = TaskPolicy::client_collective(&self.task)?;
        let capabilities = [
            crate::fabric::modules::Capability::MathEvaluate,
            crate::fabric::modules::Capability::SqlQuery,
            crate::fabric::modules::Capability::SortingVerify,
            crate::fabric::modules::Capability::OrderingVerify,
            crate::fabric::modules::Capability::PathVerify,
            crate::fabric::modules::Capability::ConstraintsSolve,
        ]
        .into_iter()
        .filter(|capability| snapshot.available(*capability))
        .filter(|capability| {
            let verifier = core_verifier_capability(*capability);
            policy
                .authorize(&self.task, "jev-plus", capability.as_str())
                .is_ok()
                && policy.authorize(&self.task, "jev-plus", verifier).is_ok()
        })
        .collect::<Vec<_>>();
        super::core_tools::Catalog::new(&capabilities, request)
    }

    /// Bind model-requested JSON arguments to the opening typed snapshot, and
    /// reserve both invocation and verifier intent for every operation before
    /// any provider is polled.
    pub(super) fn prepare_core_work(
        &mut self,
        runtime: &AgentRuntime,
        request: &Turn,
        proposals: &[super::core_tools::Proposal],
    ) -> Result<PreparedTypedWork> {
        self.prepare_core_work_at(runtime, request, proposals, self.state.calls, false)
    }

    /// Expert calls may dispatch the same deterministic core tools at cycle
    /// zero, but only when the durable event ledger proves the exact call.
    pub(super) fn prepare_expert_core_work(
        &mut self,
        runtime: &AgentRuntime,
        request: &Turn,
        proposals: &[super::core_tools::Proposal],
        caller: &ExpertCoreCall,
    ) -> Result<PreparedTypedWork> {
        self.validate_expert_core_call(request, caller)?;
        self.prepare_core_work_at(runtime, request, proposals, caller.call, true)
    }

    fn validate_expert_core_call(&self, request: &Turn, caller: &ExpertCoreCall) -> Result<()> {
        if self.generation != 0
            || caller.call == 0
            || caller.call > self.state.calls
            || !valid_core_digest(&caller.request_sha256)
            || !valid_core_digest(&caller.content_sha256)
            || caller.model.is_empty()
            || caller.model.len() > 256
            || caller.model.chars().any(char::is_control)
            || self.state.cycles != 0
            || request
                .messages
                .get(self.state.objective_index)
                .is_none_or(|message| message.role != "user" || message.content.is_none())
        {
            bail!("Expert core dispatch is outside its original validated model call")
        }
        // The expert's paid dispatch may use a derived prompt. The caller hash
        // binds that exact prompt to the ledger; the session root binds this
        // request independently to the original objective.
        self.original_objective(request)?;
        remaining(&self.state.clock)?;
        let db = self.store.lock()?;
        let started = Instant::now();
        db.progress_handler(
            1000,
            Some(move || started.elapsed() > Duration::from_secs(1)),
        );
        let _progress = SessionSqlProgressReset(&db);
        let mut statement = db.prepare(
            "SELECT kind,CASE WHEN length(CAST(source AS BLOB))<=64 THEN source END,
             CASE WHEN length(CAST(payload AS BLOB))<=8192 THEN payload END
             FROM events WHERE task_id=?1 AND kind IN ('agent_model_dispatch','agent_reasoning') ORDER BY id LIMIT 513",
        )?;
        let rows = statement.query_map([&self.task], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;
        let mut dispatch = None;
        let mut reasoning = None;
        for (position, row) in rows.enumerate() {
            if position >= 512 {
                bail!("Expert core receipt history exceeds its row bound")
            }
            let (kind, source, payload) = row?;
            if source != "jev-r" {
                continue;
            }
            let value = crate::strict_json::value(payload.as_bytes(), 8192)?;
            if value["call"].as_u64() != Some(caller.call as u64) {
                continue;
            }
            if kind == "agent_model_dispatch" {
                if dispatch.is_some()
                    || value["role"] != "expert-advisory"
                    || value["cycle"] != 0
                    || value["request_sha256"] != caller.request_sha256
                    || value["authorized_capability"] != "llm.generate"
                    || value["client_tools_executed"] != false
                {
                    bail!("Expert core call has an ambiguous or mismatched dispatch receipt")
                }
                dispatch = Some(position);
            } else if kind == "agent_reasoning" {
                if reasoning.is_some()
                    || value["role"] != "solver"
                    || value["model"] != caller.model
                    || value["content_sha256"] != caller.content_sha256
                {
                    bail!("Expert core call has an ambiguous or mismatched outcome receipt")
                }
                reasoning = Some(position);
            }
        }
        if !dispatch
            .zip(reasoning)
            .is_some_and(|(sent, completed)| sent < completed)
        {
            bail!("Expert core call lacks a unique ordered dispatch and outcome receipt")
        }
        Ok(())
    }

    /// Validate an expert's paid call receipt and authorization for a possible
    /// exact reuse of a task-local, previously admitted core batch.
    pub(super) fn validate_expert_core_reuse(
        &self,
        request: &Turn,
        proposals: &[super::core_tools::Proposal],
        caller: &ExpertCoreCall,
    ) -> Result<()> {
        self.validate_expert_core_call(request, caller)?;
        self.state.validate_core_pending_bytes()?;
        if proposals.is_empty()
            || proposals.len() > 4
            || super::core_reuse::key(proposals).is_none()
            || caller.call > self.state.call_limit
            || self.state.core_pending.contains_key(&caller.call)
            || self.state.core_reused_calls.contains(&caller.call)
            || self
                .state
                .core_memory
                .as_ref()
                .is_some_and(|memory| memory.contains_call(caller.call))
        {
            bail!("Expert core reuse is outside its eligible call namespace")
        }
        let mut fingerprints = BTreeSet::new();
        for proposal in proposals {
            if !fingerprints.insert(proposal.fingerprint.as_str()) {
                bail!("Expert core reuse contains duplicate proposals")
            }
        }
        let policy = TaskPolicy::client_collective(&self.task)?;
        for proposal in proposals {
            policy.authorize(&self.task, "jev-plus", proposal.capability.as_str())?;
            policy.authorize(
                &self.task,
                "jev-plus",
                core_verifier_capability(proposal.capability),
            )?;
        }
        Ok(())
    }

    /// Return checked results already present in this task's memory when every
    /// proposal matches. A miss leaves the alias namespace available for the
    /// caller to schedule and does not dispatch or charge any work.
    pub(super) fn reuse_expert_core_work(
        &mut self,
        request: &Turn,
        proposals: &[super::core_tools::Proposal],
        caller: &ExpertCoreCall,
    ) -> Result<Option<CoreReuse>> {
        self.validate_expert_core_reuse(request, proposals, caller)?;
        let Some(memory) = self.state.core_memory.as_ref() else {
            return Ok(None);
        };
        let root = self.original_objective(request)?;
        memory.validate(root)?;
        let records = proposals
            .iter()
            .map(|proposal| memory.matching_record(&proposal.fingerprint))
            .collect::<Vec<_>>();
        if records.iter().any(Option::is_none) {
            return Ok(None);
        }

        // Re-resolve against the immutable checked outputs captured in this
        // memory. This confirms prior-record bindings still mean the same
        // thing before returning any references.
        let outputs = memory.outputs()?;
        let batch = super::core_batch::Batch::prepare(proposals, caller.call, &outputs)?;
        let root_digest = sha256(root.as_bytes());
        let mut references = Vec::with_capacity(proposals.len());
        for (index, (proposal, maybe_record)) in proposals.iter().zip(records).enumerate() {
            let record = maybe_record
                .ok_or_else(|| anyhow::anyhow!("Checked core reuse record disappeared"))?;
            let (arguments, _) = batch.resolve_completed(index, &outputs, &BTreeMap::new())?;
            let mut expected =
                super::typed_experts::Work::from_arguments(proposal.capability, &arguments)?;
            expected.mark_translated_digest(&root_digest)?;
            if record.name != proposal.name
                || record.capability != proposal.capability.as_str()
                || record.arguments_json != proposal.arguments_json
                || record.fingerprint != proposal.fingerprint
                || record.criterion_sha256 != expected.criterion_digest()
            {
                bail!("Checked core reuse record does not match its proposal proof")
            }
            references.push(json!({
                "operation_id":record.operation_id.clone(),
                "result_sha256":record.result_sha256.clone()
            }));
        }
        let context = memory.context()?;
        let memory_digest = memory.digest()?;
        let reference_digest = sha256(&serde_json::to_vec(&references)?);
        let mut next = self.state.clone();
        if !next.core_reused_calls.insert(caller.call) {
            bail!("Expert core alias namespace was already consumed")
        }
        self.write_active(
            next,
            "agent_core_memory_reused",
            json!({
                "format_version":1,
                "root_query_sha256":root_digest,
                "memory_sha256":memory_digest,
                "model_call_index":caller.call,
                "references_sha256":reference_digest,
                "references":references,
                "operation_count":proposals.len(),
                "checked_results_reused":true,
                "compiled_operation_contracts_match":true,
                "original_goal_semantics_verified":false,
                "new_operation_records":0,
                "permissions_granted":false
            }),
        )?;
        Ok(Some(CoreReuse {
            references,
            context,
        }))
    }

    fn prepare_core_work_at(
        &mut self,
        runtime: &AgentRuntime,
        request: &Turn,
        proposals: &[super::core_tools::Proposal],
        model_call_index: usize,
        expert_origin: bool,
    ) -> Result<PreparedTypedWork> {
        self.state.validate_core_pending_bytes()?;
        let root = self.original_objective(request)?;
        let available_slots = self.state.tool_limit.saturating_sub(
            self.state
                .proposals
                .saturating_add(self.state.internal_tools),
        );
        if (!expert_origin && self.state.cycles == 0)
            || model_call_index == 0
            || model_call_index > self.state.calls
            || model_call_index > self.state.call_limit
            || !self.state.pending.is_empty()
            || self.state.core_pending.contains_key(&model_call_index)
            || self.state.core_reused_calls.contains(&model_call_index)
            || self
                .state
                .core_memory
                .as_ref()
                .is_some_and(|memory| memory.contains_call(model_call_index))
            || proposals.is_empty()
            || proposals.len() > 4
            || available_slots < (proposals.len() as u32).saturating_mul(2)
            || runtime.cfg.language_model().is_none()
            || self.state.configuration != sha256(&serde_json::to_vec(&runtime.cfg)?)
            || !Arc::ptr_eq(&self.store, &runtime.store)
        {
            bail!("Model-requested core work is outside its pinned task or solver budget")
        }
        let snapshot = self
            .core_typed_experts
            .as_ref()
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Opening core expert snapshot is unavailable"))?;
        let memory = match &self.state.core_memory {
            Some(memory) => {
                memory.validate(root)?;
                memory.clone()
            }
            None => super::core_tools::Memory::new(root)?,
        };
        let baseline_bytes = memory.encoded_bytes()?;
        let pending_count = self
            .state
            .core_pending
            .values()
            .map(Vec::len)
            .sum::<usize>();
        if memory
            .len()
            .saturating_add(pending_count)
            .saturating_add(proposals.len())
            > 16
            || memory.len().saturating_add(proposals.len()) > 16
        {
            bail!("Task core memory record limit exceeded")
        }
        let mut fingerprints = std::collections::BTreeSet::new();
        for proposal in proposals {
            if memory.contains_fingerprint(&proposal.fingerprint)
                || self
                    .state
                    .core_pending
                    .values()
                    .flatten()
                    .any(|fp| fp == &proposal.fingerprint)
                || !fingerprints.insert(proposal.fingerprint.as_str())
            {
                bail!("Repeated model-requested core operation")
            }
        }
        let outputs = memory.outputs()?;
        let batch = super::core_batch::Batch::prepare(proposals, model_call_index, &outputs)?;
        let mut plan = super::typed_experts::Plan::from_reasoning_graph(root, batch.document())?;
        if !snapshot.bind_plan(&mut plan)? {
            bail!("No pinned typed route is available for a proposed core operation")
        }
        // Preflight all durable metadata before charging work. The synthetic
        // JSON result is only a lower-bound size marker; it is dropped here and
        // never persisted or exposed as evidence.
        let mut projected_memory = memory.clone();
        for (index, (proposal, entry)) in proposals.iter().zip(plan.entries()).enumerate() {
            let metadata = entry
                .provider_route
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("Pinned provider proof is missing"))?
                .audit_metadata();
            let mut provider_identity = serde_json::Map::new();
            for key in [
                "route",
                "catalog_sha256",
                "expert_id",
                "expert_version",
                "canonical_manifest_sha256",
                "provider_module_id",
                "provider_module_version",
                "capability",
                "capability_version",
            ] {
                if let Some(value) = metadata.get(key) {
                    provider_identity.insert(key.to_owned(), value.clone());
                }
            }
            let operation_id = format!("core_{}_{}", model_call_index, index);
            projected_memory.append(core_record(
                proposal,
                operation_id,
                model_call_index,
                entry.work.criterion_digest().to_owned(),
                core_provider_identity(
                    &serde_json::Value::Object(provider_identity),
                    batch.sizing_provenance(index)?,
                )?,
                "null".to_owned(),
            ))?;
        }
        let projected_bytes = projected_memory.encoded_bytes()?;
        let growth = projected_bytes
            .checked_sub(baseline_bytes)
            .ok_or_else(|| anyhow::anyhow!("Projected core memory shrank unexpectedly"))?;
        // Reservations are independent of completion order. Every batch
        // prepared against an empty record list pays for one future comma.
        let reserved_bytes = growth
            .checked_add(usize::from(memory.len() == 0))
            .ok_or_else(|| anyhow::anyhow!("Core byte reservation overflow"))?;
        let existing_reserved_bytes = self
            .state
            .core_pending_bytes
            .values()
            .try_fold(0usize, |sum, bytes| sum.checked_add(*bytes))
            .ok_or_else(|| anyhow::anyhow!("Core pending byte reservation overflow"))?;
        if reserved_bytes == 0
            || reserved_bytes > super::core_tools::MEMORY_BYTE_LIMIT
            || baseline_bytes
                .checked_add(existing_reserved_bytes)
                .and_then(|total| total.checked_add(reserved_bytes))
                .is_none_or(|total| total > super::core_tools::MEMORY_BYTE_LIMIT)
        {
            bail!("Core memory byte budget cannot reserve this batch")
        }
        let mut projected_state = self.state.clone();
        projected_state.core_memory = Some(projected_memory);
        raw_state(&projected_state)?;
        let mut reserved_state = self.state.clone();
        reserved_state.core_pending.insert(
            model_call_index,
            fingerprints.iter().map(|fp| (*fp).to_owned()).collect(),
        );
        reserved_state
            .core_pending_bytes
            .insert(model_call_index, reserved_bytes);
        raw_state(&reserved_state)?;
        self.write_active(
            reserved_state,
            "agent_core_batch_reserved",
            json!({"model_call_index":model_call_index,"operation_count":proposals.len(),"fingerprints_sha256":sha256(&serde_json::to_vec(&fingerprints)?),"reserved_memory_bytes":reserved_bytes,"expert_origin":expert_origin,"permissions_granted":false}),
        )?;
        let policy = TaskPolicy::client_collective(&self.task)?;
        let total = runtime.cfg.subtask_parallelism.unwrap_or(1).clamp(1, 4);
        let concurrency = total.min(runtime.cfg.expert_parallelism.unwrap_or(1).clamp(1, 4));
        let deadline = self
            .deadline
            .min(remaining(&self.state.clock)?)
            .min(Instant::now() + Duration::from_secs(runtime.cfg.timeout_seconds.min(300)));
        for (index, (proposal, entry)) in proposals.iter().zip(plan.entries()).enumerate() {
            let capability = proposal.capability.as_str();
            let verifier = core_verifier_capability(proposal.capability);
            policy.authorize(&self.task, "jev-plus", capability)?;
            policy.authorize(&self.task, "jev-plus", verifier)?;
            let details = json!({
                "operation_id":entry.id,
                "proposal_id":proposal.call_id,
                "name":proposal.name,
                "arguments_sha256":sha256(proposal.arguments_json.as_bytes()),
                "input_bindings":batch.admission_provenance(index)?,
                "root_query_sha256":sha256(root.as_bytes()),
                "snapshot_sha256":snapshot.digest(),
                "criterion_sha256":entry.work.criterion_digest(),
                "criterion_pending_bindings":entry.template.as_ref().is_some_and(|template| template.binding_sources().next().is_some()),
                "criterion_origin":"reasoning_model_arguments",
                "original_goal_semantics_verified":false,
                "permissions_granted":false
            });
            self.reserve_internal_tool("jev-plus", capability, details.clone())?;
            self.reserve_internal_tool("jev-plus", verifier, details)?;
        }
        let (identity, entries) = plan.into_parts();
        Ok(PreparedTypedWork {
            store: self.store.clone(),
            task: self.task.clone(),
            clock: self.state.clock.clone(),
            snapshot,
            identity,
            entries,
            deadline,
            concurrency,
            shared_slots: None,
            reasoning_arguments: true,
            core_call: Some(model_call_index),
        })
    }

    /// Build the exact memory candidate from a complete checked result set
    /// without publishing it to the session checkpoint.
    fn core_memory_from_result(
        &self,
        request: &Turn,
        proposals: &[super::core_tools::Proposal],
        result: &TypedResult,
        model_call_index: usize,
    ) -> Result<super::core_tools::Memory> {
        self.state.validate_core_pending_bytes()?;
        let root = self.original_objective(request)?;
        let root_digest = sha256(root.as_bytes());
        if !result.reasoning_arguments()
            || result.core_call != Some(model_call_index)
            || result.identity.query_digest() != root_digest
            || self.state.core_reused_calls.contains(&model_call_index)
            || result.parts.is_empty()
            || result.parts.len() != proposals.len()
            || result.parts.len() > 4
        {
            bail!("Core result is incomplete or belongs to another task")
        }
        let result_ids = result
            .parts
            .iter()
            .map(|part| part.id.clone())
            .collect::<std::collections::BTreeSet<_>>();
        if result_ids.len() != result.parts.len() {
            bail!("Core result contains duplicate operation identities")
        }
        let reserved = self
            .state
            .core_pending
            .get(&model_call_index)
            .ok_or_else(|| anyhow::anyhow!("Core result has no durable pending intent"))?;
        let proposed_fingerprints = proposals
            .iter()
            .map(|proposal| proposal.fingerprint.clone())
            .collect::<std::collections::BTreeSet<_>>();
        if proposed_fingerprints.len() != proposals.len()
            || reserved
                .iter()
                .cloned()
                .collect::<std::collections::BTreeSet<_>>()
                != proposed_fingerprints
        {
            bail!("Core result does not match its reserved pending intent")
        }
        let expected_ids = (0..proposals.len())
            .map(|index| format!("core_{}_{}", model_call_index, index))
            .collect::<std::collections::BTreeSet<_>>();
        if result_ids != expected_ids {
            bail!("Core result operation identities do not match its proposals")
        }
        let mut memory = match &self.state.core_memory {
            Some(memory) => {
                memory.validate(root)?;
                memory.clone()
            }
            None => super::core_tools::Memory::new(root)?,
        };
        if memory.contains_call(model_call_index) {
            bail!("Core model-call namespace is already committed")
        }
        // Reconstruct the original batch against its pre-batch memory. Sibling
        // data is available only through the already validated dependency graph.
        let outputs = memory.outputs()?;
        let batch = super::core_batch::Batch::prepare(proposals, model_call_index, &outputs)?;
        let checked_outputs = result
            .parts
            .iter()
            .map(|part| (part.id.clone(), part.value.clone()))
            .collect::<std::collections::BTreeMap<_, _>>();
        for (index, proposal) in proposals.iter().enumerate() {
            let operation_id = format!("core_{}_{}", model_call_index, index);
            let part = result
                .parts
                .iter()
                .find(|part| part.id == operation_id)
                .ok_or_else(|| anyhow::anyhow!("A proposed core result is missing"))?;
            let (arguments, input_provenance) =
                batch.resolve_completed(index, &outputs, &checked_outputs)?;
            let mut expected_work =
                super::typed_experts::Work::from_arguments(proposal.capability, &arguments)?;
            expected_work.mark_translated_digest(&root_digest)?;
            if part.work.capability() != proposal.capability
                || part.work.criterion_digest() != expected_work.criterion_digest()
                || part.work.translated() != result.translated()
                || memory.contains_fingerprint(&proposal.fingerprint)
            {
                bail!("Checked core result does not match its proposal")
            }
            let provider_identity =
                core_provider_identity(&part.provider_identity, &input_provenance)?;
            memory.append(core_record(
                proposal,
                part.id.clone(),
                model_call_index,
                part.work.criterion_digest().to_owned(),
                provider_identity,
                part.value.clone(),
            ))?;
        }
        memory.validate(root)?;
        let other_pending_bytes = self
            .state
            .core_pending_bytes
            .iter()
            .filter(|(call, _)| **call != model_call_index)
            .try_fold(0usize, |sum, (_, bytes)| sum.checked_add(*bytes))
            .ok_or_else(|| anyhow::anyhow!("Core pending byte reservation overflow"))?;
        if memory
            .encoded_bytes()?
            .checked_add(other_pending_bytes)
            .is_none_or(|total| total > super::core_tools::MEMORY_BYTE_LIMIT)
        {
            bail!("Core result would exceed byte reservations held by other batches")
        }
        Ok(memory)
    }

    /// Preview the bounded, untrusted memory context before final presentation.
    pub(super) fn preview_core_context(
        &self,
        request: &Turn,
        proposals: &[super::core_tools::Proposal],
        result: &TypedResult,
    ) -> Result<String> {
        let call = result
            .core_call
            .ok_or_else(|| anyhow::anyhow!("Typed result is not a core batch"))?;
        self.core_memory_from_result(request, proposals, result, call)?
            .context()
    }

    pub(super) fn preview_expert_core_context(
        &self,
        request: &Turn,
        proposals: &[super::core_tools::Proposal],
        result: &TypedResult,
        caller: &ExpertCoreCall,
    ) -> Result<String> {
        self.validate_expert_core_call(request, caller)?;
        if result.core_call != Some(caller.call) {
            bail!("Expert core result call index does not match its receipt")
        }
        self.core_memory_from_result(request, proposals, result, caller.call)?
            .context()
    }

    /// Persist only a complete checked result set, in the same checkpoint
    /// update as its audit event. Model-selected operations remain unverified
    /// as interpretations of the original user goal.
    pub(super) fn complete_core_work(
        &mut self,
        request: &Turn,
        proposals: &[super::core_tools::Proposal],
        result: &TypedResult,
    ) -> Result<String> {
        let call = result
            .core_call
            .ok_or_else(|| anyhow::anyhow!("Typed result is not a core batch"))?;
        self.complete_core_work_at(request, proposals, result, call)
    }

    pub(super) fn complete_expert_core_work(
        &mut self,
        request: &Turn,
        proposals: &[super::core_tools::Proposal],
        result: &TypedResult,
        caller: &ExpertCoreCall,
    ) -> Result<String> {
        self.validate_expert_core_call(request, caller)?;
        if result.core_call != Some(caller.call) {
            bail!("Expert core result call index does not match its receipt")
        }
        self.complete_core_work_at(request, proposals, result, caller.call)
    }

    fn complete_core_work_at(
        &mut self,
        request: &Turn,
        proposals: &[super::core_tools::Proposal],
        result: &TypedResult,
        model_call_index: usize,
    ) -> Result<String> {
        let root = self.original_objective(request)?;
        let root_digest = sha256(root.as_bytes());
        let memory = self.core_memory_from_result(request, proposals, result, model_call_index)?;
        let context = memory.context()?;
        let digest = memory.digest()?;
        let released_bytes = self
            .state
            .core_pending_bytes
            .get(&model_call_index)
            .copied()
            .ok_or_else(|| anyhow::anyhow!("Core memory byte reservation is missing"))?;
        let mut next = self.state.clone();
        next.core_memory = Some(memory);
        next.core_pending.remove(&model_call_index);
        next.core_pending_bytes.remove(&model_call_index);
        self.write_active(
            next,
            "agent_core_memory_admitted",
            json!({
                "format_version":1,
                "root_query_sha256":root_digest,
                "memory_sha256":digest,
                "model_call_index":model_call_index,
                "released_memory_reservation_bytes":released_bytes,
                "committed_memory_bytes":context.len(),
                "memory_byte_limit":super::core_tools::MEMORY_BYTE_LIMIT,
                "operation_count":proposals.len(),
                "compiled_operation_contracts_match":true,
                "original_goal_semantics_verified":false,
                "criterion_origin":"reasoning_model_arguments",
                "permissions_granted":false
            }),
        )?;
        Ok(context)
    }

    /// Restore bounded task memory after a client-tool continuation without
    /// recapturing or restoring executable provider pins.
    pub(super) fn restore_core_memory(&self, request: &Turn) -> Result<Option<String>> {
        let Some(memory) = &self.state.core_memory else {
            return Ok(None);
        };
        remaining(&self.state.clock)?;
        memory.validate(self.original_objective(request)?)?;
        Ok(Some(memory.context()?))
    }

    pub(super) fn core_memory_len(&self) -> usize {
        self.state
            .core_memory
            .as_ref()
            .map_or(0, |memory| memory.len())
    }

    /// Reserve every independent invocation before polling any operation. The
    /// remaining tool slots are held by this exclusive task claim for final checks.
    pub fn reserve_typed_work(
        &mut self,
        runtime: &AgentRuntime,
        request: &Turn,
        mut plan: super::typed_experts::Plan,
    ) -> Result<PreparedTypedWork> {
        let interpreted_calls = usize::from(plan.translated());
        if !self.typed_plan_eligible(request, &mut plan, interpreted_calls)?
            || runtime.cfg.language_model().is_none()
            || self.state.configuration != sha256(&serde_json::to_vec(&runtime.cfg)?)
            || !Arc::ptr_eq(&self.store, &runtime.store)
        {
            bail!("Typed expert plan is outside its original admission boundary")
        }
        let total = runtime.cfg.subtask_parallelism.unwrap_or(1).clamp(1, 4);
        let concurrency = total.min(runtime.cfg.expert_parallelism.unwrap_or(1).clamp(1, 4));
        let snapshot = self
            .typed_experts
            .take()
            .ok_or_else(|| anyhow::anyhow!("Opening typed expert snapshot is unavailable"))?;
        let translated = plan.translated();
        let (identity, entries) = plan.into_parts();
        for entry in &entries {
            self.reserve_internal_tool("jev-plus",entry.work.capability().as_str(),json!({
                "operation_id":entry.id,"root_query_sha256":identity.query_digest(),
                "snapshot_sha256":snapshot.digest(),"module_count":snapshot.module_count(),
                "original_query_sha256":entry.work.query_digest(),"criterion_sha256":entry.work.criterion_digest(),
                "permissions_granted":false,"model_selected_input":translated,
                "criterion_origin":if translated {"language_translation"} else {"original_exact_goal"}
            }))?;
        }
        let deadline = self
            .deadline
            .min(remaining(&self.state.clock)?)
            .min(Instant::now() + Duration::from_secs(runtime.cfg.timeout_seconds.min(300)));
        Ok(PreparedTypedWork {
            store: self.store.clone(),
            task: self.task.clone(),
            clock: self.state.clock.clone(),
            snapshot,
            identity,
            entries,
            deadline,
            concurrency,
            shared_slots: None,
            reasoning_arguments: false,
            core_call: None,
        })
    }

    /// Admit checked typed results as compact, untrusted evidence for a later
    /// reasoning phase. This checks each operation contract, not the user's
    /// original meaning and not the eventual language presentation.
    pub fn admit_typed_evidence(
        &mut self,
        request: &Turn,
        result: &TypedResult,
        independent_advisory_calls: usize,
    ) -> Result<String> {
        if self.generation != 0
            || self.state.typed_evidence_sha256.is_some()
            || independent_advisory_calls > self.state.call_limit
            || self.state.calls
                != usize::from(result.translated()).saturating_add(independent_advisory_calls)
            || self.state.cycles != 0
            || result.identity.query_digest()
                != sha256(self.original_objective(request)?.as_bytes())
            || result.parts.is_empty()
            || result.parts.len() > super::typed_experts::MAX_TYPED_OPERATIONS
        {
            bail!("Typed evidence is outside the opening translated task")
        }
        remaining(&self.state.clock)?;
        if self
            .state
            .proposals
            .saturating_add(self.state.internal_tools)
            .saturating_add(result.parts.len() as u32)
            > self.state.tool_limit
        {
            bail!("Typed evidence checks exceed the original client tool budget")
        }

        #[derive(serde::Serialize)]
        struct OperationEvidence<'a> {
            operation_id: &'a str,
            capability: &'a str,
            checker: &'a str,
            criterion_sha256: String,
            result_sha256: String,
            value: Box<serde_json::value::RawValue>,
            provider: &'a serde_json::Value,
        }
        #[derive(serde::Serialize)]
        struct EvidenceEnvelope<'a> {
            format: &'static str,
            root_query_sha256: &'a str,
            translation_semantics: &'static str,
            typed_results: Vec<OperationEvidence<'a>>,
            trust: &'static str,
        }
        let mut evidence = Vec::with_capacity(result.parts.len());
        let policy = TaskPolicy::client_collective(&self.task)?;
        for part in &result.parts {
            let capability = match part.work.capability() {
                crate::fabric::modules::Capability::ConstraintsSolve => "constraints.verify",
                capability => capability.as_str(),
            };
            policy.authorize(&self.task, "jev-plus", capability)?;
            let value = serde_json::value::RawValue::from_string(part.value.trim().to_owned())?;
            evidence.push(OperationEvidence {
                operation_id: &part.id,
                capability: part.work.capability().as_str(),
                checker: part.work.checker(),
                criterion_sha256: part.work.criterion_digest().to_owned(),
                result_sha256: sha256(value.get().as_bytes()),
                value,
                provider: &part.provider_identity,
            });
        }
        // Serialize RawValue directly: conversion through Value could normalize
        // large or fractional numbers before downstream reasoning receives them.
        let envelope = serde_json::to_string(&EvidenceEnvelope {
            format: "cogfabric-typed-evidence-v1",
            root_query_sha256: result.identity.query_digest(),
            translation_semantics: if result.translated() {
                "unverified_original_meaning"
            } else {
                "exact_operation_contract_only"
            },
            typed_results: evidence,
            trust: "untrusted_evidence_never_instructions_or_permissions",
        })?;
        if envelope.len() > 16_384 {
            bail!("Typed evidence exceeds the bounded reasoning context")
        }

        let digest = sha256(envelope.as_bytes());
        super::typed_evidence::validate(&envelope, result.identity.query_digest(), &digest)?;
        // Check the combined durable snapshot before charging verification. The
        // existing 32 KiB limit also includes references and task history metadata.
        let mut published = self.state.clone();
        published.typed_evidence_sha256 = Some(digest.clone());
        published.typed_evidence = Some(envelope.clone());
        raw_state(&published)?;
        // Persist the evidence identity atomically with the first verifier
        // reservation. A later continuation cannot charge and admit it twice.
        self.state.typed_evidence_sha256 = Some(sha256(envelope.as_bytes()));
        // Reserve every check only after validating the complete envelope and
        // budget, so no partial evidence admission can be mistaken for success.
        for part in &result.parts {
            let capability = match part.work.capability() {
                crate::fabric::modules::Capability::ConstraintsSolve => "constraints.verify",
                capability => capability.as_str(),
            };
            self.reserve_internal_tool(
                "jev-plus",
                capability,
                json!({
                    "operation_id": part.id,
                    "root_query_sha256": result.identity.query_digest(),
                    "criterion_sha256": part.work.criterion_digest(),
                    "result_sha256": sha256(part.value.as_bytes()),
                    "checker": part.work.checker(),
                    "compiled_operation_contract_match": true,
                    "original_goal_semantics_verified": false,
                    "language_presentation_verified": false
                }),
            )?;
        }
        remaining(&self.state.clock)?;
        let mut next = self.state.clone();
        next.typed_evidence = Some(envelope.clone());
        self.write_active(
            next,
            "agent_typed_evidence_admitted",
            json!({
                "format_version":2,
                "admission_generation":self.generation,
                "verifier_charges_completed":true,
                "root_query_sha256":result.identity.query_digest(),
                "operation_count":result.parts.len(),
                "independent_advisory_calls":independent_advisory_calls,
                "evidence_sha256":sha256(envelope.as_bytes()),
                "compiled_operation_contracts_match":true,
                "original_goal_semantics_verified":false,
                "language_presentation_verified":false,
                "language_translated":result.translated(),
                "permissions_granted":false
            }),
        )?;
        Ok(envelope)
    }

    /// Restore only a finalized evidence checkpoint. This never reselects routes,
    /// invokes providers, reruns checks, grants capabilities or refunds charges.
    pub fn restore_typed_evidence(&self, request: &Turn) -> Result<Option<String>> {
        let Some(digest) = self.state.typed_evidence_sha256.as_deref() else {
            if self.state.typed_evidence.is_some() {
                bail!("Typed evidence checkpoint has no integrity identity")
            }
            return Ok(None);
        };
        remaining(&self.state.clock)?;
        let raw = self.state.typed_evidence.as_deref().ok_or_else(|| {
            anyhow::anyhow!(
                "Typed evidence admission is incomplete or legacy; no providers were replayed"
            )
        })?;
        let root = sha256(self.original_objective(request)?.as_bytes());
        let count = super::typed_evidence::validate(raw, &root, digest)?;
        let db = self.store.lock()?;
        let started = Instant::now();
        db.progress_handler(
            1000,
            Some(move || started.elapsed() > Duration::from_secs(1)),
        );
        let scan_deadline = SessionSqlProgressReset(&db);
        let mut statement = db.prepare(
            "SELECT CASE WHEN length(CAST(payload AS BLOB))<=8192 THEN payload END FROM events WHERE task_id=?1 AND kind='agent_typed_evidence_admitted' AND source='jev-r' ORDER BY id LIMIT 2"
        )?;
        let records = statement
            .query_map([&self.task], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        if records.len() != 1 {
            bail!("Typed evidence checkpoint has no unique finalized admission record")
        }
        let record = crate::strict_json::value(records[0].as_bytes(), 8192)?;
        if record["format_version"] != 2
            || record["admission_generation"] != 0
            || record["evidence_sha256"].as_str() != Some(digest)
            || record["root_query_sha256"].as_str() != Some(root.as_str())
            || record["operation_count"].as_u64() != Some(count as u64)
            || record["verifier_charges_completed"] != true
            || record["compiled_operation_contracts_match"] != true
            || record["original_goal_semantics_verified"] != false
            || record["language_presentation_verified"] != false
            || record["permissions_granted"] != false
        {
            bail!("Typed evidence admission record does not match the checkpoint")
        }
        drop(statement);
        drop(scan_deadline);
        drop(db);
        remaining(&self.state.clock)?;
        self.store.event(
            &self.task,
            "agent_typed_evidence_restored",
            "verifier-r",
            json!({
                "format_version":1,"generation":self.generation,"root_query_sha256":root,
                "evidence_sha256":digest,"operation_count":count,"providers_replayed":false,
                "verification_charges_replayed":false,"original_goal_semantics_verified":false,
                "permissions_granted":false
            }),
        )?;
        Ok(Some(raw.to_owned()))
    }

    /// Save completed model advice as task-bound structured data, never facts.
    pub fn save_expert_advice(&mut self, request: &Turn, summary: &str) -> Result<String> {
        if self.generation != 0
            || self.state.calls == 0
            || self.state.cycles != 0
            || self.state.expert_advice.is_some()
        {
            bail!("Expert advice publication is outside its opening task phase")
        }
        remaining(&self.state.clock)?;
        let root = sha256(self.original_objective(request)?.as_bytes());
        let references = sha256(self.state.references.as_bytes());
        let record = super::expert_advice::Record::new(summary, &root, &references)?;
        record.validate_core_references(self.state.core_memory.as_ref())?;
        let context = record.context()?;
        let digest = record.digest().to_owned();
        let mut next = self.state.clone();
        next.expert_advice = Some(record);
        self.write_active(
            next,
            "agent_expert_advice_saved",
            json!({
                "format_version":1,"root_query_sha256":root,"references_sha256":references,
                "summary_sha256":digest,"admission_generation":self.generation,
                "original_goal_semantics_verified":false,"permissions_granted":false
            }),
        )?;
        Ok(context)
    }

    /// Restore advisory context without expert discovery, model calls or charges.
    pub fn restore_expert_advice(&self, request: &Turn) -> Result<Option<String>> {
        let Some(record) = &self.state.expert_advice else {
            return Ok(None);
        };
        remaining(&self.state.clock)?;
        let root = sha256(self.original_objective(request)?.as_bytes());
        let references = sha256(self.state.references.as_bytes());
        record.validate(&root, &references)?;
        record.validate_core_references(self.state.core_memory.as_ref())?;
        let context = record.context()?;
        let db = self.store.lock()?;
        let started = Instant::now();
        db.progress_handler(
            1000,
            Some(move || started.elapsed() > Duration::from_secs(1)),
        );
        let scan_deadline = SessionSqlProgressReset(&db);
        let mut statement = db.prepare(
            "SELECT CASE WHEN length(CAST(payload AS BLOB))<=8192 THEN payload END FROM events WHERE task_id=?1 AND kind='agent_expert_advice_saved' AND source='jev-r' ORDER BY id LIMIT 2"
        )?;
        let records = statement
            .query_map([&self.task], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        if records.len() != 1 {
            bail!("Expert advice has no unique task publication record")
        }
        let saved = crate::strict_json::value(records[0].as_bytes(), 8192)?;
        if saved["format_version"] != 1
            || saved["admission_generation"] != 0
            || saved["root_query_sha256"].as_str() != Some(root.as_str())
            || saved["references_sha256"].as_str() != Some(references.as_str())
            || saved["summary_sha256"].as_str() != Some(record.digest())
            || saved["original_goal_semantics_verified"] != false
            || saved["permissions_granted"] != false
        {
            bail!("Expert advice publication does not match its task checkpoint")
        }
        drop(statement);
        drop(scan_deadline);
        drop(db);
        remaining(&self.state.clock)?;
        self.store.event(
            &self.task,
            "agent_expert_advice_restored",
            "jev-plus",
            json!({
                "format_version":1,"summary_sha256":record.digest(),"generation":self.generation,
                "model_calls_replayed":false,"permissions_granted":false,"verified":false
            }),
        )?;
        Ok(Some(context))
    }

    pub fn verify_typed_presentation(
        &mut self,
        request: &Turn,
        result: &TypedResult,
        completion: &Completion,
    ) -> Result<Option<bool>> {
        if self.generation != 0
            || self.state.calls != usize::from(result.translated()) + 1
            || self.state.cycles != 1
            || !completion.tool_calls.is_empty()
            || result.identity.query_digest()
                != sha256(self.original_objective(request)?.as_bytes())
        {
            bail!("Typed presentation is outside the original expert task")
        }
        let answer = completion.content.as_deref().unwrap_or("");
        for part in &result.parts {
            let capability = match part.work.capability() {
                crate::fabric::modules::Capability::ConstraintsSolve => "constraints.verify",
                capability => capability.as_str(),
            };
            self.reserve_internal_tool("jev-plus",capability,json!({
                "operation_id":part.id,"root_query_sha256":result.identity.query_digest(),
                "original_query_sha256":part.work.query_digest(),"criterion_sha256":part.work.criterion_digest(),
                "result_sha256":sha256(part.value.as_bytes()),"answer_sha256":sha256(answer.as_bytes()),
                "checker":part.work.checker(),"presentation_check":true,"client_tools_executed":false
            }))?;
        }
        remaining(&self.state.clock)?;
        let checked = (|| -> Result<()> {
            let ids = result
                .parts
                .iter()
                .map(|part| part.id.as_str())
                .collect::<Vec<_>>();
            let presented = result.identity.presentation_parts(&ids, answer)?;
            for (part, presented) in result.parts.iter().zip(presented) {
                if result.identity.requires_exact_output(&part.id)
                    && presented.trim() != part.value.trim()
                {
                    bail!("Typed graph presentation changed a consumed dependency result")
                }
                part.work.verify_presentation(&part.value, &presented)?;
            }
            Ok(())
        })();
        remaining(&self.state.clock)?;
        self.store.event(&self.task,"agent_typed_presentation_check","verifier-r",json!({
            "original_query_sha256":result.identity.query_digest(),"operations":result.parts.len(),
            "answer_sha256":sha256(answer.as_bytes()),"presentation_matches_criterion":checked.is_ok(),
            "verified":checked.is_ok() && !result.translated(),
            "original_goal_semantics_verified":!result.translated(),
            "language_translated":result.translated(),"client_results_verified":false
        }))?;
        checked?;
        Ok((!result.translated()).then_some(true))
    }

    /// Refine the first expert selection from the catalog frozen at opening.
    /// The catalog is one-use process memory; tool continuations keep the saved
    /// selected subset and never discover newer registrations or model settings.
    pub fn select_experts(
        &mut self,
        runtime: &AgentRuntime,
        interpretation: &crate::language::Interpretation,
    ) -> Result<bool> {
        let Some(catalog) = self.expert_catalog.take() else {
            return Ok(false);
        };
        if self.generation != 0
            || self.state.calls != 1
            || self.state.cycles != 0
            || self.state.internal_tools == 0
            || !self.state.pending.is_empty()
            || runtime.cfg.language_model().is_none()
            || self.state.configuration != sha256(&serde_json::to_vec(&runtime.cfg)?)
            || !Arc::ptr_eq(&self.store, &runtime.store)
        {
            bail!(
                "Expert catalog selection is outside the original language interpretation boundary"
            )
        }
        remaining(&self.state.clock)?;
        TaskPolicy::client_collective(&self.task)?.authorize(
            &self.task,
            "jev-plus",
            "knowledge.search",
        )?;
        let selected = super::session_context::select_from_catalog(
            &catalog,
            interpretation,
            &self.state.references,
            &self.state.reference_pins,
            &runtime.cfg,
            runtime.native_policy.as_deref(),
        )?;
        remaining(&self.state.clock)?;
        let Some((references, reference_pins)) = selected else {
            self.write_active(
                self.state.clone(),
                "agent_expert_catalog_selection",
                json!({
                    "format_version":1,
                    "catalog_sha256":catalog.digest(),
                    "catalog_entries":catalog.len(),
                    "interpretation_sha256":sha256(&serde_json::to_vec(interpretation)?),
                    "replaced":false,
                    "reason":"no_supported_context_replacement",
                    "original_budget_preserved":true,
                    "permissions_granted":false,
                    "live_registry_read":false
                }),
            )?;
            return Ok(false);
        };
        let mut next = self.state.clone();
        let previous_sha256 = sha256(next.references.as_bytes());
        next.references = references;
        next.reference_pins = reference_pins;
        next.validate(&runtime.cfg)?;
        let payload = json!({
            "format_version":1,
            "catalog_sha256":catalog.digest(),
            "catalog_entries":catalog.len(),
            "interpretation_sha256":sha256(&serde_json::to_vec(interpretation)?),
            "previous_reference_sha256":previous_sha256,
            "selected_reference_sha256":sha256(next.references.as_bytes()),
            "selected_pins_sha256":sha256(&serde_json::to_vec(&next.reference_pins)?),
            "replaced":true,
            "original_budget_preserved":true,
            "permissions_granted":false,
            "live_registry_read":false
        });
        self.write_active(next, "agent_expert_catalog_selection", payload)?;
        Ok(true)
    }
    pub fn reserve(
        &mut self,
        role: &str,
        allocation: usize,
        bytes: &[u8],
        cycle: usize,
    ) -> Result<()> {
        if cycle == 0
            && !matches!(
                role,
                "interface" | "planner" | "explorer" | "expert-advisory"
            )
        {
            bail!("A collective solver cycle is required")
        }
        self.state.validate_limits(allocation, bytes.len(), cycle)?;
        remaining(&self.state.clock)?;
        let mut next = self.state.clone();
        next.calls += 1;
        next.cycles = cycle;
        next.reserved += allocation;
        next.prompt_bytes += bytes.len();
        let payload = json!({"role":role,"call":next.calls,"cycle":cycle,"allocation":allocation,"prompt_bytes":bytes.len(),"request_sha256":sha256(bytes),"client_tools_executed":false,"authorized_capability":"llm.generate"});
        self.write_active(next, "agent_model_dispatch", payload)
    }
    pub fn reserve_internal_tool(
        &mut self,
        actor: &str,
        capability: &str,
        payload: serde_json::Value,
    ) -> Result<()> {
        if self
            .state
            .proposals
            .saturating_add(self.state.internal_tools)
            >= self.state.tool_limit
        {
            bail!("Client collective internal tool budget exhausted")
        }
        TaskPolicy::client_collective(&self.task)?.authorize(&self.task, actor, capability)?;
        remaining(&self.state.clock)?;
        let mut next = self.state.clone();
        next.internal_tools += 1;
        self.write_active(
            next,
            "agent_internal_tool_dispatch",
            json!({
                "actor": actor,
                "capability": capability,
                "client_tools_executed": false,
                "details": payload
            }),
        )
    }
    fn write_active(&mut self, next: State, kind: &str, payload: serde_json::Value) -> Result<()> {
        next.validate_core_pending_bytes()?;
        let (raw, checksum) = raw_state(&next)?;
        let mut db = self.store.lock()?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if tx.execute("UPDATE agent_sessions SET snapshot=?2,snapshot_sha256=?3,updated=unixepoch() WHERE id=?1 AND state='active' AND claim=?4 AND generation=?5 AND snapshot_sha256=?6",params![self.id,raw,checksum,self.claim,self.generation,self.checksum])? != 1 { bail!("Collective session ownership or reservation changed") }
        let running: bool = tx.query_row(
            "SELECT status='running' FROM tasks WHERE id=?1",
            [&self.task],
            |r| r.get(0),
        )?;
        if !running {
            bail!("Collective task stopped before reservation")
        }
        audit(&tx, &self.task, kind, payload)?;
        tx.commit()?;
        self.state = next;
        self.checksum = checksum;
        Ok(())
    }
    pub fn verify(&mut self, request: &Turn, completion: &Completion) -> Result<Option<bool>> {
        if !completion.tool_calls.is_empty() {
            return Ok(None);
        }
        let query = request
            .messages
            .get(self.state.objective_index)
            .and_then(|m| m.content.as_deref())
            .ok_or_else(|| anyhow::anyhow!("Original collective objective is missing"))?;
        let Some(criterion) = crate::verification::Criterion::from_query(query)? else {
            return Ok(None);
        };
        if self
            .state
            .internal_tools
            .saturating_add(self.state.proposals)
            >= self.state.tool_limit
        {
            return Ok(None);
        }
        TaskPolicy::client_collective(&self.task)?.authorize(
            &self.task,
            "jev-plus",
            criterion.capability(),
        )?;
        let mut next = self.state.clone();
        next.internal_tools += 1;
        self.write_active(next,"agent_checker_dispatch",json!({"capability":criterion.capability(),"criterion":criterion.name(),"objective_sha256":sha256(query.as_bytes()),"answer_sha256":sha256(completion.content.as_deref().unwrap_or("").as_bytes()),"client_results_verified":false}))?;
        remaining(&self.state.clock)?;
        let verified = criterion
            .check(completion.content.as_deref().unwrap_or(""))
            .is_ok();
        remaining(&self.state.clock)?;
        self.store.event(&self.task,"agent_checker_result","verifier-r",json!({"criterion":criterion.name(),"verified":verified,"client_results_verified":false}))?;
        Ok(Some(verified))
    }
    fn checkpoint(
        &mut self,
        request: &Turn,
        completion: &mut Completion,
        advice: String,
        verified: bool,
    ) -> Result<()> {
        remaining(&self.state.clock)?;
        self.state.validate_core_pending_bytes()?;
        if !self.state.core_pending.is_empty() || !self.state.core_pending_bytes.is_empty() {
            bail!("Cannot publish a session checkpoint while typed core work is unresolved")
        }
        if self.generation >= 64 {
            bail!("Collective checkpoint generation exhausted")
        }
        if self
            .state
            .proposals
            .saturating_add(self.state.internal_tools)
            .saturating_add(completion.tool_calls.len() as u32)
            > self.state.tool_limit
        {
            bail!("Collective session tool proposal budget exhausted")
        }
        if !completion.tool_calls.is_empty()
            && request.messages.len() + 1 + completion.tool_calls.len() > 254
        {
            bail!("Client history has no room for the proposed tool results; start a new task")
        }
        let mut next = self.state.clone();
        next.pending.clear();
        for call in &mut completion.tool_calls {
            call.id = format!("cgf_{}", uuid::Uuid::new_v4().simple());
            call.function.arguments = serde_json::to_string(&crate::strict_json::value(
                call.function.arguments.as_bytes(),
                32000,
            )?)?;
            next.pending.push(call.id.clone());
        }
        completion.validate(
            request,
            self.state.tool_limit - self.state.proposals - self.state.internal_tools,
        )?;
        let mut messages = request.messages.clone();
        messages.push(Message {
            role: "assistant".into(),
            content: completion.content.clone(),
            tool_calls: completion.tool_calls.clone(),
            tool_call_id: None,
        });
        next.history_len = messages.len();
        next.history_sha256 = history_hash(&messages)?;
        next.proposals += completion.tool_calls.len() as u32;
        let waiting = !next.pending.is_empty();
        next.advice = if waiting { advice } else { String::new() };
        if !waiting {
            next.expert_advice = None;
            next.references.clear();
        }
        if next.advice.len() > 2000 {
            bail!("Collective advice checkpoint exceeds bounds")
        }
        if waiting {
            super::reasoning_memory::validate_root(
                &next.advice,
                self.original_objective(request)?,
                &next.references,
            )?;
        }
        let (raw, checksum) = raw_state(&next)?;
        let mut db = self.store.lock()?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if tx.execute("UPDATE agent_sessions SET state=?2,generation=generation+1,snapshot=?3,snapshot_sha256=?4,updated=unixepoch() WHERE id=?1 AND state='active' AND claim=?5 AND generation=?6 AND snapshot_sha256=?7",params![self.id,if waiting {"waiting"} else {"closed"},raw,checksum,self.claim,self.generation,self.checksum])? != 1 { bail!("Collective checkpoint ownership changed") }
        for call in &completion.tool_calls {
            let hash = history_hash(&[Message {
                role: "assistant".into(),
                content: None,
                tool_calls: vec![call.clone()],
                tool_call_id: None,
            }])?;
            tx.execute("INSERT INTO agent_session_calls(id,session_id,generation,call_sha256) VALUES (?1,?2,?3,?4)",params![call.id,self.id,self.generation+1,hash])?;
        }
        if tx.execute(
            "UPDATE tasks SET status=?2,updated=unixepoch() WHERE id=?1 AND status='running'",
            params![
                self.task,
                if waiting {
                    "waiting_client"
                } else {
                    if verified {
                        "completed"
                    } else {
                        "unverified"
                    }
                }
            ],
        )? != 1
        {
            bail!("Collective task stopped before checkpoint publication")
        }
        audit(
            &tx,
            &self.task,
            "agent_session_checkpoint",
            json!({"generation":self.generation+1,"state":if waiting {"waiting"} else {"closed"},"history_sha256":next.history_sha256,"pending_tools":next.pending.len(),"calls":next.calls,"cycles":next.cycles,"reserved_output_tokens":next.reserved,"proposals":next.proposals,"verified":verified}),
        )?;
        remaining(&self.state.clock)?;
        tx.commit()?;
        self.done = true;
        self.state = next;
        Ok(())
    }
    fn fail(&mut self, status: &str) -> Result<()> {
        if self.done {
            return Ok(());
        }
        let mut next = self.state.clone();
        next.advice.clear();
        next.references.clear();
        let (raw, checksum) = raw_state(&next)?;
        let mut db = self.store.lock()?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if tx.execute("UPDATE agent_sessions SET state='failed',snapshot=?2,snapshot_sha256=?3,updated=unixepoch() WHERE id=?1 AND state='active' AND claim=?4",params![self.id,raw,checksum,self.claim])? == 1 {
            tx.execute("UPDATE tasks SET status=?2,updated=unixepoch() WHERE id=?1 AND status='running'",params![self.task,status])?;
            audit(&tx,&self.task,"agent_session_failed",json!({"status":status,"calls":next.calls,"reserved_output_tokens":next.reserved,"automatic_replay":false}))?;
        }
        tx.commit()?;
        self.done = true;
        Ok(())
    }
}
impl State {
    fn validate_limits(&self, allocation: usize, bytes: usize, cycle: usize) -> Result<()> {
        if allocation == 0
            || self.calls >= self.call_limit
            || cycle > self.cycle_limit
            || cycle < self.cycles
            || self.reserved.saturating_add(allocation) > self.output_limit
            || self.prompt_bytes.saturating_add(bytes) > self.input_limit
        {
            bail!("Collective session resource budget exhausted")
        }
        Ok(())
    }
}
impl Drop for Lease {
    fn drop(&mut self) {
        let _ = self.fail("interrupted");
    }
}

pub(super) async fn run(
    runtime: &AgentRuntime,
    request: Turn,
    tokens: usize,
    cancel: impl Future<Output = ()>,
) -> Result<TurnResult> {
    let _lease = if let Some(owner) = &runtime.native_owner {
        owner.validate(&runtime.store)?;
        None
    } else {
        if runtime.native_policy.is_some() {
            bail!("Client native scope is missing its exclusive workspace owner")
        }
        runtime
            .store
            .runtime_lock
            .as_deref()
            .map(|p| ExecutionLease::acquire(p, false))
            .transpose()?
    };
    let registration = serde_json::to_value(runtime.store.pin_model(&runtime.cfg)?)?;
    let mut session = Lease::begin(runtime, &request, &registration)?;
    let task = session.task.clone();
    let mut deadline = session.deadline;
    tokio::pin!(cancel);
    let result=async {
        let mut routed = runtime.clone();
        let mut native_scope = None;
        if let Some(policy) = &runtime.native_policy {
            let owner = runtime.native_owner.clone().ok_or_else(||anyhow::anyhow!("Missing native client owner"))?;
            let scope = tokio::select! {
                biased;
                _=&mut cancel => {session.fail("cancelled")?; bail!("Agent native scope preparation cancelled")},
                result=tokio::time::timeout_at(tokio::time::Instant::from_std(deadline),
                    super::native_scope::NativeClientScope::prepare_owned_session(runtime.store.clone(), owner, &task, &runtime.cfg, policy.clone(), true, runtime.native_on_demand)) => {
                    result.map_err(|_|anyhow::anyhow!("Agent native scope preparation deadline exhausted"))??
                }
            };
            deadline = deadline.min(scope.deadline()?);
            runtime.store.event(&task,"agent_native_bridge","security-kernel",json!({
                "policy_sha256":scope.policy_sha256(),"runtime_bridge_connected":true,
                "response_instance_attested":false,"evidence":"model_key_and_inventory",
                "automatic_activation":runtime.native_on_demand,"permissions_granted":false
            }))?;
            let scope = Arc::new(scope);
            routed.backend = Arc::new(super::native_chat::ScopedChat::new(
                runtime.backend.clone(), runtime.cfg.clone(), scope.clone())?);
            native_scope = Some(scope);
        }

        runtime.store.event(&task,"model_registration","runtime",json!({"registration":registration,"weight_identity_verified":false}))?;
        runtime.store.event(&task,"agent_admission","jev-plus",json!({"collective":true,"mode":session.state.mode,"calls":session.state.call_limit,"cycles":session.state.cycle_limit,"reserved_output_tokens":tokens,"session_budget_scope":"tool_loop","session_output_limit":session.state.output_limit,"model":runtime.cfg.model,"client_tools_executed":false,"verification":"unverified"}))?;
        runtime.store.event(&task,"agent_client_capabilities","jev-plus",client_capability_audit(&request)?)?;
        let outcome=tokio::select! {
            biased;
            _=&mut cancel => {session.fail("cancelled")?; bail!("Agent collective session cancelled")},
            result=tokio::time::timeout_at(tokio::time::Instant::from_std(deadline),collective::run(&routed,routed.backend.clone(),&task,&request,tokens,deadline,&mut session)) => match result {
                Ok(result)=>result?,
                Err(_)=>{session.fail("exhausted")?;bail!("Agent collective session deadline exhausted")}
            }
        };
        let mut completion=outcome.completion;
        session.checkpoint(&request,&mut completion,outcome.advice,outcome.verification == Some(true))?;
        if completion.tool_calls.is_empty() {
            if let Some(scope) = native_scope {
                // The response is already durable. Cleanup cannot erase it or
                // turn its completed provider call into permission to replay.
                let cleanup_intent = runtime.store.event(&task,"agent_native_cleanup","security-kernel",json!({
                    "phase":"pending","response_already_committed":true,
                    "provider_stop_proven":false,"automatic_replay":false
                }));
                if cleanup_intent.is_ok() {
                    let cleanup = scope.finish_session().await;
                    // Managed release has its own durable ledger. Failure to
                    // publish optional cleanup diagnostics cannot erase a reply.
                    let _ = runtime.store.event(&task,"agent_native_cleanup","security-kernel",json!({
                        "phase":"finished","cleanup_succeeded":cleanup.is_ok(),"cleanup_failed":cleanup.is_err(),
                        "response_already_committed":true,"provider_stop_proven":false,
                        "automatic_replay":false
                    }));
                }
            }
        }
        Ok(TurnResult{task_id:task.clone(),completion})
    }.await;
    if result.is_err() {
        session.fail("failed")?;
    }
    result
}
