# OpenAI-compatible agent API

The optional `cogfabric-server` executable serves local agent clients, including
OpenCode. It is a separate workspace crate built on the `cogfabric` library.
The core does not depend on Axum or the HTTP server. The existing CLI and typed
Unix socket service continue to work independently.

## Start the server

Install the optional server separately from the core CLI:

```bash
cargo install --locked --path . --root /tmp/cognitive-fabric-install
cargo install --locked --path crates/cogfabric-server --root /tmp/cognitive-fabric-install
bash scripts/installed-http-smoke.sh /tmp/cognitive-fabric-install/bin/cogfabric /tmp/cognitive-fabric-install/bin/cogfabric-server
```

Use a fresh install directory and the verified bundled bindings described in README.
No `flatc` is needed; unset stale `FLATC` exports for ordinary builds. The
HTTP smoke requires Bash, jq, curl, OpenSSL, rg and GNU timeout. It uses private
temporary state, a random test key and an unavailable upstream; it verifies
discovery/authentication, origin denial, actual arithmetic/SQL worker reaping
and successful shutdown without requiring a model. Evidence paths are printed.
This does not test native model inference or coding quality.

Build with the verified FlatBuffers bindings (optional compiler regeneration is
documented in PROTOCOL.md):

```bash
cargo build --locked --workspace --bins
# Use separate state for the conversation adapter.
cargo run -p cogfabric -- --home "$HOME/.local/share/cogfabric-http" configure-local \
  --api-base http://127.0.0.1:1234/v1 --model qwen/qwen3-8b --reasoning-effort none
export COGFABRIC_SERVER_API_KEY="$(openssl rand -hex 32)"
cargo run -p cogfabric-server -- --home "$HOME/.local/share/cogfabric-http" \
  --bind 127.0.0.1:8234
```

Use the same dedicated server key in the client's environment. It is different
from `COGFABRIC_INFERENCE_API_KEY`, which authenticates the upstream model only
when the existing explicit inference scope is configured. Neither key belongs in
Git or an OpenCode config file. All routes require server bearer authentication.
Ctrl-C stops admission, cancels pending turns and drains native ownership for
up to ten seconds. Cleanup failure disables further native worker admission and
makes shutdown exit with an error; inspect its audit before restarting.

Native conversations support direct inference and opt-in managed inference.
Enable the latter before starting the server:

```bash
cargo run -p cogfabric -- --home "$HOME/.local/share/cogfabric-http" configure-worker --enabled true
```

Managed mode requires the persistent database in that existing private workspace.
Each turn launches the same running Rust image lazily, sends typed trusted settings,
and exchanges native conversation FlatBuffers over an owner-local Unix socket.
The host and worker authenticate each other's UID/PID; a one-task, one-call grant
limits input, reserved output, proposals and the original deadline. The parent
retains the child, socket, execution lease and process-capacity slot through
reaping, including cancellation. Managed requests must fit the 256 KiB native FlatBuffers frame; requests that
exceed it are rejected before creating a task or launching a child.
The per-host native child ceiling is eight,
including children awaiting cleanup. The process is not an untrusted-code sandbox.

Enabled immutable model registrations are supported through `model-select`;
the model, endpoint and output ceiling must match the selected version. Each turn
snapshots that registration before task creation. Disabling it after admission
leaves the admitted turn pinned; later admissions fail. Role overrides remain
unsupported and fail at startup. Managed arithmetic/SQL on collective aliases
continue to use their existing typed worker implementations. Library embeddings
that opt into native workers must delegate the hidden `agent-worker` command to
`cogfabric::agent::process::run_worker`; both supplied binaries already do so.

## Models and requests

`GET /v1/models` lists five stable API aliases. They are runtime profiles, not
additional resident neural models:

| Alias | Behavior |
|---|---|
| `cogfabric-agent` | One bounded native conversation/solver turn; text and function proposals. The client owns its agent loop and tool execution. |
| `cogfabric-fast` | Existing core Engine, FAST mode. |
| `cogfabric-standard` | Existing core Engine, STANDARD mode. |
| `cogfabric-deep` | Existing core Engine, DEEP mode. |
| `cogfabric-research` | Existing core Engine, RESEARCH mode. |

`POST /v1/chat/completions` accepts one completion (`n:1`), text messages and
optional function definitions. Plain string content and arrays of text parts are
supported. Native messages preserve system/developer/user/assistant/tool roles;
function IDs and JSON-object arguments are validated, and tool results must match
outstanding assistant calls. Duplicate IDs, orphan results, unresolved calls and
undeclared output functions and reused historical output-call IDs fail closed. Tool arguments still require the
client's own schema validation and permission checks before execution.

Supported options: `max_tokens` **or** `max_completion_tokens`, `temperature`,
`tools`, `tool_choice` (`auto`, `none`, `required`, or a named function),
`parallel_tool_calls`, `stream`, `stream_options.include_usage`, and `store:false`.
Unknown options and `store:true` are rejected. There is no server-side conversation
storage/replay API; each request supplies its explicit history. Authentication
and client-supplied system text never grant core file, shell or network tools.

Collective aliases accept exactly one user text goal (up to 12,000 bytes), without
client function definitions, generation overrides or conversational history.
They use operator-configured Engine budgets and existing independent checkers.
Use `cogfabric-agent` for an external application's multi-turn tool workflow.
The native agent path does not silently run the full collective or claim its
answers are independently verified.

Responses use ordinary Chat Completions envelopes, native `tool_calls`, finish
reasons, and reported token usage. Missing usage remains null rather than an
invented count. `chatcmpl-TASK_UUID` links to the durable task. Inspect it with:

```bash
cargo run -p cogfabric -- --home "$HOME/.local/share/cogfabric-http" --json task TASK_UUID
```

With `stream:true`, the server sends SSE completion chunks, indexed tool calls,
an optional final usage chunk, and `[DONE]`. Upstream inference currently uses a
**buffered** native completion; SSE provides client framing and keepalives, not
live upstream token delivery. Stream errors contain a sanitized error object.
Dropping either a streaming response or a pending non-streaming request cancels
local inference; an external model server may continue work after disconnect.
Responses API, embeddings, images, audio, remote HTTP access and multi-tenant
isolation are not implemented. Compatibility is tested with OpenCode v2.0.26,
not asserted for every OpenAI client.

## OpenCode v2 configuration

Add this to the project `opencode.json` (v1 uses a different configuration shape):

```json
{
  "model": "cogfabric/cogfabric-agent",
  "providers": {
    "cogfabric": {
      "name": "Cognitive Fabric",
      "env": ["COGFABRIC_SERVER_API_KEY"],
      "package": "@opencode/ai/providers/openai-compatible",
      "settings": { "baseURL": "http://127.0.0.1:8234/v1" },
      "models": {
        "cogfabric-agent": {
          "capabilities": { "tools": true, "input": ["text"], "output": ["text"] },
          "limit": { "context": 32768, "output": 1024 }
        }
      }
    }
  },
  "permissions": [
    { "action": "shell", "resource": "*", "effect": "ask" },
    { "action": "edit", "resource": "*", "effect": "ask" }
  ]
}
```

Then run `opencode run --standalone --model cogfabric/cogfabric-agent 'Your task'`
from that project, with the same server key exported. The declared context limit
must match the actual model configuration. The output limit must fit the saved
Cognitive Fabric `max_tokens` ceiling (default 1600). Changing a client's declared
limits never raises core limits. Review the client's permissions before allowing
file edits or commands; those operations run with the client's authority.

Configuration references: [OpenCode v2 providers](https://opencode.ai/v2/docs/providers),
[model capabilities](https://opencode.ai/v2/docs/models), and
[permissions](https://opencode.ai/v2/docs/permissions).

## Limits and audit semantics

The official server binds numeric loopback only and rejects browser origins.
Accepted connections are capped at 32, with a 180-second connection lifetime;
up to 16 authenticated requests may read bodies or await headers concurrently,
and eight completion producers may run. Saturated request/turn admission returns
429. HTTP bodies are limited to 512 KiB, parsing/header production to 150 seconds,
and stalled SSE production to 30 seconds. Axum owns HTTP parsing; core APIs receive
typed Rust values. Internal IPC remains versioned FlatBuffers, not HTTP JSON.

Native turns are FAST, one solver call and one cycle. Input bytes, reserved output,
wall time and tool-proposal counts obey existing operator configuration. Native
requests allow at most 256 messages and 64 definitions; returned proposals are
capped at 16 and `max_tool_calls`, with one when parallel calls are disabled.
Function arguments are limited to 32,000 bytes and must decode to a JSON object.
Provider HTTP responses are limited to 256 KiB, without redirects, proxies or
non-loopback endpoints. A longer core deadline does not extend the HTTP connection
lifetime. Each client turn is a separate task; client session-wide step limits
remain the client's responsibility.

Native tasks use the existing global SQLite task admission and runtime leases.
`agent_admission`, policy and `agent_result` events record request/content hashes,
reserved resources, actual reported usage, model identity and registration
snapshot (with weight identity explicitly unverified), elapsed time and
proposal counts. Raw client conversations, function arguments and output bodies
are not persisted by this path. Tool proposals and client-reported results remain
**unverified**; no tools are executed by the conversation runtime. Failed,
cancelled, exhausted or dropped turns retain admission records. These native
turns do not implement the collective reasoning checkpoint format and cannot be
resumed through `resume-task`.

## Reproduce the live client test

With LM Studio exposing the tool-capable Qwen model, OpenCode v2, jq, rg, OpenSSL, sqlite3 (for managed-worker audit checks),
GNU timeout and the built server available:

```bash
COGFABRIC_MANAGED_INFERENCE=true COGFABRIC_MODEL=qwen/qwen3-8b \
  COGFABRIC_API_BASE=http://127.0.0.1:1234/v1 bash scripts/opencode-smoke.sh "$PWD/target/debug/cogfabric-server"
```

The script creates isolated state, a fresh random API key and a disposable file.
OpenCode is denied all actions except reading/editing that exact file. It must
perform read → edit/write → read, report the observed new status, and produce the
expected actual file contents. In managed mode it also checks that every admitted
turn launched a native worker and that every such child has a reaping audit.
The script stops only its own server and retains
logs/state at the printed path. Omit `COGFABRIC_MANAGED_INFERENCE=true` to test
direct inference instead. This is an optional real-backend test, not an
unattended CI dependency or a general coding-quality benchmark.

Shutdown also drains collective inference and tool process ownership after
request handlers finish, including reapers triggered by client disconnects.
The collective drain has its own ten-second limit and sticky failure handling.
Both native and collective cleanup are awaited before returning shutdown status.
A cleanup/audit error blocks new workers of its affected family and makes server
shutdown fail; forced host termination remains outside these guarantees.


### Unambiguous JSON at agent boundaries

Agent HTTP request bodies, native provider response objects, decoded native tool
schemas and tool-argument objects now reject duplicate decoded JSON keys at every
nesting level. This includes escaped spellings of the same name and duplicate
objects inside arrays. Byte caps remain 512 KiB for HTTP request bodies, 256 KiB
for provider responses and 32,000 bytes for schemas/arguments; ordinary JSON
recursion limits and trailing-data rejection remain enforced. The reusable Rust
`strict_json::value` helper preserves accepted JSON values without echoing the
rejected key. Raw valid tool argument strings are not rewritten.

Duplicate HTTP bodies fail before native inference. Ambiguous provider objects
cannot supply a completion; ambiguous tool proposals and correlated history are
also rejected. This eliminates a class of parser disagreement, not all prompt
injection or schema violations. Tool JSON Schema semantics and action permissions
remain the client's responsibility; Fabric proposes tools and executes none through
this API. No remote reference resolution, host capability, new tool or model
permission is introduced. Internal FlatBuffers framing/schema is unchanged.


### Exact native tool-choice and text-part controls

Named `tool_choice` objects accept exactly
`{"type":"function","function":{"name":"declared_tool"}}`. Unknown outer or
nested fields, non-object function controls and undeclared names are rejected.
String choices `auto`, `none` and `required` retain existing semantics. This avoids
forwarding extra control fields in direct mode that native managed transport would
otherwise drop. Strict-schema declaration belongs to the tool definition, not the
named choice; the client still validates schemas and approves actions.

HTTP text parts accept exactly `type: text` and a string `text`. Extra image or
other fields are explicitly rejected rather than silently removed while flattening
text. These unsupported shapes return 400 before inference. Valid named choices
and ordinary text arrays remain supported. No tool execution, permission, budget,
conversation role or FlatBuffers schema is added.
