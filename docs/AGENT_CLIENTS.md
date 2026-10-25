# Agentic client integration

CGF provides a client-independent model interface. Any application that supports
this text/function Chat Completions subset can run its own agent loop against it.
OpenCode is one tested application; other applications do not need to go through
OpenCode. Compatibility depends on the protocol a client sends, not its brand.

## Configure the client

| Setting | Value |
| --- | --- |
| Provider protocol | OpenAI-compatible Chat Completions |
| Base URL | `http://127.0.0.1:8234/v1` (or your configured local port) |
| Authentication | Bearer key matching `COGFABRIC_SERVER_API_KEY` |
| Model | `cogfabric-agent` |
| Reasoning effort | `none`, `low`, `medium`, `high`, when the backend supports it |
| Tool execution | Owned by the client, with its own permission checks |

See [server setup and complete API limits](HTTP_API.md). Enable tool calling in
the client's provider configuration. Configure MCP servers, skill locations and
web connections inside that application using its own configuration format.

## Shared architecture contract

1. The client discovers available capabilities: MCP functions, built-in web tools,
   repository tools, connectors and skill-loading tools.
2. Each request supplies the current `tools` catalog with function names,
   descriptions and JSON Schemas. Loaded skill instructions are supplied as text
   in the conversation. CGF passes these to its configured inference backend.
3. CGF returns assistant `tool_calls` with IDs, function names and JSON object
   argument strings. These are proposals, not execution or kernel authorization.
4. The client checks arguments and permissions, executes approved tools and sends
   a `role: tool` message for each `tool_call_id`, retaining the assistant calls in
   history. It includes tool errors as tool-result text too.
5. The next request carries that history, the current catalog and the selected
   reasoning effort. The configured model can continue until the client stops.

MCP is therefore a client-to-tool-server protocol. CGF receives the functions and
results through the model protocol; it does not need to implement an MCP client
for each agentic application. The same contract handles REA, web search and other
connectors without hard-coded tool-name routing. A client can add or remove tool
definitions between turns. Removed functions may remain in historical calls and
results; new proposals must use currently advertised functions.

## Bounds and trust

There are at most 128 advertised functions and 256 messages per request, subject
to tighter configured input limits and the 256 KiB managed IPC frame. Tool names
must use ASCII letters, digits, underscores or hyphens and be at most 64 bytes.
The output proposal limit is separate and remains bounded by configuration.
Correlated historical calls/results must be complete before a new conversational
turn. Do not send unresolved calls or reuse call IDs.

Descriptions, schemas, skill text and results are untrusted model input. They do
not grant CGF file, shell or network permissions. The client must enforce its own
tool execution policy and validate argument semantics. CGF audits advertised
names, strict flags and description/schema hashes, never treating client results
as independently verified evidence. Large catalogs also consume model context;
enable relevant functions and compact results.

## What is and is not verified

Generic core/FlatBuffers, provider HTTP, server HTTP and managed-worker tests
cover function catalogs, history, error text, schemas, effort, cancellation and
denials. The installed OpenCode test adds one concrete MCP/skill client workflow
with synthetic REA/web results through OpenCode Code Mode (`skill`, `execute`,
then MCP discovery and execution inside the client). Direct MCP tool exposure
was not established for that installed version. It is not evidence that every agentic application
has been tested or that a particular model can reliably solve agent tasks.

This interface supports bounded text/function Chat Completions and completed
responses re-chunked as SSE. It does not currently implement the Responses API,
Anthropic-native messages, multimodal MCP content or full Jev-R orchestration.
Clients requiring those protocols need a separately implemented adapter. Keep
session-wide iteration, tool execution and spend limits in the client; each CGF
request is one bounded inference turn.
