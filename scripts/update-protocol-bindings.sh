#!/usr/bin/env bash
# Regenerate reviewed bindings; these support builds without a native flatc install.
set -euo pipefail
if [[ $# != 1 || "$1" != /* || ! -x "$1" ]]; then
    echo 'Usage: bash scripts/update-protocol-bindings.sh /absolute/path/to/flatc' >&2
    exit 2
fi
compiler=$1
[[ $("$compiler" --version) == 'flatc version 25.12.19' ]] || { echo 'Expected flatc 25.12.19.' >&2; exit 1; }
cd "$(dirname "${BASH_SOURCE[0]}")/.."
schemas=(cognitive module events bootstrap event_stream tool sql_tool tool_bootstrap agent general_module gpu)
staging=$(mktemp -d /tmp/cogfabric-protocol-bindings.XXXXXX)
# Retain staging as reproducible developer evidence; never delete caller-owned files.
inputs=()
for schema in "${schemas[@]}"; do inputs+=("protocol/$schema.fbs"); done
"$compiler" --rust --gen-object-api -o "$staging" "${inputs[@]}"
mkdir -p protocol/generated
{
    echo 'flatc=25.12.19'
    echo 'upstream=7e163021e59cca4f8e1e35a7c828b5c6b7915953'
    for schema in "${schemas[@]}"; do
        digest=$(sha256sum "protocol/$schema.fbs"); digest=${digest%% *}
        printf 'schema %s.fbs %s\n' "$schema" "$digest"
        digest=$(sha256sum "$staging/${schema}_generated.rs"); digest=${digest%% *}
        printf 'rust %s_generated.rs %s\n' "$schema" "$digest"
    done
} > "$staging/manifest.sha256"
for schema in "${schemas[@]}"; do cp "$staging/${schema}_generated.rs" "protocol/generated/${schema}_generated.rs"; done
cp "$staging/manifest.sha256" protocol/generated/manifest.sha256
printf 'Generated binding evidence: %s\nReview and commit schemas, bindings and manifest together.\n' "$staging"
