//! Same-image process transport for the fixed, reviewed typed-module worker.
//!
//! This is a separate process boundary, not a sandbox. The worker binary is the
//! current application image and is admitted only for the compile-time module
//! descriptor below. The transport grants one already-authorized request.
use super::{
    protocol::{self, Envelope, Kind},
    AuthorizationContext, Capability, Descriptor, FiniteConstraint, FiniteCriterion, LinearTerm,
    Module, ModuleFuture, Relation, Request, Response, SqlValue, TaskBinding,
};
use crate::{
    fabric::{
        unix::process::{managed_executable, SocketDirectory},
        worker_cleanup,
    },
    recovery::ExecutionLease,
    store::Store,
};
use anyhow::{bail, Context, Result};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    path::Path,
    sync::{Arc, Weak},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::UnixStream,
    process::Child,
};

const WORKER_ID: &str = "builtin.typed";
const WORKER_VERSION: &str = "1.0.0";
const MAX_REQUEST_LIFETIME: Duration = Duration::from_secs(300);

fn builtin_descriptor() -> Descriptor {
    Descriptor {
        id: WORKER_ID.into(),
        version: WORKER_VERSION.into(),
        capabilities: [
            Capability::MathEvaluate,
            Capability::SqlQuery,
            Capability::SortingVerify,
            Capability::OrderingVerify,
            Capability::PathVerify,
            Capability::ConstraintsSolve,
        ]
        .into_iter()
        .collect(),
    }
}

/// Process adapter for the compile-time built-in module worker.
pub struct ProcessModule {
    descriptor: Descriptor,
    store: Arc<Store>,
    lifecycle_ownership: Option<Weak<crate::residency::lifecycle::Ownership>>,
}

impl ProcessModule {
    pub fn new(descriptor: Descriptor, store: Arc<Store>) -> Result<Self> {
        Self::new_with_ownership(descriptor, store, None)
    }

    /// Construct the exact built-in process module descriptor.
    pub fn builtin(store: Arc<Store>) -> Result<Self> {
        Self::new(builtin_descriptor(), store)
    }

    pub(crate) fn new_with_ownership(
        descriptor: Descriptor,
        store: Arc<Store>,
        lifecycle_ownership: Option<Arc<crate::residency::lifecycle::Ownership>>,
    ) -> Result<Self> {
        descriptor.validate()?;
        if descriptor != builtin_descriptor() {
            bail!("No reviewed process worker is compiled for this module identity")
        }
        if let Some(ownership) = &lifecycle_ownership {
            ownership.validate(&store)?;
        }
        Ok(Self {
            descriptor,
            store,
            lifecycle_ownership: lifecycle_ownership.as_ref().map(Arc::downgrade),
        })
    }

    pub(crate) fn builtin_with_ownership(
        store: Arc<Store>,
        lifecycle_ownership: Option<Arc<crate::residency::lifecycle::Ownership>>,
    ) -> Result<Self> {
        Self::new_with_ownership(builtin_descriptor(), store, lifecycle_ownership)
    }

    async fn execute_process(
        &self,
        authorization: &AuthorizationContext,
        task: &str,
        deadline: Instant,
        request: Request,
    ) -> Result<Response> {
        uuid::Uuid::parse_str(task)?;
        let lifecycle_ownership = self
            .lifecycle_ownership
            .as_ref()
            .map(|weak| {
                weak.upgrade()
                    .context("Original native module ownership has ended")
            })
            .transpose()?;
        if let Some(owner) = &lifecycle_ownership {
            owner.validate(&self.store)?;
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() || remaining > MAX_REQUEST_LIFETIME {
            bail!("Typed worker deadline is expired or exceeds the hard limit")
        }
        let capability = request.capability();
        if !self.descriptor.capabilities.contains(&capability) {
            bail!("Typed worker request exceeds its registered descriptor")
        }
        request.validate_and_size()?;
        let actor = capability.actor();
        authorization.validate(
            task,
            actor,
            capability,
            &self.descriptor,
            deadline,
            &request,
        )?;
        let policy = if capability == Capability::ConstraintsSolve {
            crate::security::TaskPolicy::reasoning_with_finite_search(task)?
        } else {
            crate::security::TaskPolicy::reasoning(task)?
        };
        policy.authorize(task, actor, capability.as_str())?;
        crate::security::authorize(actor, capability.as_str())?;
        let correlation = authorization.correlation.clone();
        let request_hash = authorization.request_digest.clone();
        let running: bool = self.store.lock()?.query_row(
            "SELECT status='running' FROM tasks WHERE id=?1",
            [task],
            |row| row.get(0),
        )?;
        if !running {
            bail!("Cannot start a typed worker for a terminal task")
        }
        // This is a transport-level audit anchor in addition to the broker's
        // dispatch intent; a failed audit prevents process launch.
        self.store.event(
            task,
            "module_process_dispatch_intent",
            actor,
            serde_json::json!({
                "format_version":1,
                "correlation_id":correlation,
                "module_id":self.descriptor.id,
                "module_version":self.descriptor.version,
                "capability":capability.as_str(),
                "request_sha256":request_hash,
                "request_bytes":request.validate_and_size()?,
                "permissions_granted":false
            }),
        )?;

        let (socket, listener, socket_path) = SocketDirectory::bind_tool()?;
        socket.record(&self.store, task)?;
        if let Some(ownership) = &lifecycle_ownership {
            ownership.validate(&self.store)?;
        }
        let lease = if lifecycle_ownership.is_some() {
            None
        } else {
            self.store.execution_lease()?
        };
        let ownership = worker_cleanup::Ownership::acquire()?;
        let mut owner = OwnedProcess {
            child: None,
            socket: Some(socket),
            lease,
            ownership: Some(ownership),
            store: self.store.clone(),
            task: task.into(),
            lifecycle_ownership,
        };

        self.store.event(
            task,
            "module_process_launch",
            "security-kernel",
            serde_json::json!({
                "format_version":1,
                "module_id":self.descriptor.id,
                "module_version":self.descriptor.version,
                "capability":capability.as_str(),
                "correlation_id":correlation,
                "request_sha256":request_hash,
                "sandboxed":false,
                "calls_granted":1
            }),
        )?;
        let mut command = tokio::process::Command::new(managed_executable()?);
        command
            .arg("module-worker")
            .arg("--socket")
            .arg(&socket_path)
            .arg("--host-pid")
            .arg(std::process::id().to_string())
            .env_clear()
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true);
        owner.child = Some(command.spawn()?);
        let pid = owner
            .child
            .as_ref()
            .and_then(Child::id)
            .context("Typed worker child PID missing")?;
        self.store.event(
            task,
            "module_process_owned",
            "fabric",
            serde_json::json!({"pid":pid,"sandboxed":false}),
        )?;

        let mut stream = tokio::select! {
            result = tokio::time::timeout_at(tokio::time::Instant::from_std(deadline.min(Instant::now() + Duration::from_secs(5))), listener.accept()) => result.context("Typed worker connection deadline exhausted")??.0,
            status = owner.child.as_mut().context("Typed worker ownership missing")?.wait() => bail!("Typed worker exited before connecting: {:?}", status?),
        };
        let peer = stream.peer_cred()?;
        if peer.uid() != unsafe { libc::geteuid() } || peer.pid().map(|p| p as u32) != Some(pid) {
            bail!("Typed worker peer credentials differ from the host grant")
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            bail!("Typed worker startup consumed the original deadline")
        }
        let expires = now_ms()
            .saturating_add(remaining.as_millis().min(MAX_REQUEST_LIFETIME.as_millis()) as u64);
        let identity = Envelope {
            kind: Kind::Request,
            correlation,
            task: task.into(),
            actor: capability.actor().into(),
            module: self.descriptor.id.clone(),
            version: self.descriptor.version.clone(),
            capability,
            expires,
            request: Some(request),
            response: None,
            failed: false,
            error: None,
        };
        tokio::time::timeout_at(
            tokio::time::Instant::from_std(deadline),
            write_frame(&mut stream, &protocol::encode(&identity)?),
        )
        .await
        .context("Typed worker request write deadline exhausted")??;
        let response = tokio::time::timeout_at(
            tokio::time::Instant::from_std(deadline),
            read_frame(&mut stream),
        )
        .await
        .context("Typed worker response deadline exhausted")??;
        let response = protocol::decode(&response)?;
        if response.kind != Kind::Response
            || response.correlation != identity.correlation
            || response.task != identity.task
            || response.actor != identity.actor
            || response.module != identity.module
            || response.version != identity.version
            || response.capability != identity.capability
            || response.expires != identity.expires
            || response.expires <= now_ms()
        {
            bail!("Typed worker response identity or deadline mismatch")
        }
        if response.failed {
            bail!(
                "Typed worker rejected request: {}",
                response
                    .error
                    .unwrap_or_else(|| "unspecified failure".into())
            )
        }
        let output = response
            .response
            .context("Typed worker returned no result")?;
        // The parent broker independently validates the result against the request.
        drop(stream);
        reap(&mut owner, deadline).await?;
        self.store.event(
            task,
            "module_process_dispatch_outcome",
            actor,
            serde_json::json!({
                "format_version":1,
                "correlation_id":identity.correlation,
                "module_id":self.descriptor.id,
                "module_version":self.descriptor.version,
                "capability":capability.as_str(),
                "request_sha256":request_hash,
                "response_sha256":crate::store::sha256(format!("{output:?}").as_bytes()),
                "status":"returned",
                "sandboxed":false
            }),
        )?;
        Ok(output)
    }
}

impl Module for ProcessModule {
    fn descriptor(&self) -> Descriptor {
        self.descriptor.clone()
    }

    fn bind_task(&self, binding: &TaskBinding) -> Result<Option<Arc<dyn Module>>> {
        if !std::ptr::eq(self.store.as_ref(), binding.store.as_ref()) {
            bail!("Task module binding belongs to another Store")
        }
        Ok(Some(Arc::new(Self::new_with_ownership(
            self.descriptor.clone(),
            self.store.clone(),
            Some(binding.ownership.clone()),
        )?)))
    }

    fn execute<'a>(
        &'a self,
        task: &'a str,
        deadline: Instant,
        request: Request,
    ) -> ModuleFuture<'a> {
        Box::pin(async move {
            let _ = (task, deadline, request);
            bail!("Process-backed typed modules require a Broker authorization context")
        })
    }

    fn execute_authorized<'a>(
        &'a self,
        authorization: AuthorizationContext,
        task: &'a str,
        deadline: Instant,
        request: Request,
    ) -> ModuleFuture<'a> {
        Box::pin(async move {
            let capability = request.capability();
            authorization.validate(
                task,
                capability.actor(),
                capability,
                &self.descriptor,
                deadline,
                &request,
            )?;
            self.execute_process(&authorization, task, deadline, request)
                .await
        })
    }
}

struct OwnedProcess {
    child: Option<Child>,
    socket: Option<SocketDirectory>,
    lease: Option<ExecutionLease>,
    ownership: Option<worker_cleanup::Ownership>,
    store: Arc<Store>,
    task: String,
    lifecycle_ownership: Option<Arc<crate::residency::lifecycle::Ownership>>,
}

impl Drop for OwnedProcess {
    fn drop(&mut self) {
        if let Some(child) = self.child.take() {
            if child.id().is_some() {
                if let (Some(socket), Some(ownership)) = (self.socket.take(), self.ownership.take())
                {
                    let lease = self.lease.take();
                    let lifecycle_ownership = self.lifecycle_ownership.take();
                    let store = self.store.clone();
                    let task = self.task.clone();
                    if let Ok(runtime) = tokio::runtime::Handle::try_current() {
                        runtime.spawn(async move {
                            let _worker_ownership = ownership;
                            let _lifecycle_ownership = lifecycle_ownership;
                            let mut child = child;
                            if let Some(pid) = child.id() {
                                let result = async {
                                    let mut forced = false;
                                    let status = match tokio::time::timeout(Duration::from_secs(1), child.wait()).await {
                                        Ok(status) => status?,
                                        Err(_) => {
                                            forced = true;
                                            child.start_kill()?;
                                            tokio::time::timeout(Duration::from_secs(2), child.wait()).await.context("Cancelled typed worker reap deadline exhausted")??
                                        }
                                    };
                                    store.event(&task, "module_process_stopped", "fabric", serde_json::json!({"pid":pid,"forced":forced,"exit_code":status.code(),"reaped":true,"dropped_owner":true}))?;
                                    Ok::<_, anyhow::Error>(())
                                }.await;
                                if result.is_err() { worker_cleanup::record_failure(); }
                            }
                            drop(child);
                            drop(socket);
                            drop(lease);
                        });
                        return;
                    }
                    worker_cleanup::record_failure();
                }
                worker_cleanup::record_failure();
            }
        }
    }
}

async fn reap(owner: &mut OwnedProcess, deadline: Instant) -> Result<()> {
    let child = owner
        .child
        .as_mut()
        .context("Typed worker child ownership missing")?;
    let pid = child.id();
    let mut forced = false;
    let wait = async {
        match tokio::time::timeout_at(tokio::time::Instant::from_std(deadline), child.wait()).await
        {
            Ok(status) => Ok(status?),
            Err(_) => {
                forced = true;
                child.start_kill()?;
                tokio::time::timeout(Duration::from_secs(2), child.wait())
                    .await
                    .context("Typed worker reap deadline exhausted")?
                    .map_err(anyhow::Error::from)
            }
        }
    };
    let status = wait.await?;
    if !status.success() {
        bail!("Typed worker exited unsuccessfully: {:?}", status.code())
    }
    owner.store.event(
        &owner.task,
        "module_process_stopped",
        "fabric",
        serde_json::json!({"pid":pid,"forced":forced,"exit_code":status.code(),"reaped":true}),
    )?;
    owner.child.take();
    owner.socket.take();
    owner.lease.take();
    owner.ownership.take();
    Ok(())
}

/// Fixed hidden CLI entrypoint for the trusted same-image module worker.
/// The caller must invoke this before loading ordinary CLI configuration.
pub async fn worker_entrypoint(socket: &Path, host_pid: u32) -> Result<()> {
    if host_pid == 0
        || !socket.is_absolute()
        || u32::try_from(unsafe { libc::getppid() }).ok() != Some(host_pid)
    {
        bail!("Invalid typed worker host identity or socket path")
    }
    let mut stream = tokio::time::timeout(Duration::from_secs(5), UnixStream::connect(socket))
        .await
        .context("Typed worker connect deadline exhausted")??;
    let peer = stream.peer_cred()?;
    if peer.uid() != unsafe { libc::geteuid() } || peer.pid().map(|p| p as u32) != Some(host_pid) {
        bail!("Typed worker host credentials mismatch")
    }
    let frame = tokio::time::timeout(Duration::from_secs(5), read_frame(&mut stream))
        .await
        .context("Typed worker request deadline exhausted")??;
    let request = protocol::decode(&frame)?;
    if request.kind != Kind::Request
        || request.module != WORKER_ID
        || request.version != WORKER_VERSION
        || request.expires <= now_ms()
        || request.expires.saturating_sub(now_ms()) > MAX_REQUEST_LIFETIME.as_millis() as u64
        || request.actor != request.capability.actor()
    {
        bail!("Typed worker request is outside the compile-time host grant")
    }
    let policy = if request.capability == Capability::ConstraintsSolve {
        crate::security::TaskPolicy::reasoning_with_finite_search(&request.task)?
    } else {
        crate::security::TaskPolicy::reasoning(&request.task)?
    };
    policy.authorize(&request.task, &request.actor, request.capability.as_str())?;
    let remaining = Duration::from_millis(request.expires.saturating_sub(now_ms()));
    let result = tokio::time::timeout(
        remaining,
        execute_builtin(
            request
                .request
                .clone()
                .context("Missing typed worker request")?,
        ),
    )
    .await
    .context("Typed worker execution deadline exhausted")?;
    let response = protocol::response_envelope(request, result)?;
    tokio::time::timeout(
        remaining,
        write_frame(&mut stream, &protocol::encode(&response)?),
    )
    .await
    .context("Typed worker response write deadline exhausted")??;
    Ok(())
}

async fn execute_builtin(request: Request) -> Result<Response> {
    match request {
        Request::Arithmetic { expression } => Ok(Response::Arithmetic {
            value: crate::math::calculate(&expression)?,
        }),
        Request::Sql { query, tables } => execute_sql(query, tables).await,
        Request::Sort {
            values,
            candidate,
            descending,
        } => {
            let mut expected = values.clone();
            expected.sort_unstable();
            if descending {
                expected.reverse();
            }
            let mut original = values;
            original.sort_unstable();
            let mut actual = candidate.clone();
            actual.sort_unstable();
            Ok(Response::SortDiagnostic {
                claimed_valid: expected == candidate && original == actual,
            })
        }
        Request::DependencyOrder { nodes, candidate } => {
            let positions = candidate
                .iter()
                .enumerate()
                .map(|(i, id)| (id.as_str(), i))
                .collect::<BTreeMap<_, _>>();
            let valid = candidate.len() == nodes.len()
                && positions.len() == nodes.len()
                && nodes.iter().all(|node| {
                    positions.contains_key(node.id.as_str())
                        && node.depends_on.iter().all(|id| {
                            positions
                                .get(id.as_str())
                                .zip(positions.get(node.id.as_str()))
                                .is_some_and(|(dependency, node)| dependency < node)
                        })
                });
            Ok(Response::OrderingDiagnostic {
                claimed_valid: valid,
            })
        }
        Request::ShortestPath {
            nodes,
            edges,
            start,
            goal,
            candidate,
        } => {
            let valid = path_is_shortest(&nodes, &edges, &start, &goal, &candidate);
            Ok(Response::PathDiagnostic {
                claimed_valid: valid,
            })
        }
        Request::FiniteDomain { criterion } => solve_finite(criterion),
    }
}

async fn execute_sql(query: String, tables: Vec<super::SqlTable>) -> Result<Response> {
    use serde_json::{json, Value};
    let tables = tables
        .into_iter()
        .map(|table| {
            let rows = table
                .rows
                .into_iter()
                .map(|row| {
                    row.into_iter()
                        .map(|value| match value {
                            SqlValue::Null => Value::Null,
                            SqlValue::Boolean(value) => Value::Bool(value),
                            SqlValue::Integer(value) => json!(value),
                            SqlValue::Real(value) => json!(value),
                            SqlValue::Text(value) => Value::String(value),
                        })
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>();
            json!({"name":table.name,"columns":table.columns,"rows":rows})
        })
        .collect::<Vec<_>>();
    let input = serde_json::to_string(&serde_json::json!({"query":query,"tables":tables}))?;
    let raw = crate::sql::execute(input).await?;
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct SqlOutput {
        columns: Vec<String>,
        rows: Vec<Vec<serde_json::Value>>,
    }
    let output: SqlOutput = serde_json::from_str(&raw)?;
    let rows = output
        .rows
        .into_iter()
        .map(|row| {
            row.into_iter()
                .map(|value| match value {
                    serde_json::Value::Null => Ok(SqlValue::Null),
                    serde_json::Value::Bool(value) => Ok(SqlValue::Boolean(value)),
                    serde_json::Value::Number(value) if value.is_i64() => {
                        Ok(SqlValue::Integer(value.as_i64().unwrap_or_default()))
                    }
                    serde_json::Value::Number(value) if value.is_f64() => value
                        .as_f64()
                        .filter(|value| value.is_finite())
                        .map(SqlValue::Real)
                        .context("Invalid SQL real output"),
                    serde_json::Value::String(value) => Ok(SqlValue::Text(value)),
                    _ => bail!("Unsupported SQL worker output value"),
                })
                .collect::<Result<Vec<_>>>()
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(Response::Sql {
        columns: output.columns,
        rows,
    })
}

fn path_is_shortest(
    nodes: &[String],
    edges: &[(String, String)],
    start: &str,
    goal: &str,
    candidate: &[String],
) -> bool {
    let mut graph = BTreeMap::<&str, Vec<&str>>::new();
    for (from, to) in edges {
        graph.entry(from).or_default().push(to);
    }
    let mut distance = BTreeMap::from([(start, 0usize)]);
    let mut queue = VecDeque::from([start]);
    while let Some(node) = queue.pop_front() {
        let next_distance = distance[node] + 1;
        for next in graph.get(node).into_iter().flatten() {
            if !distance.contains_key(next) {
                distance.insert(next, next_distance);
                queue.push_back(next);
            }
        }
    }
    let Some(shortest) = distance.get(goal).copied() else {
        return false;
    };
    if candidate.first().map(String::as_str) != Some(start)
        || candidate.last().map(String::as_str) != Some(goal)
        || candidate.len() != shortest + 1
        || candidate.iter().any(|node| !nodes.contains(node))
    {
        return false;
    }
    candidate.windows(2).all(|pair| {
        edges
            .iter()
            .any(|(from, to)| from == &pair[0] && to == &pair[1])
    })
}

fn solve_finite(criterion: FiniteCriterion) -> Result<Response> {
    let mut values = BTreeMap::new();
    let mut explored = 0usize;
    fn search(
        index: usize,
        criterion: &FiniteCriterion,
        values: &mut BTreeMap<String, i64>,
        explored: &mut usize,
    ) -> bool {
        if index == criterion.variables.len() {
            return criterion
                .constraints
                .iter()
                .all(|constraint| satisfies(constraint, values));
        }
        let variable = &criterion.variables[index];
        for value in &variable.domain {
            *explored += 1;
            if *explored > 100_000 {
                return false;
            }
            values.insert(variable.id.clone(), *value);
            if partial_constraints_hold(criterion, values)
                && search(index + 1, criterion, values, explored)
            {
                return true;
            }
        }
        values.remove(&variable.id);
        false
    }
    let found = search(0, &criterion, &mut values, &mut explored);
    if found {
        Ok(Response::FiniteAssignment { values })
    } else {
        bail!("Finite-domain search found no assignment within its bounded search")
    }
}

fn partial_constraints_hold(criterion: &FiniteCriterion, values: &BTreeMap<String, i64>) -> bool {
    criterion
        .constraints
        .iter()
        .all(|constraint| match constraint {
            FiniteConstraint::AllDifferent { variables } => {
                let assigned = variables
                    .iter()
                    .filter_map(|id| values.get(id))
                    .collect::<Vec<_>>();
                assigned.iter().collect::<BTreeSet<_>>().len() == assigned.len()
            }
            FiniteConstraint::Linear {
                terms,
                relation,
                rhs,
            } => {
                if terms
                    .iter()
                    .any(|term| !values.contains_key(&term.variable))
                {
                    return true;
                }
                checked_linear_sum(terms, values).is_some_and(|sum| compare(sum, *relation, *rhs))
            }
        })
}

fn satisfies(constraint: &FiniteConstraint, values: &BTreeMap<String, i64>) -> bool {
    match constraint {
        FiniteConstraint::AllDifferent { variables } => {
            variables
                .iter()
                .filter_map(|id| values.get(id))
                .collect::<BTreeSet<_>>()
                .len()
                == variables.len()
        }
        FiniteConstraint::Linear {
            terms,
            relation,
            rhs,
        } => checked_linear_sum(terms, values).is_some_and(|sum| compare(sum, *relation, *rhs)),
    }
}

fn checked_linear_sum(terms: &[LinearTerm], values: &BTreeMap<String, i64>) -> Option<i64> {
    terms.iter().try_fold(0i64, |sum, term| {
        values
            .get(&term.variable)?
            .checked_mul(term.coefficient)?
            .checked_add(sum)
    })
}

fn compare(value: i64, relation: Relation, rhs: i64) -> bool {
    match relation {
        Relation::Equal => value == rhs,
        Relation::NotEqual => value != rhs,
        Relation::LessOrEqual => value <= rhs,
        Relation::GreaterOrEqual => value >= rhs,
    }
}

async fn read_frame(stream: &mut UnixStream) -> Result<Vec<u8>> {
    let length = stream.read_u32().await? as usize;
    if !(8..=protocol::MAX_FRAME).contains(&length) {
        bail!("General module frame length exceeds bounds")
    }
    let mut bytes = vec![0; length];
    stream.read_exact(&mut bytes).await?;
    Ok(bytes)
}

async fn write_frame(stream: &mut UnixStream, bytes: &[u8]) -> Result<()> {
    if !(8..=protocol::MAX_FRAME).contains(&bytes.len()) {
        bail!("General module frame length exceeds bounds")
    }
    stream.write_u32(u32::try_from(bytes.len())?).await?;
    stream.write_all(bytes).await?;
    Ok(())
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}

#[cfg(test)]
mod ownership_tests {
    use super::*;
    use crate::fabric::modules::{Broker, Grant, Limits};

    #[tokio::test]
    async fn idle_adapter_does_not_retain_or_reacquire_ended_native_authority() {
        let root = std::env::temp_dir().join(format!(
            "fabric-native-module-owner-{}",
            uuid::Uuid::new_v4()
        ));
        let store = Arc::new(Store::open(&root.join("state.db")).unwrap());
        let task = store
            .create_task("Expired native module owner", "fast")
            .unwrap();
        let owner = crate::residency::lifecycle::Ownership::acquire(store.clone()).unwrap();
        let module =
            ProcessModule::builtin_with_ownership(store.clone(), Some(owner.clone())).unwrap();
        drop(owner);
        let replacement_owner =
            crate::residency::lifecycle::Ownership::acquire(store.clone()).unwrap();
        let broker = Broker::new(store.clone());
        broker
            .register(Arc::new(module), Limits::default())
            .unwrap();
        let pin = broker.pin(&task, "builtin.typed", "1.0.0").unwrap();
        let policy = crate::security::TaskPolicy::reasoning(&task).unwrap();
        let grant =
            Grant::narrow(&task, "jev-plus", [Capability::MathEvaluate], 1, &policy).unwrap();
        assert!(broker
            .dispatch(
                &pin,
                &grant,
                &policy,
                "jev-plus",
                Request::Arithmetic {
                    expression: "6*7".into()
                },
                Instant::now() + Duration::from_secs(1)
            )
            .await
            .is_err());
        let trace = store.task(&task).unwrap();
        assert!(!trace["events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|event| event["kind"] == "module_process_launch"));
        drop(pin);
        drop(broker);
        drop(replacement_owner);
        drop(store);
        std::fs::remove_dir_all(root).unwrap();
    }
}
