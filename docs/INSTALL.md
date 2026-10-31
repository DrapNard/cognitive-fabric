# Install Cognitive Fabric from source

The project does not currently publish versioned binary release assets. The
setup script builds the core CLI, standalone chat CLI, and optional
OpenAI-compatible HTTP server from the selected Git revision and installs them
under a user-owned prefix. It does not use `sudo`, configure a model provider,
start a server, or alter the runtime state directory.

## Requirements

- Git, `curl`, Rust stable (installed through rustup or another toolchain
  manager), a C compiler, and a native linker.
- A supported Unix-like build environment. Unix domain socket workflows are
  intended for Linux and other supported Unix systems.
- Network access to GitHub and Cargo's configured crate registry for source and
  dependencies.

The checkout selects the Rust stable channel in `rust-toolchain.toml`; it does
not pin an exact compiler release. `Cargo.lock` pins dependency versions. The
reviewed FlatBuffers Rust bindings are committed, so a separate `flatc`
installation is not needed for ordinary builds. Rustup may download the current
stable toolchain when the build starts.

## Quick install

Review the setup script and repository revision before running it, especially
when installing source you do not control. This inspect-first path downloads the
script and builds from that same development branch:

```sh
curl -fsSLo setup.sh https://raw.githubusercontent.com/DrapNard/cognitive-fabric/develop/v1.0/scripts/setup.sh
less setup.sh
sh setup.sh --ref develop/v1.0 --prefix "$HOME/.local"
```

If you already trust the current development branch, the one-line form is:

```sh
curl -fsSL https://raw.githubusercontent.com/DrapNard/cognitive-fabric/develop/v1.0/scripts/setup.sh | sh -s -- --ref develop/v1.0 --prefix "$HOME/.local"
```

This runs the downloaded installer before you inspect it. The development
branch can move. For a reproducible install, replace
`develop/v1.0` in both the raw-script URL and `--ref` with the same full,
reviewed commit ID:

```sh
curl -fsSL https://raw.githubusercontent.com/DrapNard/cognitive-fabric/COMMIT_ID/scripts/setup.sh | sh -s -- --ref COMMIT_ID --prefix "$HOME/.local"
```

`COMMIT_ID` is a placeholder; use an actual 40- or 64-character Git commit ID.
Fetching a pinned source commit does not independently authenticate the script
download, so inspect the script at that commit first or download it, review it,
and run the local copy:

```sh
curl -fsSLo setup.sh https://raw.githubusercontent.com/DrapNard/cognitive-fabric/COMMIT_ID/scripts/setup.sh
less setup.sh
sh setup.sh --ref COMMIT_ID --prefix "$HOME/.local"
```

The installer accepts `--prefix PATH`, `--ref BRANCH_OR_COMMIT`, `--replace`,
and `--nvml-gpu`. To configure Qwen as the language interface with no reasoning
LLM during installation, pass `--language-model MODEL_ID`
and `--jev-r-only`; setup then runs `cogfabric configure-network` after
installing. Jev-R uses compiled solver paths for exact arithmetic, SQL,
dependency ordering, integer sorting, shortest paths, and bounded finite-domain
search; it reports unsupported for other domains. No reasoning LLM is configured
or dispatched in this mode. The language model supplies routing metadata; it
does not yet translate arbitrary requests into solver input formats. Optional
`--parallelism 1..4` and `--expert-parallelism 1..4` set the bounded reasoning
and expert widths (both default to 2). These are exact provider model IDs; setup
records the language route but does not contact LM Studio or load model weights.

For model-assisted Jev-R with Qwen reserved for language roles, pass both
`--language-model MODEL_ID` and `--reasoning-model MODEL_ID`. Jev-R roles share
the selected reasoning backbone. `--domain-experts` additionally registers
prompt-routed workflows for the eight built-in skills; it currently requires
model key `qwen3.6-35b-a3b` and the standard loopback LM Studio endpoint. These
workflows are advisory, not independently trained or verified. Configuration is
written to both CLI and OpenCode homes; model weights remain unloaded. Example:

```sh
sh setup.sh --ref develop/v1.0 --prefix "$HOME/.local" --replace \
  --language-model qwen/qwen3-8b \
  --reasoning-model qwen3.6-35b-a3b \
  --domain-experts --parallelism 2 --expert-parallelism 2
```

The default prefix is `$HOME/.local`; add its `bin` directory to `PATH` if
needed. Existing `cogfabric`, `cogfabric-chat`, or
`cogfabric-server` binaries in that prefix cause the script to stop. The installer also adds `cogfabric-opencode`,
a startup helper for the local API and OpenCode (requires Bash, jq, curl, GNU
timeout and OpenSSL at runtime; OpenCode is installed separately). See
[OpenCode startup](OPENCODE.md). Pass
`--replace` only when you intend to replace them. The build is locked by
`Cargo.lock`; optional features such as
`wasm-experts` remain disabled. `--nvml-gpu` enables NVML device observation in
the core CLI; it does not add CUDA inference or guarantee an NVML library or
driver is available at runtime. See [GPU observations](GPU_OBSERVATIONS.md).

## Verify the installed commands

```sh
"$HOME/.local/bin/cogfabric" --version
"$HOME/.local/bin/cogfabric" --help
"$HOME/.local/bin/cogfabric-chat" --help
"$HOME/.local/bin/cogfabric-server" --help
```

Try a deterministic local request without configuring an inference provider:

```sh
"$HOME/.local/bin/cogfabric" --backend demo ask 'calc: (7 + 5) * 3'
```

For provider configuration, HTTP client setup, and security boundaries, see
[configuration](CONFIGURATION.md), [HTTP API](HTTP_API.md), and the
[threat model](THREAT_MODEL.md). Installation success alone does not establish
model quality, GPU inference, neural safety, or release readiness.
