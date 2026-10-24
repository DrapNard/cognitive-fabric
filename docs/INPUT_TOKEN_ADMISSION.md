# Exact input-token admission

Architecture sections 4, 5, 7, 9 and 10 require enforced resource ceilings.
The optional llama.cpp adapter counts the exact token IDs submitted for generation,
including the rendered chat template, and reserves them cumulatively per task.
This setting is disabled by default and requires a separately running llama.cpp
server exposing `/apply-template`, `/tokenize` and `/completion`.

## Configuration

First configure `openai-local` with the server's loopback `/v1` base and exact
reported model alias. Remove role profiles and use default/none provider effort.
Then run:

```sh
cogfabric configure-input-tokens --max-input-tokens 32768 --max-context-tokens 8192
# Disable:
cogfabric configure-input-tokens --disable
```

Use `--home PATH` before the subcommand for a separate state directory. Restart
running services after changes. Existing tasks retain their admitted settings.
The context ceiling must not exceed the server's actual per-slot context.
`max-input-tokens` is the cumulative input allowance per task (1–16,777,216);
`max-context-tokens` is the input-plus-reserved-output ceiling per call (128–131,072).
Each input has at most 65,536 IDs, and each task at most 64 admission events.
Other call, output, byte, wall-time and queue limits still apply.

For an authenticated native server, add `--authenticate` and provide
`COGFABRIC_LLAMA_API_KEY` in the service environment. Managed children receive
that dedicated credential only when explicitly enabled. The OpenAI-compatible
`COGFABRIC_INFERENCE_API_KEY` is never reused for these native routes.

## Admission and accounting

The adapter renders the complete unchanged text message history, tokenizes that
rendered string with `add_special:false` and `parse_special:true`, and submits
exactly the returned token array. It adds no implicit BOS token after rendering.
The cumulative input charge commits in SQLite before completion dispatch. A
provider error, cancellation or uncertain outcome does not refund the charge.
Concurrent reservations serialize transactionally; malformed history, changed
settings, audit failure and exhaustion prevent generation. Stored admission events
contain counts and hashes rather than prompt text or token arrays.

The context calculation includes the unchanged output allocation. Provider-reported
usage remains separate from these admission counts. Responses must report the
configured model alias, no truncation and a supported stop reason. Alias checking
is not immutable-weight or tokenizer attestation. Two extra bounded HTTP requests
are needed for each generation; their time shares the original request deadline.

## Supported scope and limits

Supported paths are local role inference and tool-free chat, including managed
workers. Typed bootstrap version 3 preserves the settings; legacy versions reject
the new field. Default settings retain the existing OpenAI-compatible path.

Tools/tool histories, role profiles, non-none provider reasoning effort, opaque
adapters without the admission capability, and native LM Studio lifecycle inference
are refused when this option is enabled. Do not enable it for an OpenCode tool
session. Ordinary OpenCode effort selection remains available with this option off.
There is no universal exact preflight API for arbitrary OpenAI-compatible servers.

Loopback fixtures cover exact wire IDs, context refusal, cumulative no-refund
accounting, concurrent admission, audit-write failure, full chat history, early
unsupported-request rejection and managed-worker credential separation. Unit tests
cover settings, corrupted ledgers and typed bootstrap compatibility. Real llama.cpp
inference has not been exercised for this change. These checks establish neither
reasoning quality, physical GPU/energy bounds, provider stopping nor neural safety.

## Verification record — 2026-10-24

- `cargo fmt --all -- --check`: passed.
- `cargo +1.99.0 check --workspace --all-targets --all-features --locked`: passed.
- `cargo +1.99.0 clippy --workspace --all-targets --locked -- -D warnings`: passed.
- `cargo +1.99.0 clippy --workspace --all-targets --all-features --locked -- -D warnings`: passed.
- `cargo +1.99.0 test -p cogfabric --lib token_admission::tests --locked`: seven passed.
- `cargo +1.99.0 test -p cogfabric --lib fabric::unix::bootstrap::tests --locked`: five passed with socket permissions. One earlier sandboxed run could not create its socket; rerun passed.
- `cargo +1.99.0 test -p cogfabric --lib security::tests::input_token_configuration_is_human_only --locked`: one passed.
- `cargo +1.99.0 test -p cogfabric --test input_token_admission --locked`: five passed with loopback/process permissions.
- `bash scripts/update-protocol-bindings.sh /tmp/cogfabric-effort-flatc/build/flatc`: regenerated using flatc 25.12.19, matching Rust FlatBuffers 25.12.19.
- Temporary-state executable configuration enable/disable workflow: passed.

The full all-target test suite was not run for this milestone, respecting the
operator's instruction to stop reflection testing. This is a focused implementation
record, not completion of the full v1.0 release gate.
