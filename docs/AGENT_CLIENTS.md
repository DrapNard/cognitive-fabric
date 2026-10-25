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
5. The next request carries that history, the current catalog and selected
   reasoning effort. With the optional collective enabled, opaque CGF-issued tool
   IDs resume one durable root-task checkpoint and its remaining budgets.

MCP is therefore a client-to-tool-server protocol. CGF receives the functions and
results through the model protocol; it does not need to implement an MCP client
for each agentic application. The same contract handles REA, web search and other
connectors without hard-coded tool-name routing. A client can add or remove tool
definitions between turns. Removed functions may remain in historical calls and
results; new proposals must use currently advertised functions.

## Durable Jev-R tool-loop sessions

The opt-in [client collective](CLIENT_COLLECTIVE.md) keeps one root task while a
client executes proposed tools and returns standard correlated tool-result
messages. CGF replaces model-generated tool IDs with opaque `cgf_` IDs. Clients
already echo tool IDs in their next request, so no CGF-specific request field is
needed. The response's task ID stays the root ID across those tool turns.

The initial request pins the effective reasoning mode/effort, model registration,
temperature, configuration and original wall deadline. The operator's configured
token ceiling, maximum calls/cycles/input bytes/tool proposals and internal-tool
budget are cumulative for the root task. A request's `max_tokens` caps additional
work for that step; it cannot increase the root ceiling. The client must send the
unmodified correlated conversation prefix and all pending tool results. CGF checks
the exact checkpoint history and proposal bindings before claiming a continuation.
Concurrent duplicates, missing or changed results, replay, unknown handles and
configuration drift are refused rather than assigned fresh budgets. Raw transcripts
are not retained as a conversation API, though small derived advice/checkpoint
context is stored privately in SQLite; see the privacy limits in the collective
guide.

Keep the collective enabled until the tool loop closes. If it is disabled during
a tool loop, CGF refuses the prefixed continuation handles; restore the original
collective configuration to continue the session, or start a new conversation.

Only tool-driven continuations have a standard handle. After a final answer closes
the checkpoint, a follow-up starts a new root task when the client echoes that
answer and adds a new user message. If the client compacts, splits or omits the
required history, or the task was interrupted during an uncertain inference call,
it must start a new conversation. The same-boot deadline restore currently works
on Linux. A one-call FAST budget cannot continue after returning a tool proposal;
select at least STANDARD for a client workflow that may need Jev-R to reason again
after tool execution.

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

An optional [client collective](CLIENT_COLLECTIVE.md) adds bounded planning,
exploration, solving and review on the same model endpoint. It defaults off and
persists one bounded root task across standard tool-result turns. It does not
expose every Engine expert, project-profile selector or custom execution facility.

This interface supports bounded text/function Chat Completions and completed
responses re-chunked as SSE. It does not currently implement the Responses API,
Anthropic-native messages or multimodal MCP content. Clients requiring those
protocols need a separately implemented adapter. The client remains responsible
for executing tools and enforcing its own permissions; CGF independently charges
and bounds the collective reasoning performed across its root task.
