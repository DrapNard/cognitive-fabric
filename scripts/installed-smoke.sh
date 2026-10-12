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
sha256sum "$binary" >"$evidence/binary.sha256"
sha256sum "${BASH_SOURCE[0]}" >"$evidence/smoke-script.sha256"
timeout 5s "$binary" --version >"$evidence/binary-version.txt"
run_at() {
    local task_home=$1 label=$2
    shift 2
    timeout 20s "$binary" --home "$task_home" --backend demo --json "$@" >"$evidence/$label.json"
}
run() { run_at "$state" "$@"; }
start_service() {
    local label=$1
    "$binary" --home "$state" --backend demo serve >"$evidence/$label.log" 2>&1 &
    service_pid=$!
    for ((attempt=0; attempt<200; attempt++)); do
        [[ -S "$state/fabric.sock" ]] && break
        kill -0 "$service_pid"
        sleep 0.05
    done
    [[ -S "$state/fabric.sock" ]]
}
check_worker() {
    local task_home=$1 label=$2 task
    task=$(jq -er '.task_id' "$evidence/$label.json")
    run_at "$task_home" "$label-trace" task "$task"
    jq -e '
        ([.events[] | select(.kind == "tool_worker_process")] | length) == 1 and
        ([.events[] | select(.kind == "tool_worker_process_stopped" and .payload.reaped)] | length) == 1 and
        ([.events[] | select(.kind == "tool_worker" and .payload.state == "authorized" and .payload.charged_calls == 1)] | length) == 1 and
        ([.events[] | select(.kind == "worker_process")] | length) == 0
    ' "$evidence/$label-trace.json" >/dev/null
}
check_sql() {
    local label=$1 task
    task=$(jq -er '.task_id' "$evidence/$label.json")
    run "$label-trace" task "$task"
    jq -e '
        any(.events[]; .kind == "tool_dispatch" and .payload.module_id == "sql.local" and .payload.module_version == "1.0.0" and .payload.capability == "sql.query") and
        any(.events[]; .kind == "verification" and .payload.checker == "sqlite_inline/v1" and .payload.verified) and
        ([.events[] | select(.kind == "tool_worker_process" or .kind == "worker_process")] | length) == 0
    ' "$evidence/$label-trace.json" >/dev/null
}
run init init
run self-test self-test
run inference-modules inference-modules
jq -e 'length == 1 and .[0].active and .[0].descriptor.id == "inference.local"' "$evidence/inference-modules.json" >/dev/null
run tool-modules tool-modules
jq -e 'length == 2 and all(.[]; .active) and any(.[]; .descriptor.capability == "math.evaluate") and any(.[]; .descriptor.capability == "sql.query")' "$evidence/tool-modules.json" >/dev/null
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
run tool-worker-configure configure-tool-worker --enabled true
jq -e '.arithmetic_process and (.sandboxed | not)' "$evidence/tool-worker-configure.json" >/dev/null
jq '.max_tool_calls = 1' "$state/config.json" >"$state/config.next"
chmod 600 "$state/config.next"
mv "$state/config.next" "$state/config.json"
run managed-arithmetic ask 'calc: 6 * 7' --mode fast
jq -e '.answer == "42" and .status == "completed" and .calls == 0 and .resources.tool_calls == 1' "$evidence/managed-arithmetic.json" >/dev/null
check_worker "$state" managed-arithmetic
run managed-sql ask 'sql:{"tables":[],"query":"SELECT 42 AS total"}' --mode fast
jq -e '.status == "completed" and .calls == 0 and (.answer | fromjson | .rows == [[42]])' "$evidence/managed-sql.json" >/dev/null
check_sql managed-sql
run sql-worker-configure configure-sql-worker --enabled true
jq -e '.sql_process and (.sandboxed | not)' "$evidence/sql-worker-configure.json" >/dev/null
run sql-process ask 'sql:{"tables":[],"query":"SELECT 84 AS total"}' --mode fast
jq -e '.status == "completed" and .calls == 0 and (.answer | fromjson | .rows == [[84]])' "$evidence/sql-process.json" >/dev/null
check_worker "$state" sql-process
if run rejected-sql ask 'sql:{"tables":[],"query":"SELECT random() AS value"}' --mode fast 2>"$evidence/rejected-sql.stderr"; then
    echo 'Unsafe SQL unexpectedly succeeded' >&2
    exit 1
else
    [[ $? == 1 ]]
fi
jq -e '.status != "completed" and .calls == 0' "$evidence/rejected-sql.json" >/dev/null
check_worker "$state" rejected-sql
if run rejected-arithmetic ask 'calc: 1 / 0' --mode fast 2>"$evidence/rejected-arithmetic.stderr"; then
    echo 'Invalid arithmetic unexpectedly succeeded' >&2
    exit 1
else
    [[ $? == 1 ]]
fi
jq -e '.status == "rejected" and .calls == 0' "$evidence/rejected-arithmetic.json" >/dev/null
check_worker "$state" rejected-arithmetic
run backup backup --include-rag-artifacts
snapshot=$(jq -er '.path' "$evidence/backup.json")
checksum=$(jq -er '.checksum' "$evidence/backup.json")
run restore restore-database "$snapshot" --checksum "$checksum" --destination "$evidence/restored" --restore-configuration --restore-rag-artifacts
jq -e --arg checksum "$checksum" '.source_checksum == $checksum and .configuration_restored and .rag_artifacts_restored == 1' "$evidence/restore.json" >/dev/null
timeout 20s "$binary" --home "$evidence/restored" --backend demo --json self-test >"$evidence/restored-self-test.json"
jq -e '.math_ok and .wire_ok' "$evidence/restored-self-test.json" >/dev/null
run_at "$evidence/restored" restored-config config-show
jq -e '.arithmetic_process and .sql_process and .max_tool_calls == 1' "$evidence/restored-config.json" >/dev/null
run_at "$evidence/restored" restored-arithmetic ask 'calc: 9 * 9' --mode fast
jq -e '.answer == "81" and .status == "completed" and .calls == 0' "$evidence/restored-arithmetic.json" >/dev/null
check_worker "$evidence/restored" restored-arithmetic
run_at "$evidence/restored" restored-sql ask 'sql:{"tables":[],"query":"SELECT 81 AS total"}' --mode fast
jq -e '.status == "completed" and .calls == 0 and (.answer | fromjson | .rows == [[81]])' "$evidence/restored-sql.json" >/dev/null
check_worker "$evidence/restored" restored-sql
start_service service
run socket-inference-modules inference-modules --socket
jq -e 'length == 1 and .[0].active and .[0].descriptor.id == "inference.local"' "$evidence/socket-inference-modules.json" >/dev/null
run socket-tool-modules tool-modules --socket
jq -e 'length == 2 and all(.[]; .active) and any(.[]; .descriptor.capability == "math.evaluate") and any(.[]; .descriptor.capability == "sql.query")' "$evidence/socket-tool-modules.json" >/dev/null
run socket ask 'calc: 10 * 12' --socket
jq -e '.answer == "120" and .status == "completed"' "$evidence/socket.json" >/dev/null
check_worker "$state" socket
run socket-sql ask 'sql:{"tables":[],"query":"SELECT 72 AS total"}' --mode fast --socket
jq -e '.status == "completed" and .calls == 0 and (.answer | fromjson | .rows == [[72]])' "$evidence/socket-sql.json" >/dev/null
check_worker "$state" socket-sql
run service-stop stop
wait "$service_pid"
service_pid=''
[[ ! -e "$state/fabric.sock" ]]
run inference-worker-configure configure-worker --enabled true
jq -e '.inference_process' "$evidence/inference-worker-configure.json" >/dev/null
jq '.max_tool_calls = 0 | .interpret_goals = false' "$state/config.json" >"$state/config.next"
chmod 600 "$state/config.next"
mv "$state/config.next" "$state/config.json"
run inference-process ask 'Explain Rust ownership' --mode fast
jq -e '.status == "demo" and .calls == 1 and .resources.tool_calls == 0' "$evidence/inference-process.json" >/dev/null
inference_task=$(jq -er '.task_id' "$evidence/inference-process.json")
run inference-process-trace task "$inference_task"
jq -e '
    ([.events[] | select(.kind == "worker_process_launch")] | length) == 1 and
    ([.events[] | select(.kind == "worker_process_owned")] | length) == 1 and
    ([.events[] | select(.kind == "worker_process")] | length) == 1 and
    ([.events[] | select(.kind == "worker_process_stopped" and .payload.reaped)] | length) == 1 and
    ([.events[] | select(.kind == "worker_process_owned")][0].payload.pid == [.events[] | select(.kind == "worker_process_stopped")][0].payload.pid)
' "$evidence/inference-process-trace.json" >/dev/null
start_service zero-tool-service
run zero-tool-socket ask 'Explain Rust ownership' --mode fast --socket
jq -e '.status == "demo" and .calls == 1 and .resources.tool_calls == 0' "$evidence/zero-tool-socket.json" >/dev/null
zero_task=$(jq -er '.task_id' "$evidence/zero-tool-socket.json")
run zero-tool-socket-trace task "$zero_task"
jq -e '
    any(.events[]; .kind == "schedule" and .payload.decision == "skip_retrieval") and
    ([.events[] | select(.kind == "worker_process_stopped" and .payload.reaped and (.payload.forced | not) and .payload.exit_code == 0)] | length) == 1 and
    ([.events[] | select(.kind == "tool_dispatch" or .kind == "tool_worker_process")] | length) == 0
' "$evidence/zero-tool-socket-trace.json" >/dev/null
if run zero-tool-socket-denial ask 'calc: 6 * 7' --mode fast --socket 2>"$evidence/zero-tool-socket-denial.stderr"; then
    echo 'Zero tool budget unexpectedly permitted arithmetic' >&2
    exit 1
else
    [[ $? == 1 ]]
fi
jq -e '.status == "failed" and .calls == 0 and .resources.tool_calls == 0' "$evidence/zero-tool-socket-denial.json" >/dev/null
denied_task=$(jq -er '.task_id' "$evidence/zero-tool-socket-denial.json")
run zero-tool-socket-denial-trace task "$denied_task"
jq -e '
    any(.events[]; .kind == "policy" and .payload.capability == "math.evaluate" and (.payload.allowed | not)) and
    ([.events[] | select(.kind == "tool_dispatch" or .kind == "tool_worker_launch" or .kind == "tool_worker_process" or .kind == "worker_process_launch")] | length) == 0
' "$evidence/zero-tool-socket-denial-trace.json" >/dev/null
run zero-tool-service-stop stop
wait "$service_pid"
service_pid=''
[[ ! -e "$state/fabric.sock" ]]
run recovered-workers recover-workers
jq -e '.interrupted_worker_sessions == [] and (.automatic_replay | not)' "$evidence/recovered-workers.json" >/dev/null
run worker-files worker-files-preview
jq -e '.entries == [] and (.more | not)' "$evidence/worker-files.json" >/dev/null
run worker-files-apply worker-files-apply "$evidence/worker-files.json" --human-approve
jq -e '.removed_directories == 0 and (.processes_killed | not)' "$evidence/worker-files-apply.json" >/dev/null
echo 'Installed binary smoke checks passed; live inference and neural safety were not evaluated.'
