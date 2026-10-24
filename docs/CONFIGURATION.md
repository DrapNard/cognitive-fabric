# Local inference configuration

Initialize a state directory with `cogfabric --home PATH init`.
Save the default local profile with:

```sh
cogfabric --home PATH configure-local --model YOUR_LOADED_MODEL \
  --api-base http://127.0.0.1:1234/v1 --temperature 0.15
cogfabric --home PATH config-show
```

`configure-local --role critic --model YOUR_LOADED_MODEL --temperature 0.0`
sets an independent role profile. Planner, solver, critic, explorer, reflector,
interface and synthesizer profiles are accepted. Roles share the default model
unless explicitly overridden; selecting a model does not load its weights.
The operator must run a compatible loopback inference server independently.
The interface/synthesizer profiles are used by the language-interface
integration; see LANGUAGE_INTERFACE.md for budgets and invocation rules.

Settings live in owner-only `config.json`, with format_version 1. Saves use an
exclusive temporary file, sync file contents, atomically rename, and sync the
state directory. Invalid profiles, unknown fields, remote endpoints, credentialed
URLs, unsupported versions and excessive budgets fail closed. Config files and
reviewed inputs use bounded reads on opened regular files without following a
final symlink; nonblocking open prevents named-pipe hangs.

Priority is explicit CLI backend override, environment variables, saved settings,
then defaults. Environment names remain COGFABRIC_BACKEND, COGFABRIC_MODEL and
COGFABRIC_API_BASE. Resource ceilings may be edited in the reviewed config file:
max_calls, max_cycles, max_tokens (reserved output tokens), max_input_bytes,
max_tool_calls, timeout_seconds and max_wall_seconds. Zero call/cycle/token/byte/tool
budgets intentionally prevent the corresponding work. A zero `max_tool_calls`
skips optional reference/episodic retrieval instead of rejecting plain inference.
An explanation can still return unverified output within its model-call, token,
input and wall-time ceilings. Explicit arithmetic/SQL dispatch and structured
verification remain denied; no check is silently treated as passed. The skip is
audited. This tool ceiling does not disable runtime persistence, registry metadata
or explicitly requested conversation references, and natural-language prohibitions
remain advisory rather than a replacement for configured limits. Timeouts must be positive.
Explicit arithmetic and inline `sql:` tasks skip unrelated reference/episodic
retrieval in tool-enabled profiles. They require one tool allowance for dispatch
and its existing host-side result check, and need no inference call. A zero tool
allowance still denies execution. Structured sort/order memory reuse retains its
separate retrieval and verifier charges. Mode limits still clamp larger
configuration values. See REASONING.md.

Restart the service after changing settings. Existing tasks retain a cloned
configuration and backend handle. Runtime model labels are not cryptographic
attestations of external server weights. Configuration does not prove GPU model
loading, reasoning quality, adapter switching or a secure training pipeline.

## Managed worker kernel limits

`configure-worker-limits --address-space-bytes BYTES --cpu-seconds SECONDS` saves
optional Linux per-process bounds; `--clear` removes them. These do not limit the
separate model server or GPU. See [scope, recovery and validation](WORKER_LIMITS.md).
