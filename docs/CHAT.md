# Terminal chat client

`cogfabric-chat` is a small interactive client for the local authenticated
OpenAI-compatible server. It uses the `cogfabric-agent` alias with text messages
only. It does not advertise, approve or execute tools, and rejects tool
proposals in responses. Run it in one
terminal while OpenCode or another agentic client uses the same server in a
separate process; the server shares its bounded request admission between them.

Start the server first as described in [HTTP_API.md](HTTP_API.md), then export the
same private server key in each client terminal:

```bash
export COGFABRIC_SERVER_API_KEY="$(cat /path/to/private/server-key)"
```

Run the terminal client:

```bash
cargo run --bin cogfabric-chat -- --server http://127.0.0.1:8234/v1
```

Or run the built executable:

```bash
target/debug/cogfabric-chat --server http://127.0.0.1:8234/v1
```

The client accepts `/help`, `/reset`, and `/quit` (also `/exit`). `/reset` drops
conversation context but does not restore the session input or output-token
budget. The server URL must be HTTP on numeric loopback or `localhost`; requests
disable proxies and redirects. By default the key is read only from
`COGFABRIC_SERVER_API_KEY`, marked sensitive in the authorization header, and is
never placed in command-line arguments or printed.

## Parameters and environment variables

Every setting below accepts both a command-line flag and an environment variable.
Flags override environment variables; environment variables override defaults.
The prefix for the environment suffixes below is `COGFABRIC_CHAT_`.

| Flag | Environment suffix | Default |
| --- | --- | --- |
| `--server` | `SERVER` | `http://127.0.0.1:8234/v1` |
| `--model` | `MODEL` | `cogfabric-agent` |
| `--home` | `HOME` | Existing `COGFABRIC_RUST_HOME` / standard state directory |
| `--api-key-env` | `API_KEY_ENV` | `COGFABRIC_SERVER_API_KEY` (variable name) |
| `--timeout-seconds` | `TIMEOUT_SECONDS` | Saved timeout, capped at 120 |
| `--temperature` | `TEMPERATURE` | Saved temperature |
| `--max-turns` | `MAX_TURNS` | 16 |
| `--max-tokens` | `MAX_TOKENS` | 256 per turn |
| `--session-seconds` | `SESSION_SECONDS` | 3600 |
| `--max-session-user-bytes` | `MAX_SESSION_USER_BYTES` | 32768 |
| `--max-output-reservation` | `MAX_OUTPUT_RESERVATION` | 4096 tokens |
| `--max-message-bytes` | `MAX_MESSAGE_BYTES` | 8192 (user and assistant) |
| `--max-transcript-bytes` | `MAX_TRANSCRIPT_BYTES` | 32768 (serialized history) |
| `--max-messages` | `MAX_MESSAGES` | 32 retained messages |
| `--max-http-response-bytes` | `MAX_HTTP_RESPONSE_BYTES` | 262144 |

For example:

```sh
COGFABRIC_CHAT_MAX_TURNS=64 COGFABRIC_CHAT_MAX_OUTPUT_RESERVATION=16384 \
  cogfabric-chat --model cogfabric-agent --timeout-seconds 120
```

The actual key stays in the environment variable named by `--api-key-env`; never
pass its secret value as an argument. The model must support text conversations;
Cognitive Fabric collective aliases do not accept multi-turn history or these
generation overrides.

Validation retains finite outer bounds: 1–1024 turns, 1–16384 output tokens per
turn, 1–1048576 reserved output tokens, 1–86400 session seconds, 1–3600 request
seconds, up to 16 MiB total user input, 256–262144 bytes per message,
1024–524288 transcript bytes, 2–256 retained messages and 1024–4194304 HTTP
response bytes. A message must fit the transcript and HTTP response bounds.
Temperature must be finite and between 0 and 2. `max_turns * max_tokens` must fit
the reserved output budget. Server-side limits can be lower and remain authoritative;
raising a client limit does not grant more server resources.

The transcript keeps the newest complete exchanges when older context must be
removed. EOF and Ctrl-C exit cleanly. Model text is escaped before terminal
printing when it contains control or bidirectional formatting characters.
`/reset` never refunds session budgets. Library callers can supply
`chat_client::Options` and `Limits` through `ClientApi::with_options`; the existing
`ClientApi::new` retains default options.

The server and client have independent processes, and the HTTP server still
enforces its own admission, time, body and inference limits. Concurrent use does
not guarantee scheduling priority or a dedicated model instance. Responses are
native model output and are not independently verified by this client.
