#!/bin/sh
# Explicitly approve lazy activation under the saved, operator-reviewable policy.
set -eu

script_dir=$(CDPATH= cd "$(dirname "$0")" && pwd -P)
cf_home=${COGFABRIC_HOME:-${XDG_DATA_HOME:-${HOME:?HOME must be set}/.local/share}/cogfabric-http}
policy=${COGFABRIC_NATIVE_POLICY:-$cf_home/native-policy.json}

if [ ! -f "$policy" ] || [ -L "$policy" ]; then
    echo "cogfabric-opencode-on-demand: no regular native policy at $policy; run setup with --native-on-demand or set COGFABRIC_NATIVE_POLICY." >&2
    exit 1
fi

printf '%s\n' 'This command approves on-demand activation of models listed in the selected local policy for this OpenCode session.'
COGFABRIC_NATIVE_POLICY=$policy \
COGFABRIC_NATIVE_ACTIVATION_APPROVED=1 \
COGFABRIC_NATIVE_MODEL_KEY_EVIDENCE_APPROVED=1 \
    exec "$script_dir/cogfabric-opencode" "$@"
