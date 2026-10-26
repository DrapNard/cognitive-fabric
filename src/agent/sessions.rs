//! Durable tool-loop continuations carried by ordinary opaque tool-call IDs.
use super::*;
use crate::execution_clock::{ExecutionStart, Snapshot};
use rusqlite::{params, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};

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
    history_len: usize,
    history_sha256: String,
    pending: Vec<String>,
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
    done: bool,
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
        self.clock.validate(cfg.max_wall_seconds)?;
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
            || self.pending.len() > 16
            || self.history_len > 255
            || self.pending.iter().any(|id| !valid_id(id))
        {
            bail!("Invalid collective session checkpoint")
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
            let state: State =
                serde_json::from_value(crate::strict_json::value(raw.as_bytes(), 32768)?)?;
            if status != "closed" {
                state.validate(&runtime.cfg)?;
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
        let (references, reference_pins) = super::session_context::capture(
            &tx,
            &objective[..query_end],
            runtime.cfg.agent_project_profile.as_deref(),
            include_retrieval,
            runtime.cfg.language_model().is_some(),
        )?;
        let internal_tools = u32::from(include_retrieval);
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
            history_len: 0,
            history_sha256: sha256(b"[]"),
            pending: vec![],
        };
        let (raw, checksum) = raw_state(&state)?;
        tx.execute("INSERT INTO tasks(id,query,mode,status,answer,created,updated) VALUES (?1,?2,?3,'running','',unixepoch(),unixepoch())",params![task,format!("Agent turn {}",sha256(&serde_json::to_vec(request)?)),mode])?;
        tx.execute("INSERT INTO agent_sessions(id,task_id,state,generation,claim,snapshot,snapshot_sha256,updated) VALUES (?1,?2,'active',0,?3,?4,?5,unixepoch())",params![id,task,claim,raw,checksum])?;
        audit(
            &tx,
            &task,
            "agent_session_start",
            json!({"format_version":1,"mode":state.mode,"call_limit":state.call_limit,"cycle_limit":state.cycle_limit,"output_limit":state.output_limit,"input_limit":state.input_limit,"tool_limit":state.tool_limit,"session_budget_scope":"tool_loop","raw_history_stored":false,"sensitive_derived_advice_stored":true,"reference_pins":state.reference_pins,"internal_tools":state.internal_tools}),
        )?;
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
            done: false,
        })
    }
    pub fn reserve(
        &mut self,
        role: &str,
        allocation: usize,
        bytes: &[u8],
        cycle: usize,
    ) -> Result<()> {
        if cycle == 0 && !matches!(role, "interface" | "planner" | "explorer") {
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
    fn write_active(&mut self, next: State, kind: &str, payload: serde_json::Value) -> Result<()> {
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
            next.references.clear();
        }
        if next.advice.len() > 2000 {
            bail!("Collective advice checkpoint exceeds bounds")
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
    let _lease = runtime
        .store
        .runtime_lock
        .as_deref()
        .map(|p| ExecutionLease::acquire(p, false))
        .transpose()?;
    let registration = serde_json::to_value(runtime.store.pin_model(&runtime.cfg)?)?;
    let mut session = Lease::begin(runtime, &request, &registration)?;
    let task = session.task.clone();
    let deadline = session.deadline;
    let result=async {
        runtime.store.event(&task,"model_registration","runtime",json!({"registration":registration,"weight_identity_verified":false}))?;
        runtime.store.event(&task,"agent_admission","jev-plus",json!({"collective":true,"mode":session.state.mode,"calls":session.state.call_limit,"cycles":session.state.cycle_limit,"reserved_output_tokens":tokens,"session_budget_scope":"tool_loop","session_output_limit":session.state.output_limit,"model":runtime.cfg.model,"client_tools_executed":false,"verification":"unverified"}))?;
        runtime.store.event(&task,"agent_client_capabilities","jev-plus",client_capability_audit(&request)?)?;
        let outcome=tokio::select! {
            biased;
            _=cancel => {session.fail("cancelled")?; bail!("Agent collective session cancelled")},
            result=tokio::time::timeout_at(tokio::time::Instant::from_std(deadline),collective::run(runtime,runtime.backend.clone(),&task,&request,tokens,deadline,&mut session)) => match result {
                Ok(result)=>result?,
                Err(_)=>{session.fail("exhausted")?;bail!("Agent collective session deadline exhausted")}
            }
        };
        let mut completion=outcome.completion;
        session.checkpoint(&request,&mut completion,outcome.advice,outcome.verification == Some(true))?;
        Ok(TurnResult{task_id:task.clone(),completion})
    }.await;
    if result.is_err() {
        session.fail("failed")?;
    }
    result
}
