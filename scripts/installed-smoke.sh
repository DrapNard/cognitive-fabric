#!/usr/bin/env bash
# Exercise an installed binary using only fresh temporary state and demo inference.
set -euo pipefail
if [[ $# != 1 ]]; then
    echo 'Usage: bash scripts/installed-smoke.sh /absolute/path/to/cogfabric' >&2
    exit 2
fi
binary=$(realpath "$1")
repository=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
evidence=$(mktemp -d /tmp/cogfabric-installed-smoke.XXXXXX)
state="$evidence/state"
service_pid=''
cleanup() {
    if [[ -n "$service_pid" ]] && kill -0 "$service_pid" 2>/dev/null; then
        timeout 5s "$binary" --home "$state" --backend demo stop >/dev/null 2>&1 || true
        if kill -0 "$service_pid" 2>/dev/null; then kill "$service_pid" 2>/dev/null || true; fi
        wait "$service_pid" 2>/dev/null || true
    fi
    echo "Smoke evidence: $evidence"
}
trap cleanup EXIT
run() {
    local label=$1
    shift
    timeout 20s "$binary" --home "$state" --backend demo --json "$@" >"$evidence/$label.json"
}
run init init
run self-test self-test
jq -e '.math_ok and .wire_ok' "$evidence/self-test.json" >/dev/null
run arithmetic ask 'calc: (7 + 5) * 3'
jq -e '.answer == "36" and .status == "completed" and .calls == 0' "$evidence/arithmetic.json" >/dev/null
run sql ask 'sql:{"tables":[],"query":"SELECT 2 + 3 AS total"}' --mode fast
jq -e '.status == "completed" and .calls == 0 and (.answer | fromjson | .rows == [[5]])' "$evidence/sql.json" >/dev/null
run security security-eval
jq -e '.success and .passed == 24 and .unexpected_accepts == 0 and (.neural_safety_evaluated | not)' "$evidence/security.json" >/dev/null
run checker-register skill-register "$repository/examples/checker_manifest.json"
run checker-activate skill-activate data.sort.s1 1.0.0
jq -e '.active and .id == "data.sort.s1" and .version == "1.0.0"' "$evidence/checker-activate.json" >/dev/null
run checker-health skill-health
jq -e '.active | any(.id == "data.sort.s1" and .checker == "integer_sort/v1" and .dependency_health == "valid")' "$evidence/checker-health.json" >/dev/null
run ingest ingest "$repository/examples/skill_document.md" --skill rust.s1 --source demo --license CC0-1.0
run search search 'Rust async mutex'
jq -e 'any(.skill_id == "rust.s1" and (.body | contains("Rust async runtime diagnostics")))' "$evidence/search.json" >/dev/null
run benchmark bench "$repository/examples/arithmetic_bench.jsonl"
jq -e '.total == 10 and .passed == 10' "$evidence/benchmark.json" >/dev/null
for version in 1 2; do
    run "candidate-$version" candidate create --skill rust.s1 --kind rag --artifact "$repository/examples/skill_document.md" --source demo --license CC0-1.0
    candidate=$(jq -er '.id' "$evidence/candidate-$version.json")
    run "integrity-$version" candidate evaluate "$candidate" --suite integrity
    # This flag demonstrates operator-attested review, not independent dataset evidence.
    run "quality-$version" candidate evaluate "$candidate" --suite quality --dataset "$repository/examples/retrieval_bench.jsonl" --dataset-source project-example --dataset-license CC0-1.0 --held-out
    jq -e '.gate_passed' "$evidence/quality-$version.json" >/dev/null
    run "promotion-$version" candidate promote "$candidate" --human-approve
    jq -e --arg id "$candidate" --argjson version "$version" '.id == $id and .status == "approved" and .version == $version' "$evidence/promotion-$version.json" >/dev/null
done
previous=$(jq -er '.id' "$evidence/candidate-1.json")
run rollback candidate rollback "$previous" --human-approve
jq -e '.rolled_back' "$evidence/rollback.json" >/dev/null
run backup backup --include-rag-artifacts
snapshot=$(jq -er '.path' "$evidence/backup.json")
checksum=$(jq -er '.checksum' "$evidence/backup.json")
run restore restore-database "$snapshot" --checksum "$checksum" --destination "$evidence/restored" --restore-configuration --restore-rag-artifacts
jq -e --arg checksum "$checksum" '.source_checksum == $checksum and .configuration_restored and .rag_artifacts_restored == 1' "$evidence/restore.json" >/dev/null
timeout 20s "$binary" --home "$evidence/restored" --backend demo --json self-test >"$evidence/restored-self-test.json"
jq -e '.math_ok and .wire_ok' "$evidence/restored-self-test.json" >/dev/null
"$binary" --home "$state" --backend demo serve >"$evidence/service.log" 2>&1 &
service_pid=$!
for ((attempt=0; attempt<200; attempt++)); do
    [[ -S "$state/fabric.sock" ]] && break
    kill -0 "$service_pid"
    sleep 0.05
done
[[ -S "$state/fabric.sock" ]]
run socket ask 'calc: 10 * 12' --socket
jq -e '.answer == "120" and .status == "completed"' "$evidence/socket.json" >/dev/null
run service-stop stop
wait "$service_pid"
service_pid=''
[[ ! -e "$state/fabric.sock" ]]
echo 'Installed binary smoke checks passed; live inference and neural safety were not evaluated.'
