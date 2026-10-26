# Cognitive network implementation

The target remains the seven-layer architecture in `ARCHITECTURE.md`. The network
is **Partial**. Existing models and deterministic Rust services implement roles;
these changes do not create newly trained Jev+ or Jev-R weights or establish speed.

## Explicit language-only model

A reserved `language` role profile is an operator routing declaration, not a new
inference actor. It selects one model for `interface` (structured interpretation)
and `synthesizer` (presentation). The default model must be a different reasoning
model. Other profiles cannot select the language model, and conflicting explicit
interface/synthesizer profiles are refused. Provider-reported model identities must
match the selected model when the boundary is enabled. Names and endpoint replies
do not attest immutable weights or detect two aliases serving identical weights.

Example configuration commands, using model keys chosen by the operator:

```sh
cogfabric configure-local --model YOUR_REASONING_MODEL --parallelism 2
cogfabric configure-local --role language --model YOUR_LANGUAGE_MODEL
```

Do not copy the placeholder names into a live provider configuration. No model is
downloaded, trained, loaded or promoted by these commands. Default configurations
without a language profile retain their prior shared-model behavior.

Direct role inference, token-target selection, native role targets and first-use
native selection preserve the boundary. Private one-instance native adapters are
constructed only after the outer immutable role map selects and validates their
model. Selecting a registered reasoning model preserves the explicit boundary
instead of silently clearing it. Token admission currently disallows role profiles;
combining it with this network configuration remains unsupported.

Typed worker bootstrap version 4 carries the explicit boundary in the existing
FlatBuffers role-profile vector. It requires the `language` profile and older
versions refuse that marker. No JSON internal transport or generated schema change
is introduced. Older workers reject version 4.

The conversation endpoint can use the distinct default reasoning backend, but its
full structured Engine delegation, language-only final rendering, independent
role-model bindings and durable client-tool continuation still need integration.
Do not describe this endpoint as the finished cognitive network.

## Bounded parallel work

`--parallelism 1..4` configures the existing `subtask_parallelism` setting. In
addition to Jev-R advisory graph waves, dependency-ready steps in one selected
structured expert workflow now run in bounded waves. The pinned manifest order is
validated before schedule publication. Each complete wave's call/token/input
limits are checked before reservation, and permission decisions and durable
reservations precede polling. Resource admission and the original wall deadline
remain active. Dropping the parent future drops locally owned pending futures;
it does not prove a remote model stopped. Unknown effects retain existing gates.

Parallel replies remain unverified advice, never permissions or independent
proof. Ranked experts are still attempted sequentially. Coordinating independent
experts with Jev-R roles, cross-task scheduling, model capacity-aware activation,
continuous service recovery and client streaming remain broader network work.
Concurrent calls on one backbone are not a measured speed improvement.

## Schema-based project memory

SQLite schema 11 adds immutable `project_memory_records`. Each canonical record
contains bounded facts, entity relations, typed constraints and evidence pointers.
Values are scalar strings (up to 256 bytes), numbers or booleans, not conversation
bodies or arbitrary nested documents. Every claim references declared evidence;
links and hashes are provenance declarations, not independent truth verification.

Records bind one exact immutable `project@version`, its checksum, an immutable
record UUID and a sequential revision. The table is outside document FTS and
cannot be updated/deleted by ordinary writes. Existing database limits still
apply; records have count/byte caps and persist through ordinary task retention.
Database backup/restore includes the table. Canonical JSON is the schema encoding:
it is structured text, not encryption, neural memory or protection from a hostile
workspace owner. Existing legacy prose memory paths have not yet been replaced.

Operator commands:

```sh
cogfabric init
cogfabric project-memory-publish memory-draft.json
cogfabric project-memory-show PROJECT_ID@VERSION
```

`init` performs the schema migration. A draft contains `project_ref`, `revision`,
`facts`, `relations`, `constraints`, and `evidence`; the CLI creates the record ID
and checksum. The referenced profile must already be published. The Rust API
exposes the same typed records and Store operations. Models cannot publish records
through these operator-only commands. Session opening now captures a bounded typed projection of the latest exact-profile
revision. Record, profile and projection hashes pin it across continuations;
checkpoint validation rejects inconsistent projections. Conflict resolution and
conversion of legacy prose remain to implement. Memory constraints are data, not task permission
or acceptance-rule overrides.

## Optional project LoRA

A project-specific LoRA is a future optional candidate, not a replacement for the
structured source-of-truth memory. Existing offline adaptation/provenance and
operator promotion gates remain authoritative. No adapter is trained or activated
by this change. With testing/evaluation disabled by the operator, no independent
quality/safety evidence is available to justify promotion. Dataset licensing,
train/held-out separation, candidate versioning, resource bounds and rollback must
be retained when that path is implemented.

## Evidence for this implementation run

Compilation, formatting and static review only. No tests, benchmarks, live model
requests, database migration execution or installation are performed for this
run. The migration and concurrent execution have not been runtime-verified.
The full network, continuous operation and performance objective remain open.

## Client routing and independent expert batches

With an explicit language profile, client conversations use the durable collective
path automatically. Interpretation and final presentation select that profile;
reasoning calls select their assigned role backend. Language calls consume the
same root call, token and wall budgets. Tool proposals return directly to the
client; presentation cannot inherit the solver's verification result. Custom
backends must support role selection before accepting this configuration.

`configure-local --expert-parallelism 2..4` opts into concurrent independent
one-call workflow experts; `1` restores sequential selection. Batches require
distinct explicit requested-capability bindings, pinned compatible manifests and
disjoint dependency closures. All calls are admitted and durably reserved against
the same task ledger before their futures run. Structured multi-step experts and
heuristic-only routes retain sequential expert selection; dependency-ready steps
inside a selected structured workflow retain their separate bounded wave support.

These changes do not implement a full client-to-Engine bridge or managed client
model activation. They do not create trained expert weights, prove inference
speedups or replace existing legacy prose memory. No live inference, tests,
benchmarks, migration execution or installation was performed for this slice.

## Client advisory concurrency

Research client sessions with `--parallelism 2..4` can admit planner and explorer
as one independent wave against the same initial advice. The complete wave plus
a minimum solver allocation and language presentation reserve must fit the root
budget. Both calls receive durable reservations before either starts; the solver
waits for both results. No sibling is detached, failures retain reservations, and
provider cancellation is not claimed on timeout. Other modes and insufficient
budgets keep sequential admission. Actual provider execution may still queue.
Compilation/static checks only; no runtime or performance evidence is claimed.

## Engine project-memory snapshot

Engine tasks with an explicit immutable project reference now capture the latest
structured project-memory revision before workers/inference. Engine and client
paths share the same bounded complete-claim projection builder; linked evidence,
partial-projection indication and record/profile/projection digests are retained.
The Engine records its selected projection and pin in the audit log and never
refreshes that revision during the task. Constraints remain untrusted data.

When the explicit language-only profile is configured, both Engine and new client
sessions omit legacy query/answer memory episodes from inference context. Existing
legacy data and audit traces are preserved; this is not encryption or removal of
all text from storage. Documents and current conversation remain language inputs.
Previously captured continuation contexts remain pinned rather than silently
rewritten. Optional LoRA training and managed client activation remain unfinished.
