#!/usr/bin/env bash
# Inspect by default; an approved operator cycle never unloads a pre-existing ID.
set -euo pipefail
umask 077
if [[ $# -lt 3 || $# -gt 5 ]]; then
    echo 'Usage: bash scripts/native-residency-smoke.sh /path/to/cogfabric http://127.0.0.1:1234/api/v1 INSTALLED_MODEL [--human-approve] [--authenticate]' >&2
    exit 2
fi
binary=$(realpath "$1")
base=$2
model=$3
shift 3
approved=false
auth=()
for option in "$@"; do
    case "$option" in
        --human-approve) [[ "$approved" == false ]] || exit 2; approved=true ;;
        --authenticate) [[ ${#auth[@]} == 0 ]] || exit 2; auth=(--authenticate) ;;
        *) echo 'Unknown option' >&2; exit 2 ;;
    esac
done
for program in jq timeout sha256sum; do command -v "$program" >/dev/null; done
evidence=$(mktemp -d /tmp/cogfabric-native-residency.XXXXXX)
state="$evidence/state"
trap 'echo "Residency evidence: $evidence"' EXIT
sha256sum "$binary" > "$evidence/binary.sha256"
sha256sum "${BASH_SOURCE[0]}" > "$evidence/script.sha256"
run() {
    local label=$1
    shift
    timeout 75 "$binary" --home "$state" --backend demo --json "$@" > "$evidence/$label.json"
}
observe() { run "$1" residency-status --api-base "$base" "${auth[@]}"; }
loaded() { jq -c '[.models[] | .key as $key | .loaded_instances[] | {key:$key,id:.id,context_length:.config.context_length}] | sort_by(.key,.id)' "$1"; }
observe before
# All selection, bounds and loopback checks also run inside Core before effects.
jq -e --arg model "$model" '([.models[] | select(.key==$model)]|length)==1 and any(.models[]; .key==$model and (.loaded_instances|length)==0)' "$evidence/before.json" >/dev/null || {
    echo 'Select one installed model with no loaded instances; existing instances are never unloaded or reused by this test.' >&2
    exit 1
}
jq --arg model "$model" --arg base "$base" --argjson approved "$approved" '{format_version:1,model:$model,api_base:$base,operator_approved:$approved,actions:["load_selected_model_context_2048","observe","unload_only_new_returned_instance","observe"],baseline_loaded:[.models[] | .key as $key | .loaded_instances[] | {key:$key,id:.id,context_length:.config.context_length}],exclusive_backend_required:true,ownership_established:false,generation_tested:false,weights_attested:false}' "$evidence/before.json" > "$evidence/plan.json"
cat "$evidence/plan.json"
if [[ "$approved" == false ]]; then
    echo 'Inspection only. Reserve the backend against other clients before approving the cycle.'
    exit 0
fi
# No trap retries mutations or cleans up uncertain effects. Failures retain audits.
run load residency-load --api-base "$base" --model "$model" --context-length 2048 --human-approve "${auth[@]}"
jq -e '.server_confirmed==true and .operation=="load" and .ownership_established==false' "$evidence/load.json" >/dev/null
instance=$(jq -er '.instance_id' "$evidence/load.json")
observe loaded
# A changed unrelated instance set aborts before unload; hashes/IDs are not leases.
expected=$(loaded "$evidence/before.json")
observed=$(jq -c --arg instance "$instance" '[.models[] | .key as $key | .loaded_instances[] | select(.id!=$instance) | {key:$key,id:.id,context_length:.config.context_length}] | sort_by(.key,.id)' "$evidence/loaded.json")
[[ "$observed" == "$expected" ]] || { echo 'Unrelated residency changed; no automatic cleanup attempted.' >&2; exit 1; }
jq -e --arg model "$model" --arg instance "$instance" 'any(.models[]; .key==$model and any(.loaded_instances[]; .id==$instance and .config.context_length==2048))' "$evidence/loaded.json" >/dev/null
run unload residency-unload --api-base "$base" --instance-id "$instance" --human-approve "${auth[@]}"
jq -e --arg instance "$instance" '.server_confirmed==true and .operation=="unload" and .instance_id==$instance' "$evidence/unload.json" >/dev/null
observe after
[[ $(loaded "$evidence/after.json") == "$expected" ]] || { echo 'Final loaded-instance inventory differs; inspect retained evidence.' >&2; exit 1; }
jq -n --arg model "$model" --arg instance "$instance" '{format_version:1,model:$model,instance_id:$instance,server_confirmed_cycle:true,baseline_loaded_instances_preserved:true,ownership_established:false,generation_tested:false,weights_attested:false,automatic_cleanup:false}' > "$evidence/result.json"
cat "$evidence/result.json"
