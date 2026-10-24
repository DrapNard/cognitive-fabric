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
and `--nvml-gpu`. The default prefix is `$HOME/.local`; add its `bin` directory
to `PATH` if needed. Existing `cogfabric`, `cogfabric-chat`, or
`cogfabric-server` binaries in that prefix cause the script to stop. Pass
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
