# OpenCode local setup

`scripts/opencode-local.sh` starts the Cognitive Fabric HTTP API on
`127.0.0.1:8234`, launches OpenCode in the current project, and shuts down only
the server process it started. If an authenticated Cognitive Fabric server is
already listening, it reuses that process and leaves it running after OpenCode
exits. The helper never starts an inference provider or loads model weights.

## One-time setup

Install `cogfabric`, `cogfabric-server`, and OpenCode v2. The helper needs Bash,
`jq`, `curl`, GNU `timeout`, and OpenSSL. Make the launcher available on your
`PATH`, or call it by its path. For example, after installing the project under
`$HOME/.local`:

```sh
export PATH="$HOME/.local/bin:$PATH"
```

Configure a private Cognitive Fabric state directory and select the model that
is already loaded in your local inference server. This example uses LM Studio
at its standard local API address; model selection does not install or load
weights:

```sh
cogfabric --home "$HOME/.local/share/cogfabric-http" init
cogfabric --home "$HOME/.local/share/cogfabric-http" configure-local \
  --api-base http://127.0.0.1:1234/v1 --model qwen/qwen3-8b
chmod 700 "$HOME/.local/share/cogfabric-http"
chmod 600 "$HOME/.local/share/cogfabric-http/config.json"
```

Start the inference provider yourself, bind it to loopback, and load the model
whose identifier matches `config.json`. For LM Studio, start its local server
with bind address `127.0.0.1` and port `1234`. The launcher checks model
discovery with `cogfabric diagnostics --probe-inference`; it stops with an
actionable error if the provider is unavailable or the configured model is not
advertised. It does not download a model, start LM Studio, or cause model weight
loading.

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
`COGFABRIC_RESIDENCY_API_KEY`; the helper uses it only for this read-only check.
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

- **Upstream unavailable / model missing:** start the local inference server,
  load the configured model, and confirm `api_base` and the exact model ID with
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
