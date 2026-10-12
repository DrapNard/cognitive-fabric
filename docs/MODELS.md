# Registered local inference configurations

Model registration stores bounded, immutable connection metadata in SQLite schema
5. It does not download, train, load or unload weights. The local inference server
continues to own physical model residency and scheduling.

Example operator-reviewed manifest:

```json
{
  "id": "local.small",
  "version": "1.0.0",
  "model": "your-server-model-id",
  "api_base": "http://127.0.0.1:1234/v1",
  "source": "operator-reviewed local installation",
  "license": "Apache-2.0",
  "context_tokens": 4096,
  "max_output_tokens": 1024,
  "weights_sha256": null
}
```

Use the actual model's license and source. `weights_sha256`, when supplied, is an
operator declaration only: an OpenAI-compatible response cannot prove loaded
weight identity. Context capacity is also declared, not tokenizer-verified.

```sh
cogfabric model-register manifest.json
cogfabric models
cogfabric model-use local.small 1.0.0
cogfabric model-disable local.small 1.0.0
cogfabric model-enable local.small 1.0.0
```

Registration rejects unknown fields, remote/credential-bearing endpoints, invalid
canonical versions, invalid hashes and incompatible output/context declarations.
At most 256 versions can be registered, with a 16 KB manifest input limit.
Canonical stored manifests have SHA-256 integrity checks. Duplicate versions
cannot be rewritten, including after disable. Registration and enable/disable
changes have transactional audit entries. Registry corruption fails reads closed.

Selection saves the existing inference configuration with an exact registration
reference, copies its model/endpoint, clears role overrides so one shared model
serves the logical roles, and narrows the task output budget to the declared output
ceiling. It does not expand existing resource limits. `configure-local` explicitly
returns to unregistered configuration. A larger-model comparison override clears
the small-model registration reference rather than attributing it to another model.

At task start, the runtime verifies the enabled registration against configured
endpoint/model/budget and records its manifest/hash in the task trace. A disable
prevents later tasks, while already pinned tasks finish with their original
configuration. Saving a different selection affects new CLI processes; restart a
running service to apply its new saved configuration. Selecting an older immutable
version restores its configuration, not neural weights. Configuration file writes
and registry lifecycle transactions are separate; task traces record what ran.

The fixed local adapter provides chat-completion inference for all seven logical
roles. It does not expose arbitrary capabilities or grant permissions from model
metadata. There are no dependency graphs between these connection records yet.
Provider-reported model IDs and optional usage remain distinct from registration
metadata. Live server validation, actual model residency changes, exact tokenizer
preflight, and weight attestation remain incomplete.

Registry reads validate that each embedded manifest identity equals its database
row identity, both during discovery/listing and exact selection. SQL excludes
manifest payloads above 16,000 bytes, identifiers/versions above 64 bytes and
checksums other than 64 bytes before copying those fields into Rust. Invalid
records fail the operation rather than being silently skipped. Hashes prove local
consistency only; an owner able to rewrite the database can rewrite its checksums.

## Native LM Studio residency inventory

`cogfabric --json residency-status --api-base http://127.0.0.1:1234/api/v1`
queries native v1 inventory and reports installed model keys and each loaded
instance ID/context capacity. Unlike OpenAI model advertisement, the native
response explicitly describes residency. Observations remain server-reported,
not weight attestation, framework ownership or successful generation evidence.
See the [native API specification](https://lmstudio.ai/docs/developer/rest/list).

Authentication is separately opt-in: `--authenticate` reads only
`COGFABRIC_RESIDENCY_API_KEY` from the invoking process environment. Set it locally,
never in chat or a manifest. The inference credential is never forwarded to the
native endpoint. No token or response body is saved; exact raw/JSON-decoded token
echoes are rejected. Transformed exfiltration and same-user process compromise
remain outside this protection.

Only literal-loopback HTTP (or pinned localhost), the explicit `/api/v1` base and
fixed GET `/models` are accepted. Proxies and redirects are disabled; requests
have a three-second timeout and 256 KiB response ceiling. Inventory is bounded
to 256 model keys, 32 instances per model, 1,024 total unique instance IDs and
validated context capacities. Duplicate identities and incomplete inventories
fail closed. Unknown provider metadata is ignored rather than becoming commands.
This operator-only read command performs no load, unload, download or mutation.
It provides instance identity needed by the subsequent lifecycle implementation;
ownership-aware loading/unloading, model switching and real authenticated host
validation remain incomplete.

## Explicit native load and unload

```sh
cogfabric residency-load --api-base http://127.0.0.1:1234/api/v1 \
  --model installed-model-key --context-length 2048 --human-approve
cogfabric residency-unload --api-base http://127.0.0.1:1234/api/v1 \
  --instance-id exact-loaded-instance --human-approve
```

Add `--authenticate` to use the dedicated residency credential. These commands
use the documented [load](https://lmstudio.ai/docs/developer/rest/load) and
[unload](https://lmstudio.ai/docs/developer/rest/unload) endpoints; they neither
download weights nor alter saved inference selection. Load requires an installed
key with no loaded instances and context between 1 and 32,768. Unload requires an
exact currently listed instance. There is no unload-all or implicit replacement.

An exclusive workspace execution lease prevents these operations while framework
tasks/workers hold their shared leases. It does not coordinate another workspace,
LM Studio's UI or other clients: the operator must ensure the instance is unused.
Inventory and load responses cannot establish exclusive ownership; every report
keeps `ownership_established: false`. No automatic cleanup/unload is authorized
merely because a load returned an instance ID.

A bounded operator task records exact intent before POST; audit failure prevents
the request. Native requests retain response/credential protections, with a
60-second mutation timeout and three-second inventory timeout. A fresh inventory
must confirm the response instance under the requested model/context, or confirm
absence after unload. Reports distinguish measured request elapsed time from
provider-reported load time. They do not prove generation or immutable weights.

Errors after sending a request conservatively record an interrupted operation
with unknown external outcome. No automatic retry, rollback or unload follows:
inspect inventory and the operation's task trace first. Process death or an audit
failure can leave a running/requested record; that is not proof the request failed.
Real authenticated host validation, exclusive cross-client ownership, automatic
residency scheduling and end-to-end switching remain incomplete.

Final residency audit and terminal task status now commit in one immediate SQLite
transaction. An audit insertion or status-update failure rolls both back; the
pre-effect requested record remains and does not establish success or failure of
the external operation. Missing or already-terminal anchors refuse finalization.
This prevents a partially committed confirmation from appearing as a completed
operation. It cannot undo a load/unload already performed by the model server.

### Trusted inference backend rollback

`fabric::Broker` retains up to 128 immutable backend versions. Host
`activate(id, version)` restores the original backend and its existing bounded
queue; `versions()` lists retained metadata without inference. Future task
snapshots use restored role routes, while in-flight tasks retain their exact
original routes. Activation rejects roles already served by another active module,
unknown versions and stopped brokers. Registration cannot redefine a version;
rollback remains available at the history ceiling. Shutdown clears retained
versions and revokes all task pins, including inactive versions.

This is a trusted embedding API, not a model/socket lifecycle command. Inactive
backend references and initialized queues remain retained until shutdown or explicit
retirement; 128 is a lifetime identity count ceiling, not a byte, GPU or process quota. A restored remote adapter can
still be disconnected or have expired grants; rollback does not renew grants,
probe health or load/unload weights. It changes neither task permissions nor
external model residency. Real model switching remains separately unverified.

Transport-bound inference adapters now expose a fixed `module_descriptor()`.
`Broker::try_with_inference` preserves and validates it; `register` rejects any
changed ID, version or role list before queue creation or inference. Ordinary
local backends still receive the default local descriptor. `with_inference`
remains a convenience for trusted valid initial adapters and panics on an invalid
fixed descriptor; embedding hosts handling fallible input should use the `try_`
constructor. Unix adapters expose their actual connected descriptor, so discovery
and task pins no longer advertise unrelated local roles. This checks trusted
metadata consistency, not executable identity or loaded weights; a malicious
in-process implementation can lie about its descriptor.

### Bounded interface output allocation

`configure-local --model MODEL --interface-tokens 384` selects the host's output
reservation for the structured language interface. `interface_tokens` defaults to
128 and accepts 1–4096. It does not raise total tokens, calls, prompt bytes or wall
limits. If its full reservation cannot fit, no interface inference is dispatched;
ordinary remaining-budget behavior still applies. Larger allocations can leave
less capacity for solving and do not guarantee valid JSON or better answers.

The host sends the admitted per-call token limit through existing typed inference
frames. Worker bootstrap's cumulative ceilings remain authoritative; workers do
not interpret human goals, so this host scheduling option needs no IPC schema
change. Saved nondefault settings are included in task configuration snapshots,
backups and restoration; default serialization omits the new field to preserve
legacy configuration identity. Old configurations deserialize to 128. Empty
provider text remains a visible failure, never an inferred answer or free retry.

### Opt-in provider reasoning control

`configure-local --model MODEL --reasoning-effort none` sends the optional
`reasoning_effort` field to the configured local compatible endpoint. Accepted
values are `none`, `low`, `medium`, `high`; CLI `default` removes the override.
Default configurations omit the field entirely and preserve legacy serialization.
The override applies to all roles and configured role models for that task.
Provider/model support varies: unsupported requests can fail or be interpreted by
the server, so the framework does not promise control over hidden reasoning.
No arbitrary extra HTTP options, new model privileges or enlarged budgets are added.

[Ollama's compatibility documentation](https://docs.ollama.com/api/openai-compatibility)
describes this field, with model-dependent behavior and `none` requesting disabled
thinking for supported boolean controls. This is a provider request, not proof of
actual internal compute or output quality. Empty text/timeouts remain failures.

Managed workers carry an explicit override through COGB bootstrap version 2; the
optional FlatBuffers field is appended compatibly. Version 1 remains emitted when
no override exists. Receivers reject version 1 with an override or version 2
without one, invalid names and oversized fields; older version-1 workers reject
version 2 rather than silently ignoring the requested setting. Cumulative worker
limits and authorization are unchanged. Task configuration snapshots and backups
retain nondefault overrides; changing host settings cannot change pinned tasks.

Structured interface parsing accepts either raw JSON or one exact outer
Markdown `json` fence, including LF/CRLF line endings. The existing 16,000-byte
limit and deny-unknown-fields schema still apply before routing. Prefix/suffix
prose, multiple objects, wrong fence languages, incomplete/nested fences and
permission fields are rejected. This handles an observed local-model formatting
wrapper without extracting JSON from arbitrary instructions or changing the
original authoritative goal. Accepted interpretation remains untrusted advice.

Planner and critic structured responses now use the same exact-fence JSON parser
as the language interface (`src/structured.rs`). Planner input remains bounded to
6,000 bytes and validated as an acyclic, bounded dependency graph; critic JSON is
bounded to 8,192 bytes with the existing strict boolean/reason schema and 1,000-byte
useful-reason limit. Interface input remains bounded to 16,000 bytes. Only raw JSON
or one exact outer `json` fence is accepted; unknown fields, surrounding prose,
multiple values and malformed wrappers fail. Plan/critic output stays advisory,
never permissions or independent success evidence, and existing budgets/stopping
rules remain authoritative. Recovery uses the same validated planner parser.

Trusted inference hosts can inspect `Broker::statuses()` for retained versions,
active selection and local adapter availability. Default backends report unknown;
Unix adapters distinguish a present stream, an exchange holding its lock and a
permanently discarded stream. Closed adapters are excluded from new routing and
cannot be reactivated; existing pinned dispatch fails before queueing. Registration
metadata remains discoverable for diagnosis. These observations neither probe
remote services nor attest loaded weights, grant renewal or future availability.
A replacement connection requires a new immutable registration; there is no
implicit reconnect, retry or budget refund.

### Explicit alternative inference routes

Trusted Rust hosts may use `Broker::register_candidate(descriptor, backend, rank)`
to approve alternative inference modules for the same role. Lower ranks are
preferred; equally ranked candidates prefer fewer occupied local queue slots,
then lexicographic module IDs break ties. Rank is bounded to
0..=1,000,000 and immutable per registered version. Overlap is permitted only
between ranked registrations. Ordinary `register` and the initial local module
remain exclusive: explicitly unregister that module before installing overlapping
ranked alternatives. Metadata cannot grant roles outside the existing allowlist
or relabel a transport-bound backend.

New task pins choose a route per role, skipping observed unavailable adapters.
Replacement, unregister and historical activation affect future pins. Existing
pins keep their backend, even if a new alternative ranks higher; a failed pinned
call is never secretly retried elsewhere. Future tasks may select another already
approved available candidate. `statuses()` exposes ranks and observed state.
Declared ranks do not establish estimated cost, calibrated escalation utility,
provider health or GPU residency. All original session permissions and ceilings
remain authoritative. No executable loader or automatic model load is added.

### Qwen3 empty-answer troubleshooting

If an endpoint returns empty answer text, internal reasoning may have consumed its
output allowance. This is a hypothesis until response metadata confirms it; do not
turn internal reasoning into a verified final answer or automatically retry with a
fresh task budget. For LM Studio's advertised `qwen/qwen3-8b`, an operator can try:

```sh
cogfabric configure-local --model qwen/qwen3-8b \
  --api-base http://127.0.0.1:1234/v1 --reasoning-effort none
cogfabric ask 'Explain Rust ownership briefly /no_think' --mode standard
```

This combination produced a real explanation locally on 2026-10-12. Prose remains
unverified. The provider may ignore either hint; see LIVE_INFERENCE.md for measured
scope and the successful task's trace identity.

The HTTP adapter now gives distinct, fixed diagnostics when an empty answer is
accompanied by provider `finish_reason=length` or nonempty `reasoning_content`.
It does not print or accept internal reasoning as answer text, increase budgets,
or retry automatically. Provider metadata remains untrusted; these diagnostics
report the response shape, not a verified explanation of model internals.


### Inactive inference backend retirement

Trusted Rust hosts may call `Broker::retire(id, version)` after deactivation or
replacement. Active versions, unknown versions and stopped brokers are rejected.
Retirement removes that version from inventory and future rollback. Existing task
pins keep their original backend and bounded queue; once the last pin and pending
work release it, the queue closes and its backend handle drops. The identity stays
reserved in immutable history, so retirement cannot redefine a version or bypass
the 128-identity ceiling. A new implementation requires a new version.

This API grants no model/socket capability, sends no provider lifecycle request
and does not attest GPU unloading or external process cleanup. Explicit backend
shutdown and owned-child cleanup remain separate responsibilities. No automatic
residency scheduling, durable tombstone storage or general plugin loader is implied.
The regression observes actual backend release through a weak reference after
successful execution through a retired pin.


Retirement and shutdown commit registry changes before releasing backend handles
outside the registry lock. A trusted backend destructor can inspect the committed
registry; arbitrary destructor blocking/panic behavior is not isolated or bounded.


Queue occupancy is sampled once per inference candidate when forming new pins.
Uninitialized queues count as empty. It measures buffered/reserved channel slots,
not an executing request, remote server load, GPU residency or expected latency.
Concurrent submissions can change occupancy immediately after selection; routing
does not reserve a slot or guarantee load balancing. Existing pins and failure
handling are unchanged. A lower-priority rank cannot win through lower queue load.
No probe, extra model call, grant, retry or provider load/unload is introduced.
