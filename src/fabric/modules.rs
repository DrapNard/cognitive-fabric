//! Trusted in-process typed task modules.
//!
//! This broker is a host embedding API. Registered implementations are ordinary
//! trusted Rust code; this module does not sandbox them or provision subprocesses.
//! Requests are owned Rust enums and never become an internal JSON or IPC protocol.

pub mod process;
mod protocol;
#[cfg(feature = "wasm-experts")]
pub mod wasm;
pub use process::{worker_entrypoint, ProcessModule};

use crate::{
    security::{self, TaskPolicy},
    store::Store,
};
use anyhow::{anyhow, bail, Context, Result};
use std::{
    collections::{BTreeMap, BTreeSet},
    future::Future,
    pin::Pin as StdPin,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, RwLock,
    },
    time::{Duration, Instant},
};
use tokio::sync::{mpsc, oneshot, watch, OnceCell};

const MAX_MODULES: usize = 32;
const MAX_VERSIONS: usize = 128;
const MAX_QUEUE: usize = 16;
const MAX_CONCURRENCY: usize = 4;
const DEFAULT_QUEUE: usize = 4;
const DEFAULT_CONCURRENCY: usize = 1;
const MAX_REQUEST_BYTES: usize = 32_000;
const MAX_RESULT_BYTES: usize = 16_384;
const MAX_DEADLINE: Duration = Duration::from_secs(300);

/// Capabilities already understood by the deterministic policy kernel. A module
/// cannot invent a capability name or turn a declaration into a task grant.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Capability {
    MathEvaluate,
    SqlQuery,
    SortingVerify,
    OrderingVerify,
    PathVerify,
    ConstraintsSolve,
}

impl Capability {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::MathEvaluate => "math.evaluate",
            Self::SqlQuery => "sql.query",
            Self::SortingVerify => "sorting.verify",
            Self::OrderingVerify => "ordering.verify",
            Self::PathVerify => "path.verify",
            Self::ConstraintsSolve => "constraints.solve",
        }
    }

    pub(crate) const fn actor(self) -> &'static str {
        "jev-plus"
    }
}

/// Immutable, host-registered identity and its finite capability declaration.
/// It deliberately has no deserializer: model or document metadata cannot
/// register a module or construct capabilities.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Descriptor {
    pub id: String,
    pub version: String,
    pub capabilities: BTreeSet<Capability>,
}

impl Descriptor {
    fn validate(&self) -> Result<()> {
        if self.id.is_empty()
            || self.id.len() > 64
            || !self
                .id
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"._-".contains(&b))
            || self.version.len() > 64
            || semver::Version::parse(&self.version)?.to_string() != self.version
            || self.capabilities.is_empty()
            || self.capabilities.len() > 8
        {
            bail!("Invalid typed module descriptor")
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum SqlValue {
    Null,
    Boolean(bool),
    Integer(i64),
    Real(f64),
    Text(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct SqlTable {
    pub name: String,
    pub columns: Vec<String>,
    pub rows: Vec<Vec<SqlValue>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DependencyNode {
    pub id: String,
    pub depends_on: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinearTerm {
    pub variable: String,
    pub coefficient: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Relation {
    Equal,
    NotEqual,
    LessOrEqual,
    GreaterOrEqual,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FiniteConstraint {
    Linear {
        terms: Vec<LinearTerm>,
        relation: Relation,
        rhs: i64,
    },
    AllDifferent {
        variables: Vec<String>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FiniteVariable {
    pub id: String,
    pub domain: Vec<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FiniteCriterion {
    pub variables: Vec<FiniteVariable>,
    pub constraints: Vec<FiniteConstraint>,
}

/// Bounded structured work supported by the built-in module contract.
#[derive(Clone, Debug, PartialEq)]
pub enum Request {
    Arithmetic {
        expression: String,
    },
    Sql {
        query: String,
        tables: Vec<SqlTable>,
    },
    Sort {
        values: Vec<i64>,
        candidate: Vec<i64>,
        descending: bool,
    },
    DependencyOrder {
        nodes: Vec<DependencyNode>,
        candidate: Vec<String>,
    },
    ShortestPath {
        nodes: Vec<String>,
        edges: Vec<(String, String)>,
        start: String,
        goal: String,
        candidate: Vec<String>,
    },
    FiniteDomain {
        criterion: FiniteCriterion,
    },
}

impl Request {
    pub fn capability(&self) -> Capability {
        match self {
            Self::Arithmetic { .. } => Capability::MathEvaluate,
            Self::Sql { .. } => Capability::SqlQuery,
            Self::Sort { .. } => Capability::SortingVerify,
            Self::DependencyOrder { .. } => Capability::OrderingVerify,
            Self::ShortestPath { .. } => Capability::PathVerify,
            Self::FiniteDomain { .. } => Capability::ConstraintsSolve,
        }
    }

    fn validate_and_size(&self) -> Result<usize> {
        let size = match self {
            Self::Arithmetic { expression } => {
                if expression.trim().is_empty() || expression.len() > 120 {
                    bail!("Invalid arithmetic module request")
                }
                expression.len()
            }
            Self::Sql { query, tables } => {
                if query.trim().is_empty() || query.len() > 4096 || tables.len() > 4 {
                    bail!("Invalid SQL module request")
                }
                let mut size = query.len();
                let mut table_names = BTreeSet::new();
                for table in tables {
                    if !sql_identifier(&table.name)
                        || !table_names.insert(&table.name)
                        || table.columns.is_empty()
                        || table.columns.len() > 16
                        || table.rows.len() > 256
                    {
                        bail!("Invalid SQL table bounds")
                    }
                    size = add_size(size, table.name.len())?;
                    let mut columns = BTreeSet::new();
                    for column in &table.columns {
                        if !sql_identifier(column) || !columns.insert(column) {
                            bail!("Invalid SQL column identity")
                        }
                        size = add_size(size, column.len())?;
                    }
                    for row in &table.rows {
                        if row.len() != table.columns.len() {
                            bail!("SQL row width mismatch")
                        }
                        for cell in row {
                            size = add_size(size, sql_value_size(cell)?)?;
                        }
                    }
                }
                size
            }
            Self::Sort {
                values, candidate, ..
            } => {
                if values.is_empty() || values.len() > 256 || candidate.len() != values.len() {
                    bail!("Invalid sort module request")
                }
                values
                    .len()
                    .saturating_add(candidate.len())
                    .saturating_mul(8)
            }
            Self::DependencyOrder { nodes, candidate } => {
                if nodes.is_empty() || nodes.len() > 64 || candidate.len() > 64 {
                    bail!("Invalid dependency-order module request")
                }
                let mut ids = BTreeSet::new();
                let mut size = 0usize;
                let mut edges = 0usize;
                for node in nodes {
                    if !module_identifier(&node.id) || !ids.insert(node.id.as_str()) {
                        bail!("Invalid dependency node identity")
                    }
                    size = add_size(size, node.id.len())?;
                    edges = edges.saturating_add(node.depends_on.len());
                    if node.depends_on.len() > 64 {
                        bail!("Dependency count exceeds bounds")
                    }
                    let mut dependencies = BTreeSet::new();
                    for dependency in &node.depends_on {
                        if !module_identifier(dependency)
                            || dependency == &node.id
                            || !dependencies.insert(dependency.as_str())
                        {
                            bail!("Invalid dependency edge")
                        }
                        size = add_size(size, dependency.len())?;
                    }
                }
                if edges > 256 {
                    bail!("Dependency graph exceeds edge bounds")
                }
                let known = nodes
                    .iter()
                    .map(|node| node.id.as_str())
                    .collect::<BTreeSet<_>>();
                if nodes
                    .iter()
                    .flat_map(|node| &node.depends_on)
                    .any(|id| !known.contains(id.as_str()))
                {
                    bail!("Dependency graph references an unknown node")
                }
                let mut indegree = nodes
                    .iter()
                    .map(|node| (node.id.as_str(), node.depends_on.len()))
                    .collect::<BTreeMap<_, _>>();
                let mut ready = nodes
                    .iter()
                    .filter(|node| node.depends_on.is_empty())
                    .map(|node| node.id.as_str())
                    .collect::<Vec<_>>();
                let mut visited = 0usize;
                while let Some(done) = ready.pop() {
                    visited += 1;
                    for dependent in nodes.iter().filter(|node| {
                        node.depends_on
                            .iter()
                            .any(|dependency| dependency.as_str() == done)
                    }) {
                        let count = indegree
                            .get_mut(dependent.id.as_str())
                            .context("Validated dependency node was lost")?;
                        *count -= 1;
                        if *count == 0 {
                            ready.push(dependent.id.as_str());
                        }
                    }
                }
                if visited != nodes.len() {
                    bail!("Dependency graph contains a cycle")
                }
                for id in candidate {
                    if !module_identifier(id) {
                        bail!("Invalid dependency-order candidate identity")
                    }
                    size = add_size(size, id.len())?;
                }
                size
            }
            Self::ShortestPath {
                nodes,
                edges,
                start,
                goal,
                candidate,
            } => {
                if nodes.is_empty() || nodes.len() > 64 || edges.len() > 256 || candidate.len() > 64
                {
                    bail!("Invalid shortest-path module bounds")
                }
                let known = nodes.iter().cloned().collect::<BTreeSet<_>>();
                if known.len() != nodes.len()
                    || nodes.iter().any(|id| !module_identifier(id))
                    || !known.contains(start)
                    || !known.contains(goal)
                {
                    bail!("Invalid shortest-path node identity")
                }
                let mut unique_edges = BTreeSet::new();
                let mut size = nodes
                    .iter()
                    .try_fold(start.len().saturating_add(goal.len()), |size, node| {
                        add_size(size, node.len())
                    })?;
                for (from, to) in edges {
                    if from == to
                        || !known.contains(from)
                        || !known.contains(to)
                        || !unique_edges.insert((from, to))
                    {
                        bail!("Invalid shortest-path edge")
                    }
                    size = add_size(size, from.len().saturating_add(to.len()))?;
                }
                for id in candidate {
                    if !module_identifier(id) {
                        bail!("Invalid shortest-path candidate identity")
                    }
                    size = add_size(size, id.len())?;
                }
                size
            }
            Self::FiniteDomain { criterion } => validate_finite_criterion(criterion)?,
        };
        if size > MAX_REQUEST_BYTES {
            bail!("Module request exceeds byte bound")
        }
        Ok(size)
    }
}

/// Structured output. It remains advisory until the caller applies the
/// original independent verifier; a module's return value is never proof.
#[derive(Clone, Debug, PartialEq)]
pub enum Response {
    Arithmetic {
        value: String,
    },
    Sql {
        columns: Vec<String>,
        rows: Vec<Vec<SqlValue>>,
    },
    /// Untrusted diagnostics only; compiled verification decides task success.
    SortDiagnostic {
        claimed_valid: bool,
    },
    OrderingDiagnostic {
        claimed_valid: bool,
    },
    PathDiagnostic {
        claimed_valid: bool,
    },
    FiniteAssignment {
        values: BTreeMap<String, i64>,
    },
}

impl Response {
    fn validate_and_size(&self, request: &Request) -> Result<usize> {
        let size = match (request, self) {
            (Request::Arithmetic { .. }, Self::Arithmetic { value }) if value.len() <= 256 => {
                value.len()
            }
            (Request::Sql { .. }, Self::Sql { columns, rows }) => {
                if columns.is_empty()
                    || columns.len() > 16
                    || columns
                        .iter()
                        .any(|column| column.is_empty() || column.len() > 128)
                    || columns.iter().collect::<BTreeSet<_>>().len() != columns.len()
                    || rows.len() > 64
                    || rows.iter().any(|row| row.len() != columns.len())
                {
                    bail!("Invalid SQL module result")
                }
                let mut size = 0usize;
                for column in columns {
                    size = add_size(size, column.len())?;
                }
                for row in rows {
                    for cell in row {
                        size = add_size(size, sql_value_size(cell)?)?;
                    }
                }
                size
            }
            (Request::Sort { .. }, Self::SortDiagnostic { .. }) => 1,
            (Request::DependencyOrder { .. }, Self::OrderingDiagnostic { .. }) => 1,
            (Request::ShortestPath { .. }, Self::PathDiagnostic { .. }) => 1,
            (Request::FiniteDomain { criterion }, Self::FiniteAssignment { values })
                if values.len() == criterion.variables.len()
                    && criterion.variables.iter().all(|variable| {
                        values
                            .get(&variable.id)
                            .is_some_and(|value| variable.domain.contains(value))
                    }) =>
            {
                values
                    .iter()
                    .try_fold(0usize, |size, (id, _)| add_size(size, id.len() + 8))?
            }
            _ => bail!("Module result does not match its typed request"),
        };
        if size > MAX_RESULT_BYTES {
            bail!("Module result exceeds byte bound")
        }
        Ok(size)
    }
}

pub type ModuleFuture<'a> = StdPin<Box<dyn Future<Output = Result<Response>> + Send + 'a>>;

/// Non-cloneable host authorization proof minted only after Broker.dispatch
/// validates the pin, actor grant, original TaskPolicy, and durable intent.
/// It is an in-process capability, not model or module metadata.
pub struct AuthorizationContext {
    task: String,
    actor: String,
    capability: Capability,
    module_id: String,
    module_version: String,
    request_digest: String,
    deadline: Instant,
    correlation: String,
}

impl AuthorizationContext {
    fn new(
        task: &str,
        actor: &str,
        capability: Capability,
        module: &Descriptor,
        request_digest: String,
        deadline: Instant,
        correlation: String,
    ) -> Self {
        Self {
            task: task.into(),
            actor: actor.into(),
            capability,
            module_id: module.id.clone(),
            module_version: module.version.clone(),
            request_digest,
            deadline,
            correlation,
        }
    }

    fn validate(
        &self,
        task: &str,
        actor: &str,
        capability: Capability,
        module: &Descriptor,
        deadline: Instant,
        request: &Request,
    ) -> Result<()> {
        if self.task != task
            || self.actor != actor
            || self.capability != capability
            || self.module_id != module.id
            || self.module_version != module.version
            || self.deadline != deadline
            || self.request_digest != request_digest(request)
            || uuid::Uuid::parse_str(&self.correlation).is_err()
            || Instant::now() >= self.deadline
        {
            bail!("Typed module authorization context does not match dispatch")
        }
        Ok(())
    }
}

/// Host-created binding context for a task-owned runtime. It has no public
/// fields or deserializer and conveys no new task capability.
pub struct TaskBinding {
    store: Arc<Store>,
    ownership: Arc<crate::residency::lifecycle::Ownership>,
}

/// Trusted Rust host implementation. Implementations receive no broker, Store,
/// credentials, task policy or filesystem/network handle through this contract.
/// A host can still capture such resources itself, so this is not a sandbox.
pub trait Module: Send + Sync {
    fn descriptor(&self) -> Descriptor;
    /// Captured trusted-host implementation metadata, never task authority.
    fn implementation_identity(&self) -> Option<serde_json::Value> {
        None
    }
    /// Optional compiled source identity, declared by trusted host code. This
    /// identifies sources only, not the executable, compiler or model weights.
    fn source_fingerprint(&self) -> Option<&'static str> {
        None
    }
    /// Return a task-scoped adapter when runtime ownership must be rebound.
    /// The default retains the same trusted implementation. This is metadata
    /// setup, not permission to invoke tools or models.
    fn bind_task(&self, _binding: &TaskBinding) -> Result<Option<Arc<dyn Module>>> {
        Ok(None)
    }
    /// Implementations are trusted host code and should honor cancellation and
    /// avoid detached work; dropping this future cannot prove external effects
    /// have stopped.
    fn execute<'a>(
        &'a self,
        task: &'a str,
        deadline: Instant,
        request: Request,
    ) -> ModuleFuture<'a>;

    /// Broker-authorized dispatch hook. Existing trusted in-process modules
    /// retain their old API; process-backed modules must require this proof.
    fn execute_authorized<'a>(
        &'a self,
        _authorization: AuthorizationContext,
        task: &'a str,
        deadline: Instant,
        request: Request,
    ) -> ModuleFuture<'a> {
        self.execute(task, deadline, request)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Limits {
    pub queue_capacity: usize,
    pub concurrency: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            queue_capacity: DEFAULT_QUEUE,
            concurrency: DEFAULT_CONCURRENCY,
        }
    }
}

impl Limits {
    fn validate(self) -> Result<Self> {
        if !(1..=MAX_QUEUE).contains(&self.queue_capacity)
            || !(1..=MAX_CONCURRENCY).contains(&self.concurrency)
        {
            bail!("Module queue or concurrency limit exceeds bounds")
        }
        Ok(self)
    }
}

/// A task-local subset of an already compiled policy. Construction checks the
/// existing deterministic policy; every dispatch checks it again. Clones share
/// one irreversible invocation counter.
#[derive(Clone, Debug)]
pub struct Grant {
    task: String,
    actor: String,
    capabilities: BTreeSet<Capability>,
    maximum_calls: usize,
    calls: Arc<AtomicUsize>,
}

impl Grant {
    pub fn narrow(
        task: &str,
        actor: &str,
        capabilities: impl IntoIterator<Item = Capability>,
        maximum_calls: usize,
        policy: &TaskPolicy,
    ) -> Result<Self> {
        uuid::Uuid::parse_str(task)?;
        let capabilities = capabilities.into_iter().collect::<BTreeSet<_>>();
        if capabilities.is_empty() || capabilities.len() > 8 || !(1..=64).contains(&maximum_calls) {
            bail!("Module task grant or invocation limit exceeds bounds")
        }
        for capability in &capabilities {
            if actor != capability.actor() {
                bail!("Module actor does not match compiled capability owner")
            }
            policy.authorize(task, actor, capability.as_str())?;
            security::authorize(actor, capability.as_str())?;
        }
        Ok(Self {
            task: task.into(),
            actor: actor.into(),
            capabilities,
            maximum_calls,
            calls: Arc::new(AtomicUsize::new(0)),
        })
    }
}

struct Work {
    task: String,
    request: Request,
    deadline: Instant,
    authorization: AuthorizationContext,
    reply: oneshot::Sender<Result<Response>>,
}

struct Registered {
    descriptor: Descriptor,
    source_fingerprint: Option<&'static str>,
    implementation_identity: Option<serde_json::Value>,
    implementation: Arc<dyn Module>,
    limits: Limits,
    queue: OnceCell<mpsc::Sender<Work>>,
    shutdown: Arc<watch::Sender<bool>>,
}

#[derive(Default)]
struct Registry {
    active: BTreeMap<String, Arc<Registered>>,
    versions: BTreeMap<(String, String), Arc<Registered>>,
    seen: BTreeSet<(String, String)>,
}

pub struct Broker {
    store: Arc<Store>,
    registry: RwLock<Registry>,
    shutdown: Arc<watch::Sender<bool>>,
    // Pins are valid only in the Broker instance that created them. Retained
    // Pins still work after activation swaps because the identity is stable.
    identity: uuid::Uuid,
}

/// Exact immutable module version selected for one task. Clones retain that
/// implementation even if a later version is activated or the registry changes.
#[derive(Clone)]
pub struct Pin {
    task: String,
    module: Arc<Registered>,
    broker_identity: uuid::Uuid,
}

impl Pin {
    pub fn descriptor(&self) -> &Descriptor {
        &self.module.descriptor
    }

    pub fn implementation_identity(&self) -> Option<&serde_json::Value> {
        self.module.implementation_identity.as_ref()
    }

    pub fn source_fingerprint(&self) -> Option<&'static str> {
        self.module.source_fingerprint
    }

    pub fn task(&self) -> &str {
        &self.task
    }

    /// Queue and execution limits captured by the original Broker registration.
    pub fn limits(&self) -> Limits {
        self.module.limits
    }

    /// Local Broker instance that issued this task pin. This is provenance
    /// metadata, not a cryptographic attestation of the module implementation.
    pub fn broker_identity(&self) -> uuid::Uuid {
        self.broker_identity
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Status {
    pub descriptor: Descriptor,
    pub active: bool,
    pub available: bool,
    pub queued: usize,
}

impl Broker {
    /// Only reviewed recovery code may reconstruct a recorded registry UUID.
    /// This restores local provenance, not process identity or executable trust.
    #[cfg(test)]
    pub(crate) fn reconstructed_builtin(
        store: Arc<Store>,
        identity: uuid::Uuid,
    ) -> Result<Arc<Self>> {
        Self::reconstructed_builtin_owned(store, identity, None)
    }
    pub(crate) fn reconstructed_builtin_owned(
        store: Arc<Store>,
        identity: uuid::Uuid,
        ownership: Option<Arc<crate::residency::lifecycle::Ownership>>,
    ) -> Result<Arc<Self>> {
        let mut broker = Self::new(store.clone());
        broker.identity = identity;
        broker.register(
            Arc::new(ProcessModule::builtin_with_ownership(store, ownership)?),
            Limits::default(),
        )?;
        Ok(Arc::new(broker))
    }

    /// Rebuild an operator supplied, already compiled Wasm registry while
    /// preserving the broker UUID recorded in an interrupted task snapshot.
    /// This restores local routing provenance only; it does not grant task
    /// permissions or attest the operator's artifacts beyond their load pins.
    #[cfg(feature = "wasm-experts")]
    pub(crate) fn reconstructed_wasm(
        store: Arc<Store>,
        identity: uuid::Uuid,
        modules: Vec<(wasm::WasmModule, Limits)>,
    ) -> Result<Arc<Self>> {
        if modules.is_empty() || modules.len() > MAX_MODULES {
            bail!("Wasm registry reconstruction module count exceeds bounds")
        }
        let mut broker = Self::new(store);
        broker.identity = identity;
        for (module, limits) in modules {
            broker.register(Arc::new(module), limits)?;
        }
        Ok(Arc::new(broker))
    }

    pub fn new(store: Arc<Store>) -> Self {
        Self {
            store,
            registry: RwLock::new(Registry::default()),
            shutdown: Arc::new(watch::channel(false).0),
            identity: uuid::Uuid::new_v4(),
        }
    }

    /// Fork the active trusted catalog for one owned native task. Binding
    /// callbacks run outside the source registry lock; the source is untouched.
    pub(crate) fn fork_owned(
        &self,
        ownership: Arc<crate::residency::lifecycle::Ownership>,
    ) -> Result<Arc<Self>> {
        ownership.validate(&self.store)?;
        let active = {
            let registry = self
                .registry
                .read()
                .map_err(|_| anyhow!("Typed module registry poisoned"))?;
            if *self.shutdown.borrow() {
                bail!("Typed module registry is shut down")
            }
            registry
                .active
                .values()
                .filter(|module| module.available())
                .map(|module| {
                    (
                        module.descriptor.clone(),
                        module.implementation.clone(),
                        module.limits,
                        module.source_fingerprint,
                        module.implementation_identity.clone(),
                    )
                })
                .collect::<Vec<_>>()
        };
        let binding = TaskBinding {
            store: self.store.clone(),
            ownership,
        };
        let fork = Arc::new(Self::new(self.store.clone()));
        for (descriptor, original, limits, fingerprint, identity) in active {
            let implementation = original.bind_task(&binding)?.unwrap_or(original);
            if implementation.descriptor() != descriptor
                || implementation.source_fingerprint() != fingerprint
                || implementation.implementation_identity() != identity
            {
                bail!("Task binding changed an immutable module descriptor")
            }
            fork.register(implementation, limits)?;
        }
        Ok(fork)
    }

    /// Process-local identity used to bind task snapshots to this Broker
    /// instance. It does not attest or sandbox registered trusted code.
    pub fn identity_id(&self) -> uuid::Uuid {
        self.identity
    }

    /// Whether a pin was issued by this exact broker instance.
    ///
    /// This checks local registry provenance only; it does not attest or
    /// sandbox the trusted implementation registered in the broker.
    pub fn owns_pin(&self, pin: &Pin) -> bool {
        self.identity == pin.broker_identity
    }

    /// Register trusted host code. Module metadata is never accepted from a task,
    /// model response, document, or IPC actor.
    pub fn register(&self, implementation: Arc<dyn Module>, limits: Limits) -> Result<()> {
        let descriptor = implementation.descriptor();
        let implementation_identity = implementation.implementation_identity();
        if implementation_identity
            .as_ref()
            .is_some_and(|identity| identity.to_string().len() > 1024)
        {
            bail!("Module implementation identity exceeds bounds")
        }
        let source_fingerprint = implementation.source_fingerprint();
        if source_fingerprint.is_some_and(|hash| {
            hash.len() != 64 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit())
        }) {
            bail!("Invalid declared module source fingerprint")
        }
        descriptor.validate()?;
        let limits = limits.validate()?;
        let key = (descriptor.id.clone(), descriptor.version.clone());
        let mut registry = self
            .registry
            .write()
            .map_err(|_| anyhow!("Typed module registry poisoned"))?;
        if *self.shutdown.borrow() {
            bail!("Typed module broker shut down")
        }
        if registry.seen.len() >= MAX_VERSIONS || registry.seen.contains(&key) {
            bail!("Typed module identity is immutable or registry capacity exhausted")
        }
        if !registry.active.contains_key(&descriptor.id) && registry.active.len() >= MAX_MODULES {
            bail!("Typed module active capacity exhausted")
        }
        if descriptor.capabilities.iter().any(|capability| {
            registry.active.values().any(|active| {
                active.available()
                    && active.descriptor.id != descriptor.id
                    && active.descriptor.capabilities.contains(capability)
            })
        }) {
            bail!("Conflicting active typed module capability")
        }
        let module = Arc::new(Registered {
            descriptor: descriptor.clone(),
            source_fingerprint,
            implementation_identity,
            implementation,
            limits,
            queue: OnceCell::new(),
            shutdown: self.shutdown.clone(),
        });
        registry.seen.insert(key.clone());
        registry.versions.insert(key, module.clone());
        registry.active.insert(descriptor.id, module);
        Ok(())
    }

    /// Activate an immutable version explicitly. The existing active version is
    /// replaced only for future pins; in-flight pins are unchanged.
    pub fn activate(&self, id: &str, version: &str) -> Result<()> {
        if id.len() > 64 || version.len() > 64 {
            bail!("Typed module selection exceeds bounds")
        }
        let mut registry = self
            .registry
            .write()
            .map_err(|_| anyhow!("Typed module registry poisoned"))?;
        if *self.shutdown.borrow() {
            bail!("Typed module broker shut down")
        }
        let module = registry
            .versions
            .get(&(id.into(), version.into()))
            .cloned()
            .context("Unknown typed module version")?;
        if !module.available() {
            bail!("Cannot activate a closed typed module version")
        }
        if module.descriptor.capabilities.iter().any(|capability| {
            registry.active.values().any(|active| {
                active.available()
                    && active.descriptor.id != id
                    && active.descriptor.capabilities.contains(capability)
            })
        }) {
            bail!("Conflicting active typed module capability")
        }
        registry.active.insert(id.into(), module);
        Ok(())
    }

    /// Remove an active route for future pins. Retained versions and existing
    /// task pins remain valid until explicitly retired.
    pub fn deactivate(&self, id: &str) -> Result<()> {
        self.registry
            .write()
            .map_err(|_| anyhow!("Typed module registry poisoned"))?
            .active
            .remove(id)
            .context("Unknown active typed module")?;
        Ok(())
    }

    pub fn retire(&self, id: &str, version: &str) -> Result<()> {
        if id.len() > 64 || version.len() > 64 {
            bail!("Typed module selection exceeds bounds")
        }
        let mut registry = self
            .registry
            .write()
            .map_err(|_| anyhow!("Typed module registry poisoned"))?;
        if registry
            .active
            .get(id)
            .is_some_and(|active| active.descriptor.version == version)
        {
            bail!("Cannot retire an active typed module version")
        }
        registry
            .versions
            .remove(&(id.into(), version.into()))
            .context("Unknown retained typed module version")?;
        Ok(())
    }

    pub fn discover(&self) -> Result<Vec<Descriptor>> {
        Ok(self
            .registry
            .read()
            .map_err(|_| anyhow!("Typed module registry poisoned"))?
            .active
            .values()
            .filter(|module| module.available())
            .map(|module| module.descriptor.clone())
            .collect())
    }

    pub fn versions(&self) -> Result<Vec<Descriptor>> {
        Ok(self
            .registry
            .read()
            .map_err(|_| anyhow!("Typed module registry poisoned"))?
            .versions
            .values()
            .map(|module| module.descriptor.clone())
            .collect())
    }

    pub fn pin(&self, task: &str, id: &str, version: &str) -> Result<Pin> {
        uuid::Uuid::parse_str(task)?;
        if id.len() > 64 || version.len() > 64 {
            bail!("Typed module selection exceeds bounds")
        }
        let module = self
            .registry
            .read()
            .map_err(|_| anyhow!("Typed module registry poisoned"))?
            .active
            .get(id)
            .filter(|module| module.descriptor.version == version)
            .filter(|module| module.available())
            .cloned()
            .context("Requested typed module version is not active and available")?;
        Ok(Pin {
            task: task.into(),
            module,
            broker_identity: self.identity,
        })
    }

    /// Restoration uses the same Store object and retained registered code;
    /// metadata can neither construct a Broker nor replace its implementation.
    pub(crate) fn require_store(&self, store: &Store) -> Result<()> {
        if !std::ptr::eq(self.store.as_ref(), store) {
            bail!("Restored typed registry belongs to another Store")
        }
        Ok(())
    }

    pub(crate) fn pin_retained(&self, task: &str, descriptor: &Descriptor) -> Result<Pin> {
        uuid::Uuid::parse_str(task)?;
        descriptor.validate()?;
        let module = self
            .registry
            .read()
            .map_err(|_| anyhow!("Typed module registry poisoned"))?
            .versions
            .get(&(descriptor.id.clone(), descriptor.version.clone()))
            .filter(|module| module.descriptor == *descriptor && module.available())
            .cloned()
            .context("Original typed module version is not retained and available")?;
        Ok(Pin {
            task: task.into(),
            module,
            broker_identity: self.identity,
        })
    }

    pub fn status(&self) -> Result<Vec<Status>> {
        let registry = self
            .registry
            .read()
            .map_err(|_| anyhow!("Typed module registry poisoned"))?;
        Ok(registry
            .versions
            .values()
            .map(|module| Status {
                descriptor: module.descriptor.clone(),
                active: registry
                    .active
                    .get(&module.descriptor.id)
                    .is_some_and(|active| Arc::ptr_eq(active, module)),
                available: module.available(),
                queued: module
                    .queue
                    .get()
                    .map_or(0, |queue| queue.max_capacity() - queue.capacity()),
            })
            .collect())
    }

    /// Dispatch one typed request under the intersection of the pin, the
    /// operator-created grant and the original deterministic task policy.
    /// Queue wait and module execution share the caller's original deadline.
    pub async fn dispatch(
        &self,
        pin: &Pin,
        grant: &Grant,
        policy: &TaskPolicy,
        actor: &str,
        request: Request,
        deadline: Instant,
    ) -> Result<Response> {
        if !self.owns_pin(pin) {
            bail!("Typed module pin was issued by another broker")
        }
        let task = pin.task.as_str();
        let capability = request.capability();
        if grant.task != task
            || grant.actor != actor
            || !grant.capabilities.contains(&capability)
            || !pin.module.descriptor.capabilities.contains(&capability)
            || actor != capability.actor()
        {
            bail!("Typed module dispatch is outside its pinned grant")
        }
        if !pin.module.available() {
            bail!(
                "Pinned typed module is closed or unavailable; it will not be retried or replaced"
            )
        }
        policy.authorize(task, actor, capability.as_str())?;
        security::authorize(actor, capability.as_str())?;
        let request_bytes = request.validate_and_size()?;
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() || remaining > MAX_DEADLINE {
            bail!("Typed module deadline is expired or exceeds the task limit")
        }
        let mut calls = grant.calls.load(Ordering::Acquire);
        loop {
            if calls >= grant.maximum_calls {
                bail!("Typed module invocation allowance exhausted")
            }
            match grant.calls.compare_exchange_weak(
                calls,
                calls + 1,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => break,
                Err(current) => calls = current,
            }
        }
        let correlation = uuid::Uuid::new_v4().to_string();
        let request_hash = request_digest(&request);
        let response_validation_request = request.clone();
        self.store.event(
            task,
            "module_dispatch_intent",
            actor,
            serde_json::json!({
                "format_version":1,
                "correlation_id":correlation,
                "module_id":pin.module.descriptor.id,
                "module_version":pin.module.descriptor.version,
                "capability":capability.as_str(),
                "request_sha256":request_hash,
                "request_bytes":request_bytes,
                "deadline_ms":remaining.as_millis().min(u64::MAX as u128) as u64,
                "module_granted_permissions":false
            }),
        )?;

        let authorization = AuthorizationContext::new(
            task,
            actor,
            capability,
            &pin.module.descriptor,
            request_hash.clone(),
            deadline,
            correlation.clone(),
        );

        let module = pin.module.clone();
        let (reply, result) = oneshot::channel();
        let work = Work {
            task: task.into(),
            request,
            deadline,
            authorization,
            reply,
        };
        let queue = module.queue().await;
        tokio::time::timeout_at(tokio::time::Instant::from_std(deadline), queue.send(work))
            .await
            .map_err(|_| anyhow!("Typed module queue deadline exhausted"))?
            .map_err(|_| anyhow!("Typed module queue is closed"))?;
        let received =
            match tokio::time::timeout_at(tokio::time::Instant::from_std(deadline), result).await {
                Err(_) => Err("deadline_unknown"),
                Ok(Err(_)) => Err("worker_stopped_unknown"),
                Ok(Ok(Err(_))) => Err("failed_or_unknown"),
                Ok(Ok(Ok(response))) => Ok(response),
            };
        let (response, status, response_hash, response_bytes) = match received {
            Ok(response) => match response.validate_and_size(&response_validation_request) {
                Ok(bytes) => {
                    let hash = response_digest(&response);
                    (Some(response), "returned", Some(hash), Some(bytes))
                }
                Err(_) => (None, "invalid_response", None, None),
            },
            Err(status) => (None, status, None, None),
        };
        // This outcome event is deliberately after execution. A failed write does
        // not refund or retry the already dispatched module operation.
        self.store.event(
            task,
            "module_dispatch_outcome",
            actor,
            serde_json::json!({
                "format_version":1,
                "correlation_id":correlation,
                "module_id":pin.module.descriptor.id,
                "module_version":pin.module.descriptor.version,
                "capability":capability.as_str(),
                "request_sha256":request_hash,
                "response_sha256":response_hash,
                "response_bytes":response_bytes,
                "status":status,
                "provider_stop_proven":false
            }),
        )?;
        response.ok_or_else(|| anyhow!("Typed module failed or returned an invalid result"))
    }

    pub fn shutdown(&self) -> Result<()> {
        let mut registry = self
            .registry
            .write()
            .map_err(|_| anyhow!("Typed module registry poisoned"))?;
        self.shutdown.send_replace(true);
        registry.active.clear();
        registry.versions.clear();
        Ok(())
    }
}

impl Drop for Broker {
    fn drop(&mut self) {
        // Queue workers retain Registered (and its sender). Explicitly break
        // that lifetime cycle when the last owning Broker goes away.
        self.shutdown.send_replace(true);
    }
}

impl Registered {
    fn available(&self) -> bool {
        !*self.shutdown.borrow() && self.queue.get().is_none_or(|sender| !sender.is_closed())
    }

    async fn queue(self: &Arc<Self>) -> &mpsc::Sender<Work> {
        self.queue
            .get_or_init(|| async {
                let (sender, mut receiver) = mpsc::channel::<Work>(self.limits.queue_capacity);
                let module = self.clone();
                let mut shutdown = self.shutdown.subscribe();
                tokio::spawn(async move {
                    let mut tasks = tokio::task::JoinSet::new();
                    loop {
                        if tasks.len() >= module.limits.concurrency {
                            tokio::select! {
                                biased;
                                _ = wait_shutdown(&mut shutdown) => {
                                    tasks.abort_all();
                                    while tasks.join_next().await.is_some() {}
                                    break;
                                }
                                joined = tasks.join_next(), if !tasks.is_empty() => {
                                    if joined.is_some_and(|result| result.is_err()) {
                                        tasks.abort_all();
                                        while tasks.join_next().await.is_some() {}
                                        break;
                                    }
                                }
                            }
                            continue;
                        }
                        tokio::select! {
                            biased;
                            _ = wait_shutdown(&mut shutdown) => {
                                tasks.abort_all();
                                while tasks.join_next().await.is_some() {}
                                break;
                            }
                            work = receiver.recv() => match work {
                                Some(work) if !work.reply.is_closed() => {
                                    let module = module.clone();
                                    let shutdown = shutdown.clone();
                                    tasks.spawn(async move { execute_work(module, work, shutdown).await });
                                }
                                Some(_) => {}
                                None => {
                                    tasks.abort_all();
                                    while tasks.join_next().await.is_some() {}
                                    break;
                                }
                            },
                            joined = tasks.join_next(), if !tasks.is_empty() => {
                                if joined.is_some_and(|result| result.is_err()) {
                                    tasks.abort_all();
                                    while tasks.join_next().await.is_some() {}
                                    break;
                                }
                            }
                        }
                    }
                });
                sender
            })
            .await
    }
}

async fn execute_work(
    module: Arc<Registered>,
    mut work: Work,
    mut shutdown: watch::Receiver<bool>,
) {
    if work.reply.is_closed() || Instant::now() >= work.deadline {
        let _ = work
            .reply
            .send(Err(anyhow!("Typed module request expired or cancelled")));
        return;
    }
    let task = work.task.clone();
    let request = work.request;
    let authorization = work.authorization;
    let expected = request.capability();
    if !module.descriptor.capabilities.contains(&expected) {
        let _ = work
            .reply
            .send(Err(anyhow!("Typed module capability changed")));
        return;
    }
    if let Err(error) = authorization.validate(
        &task,
        expected.actor(),
        expected,
        &module.descriptor,
        work.deadline,
        &request,
    ) {
        let _ = work.reply.send(Err(error));
        return;
    }
    let deadline = tokio::time::Instant::from_std(work.deadline);
    let result = tokio::select! {
        biased;
        _ = wait_shutdown(&mut shutdown) => Err(anyhow!("Typed module broker shut down")),
        _ = work.reply.closed() => Err(anyhow!("Typed module caller cancelled")),
        result = tokio::time::timeout_at(deadline, module.implementation.execute_authorized(authorization, &task, work.deadline, request.clone())) => {
            match result {
                Ok(result) => result,
                Err(_) => Err(anyhow!("Typed module execution deadline exhausted")),
            }
        }
    };
    let result = result.and_then(|response| {
        response.validate_and_size(&request)?;
        Ok(response)
    });
    let _ = work.reply.send(result);
}

async fn wait_shutdown(shutdown: &mut watch::Receiver<bool>) {
    let _ = shutdown.wait_for(|stopped| *stopped).await;
}

fn request_digest(request: &Request) -> String {
    // Debug rendering is internal metadata hashing, never an IPC representation.
    crate::store::sha256(format!("{request:?}").as_bytes())
}

fn response_digest(response: &Response) -> String {
    crate::store::sha256(format!("{response:?}").as_bytes())
}

fn sql_identifier(value: &str) -> bool {
    (1..=32).contains(&value.len())
        && value
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_lowercase())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
        && !value.starts_with("sqlite_")
}

fn module_identifier(value: &str) -> bool {
    (1..=32).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

fn sql_value_size(value: &SqlValue) -> Result<usize> {
    Ok(match value {
        SqlValue::Null => 1,
        SqlValue::Boolean(_) => 1,
        SqlValue::Integer(_) => 8,
        SqlValue::Real(value) if value.is_finite() => 8,
        SqlValue::Real(_) => bail!("SQL real cell must be finite"),
        SqlValue::Text(value) if value.len() <= 2_048 => value.len(),
        SqlValue::Text(_) => bail!("SQL text cell exceeds bounds"),
    })
}

fn add_size(size: usize, additional: usize) -> Result<usize> {
    size.checked_add(additional)
        .context("Typed module payload size overflow")
}

fn validate_finite_criterion(criterion: &FiniteCriterion) -> Result<usize> {
    if criterion.variables.is_empty()
        || criterion.variables.len() > 32
        || criterion.constraints.is_empty()
        || criterion.constraints.len() > 128
    {
        bail!("Invalid finite-domain criterion bounds")
    }
    let mut variables = BTreeSet::new();
    let mut size = 0usize;
    for variable in &criterion.variables {
        if !module_identifier(&variable.id)
            || !variables.insert(variable.id.as_str())
            || variable.domain.is_empty()
            || variable.domain.len() > 64
            || variable.domain.iter().collect::<BTreeSet<_>>().len() != variable.domain.len()
        {
            bail!("Invalid finite-domain variable")
        }
        size = add_size(
            size,
            variable.id.len().saturating_add(variable.domain.len() * 8),
        )?;
    }
    for constraint in &criterion.constraints {
        let mut references = BTreeSet::new();
        match constraint {
            FiniteConstraint::Linear { terms, .. } => {
                if terms.is_empty() || terms.len() > 16 {
                    bail!("Invalid finite linear constraint bounds")
                }
                size = add_size(size, 8)?;
                for term in terms {
                    if !variables.contains(term.variable.as_str())
                        || !references.insert(term.variable.as_str())
                        || term.coefficient == 0
                        || !(-1_000_000..=1_000_000).contains(&term.coefficient)
                    {
                        bail!("Invalid finite linear term")
                    }
                    size = add_size(size, term.variable.len().saturating_add(8))?;
                }
            }
            FiniteConstraint::AllDifferent { variables: ids } => {
                if ids.len() < 2 || ids.len() > 32 {
                    bail!("Invalid all-different constraint bounds")
                }
                for id in ids {
                    if !variables.contains(id.as_str()) || !references.insert(id.as_str()) {
                        bail!("Invalid all-different variable")
                    }
                    size = add_size(size, id.len())?;
                }
            }
        }
    }
    Ok(size)
}

#[cfg(test)]
mod tests;
