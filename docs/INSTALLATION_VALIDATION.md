# Clean Arch installation validation

This workflow validates prerequisite setup, a fresh Cargo dependency download,
the pinned FlatBuffers compiler, mandatory checks and an optimized installed
CLI/service. It uses rootless Podman and a read-only archive of tracked source;
no host package installation, model weights, credentials or Git metadata are
mounted. Container isolation is not a security evaluation of untrusted plugins.

From the repository root, with Podman available, create a new source snapshot:

```bash
snapshot=$(mktemp -d /tmp/cogfabric-arch-source.XXXXXX)
git rev-parse HEAD > "$snapshot/source-commit.txt"
git archive HEAD | tar -x -C "$snapshot"
```

The validated official Arch image digest is:
`docker.io/library/archlinux@sha256:363129b67b2bc6c5a634f8fb4961a7e2eba213a2ec075d159127b1260290a722`.
Use it explicitly rather than assuming a mutable image tag is unchanged:

```bash
podman run --rm --volume "$snapshot:/source:ro" \
  docker.io/library/archlinux@sha256:363129b67b2bc6c5a634f8fb4961a7e2eba213a2ec075d159127b1260290a722 \
  bash -c '
    set -euo pipefail
    pacman -Syu --noconfirm --needed rust flatbuffers base-devel cmake git jq curl openssl ripgrep
    cp -a /source /tmp/cognitive-fabric
    cd /tmp/cognitive-fabric
    rustc --version
    cargo --version
    flatc --version
    bash scripts/build-flatc.sh /tmp/cogfabric-flatc
    export FLATC=/tmp/cogfabric-flatc/build/flatc
    cargo fmt --all -- --check
    cargo check --locked --workspace --all-targets
    cargo test --locked --workspace --all-targets
    cargo clippy --locked --workspace --all-targets -- -D warnings
    cargo install --locked --path . --root /tmp/cogfabric-install
    cargo install --locked --path crates/cogfabric-server --root /tmp/cogfabric-install
    bash scripts/installed-smoke.sh /tmp/cogfabric-install/bin/cogfabric
    bash scripts/installed-http-smoke.sh /tmp/cogfabric-install/bin/cogfabric /tmp/cogfabric-install/bin/cogfabric-server
    sha256sum /tmp/cogfabric-install/bin/cogfabric /tmp/cogfabric-install/bin/cogfabric-server
  '
```

Retain the terminal log externally: `--rm` removes the disposable container and
its build/state artifacts after exit. The read-only source snapshot remains for
inspection. Arch repositories are rolling, so the image pin alone does not pin
package versions; record reported toolchain/compiler versions on every run.
This workflow tests demo inference. Live local-server inference, GPU attribution,
CachyOS-specific packages and independent model quality require separate checks.

On 2026-10-13, source `35c4bdc` passed this workflow with Rust/Cargo 1.99.0,
FlatBuffers 25.12.19 and all 359 tests. Optimized install and the full installed
smoke passed. See `RELEASE_AUDIT.md` for evidence and scope limitations.

The workflow above now includes the optional HTTP package and its offline smoke.
That expanded clean-OS workflow has not yet been run; the historical result above
covers the earlier core-only source. Current host source installs of both packages
and offline/live client smokes are recorded separately in RELEASE_AUDIT.md.
