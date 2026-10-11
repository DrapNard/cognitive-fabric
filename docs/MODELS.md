# Registered local inference configurations

Model registration stores bounded, immutable connection metadata in SQLite schema
5. It does not download, train, load or unload weights. The local inference server
continues to own physical model residency and scheduling.

Example operator-reviewed manifest:

```json
{
  "id": "local.small",
  "version": "1.0.0",
  "model": "your-server-model-id",
  "api_base": "http://127.0.0.1:1234/v1",
  "source": "operator-reviewed local installation",
  "license": "Apache-2.0",
  "context_tokens": 4096,
  "max_output_tokens": 1024,
  "weights_sha256": null
}
```

Use the actual model's license and source. `weights_sha256`, when supplied, is an
operator declaration only: an OpenAI-compatible response cannot prove loaded
weight identity. Context capacity is also declared, not tokenizer-verified.

```sh
cogfabric model-register manifest.json
cogfabric models
cogfabric model-use local.small 1.0.0
cogfabric model-disable local.small 1.0.0
cogfabric model-enable local.small 1.0.0
```

Registration rejects unknown fields, remote/credential-bearing endpoints, invalid
canonical versions, invalid hashes and incompatible output/context declarations.
At most 256 versions can be registered, with a 16 KB manifest input limit.
Canonical stored manifests have SHA-256 integrity checks. Duplicate versions
cannot be rewritten, including after disable. Registration and enable/disable
changes have transactional audit entries. Registry corruption fails reads closed.

Selection saves the existing inference configuration with an exact registration
reference, copies its model/endpoint, clears role overrides so one shared model
serves the logical roles, and narrows the task output budget to the declared output
ceiling. It does not expand existing resource limits. `configure-local` explicitly
returns to unregistered configuration. A larger-model comparison override clears
the small-model registration reference rather than attributing it to another model.

At task start, the runtime verifies the enabled registration against configured
endpoint/model/budget and records its manifest/hash in the task trace. A disable
prevents later tasks, while already pinned tasks finish with their original
configuration. Saving a different selection affects new CLI processes; restart a
running service to apply its new saved configuration. Selecting an older immutable
version restores its configuration, not neural weights. Configuration file writes
and registry lifecycle transactions are separate; task traces record what ran.

The fixed local adapter provides chat-completion inference for all seven logical
roles. It does not expose arbitrary capabilities or grant permissions from model
metadata. There are no dependency graphs between these connection records yet.
Provider-reported model IDs and optional usage remain distinct from registration
metadata. Live server validation, actual model residency changes, exact tokenizer
preflight, and weight attestation remain incomplete.

Registry reads validate that each embedded manifest identity equals its database
row identity, both during discovery/listing and exact selection. SQL excludes
manifest payloads above 16,000 bytes, identifiers/versions above 64 bytes and
checksums other than 64 bytes before copying those fields into Rust. Invalid
records fail the operation rather than being silently skipped. Hashes prove local
consistency only; an owner able to rewrite the database can rewrite its checksums.

## Native LM Studio residency inventory

`cogfabric --json residency-status --api-base http://127.0.0.1:1234/api/v1`
queries native v1 inventory and reports installed model keys and each loaded
instance ID/context capacity. Unlike OpenAI model advertisement, the native
response explicitly describes residency. Observations remain server-reported,
not weight attestation, framework ownership or successful generation evidence.
See the [native API specification](https://lmstudio.ai/docs/developer/rest/list).

Authentication is separately opt-in: `--authenticate` reads only
`COGFABRIC_RESIDENCY_API_KEY` from the invoking process environment. Set it locally,
never in chat or a manifest. The inference credential is never forwarded to the
native endpoint. No token or response body is saved; exact raw/JSON-decoded token
echoes are rejected. Transformed exfiltration and same-user process compromise
remain outside this protection.

Only literal-loopback HTTP (or pinned localhost), the explicit `/api/v1` base and
fixed GET `/models` are accepted. Proxies and redirects are disabled; requests
have a three-second timeout and 256 KiB response ceiling. Inventory is bounded
to 256 model keys, 32 instances per model, 1,024 total unique instance IDs and
validated context capacities. Duplicate identities and incomplete inventories
fail closed. Unknown provider metadata is ignored rather than becoming commands.
This operator-only read command performs no load, unload, download or mutation.
It provides instance identity needed by the subsequent lifecycle implementation;
ownership-aware loading/unloading, model switching and real authenticated host
validation remain incomplete.
