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
disable proxies and redirects. The key is read only from
`COGFABRIC_SERVER_API_KEY`, marked sensitive in the authorization header, and is
never placed in command-line arguments or printed.

Per-session limits are at most 16 turns, 32 KiB total user text, 8 KiB per user
message, 8 KiB per assistant reply, 32 KiB retained transcript, 4096 total
reserved output tokens, and one minute to eight hours of session wall time.
Defaults are 16 turns, 256 output tokens per turn (4096 reserved), and one hour. Requests use
the operator's configured temperature and timeout, capped at 120 seconds. The
transcript keeps the newest complete exchanges when older context must be
removed. EOF and Ctrl-C exit cleanly. Model text is escaped before terminal
printing when it contains control or bidirectional formatting characters.

The server and client have independent processes, and the HTTP server still
enforces its own admission, time, body and inference limits. Concurrent use does
not guarantee scheduling priority or a dedicated model instance. Responses are
native model output and are not independently verified by this client.
