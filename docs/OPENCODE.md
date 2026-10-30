# OpenCode local setup

`scripts/opencode-local.sh` starts the Cognitive Fabric HTTP API on
`127.0.0.1:8234`, launches OpenCode in the current project, and shuts down only
the server process it started. If an authenticated Cognitive Fabric server is
already listening, it reuses that process and leaves it running after OpenCode
exits. The helper requires a running inference provider and can load the
configured installed LM Studio model automatically.

For a separately configured language/reasoning network, the experimental
on-demand native mode can activate an approved model only when an admitted
client role needs it. Supply an already reviewed lifecycle policy and explicitly
approve both activation and model-key evidence:

You can first print a draft for the saved network configuration. Choose the
context length to match your provider and available memory; add each optional
expert model with a separate `--extra-model MODEL_KEY` argument:

```sh
cogfabric --home "$HOME/.local/share/cogfabric-http" --json native-policy-draft \
  --api-base http://127.0.0.1:1234/api/v1 --context-length 32768
```

Review the output, save it as a private policy file, and use that file below.
Drafting does not check installed models, approve the policy or load anything.
Optional repeated `--model-context MODEL_KEY=NUMBER` and
`--model-kv-gpu MODEL_KEY=true|false` arguments override individual models.
Use exact configured or `--extra-model` keys; ensure each context is large
enough for the requests sent to that route.

```sh
export COGFABRIC_NATIVE_POLICY=/absolute/path/to/reviewed-policy.json
export COGFABRIC_NATIVE_ACTIVATION_APPROVED=1
export COGFABRIC_NATIVE_MODEL_KEY_EVIDENCE_APPROVED=1
cogfabric-opencode
```

The helper passes these approvals to a new local server, skips eager model loads
and does not require unloaded models to appear in the OpenAI model list. It
refuses to reuse an existing server because it cannot verify that server's pinned
policy. The policy and task gates still determine which installed models may be
loaded; no model is downloaded or automatically retried after an uncertain
outcome. Model-key/inventory evidence is weaker than response-instance
attestation. Start the native provider yourself. This mode is not a readiness
or performance guarantee. The same server flags also support other compatible
clients; see [on-demand client activation](COGNITIVE_NETWORK.md#on-demand-client-model-activation-experimental).

Inspect configured role routes without loading models:

```sh
cogfabric --home "$HOME/.local/share/cogfabric-http" --json diagnostics
```

The `network_configuration` section describes saved configuration, not a running
server's policy, loaded models or proof that different model names mean different
weights.

The launcher always adds `--standalone` to OpenCode. Its private session inherits
the current ephemeral `COGFABRIC_SERVER_API_KEY`; an already-running background
OpenCode service can retain an older environment and fail with `A valid server
bearer token is required`. This does not disable authentication or persist the
key. `--server` is refused because a separate OpenCode server cannot inherit the
launcher's environment. Close an older launcher session before starting a new one;
start CGF-backed conversations with `cogfabric-opencode`, not a separately launched
OpenCode process.

## MCP, skills and web connections

OpenCode owns MCP connections, skill discovery, web access, credentials and tool
permissions. Use `cogfabric/cogfabric-agent` as its model: CGF forwards the current
function names, descriptions and JSON Schemas to the configured inference backend.
The backend proposes calls; OpenCode authorizes and executes them, then sends their
results back in the conversation. A REA MCP connection belongs in OpenCode's
configuration, alongside any other MCP servers; CGF needs no separate REA connection.

The same path carries client-loaded skill text, web results and tool errors. These
remain untrusted conversation data, never kernel permissions or independently
verified evidence. CGF preserves correlated historical results even when a tool is
removed from the current catalog, but new proposals must use the current catalog.
Reasoning effort follows the configured reasoning routes; an explicit
language-only model remains assigned to interpretation and presentation.

Each turn accepts up to 128 function definitions, subject to the existing request,
per-schema and typed IPC frame limits. A large catalog can exhaust those limits
before reaching 128 tools; enable only relevant client tools and keep schemas and
results compact. Output proposals still obey the configured tool-call limit.
The audit stores catalog names and description/schema hashes, not raw skill text,
schemas or result bodies. Names can themselves be sensitive; avoid secrets in names.

This is bounded text/function Chat Completions compatibility, not support for every
client API or multimodal MCP result. The default agent endpoint can run a single
model turn; an explicitly configured client collective adds bounded Jev+ and Jev-R
orchestration. General heterogeneous reasoning and recovery remain incomplete.
The selected model still needs reliable tool-calling ability for client tools.

An opt-in installed OpenCode test exercises project skill loading and synthetic
REA/web MCP tools through an isolated loopback fixture:

```sh
cargo test -p cogfabric-server --test opencode_mcp_client -- --ignored
```

The installed v2.0.32 fixture uses Code Mode: OpenCode exposes `skill` and
`execute`, discovers MCP functions with `search`, and executes the discovered
functions inside the client. Direct MCP function exposure was not verified.
It proves protocol interoperability with its deterministic backend, not real REA
analysis, live web access or reasoning quality. MCP and permission configuration
is client-version specific; see the [OpenCode v2 MCP documentation](https://dev.opencode.ai/v2/docs/mcp-servers/).

## One-time setup

Install `cogfabric`, `cogfabric-server`, and OpenCode v2. The helper needs Bash,
`jq`, `curl`, GNU `timeout`, and OpenSSL. Make the launcher available on your
`PATH`, or call it by its path. For example, after installing the project under
`$HOME/.local`:

```sh
export PATH="$HOME/.local/bin:$PATH"
```

Configure a private Cognitive Fabric state directory and select the model that
is already downloaded in your local inference server. This example uses LM Studio
at its standard local API address; model selection does not install or load
weights:

```sh
cogfabric --home "$HOME/.local/share/cogfabric-http" init
cogfabric --home "$HOME/.local/share/cogfabric-http" configure-local \
  --api-base http://127.0.0.1:1234/v1 --model qwen/qwen3-8b
chmod 700 "$HOME/.local/share/cogfabric-http"
chmod 600 "$HOME/.local/share/cogfabric-http/config.json"
```

Start the inference provider yourself and bind it to loopback. For LM Studio,
start its local server on `127.0.0.1:1234`. Running `cogfabric-opencode` authorizes
loading the exact configured installed LLM through Core's audited
`residency-managed-load` command when no matching instance is loaded. The native
API request is `POST /api/v1/models/load` with the configured model key and
`context_length: 32768` and `offload_kv_cache_to_gpu: false`, with configuration
confirmation enabled. The context cache defaults to CPU RAM to reduce VRAM demand;
model-layer placement remains controlled by LM Studio. Set
`COGFABRIC_OPENCODE_KV_GPU=true` to request GPU caching on a subsequent new load.
Only literal `true` and `false` are accepted. Existing compatible instances are reused. Other instances
are never unloaded; a matching instance with insufficient context requires an
explicit manual reload.

Set `COGFABRIC_OPENCODE_AUTO_LOAD=0` to disable loading (default: `1`). A load is
attempted once, bounded by Core's 60-second HTTP deadline and a 90-second launcher
timeout. Failed/uncertain operations are never retried automatically; inspect
`residency-managed-status` and resolve unknown history explicitly before retrying.
A confirmed load is recorded in the existing workspace database and remains loaded
after OpenCode exits. The launcher never downloads weights or starts LM Studio.
Generic OpenAI-compatible providers without the native API still require manual
loading. Discovery and context checks run before OpenCode starts.

## Reasoning effort and slow local inference

The OpenAI-compatible endpoint requires standard effort values. In this LM Studio
installation, `reasoning_effort: "none"` disables Qwen thinking and produces a
visible answer. Its internal model log describes thinking as `on`/`off`, but those
are **not** valid `reasoning_effort` values on `/v1/chat/completions`.

```sh
cogfabric --home "$HOME/.local/share/cogfabric-http" configure-local \
  --model qwen/qwen3-8b --reasoning-effort none --timeout-seconds 120
```

Client effort overrides the configured default. Select the `#none` variant in
OpenCode for a first response without internal thinking. The other effort choices
remain standard wire values; a binary-thinking model cannot provide four distinct
internal thinking depths. CGF's optional collective mode separately applies its
bounded orchestration modes. No provider reasoning text is treated as an answer.

`--timeout-seconds` accepts 1–300 seconds. The original task wall deadline and
HTTP service limits still apply; increasing this value never raises other budgets.
CPU context caching can be slower, and a short output allowance can be exhausted
by thinking before a visible answer appears. Empty reasoning-only output is not
accepted as success; failed calls are never silently retried by CGF. Restart the
CGF launcher after changing settings and begin a fresh conversation; existing
durable collective sessions retain their original configuration pins.

The tested provider accepts string `tool_choice` modes `none`, `auto`, and
`required`, but rejects the OpenAI named-function object form. CGF preserves a
client's requested selection rather than silently weakening it. The live tool
proposal check uses one declared function with `required`; it does not execute
that function or prove arbitrary client workflows.

## Project OpenCode configuration

The current project directory must contain an OpenCode v2 `opencode.json` with
the provider configuration from [`opencode.example.json`](../opencode.example.json).
Copy that example into a project that does not already have a config; merge its
provider fields into an existing config so unrelated settings and permissions
stay intact. The launcher checks the provider and never creates, rewrites, or
replaces project configuration.

```json
{
  "$schema": "https://opencode.ai/config.json",
  "model": "cogfabric/cogfabric-agent",
  "providers": {
    "cogfabric": {
      "name": "Cognitive Fabric",
      "env": ["COGFABRIC_SERVER_API_KEY"],
      "package": "@opencode/ai/providers/openai-compatible",
      "settings": {
        "baseURL": "http://127.0.0.1:8234/v1"
      },
      "models": {
        "cogfabric-agent": {
          "capabilities": {"tools": true, "input": ["text"], "output": ["text"]},
          "variants": [
            {"id": "none", "settings": {"reasoningEffort": "none"}},
            {"id": "low", "settings": {"reasoningEffort": "low"}},
            {"id": "medium", "settings": {"reasoningEffort": "medium"}},
            {"id": "high", "settings": {"reasoningEffort": "high"}}
          ],
          "limit": {"context": 32768, "output": 1024}
        }
      }
    }
  }
}
```

Set OpenCode's shell and edit permissions to ask before allowing actions. Review
those permissions for each project: file edits and commands run with the local
user's authority. The `cogfabric-agent` endpoint proposes tool calls to OpenCode;
the Cognitive Fabric HTTP conversation path does not itself execute them.

### Choose reasoning effort

All four effort choices use the same `cogfabric/cogfabric-agent` model. In the
OpenCode TUI, press **Ctrl+T** to cycle the model's variants. Select `none`,
`low`, `medium`, or `high`; the selected effort is sent with each model request
as `reasoning_effort`. You can select a variant directly for one run too:

```sh
opencode run --model cogfabric/cogfabric-agent#high "Review this project"
```

For a normal TUI session, choose the model once with `/models`, then press
Ctrl+T to change its effort without switching model aliases. These choices
control the backend's configured reasoning effort; they do not select Jev+
workflow aliases such as `cogfabric-deep` or `cogfabric-research`.

## Start OpenCode

From a project directory containing the provider configuration, run the helper
from a Cognitive Fabric checkout:

```sh
bash /path/to/cognitive-fabric/scripts/opencode-local.sh
```

After installing with `scripts/setup.sh`, the installed command is
`cogfabric-opencode` (ensure the install prefix's `bin` directory is on `PATH`).

Pass ordinary OpenCode arguments through the helper as needed, for example
`bash /path/to/cognitive-fabric/scripts/opencode-local.sh run "Summarize this project"`.

The helper checks the selected private workspace has `backend: "openai-local"`,
`max_tokens` of at least 1024, and `max_tool_calls` of at least one. The OpenCode
provider declares a 1024-token output allowance, so it cannot exceed the saved
server ceiling. Its declared context size is 32768; ensure the loaded model and
provider configuration actually support that context. When the configured
loopback endpoint exposes LM Studio's native model inventory, the helper checks
the context length of the configured loaded model and stops if any matching
instance is below 32768. If the provider does not expose that native endpoint,
the helper warns that the actual context is unknown and continues. The server
itself still enforces its configured request, tool, wall-time, and connection
limits.

For an authenticated LM Studio native inventory, set its separate
`COGFABRIC_RESIDENCY_API_KEY`; the helper uses it for native inventory and approved model loading only.
The upstream inference key and the Cognitive Fabric HTTP server key are not
forwarded to this endpoint.

The server API key is required from the environment when reusing an existing
server. When the port is unused, the helper creates a random temporary key for
the server it starts and passes it to OpenCode. It does not retain or print the
key. The local HTTP probe uses a mode-600 temporary curl header file, removed
after discovery and again on exit. A supplied key must be at least 32 characters. Do not put the key in
`opencode.json`, shell history, or a checked-in file.

The helper stores its server output in a mode-600 file under a mode-700
temporary directory and prints its path on exit for diagnosis. The log is kept
after exit; remove it when no longer needed. The temporary curl-auth file is
removed after the API check. The helper only signals and reaps the server PID it
started; it does not shut down a pre-existing service.

## Troubleshooting

- **A valid server bearer token is required:** close the old CGF launcher session,
  then rerun `cogfabric-opencode`. Updated launchers create a private OpenCode
  session with the matching key. Running `opencode` directly can reuse a background
  process with stale credentials. A pre-existing CGF server is reused only when its
  supplied token authenticates; the launcher never resets its key or stops it.

- **Unknown load after CUDA out of memory:** inspect LM Studio's log and native
  inventory first. After independently confirming the failed operation has stopped
  and the selected model is unloaded, explicitly resolve its retained task:

  ```sh
  cogfabric --home "$HOME/.local/share/cogfabric-http" residency-managed-resolve \
    --api-base http://127.0.0.1:1234/api/v1 --task-id TASK_UUID \
    --human-approve --provider-stop-acknowledged
  cogfabric-opencode
  ```

  Resolution preserves the original unknown outcome; it does not retry the load.
  The next launcher invocation requests CPU context caching by default. Do not
  acknowledge provider stop while loading or inference is still active.

- **Upstream unavailable / model missing:** start the local inference server,
  ensure the configured model is installed (or load it manually for non-LM Studio providers), and confirm `api_base` and the exact model ID with
  `cogfabric --home "$HOME/.local/share/cogfabric-http" config-show`.
- **Port 8234 responds with 401/403:** provide that server's
  `COGFABRIC_SERVER_API_KEY`; the launcher will verify `GET /v1/models` with the
  bearer key and refuse to stop the server.
- **Provider config mismatch:** merge the configuration above into the
  `opencode.json` in the project you launch from. The helper leaves the file
  untouched and prints its required path.
- **Different state directory or binaries:** set `COGFABRIC_HOME`,
  `COGFABRIC_BIN`, `COGFABRIC_SERVER_BIN`, or `OPENCODE_BIN` before running the
  helper.

This integration is for one local operator on loopback. OpenCode's own tool
permissions remain essential; a bearer key authenticates API access and does
not grant a separate server-side permission to execute client tools.

## Verified compatibility scope

On 2026-10-24, installed OpenCode v2.0.32 completed read/edit/read against both
a deterministic in-process fixture and the configured local Qwen3-8B backend.
The fixture is an explicitly ignored opt-in regression, run with:

```sh
cargo test -p cogfabric-server --test opencode_client -- --ignored --nocapture
```

The local-model smoke uses disposable workspace/state and permission rules
allowing only fixture read/edit. It does not prove general coding quality or
compatibility with every agent application. The API implements Chat Completions,
not the Responses API; choose a client's OpenAI-compatible Chat Completions mode.

A normal repository session was also checked: Qwen initially had only 4096 loaded
context and failed on OpenCode's approximately 7,400-token coding request. After
operator-approved reload at 32768 context and one parallel slot, OpenCode executed
`read` on Cargo.toml and reported the root package name correctly. Automatic GPU
placement failed to load that configuration on this host; `--gpu 0.35` succeeded.
That ratio is a host-specific working configuration, not a general recommendation
or a speed measurement. The helper never applies it automatically.

To retain a default effort in project configuration, set `model` to
`"cogfabric/cogfabric-agent#medium"` (or another defined effort variant). The
startup helper accepts these variant selections. This still sends the model ID
`cogfabric-agent` to the API; the suffix selects the request setting, not weights.
# Native-only client launcher

When the workspace is configured with `configure-network --language-model MODEL
--native-only-client`, `cogfabric-opencode` uses the configured language model
for its existing inventory and optional load checks, rather than the disabled
reasoning-model placeholder. When it starts a server without a native lifecycle
policy, it enables the same-image typed module worker. With a reviewed native
policy, that policy must set `typed_module_worker` to `true`; existing operator
approval and model-key evidence requirements still apply. The launcher refuses
to reuse an existing server in this mode because it cannot verify that server's
worker configuration. It leaves that server running.

Other OpenAI-compatible clients can use the same server: when starting it
directly without a native lifecycle policy, pass `--typed-module-worker` along
with the normal home, bind address and authentication configuration. With a
native lifecycle policy, enable the worker in that reviewed policy instead.

Native-only mode currently supports bounded structured tasks and limited client
tool routes. It does not yet provide general OpenCode tool planning or prove
compatibility with an arbitrary agentic workflow. No tests, live inference, or
model-loading operations were run for this launcher change.
