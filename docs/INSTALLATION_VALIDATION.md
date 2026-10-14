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

The expanded workflow above passed on 2026-10-13 for source
`a9933b323c54d6441210e104ec59d37808370357`, with Rust/Cargo 1.99.0 and FlatBuffers
25.12.19. All 417 workspace tests, formatting, compilation and Clippy passed.
Both optimized packages installed, and the installed CLI/socket and offline HTTP
smokes passed. Log: `/tmp/cogfabric-clean-server-validation-20261013.log`.
RELEASE_AUDIT.md records binary hashes and the exact image/source scope. Container
removal discarded its temporary JSON evidence; the retained log records successful
script assertions. This does not verify native inference or GPU setup inside Arch.


### Current optimized host installation refresh

On 2026-10-13, exact source `840e63d55fa7af3b99a24d683104064e4a9f2c24`
installed both packages into `/tmp/cogfabric-current-install.rTuVwk` using
Rust/Cargo 1.97.1 and flatc/Rust FlatBuffers 25.12.19. Commands:
`cargo install --offline --locked --path . --root PREFIX` and
`cargo install --offline --locked --path crates/cogfabric-server --root PREFIX`.
Core optimized build: 2m17s; server: 56.30s. This is the current host with cached
Cargo dependencies, not a fresh OS/dependency-download validation.

Both `scripts/installed-smoke.sh PREFIX/bin/cogfabric` and
`scripts/installed-http-smoke.sh PREFIX/bin/cogfabric PREFIX/bin/cogfabric-server`
passed. Evidence directories: `/tmp/cogfabric-installed-smoke.PRz9GB` and
`/tmp/cogfabric-installed-http.dWe7xP`; installation/toolchain/source/hash logs
remain under the installation prefix. Core checks cover persistence, RAG candidate
promotion/rollback, typed sockets, demo inference, same-image tool/inference
workers, denials, recovery and cleanup. HTTP checks cover authentication, Origin
denial, discovery, typed arithmetic/SQL workers with confirmed reaping and graceful
shutdown, deliberately using an unavailable upstream. Installed disable-set help
also parses. No live-model quality or GPU inference is established by these runs.

Binary SHA-256:
- CLI: `e16e12e89725a820f93518e31037e0c0fc14c1e0a49972eff4d73482be571ef2`
- Server: `61e40bc77fcc1978f9e3fdf799af7449178462c88544b33021c7ed38f69d5594`

Script SHA-256:
- CLI smoke: `afa1fba2e74971edf335ae7f485d10e0d2fe09566039d5d7dcccd679242073cd`
- HTTP smoke: `950ff16da845b970466e5af0b335dde9733e457760b4b81a65cc4dfc81976723`

Hosted CI run `38420337608` also completed successfully for this exact source:
https://github.com/DrapNard/cognitive-fabric/actions/runs/38420337608
The earlier clean Arch evidence remains scoped to its recorded older source;
this refresh does not replace it with a claim of current clean-OS validation.
Source verification already passed all 462 tests, formatting, workspace check
and Clippy before these installs; documentation-only evidence recording does not
require repeating those checks. Full V1 remains incomplete: general scheduling,
independently provisioned executable experts/isolation, physical model residency
and independent unseen reasoning/robustness evaluation are still open.


### Pinned bundled FlatBuffers bindings

Ordinary source builds now use `protocol/generated/*_generated.rs`, generated
with flatc 25.12.19 from reviewed upstream commit
`7e163021e59cca4f8e1e35a7c828b5c6b7915953`. These generated files are deliberately
committed to remove the native schema-compiler prerequisite reported during CLI
installation. They do not replace FlatBuffers, change schemas or weaken frame
verification. `protocol/generated/manifest.sha256` binds all nine schemas and
nine Rust outputs to SHA-256 digests and the generator pin. Build support verifies
the complete inventory, byte limits and exact Rust runtime dependency pin before
copying bindings into Cargo's output directory. Source and Cargo-normalized
package manifest forms are supported.

With `FLATC` unset, no compiler is discovered or executed from PATH. If a stale
export points to a removed temporary compiler, unset it. Explicit `FLATC` remains
strict: the compiler must report 25.12.19 and regenerate byte-identical bundle
outputs. A wrong/missing compiler, stale schema, changed generated file, missing/
duplicate manifest entry or incompatible Rust pin fails the build. Hashes are
local consistency evidence, not signatures against an owner changing code and
hashes together; the configured native compiler remains trusted developer code.

For schema development:

```bash
bash scripts/build-flatc.sh /absolute/path/to/new-compiler-build
bash scripts/update-protocol-bindings.sh /absolute/path/to/new-compiler-build/build/flatc
export FLATC=/absolute/path/to/new-compiler-build/build/flatc
cargo check --locked --workspace --all-targets
cargo test --locked --workspace --all-targets
```

Review and commit schemas, generated bindings and the manifest together. The
update helper retains staging evidence and does not install privileged tools.
CI builds the pinned compiler, checks regenerated output equality, and additionally
checks ordinary bundled builds. CMake/C++ are needed for compiler development;
normal Rust/SQLite builds still need a C toolchain/linker but no `flatc`. Never
hand-edit generated bindings or use a hash update as protocol compatibility proof;
versioning, malformed-frame, socket and compatibility tests remain required.


### Optimized CLI and server installation without flatc — 2026-10-13

Exact runtime source `84e097cec37c71a09d71b75a5eee6bb841376979` installed both
optimized packages with `FLATC` unset into
`/tmp/cogfabric-bundled-install.nVwT2q`. Reported Rust/Cargo: 1.97.1. Commands:
`cargo install --offline --locked --path . --root PREFIX` and
`cargo install --offline --locked --path crates/cogfabric-server --root PREFIX`.
Core release build took 1m30s and server build 1m02s as reported by Cargo. This
uses the current host's C toolchain/linker and cached dependencies; it is not a
fresh OS or dependency-download study. No native FlatBuffers compiler was selected.

Both complete installed workflows passed:
- `scripts/installed-smoke.sh PREFIX/bin/cogfabric`: local persistence, reviewed
  example RAG promotion/rollback, typed sockets, demo inference, managed workers,
  policy denials, recovery and cleanup. Evidence:
  `/tmp/cogfabric-installed-smoke.RRHCKp`.
- `scripts/installed-http-smoke.sh PREFIX/bin/cogfabric PREFIX/bin/cogfabric-server`:
  authenticated discovery, Origin denial, arithmetic/SQL worker verification and
  reaping, and graceful shutdown against deliberately unavailable upstream
  inference. Evidence: `/tmp/cogfabric-installed-http.20mTuz`.

Binary SHA-256:
- CLI: `cd8a29fe6a37a71e6fb0339630d9c55665140ebe2c368df72dbf8704941d51af`
- Server: `6e4d5455e3a64a8eef9ad3cdd4c4515aa4fa3efa811670662b83ade45c904bc0`

Bundled generation manifest SHA-256:
`069657844f4c458c72858b69261e7097f2f6f59db8098f10bceba094c6391ba7`.
Source/toolchain/install/smoke/hash logs remain under the installation prefix.
The corresponding hosted CI run
[38454460668](https://github.com/DrapNard/cognitive-fabric/actions/runs/38454460668)
succeeded, including exact pinned compiler regeneration, all mandatory gates,
bundled all-target checking, local/HTTP workflows, security boundary probes and
offline training preflight. Source tests remain 490 passing (440 library,
50 integration); no runtime code changed in this evidence/documentation update.

No live-model coding quality, current clean-OS/GPU setup, provider ownership or
neural safety is established by installation success. The earlier clean Arch
results stay scoped to their recorded source. General scheduling, independently
provisioned isolated experts, physical residency and independent unseen model
studies remain open V1 requirements. No public crate/repository publication or
release tag was performed. HTTP_API.md now describes ordinary bundled builds
rather than requiring an obsolete compiler export.

### Current optimized installation refresh

Source `fc839768a0dc2105dbdd1c971f9ba3abc2cf31ab` was installed offline with locked cached dependencies on
Rust/Cargo 1.97.1, leaving FLATC unset. Both packages built in optimized mode:
core 2m46s, HTTP server 1m04s (Cargo-reported build durations, not startup latency).
Prefix: `/tmp/cogfabric-current-install.cj8eTv`. Both `scripts/installed-smoke.sh` and
`scripts/installed-http-smoke.sh` passed against the installed executables.
CLI/socket evidence: `/tmp/cogfabric-installed-smoke.eQsFRe`;
HTTP evidence: `/tmp/cogfabric-installed-http.TrhSq2`.

Installed CLI SHA256:
`0f3b665c3b9416bb2736774bc847c9114ac0bdf4b9bd1e697286a58f052b1aa1`.
Installed server SHA256:
`02a8b456b08bafea1e05f29987af6ec6ff4c9db846603e96c59c1ae01ae817c0`.
Protocol bundle manifest SHA256:
`069657844f4c458c72858b69261e7097f2f6f59db8098f10bceba094c6391ba7`.
Build and smoke logs are retained under the prefix. Core smoke covers the existing
fresh-state installed CLI/socket workflows. HTTP smoke verifies authenticated
discovery, Origin denial, typed arithmetic/SQL workers and graceful cleanup.
The installed runtime includes concise advice and scoped subtask prompts.

This refresh uses the current host's cached dependencies and C toolchain; it is
not a new clean-OS validation. The observed LM Studio inventory had no loaded
models after their idle TTL, and these offline smokes perform no live inference.
The earlier real graph measurements remain source-specific. Independent quality,
physical residency and general expert lifecycle gates remain incomplete.

### Current installed live socket inference

The optimized CLI from source `fc839768a0dc2105dbdd1c971f9ba3abc2cf31ab`
was run through its real Unix-socket service with Qwen3 8B in fresh state.
FAST `sort:{"values":[3,1,2,1]}` returned `[1,1,2,3]`, independently checked,
with one call in 3.00 seconds. DEEP on the three-node directed graph with edges
`a->b`, `b->c`, `a->c` returned the minimum path `["a","c"]`, independently
checked, with six calls in 26.67 seconds under the original 30-second budget.
The latter audit contains one plan, one committed schedule and four subtask
resource records, so installed scoped intermediate calls actually ran.
Both clients exited zero. The owned service stopped and its socket was removed.

Raw outcomes, audit counts, installed source/binary identity and limitations are
retained in `evaluation/results/qwen3-8b-current-installed-socket-smoke.json`.
Temporary state/logs: `/tmp/cogfabric-installed-live-socket.p9vx_m4i`.
No model was initially loaded; the existing Qwen weights were loaded via LM Studio
CLI with 8192 context, 0.35 requested offload, one parallel prediction and 600s
idle TTL (3.40 seconds reported load). Other models/workloads were not unloaded.
This is two installed smoke tasks, not a held-out reasoning/latency study or proof
of native framework residency ownership. The prior 96-task graph report remains
source-specific. Broader independent quality and model/expert lifecycle stay open.

### Real-provider phase handoff and optimized refresh

Source `18de498` completed a public three-node path task in RESEARCH mode using
real Qwen3 8B: verified `["a","c"]`, three charged calls, 14.48 seconds within
an unchanged 30-second task ceiling. The planner hit its audited 9999 ms phase
ceiling, emitted typed planning_failure with partial strategy discarded, then
exploration ran under a 6665 ms ceiling and the original solver completed.
Reported input/output usage remained unknown after interruption. The distinct
endpoints and direct a->c edge independently establish the minimum one-edge path.
Raw outcome, configuration, dispatch/phase audits, binary hash and limitations:
`evaluation/results/qwen3-8b-research-phase-limits-smoke.json`.
Temporary evidence: `/tmp/cogfabric-live-phase-limits.nhyn3roq`.
This is one debug-CLI smoke, not a quality or causal latency study; provider stop,
immutable weights and framework-native residency ownership remain unproven.

The same source was then installed offline with locked cached dependencies and
FLATC unset on Rust/Cargo 1.97.1. Both optimized packages and installed workflow
scripts passed. Prefix/logs: `/tmp/cogfabric-phase-install.Z1M9bI`.
Cargo reported core build 1m31s and HTTP build 1m04s. CLI/socket evidence:
`/tmp/cogfabric-installed-smoke.pl80HA`; authenticated HTTP discovery, Origin denial,
typed arithmetic/SQL worker and graceful cleanup evidence:
`/tmp/cogfabric-installed-http.cnkIYD`.
CLI SHA256: `6f3f7faae624d5ef36098ded440882a6019832352b5fe1dddd3d2729793e1519`.
Server SHA256: `353996f14767a7eed63c05260bdde5028b01fedb050fe1cd26dbc59500d8f76d`.
These installed smokes do not themselves invoke a model; the real-provider result
above used the debug binary. This is a current-host refresh, not new clean-OS
proof. All 500 mandatory tests/checks remain tied to the preceding runtime gate;
no runtime source changed in this evidence-only update. General phase recovery,
module/residency lifecycle and independent quality evaluation remain incomplete.
