# Rust Port Quality Review (pending)

Compilation, formatting, Clippy, tests and runtime integration could not be run
because the authoring container has no `rustc`, `cargo` or `flatc` and no external
package network access. The Rust branch is intentionally alpha.

CI is configured in `.github/workflows/ci.yml`. No passed CI runs are claimed.
The next action is to compile on an Arch machine and fix any build errors
before merging the Rust branch into main or publishing a stable release.

See `docs/RUST_PORT.md` and `docs/IMPLEMENTATION_MATRIX.md`.
