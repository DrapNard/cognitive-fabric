#!/usr/bin/env bash
# Build the schema compiler matching Cargo.lock; no privileged install or overwrite.
set -euo pipefail
if [[ $# != 1 || -z "$1" ]]; then
    echo 'Usage: bash scripts/build-flatc.sh /absolute/path/to/new-build-directory' >&2
    exit 2
fi
if [[ "$1" != /* ]]; then
    echo 'The compiler build destination must be an absolute path.' >&2
    exit 2
fi
for dependency in git cmake c++; do
    if ! command -v "$dependency" >/dev/null 2>&1; then
        echo "Missing build prerequisite: $dependency" >&2
        exit 2
    fi
done
destination=$1
if [[ -e "$destination" || -L "$destination" ]]; then
    echo 'Refusing to overwrite an existing compiler build destination.' >&2
    exit 2
fi
umask 077
mkdir -- "$destination"
# Retain failed work for inspection; never recursively delete caller-owned paths.
git clone --depth 1 --branch v25.12.19 \
    https://github.com/google/flatbuffers.git "$destination/source"
expected=7e163021e59cca4f8e1e35a7c828b5c6b7915953
actual=$(git -C "$destination/source" rev-parse HEAD)
if [[ "$actual" != "$expected" ]]; then
    echo 'FlatBuffers upstream commit differs from the reviewed compiler pin.' >&2
    exit 1
fi
cmake -S "$destination/source" -B "$destination/build" \
    -DFLATBUFFERS_BUILD_TESTS=OFF
cmake --build "$destination/build" --target flatc --parallel 2
compiler="$destination/build/flatc"
if [[ $("$compiler" --version) != 'flatc version 25.12.19' ]]; then
    echo 'The built compiler reports an incompatible version.' >&2
    exit 1
fi
printf 'Compiler source commit: %s\n' "$actual"
printf 'Run in your shell: export FLATC=%q\n' "$compiler"
