# Optional bounded Wasm experts

Architecture coverage: Expert Network (§6), typed communication (§7), resource
limits (§9), and independent permissions/isolation (§10). This is an optional
execution adapter, not a newly trained Jev model or a universal native sandbox.

Build with `cargo build --locked --features wasm-experts`. For `ask` or `serve`,
repeat `--wasm-expert /absolute/path/expert.json` at most eight times. The manifest
is strict JSON:

```json
{
  "format_version": 1,
  "id": "example.math",
  "version": "1.0.0",
  "module": "expert.wasm",
  "sha256": "REPLACE_WITH_EXACT_LOWERCASE_SHA256",
  "capabilities": ["math.evaluate"]
}
```

The module must be a regular sibling `.wasm` file, opened without following its
final symlink. Bytes are read once and hash checked before compilation. The
manifest is operator-selected configuration; hashes establish integrity relative
to that selection, not publisher authenticity. Unknown fields, duplicate or
unknown capabilities, and unsafe artifact names fail. Other supported capability
names are `sql.query`, `sorting.verify`, `ordering.verify`, `path.verify`, and
`constraints.solve`. Declaration grants no invocation permission.

## ABI version 1

The guest exports exactly `memory` and
`execute(request_ptr: i32, request_len: i32) -> i64`, with no imports or start
function. Each call gets a fresh instance. The host writes a FlatBuffers
`ExpertRequestPayload` at address zero with identifier `CGWI`. The guest returns
an `ExpertResultPayload` with identifier `CGWO`; the upper 32 return bits hold
the unsigned output offset and the lower 32 bits its length. Both payloads have
version 1 and a matching capability, using `protocol/general_module.fbs` unions.
The request is read-only input by convention; output must fit guest memory.

These payloads contain no task authorization, grants or host resource handles.
The existing `CGMD` process packet remains unchanged. Result framing, verification
complexity, SQL/vector/string sizes and total output are checked before native
object unpacking; the response must also match the original request. Independent
host criteria decide answer acceptance. A guest's success claim is not proof.

## Enforced limits and authority

- Artifact: 1,000,000 bytes; each message: 65,536 bytes.
- Linear memory: 1 MiB; table elements: 256; one instance/memory/table.
- Execution: 2,000,000 fuel, stack height 65,536, recursion depth 64.
- Two shared execution permits; waiting observes the original task deadline.
- No WASI, imports, host callbacks, filesystem, network, credentials or model API.
- Every dispatch requires the existing host-minted task authorization context.

A blocking interpreter job retains its permit after caller timeout/cancellation
until it exits; fuel bounds interpreter work. This is not hard wall-clock
preemption or process-level protection against an interpreter implementation bug.
Compilation has artifact/structural bounds but no hard compilation-time deadline.

## Version pinning and recovery

Registry pins retain compiled bytes even if the source pathname changes. Version-4
task snapshots include module/settings/host-source hashes and the payload schema hash through
an implementation identity, plus exact broker queue/concurrency limits. Retained
continuation compares those fields. Legacy version-1/2/3 snapshots retain their
existing recovery rules and cannot carry later-version fields. Builtin
reconstruction never substitutes a builtin worker for a guest identity.

For an unfinished nonnative task with a version-4 snapshot, explicitly supply the
original manifests after restart:

```bash
cogfabric --home /absolute/state resume-task TASK_UUID --human-approve \
  --restore-wasm-expert /absolute/path/expert.json
```

Repeat at most eight manifests. The host compiles only these operator-supplied
paths, then requires an exact module set, hashes, capabilities and original limits.
An exclusive execution lease and bounded transactional history inspection precede
the reconstruction audit. Audit failure rolls back the event; no invocation or
permission is created by reconstruction. Ordinary continuation independently
checks the original checkpoint, budget, absolute deadline and uncertain calls.
Persisted metadata never supplies a path or new executable. Native-owned tasks,
mixed builtin/guest registries and old snapshots without recorded limits are not
reconstructed by this option. Hashes do not authenticate a hostile database owner.

## Verification

`cargo test --locked --features wasm-experts --test wasm_experts` covers actual
CLI and engine dispatch, unchanged host arithmetic verification, artifact pins,
mutation after compilation, strict manifests, typed size limits, imports, start
functions, memory growth, fuel exhaustion and invalid output ranges/frames.
The default build tests refusal before state initialization. Recovery unit tests
reject guest metadata on builtin reconstruction, including legacy snapshots.
Full-workspace checks must also pass before publishing a completed change.
Run default and feature-enabled suites serially when they share a Cargo target
directory: CLI integration tests launch that directory's executable, which a
concurrent build with different features can replace.
