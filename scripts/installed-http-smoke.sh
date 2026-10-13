#!/usr/bin/env bash
# Verify installed HTTP transport and same-image tools without a model server.
set -euo pipefail
umask 077
if [[ $# != 2 ]]; then
    echo 'Usage: bash scripts/installed-http-smoke.sh /path/to/cogfabric /path/to/cogfabric-server' >&2
    exit 2
fi
for program in jq curl openssl timeout rg; do command -v "$program" >/dev/null; done
core=$(realpath "$1")
server=$(realpath "$2")
evidence=$(mktemp -d /tmp/cogfabric-installed-http.XXXXXX)
state="$evidence/state"
mkdir "$state"
server_pid=''
cleanup() {
    if [[ -n "$server_pid" ]]; then
        kill -INT "$server_pid" 2>/dev/null || true
        for _ in {1..100}; do kill -0 "$server_pid" 2>/dev/null || break; sleep 0.1; done
        kill -TERM "$server_pid" 2>/dev/null || true
        wait "$server_pid" 2>/dev/null || true
    fi
    echo "HTTP smoke evidence: $evidence"
}
trap cleanup EXIT
sha256sum "$core" "$server" > "$evidence/binaries.sha256"
sha256sum "${BASH_SOURCE[0]}" > "$evidence/smoke-script.sha256"
# Deliberately unavailable upstream. Explicit math/SQL must not invoke inference.
jq -n '{backend:"openai-local",model:"offline-smoke-unused",api_base:"http://127.0.0.1:1/v1",arithmetic_process:true,sql_process:true,max_tool_calls:1}' > "$state/config.json"
export COGFABRIC_SERVER_API_KEY
COGFABRIC_SERVER_API_KEY=$(openssl rand -hex 32)
# No user provider credentials/configuration are inherited by this server.
env -i COGFABRIC_SERVER_API_KEY="$COGFABRIC_SERVER_API_KEY" "$server" --home "$state" --bind 127.0.0.1:0 > "$evidence/server.log" 2>&1 &
server_pid=$!
for _ in {1..100}; do
    if rg -q '^Cognitive Fabric HTTP API listening on ' "$evidence/server.log"; then break; fi
    kill -0 "$server_pid" 2>/dev/null || { cat "$evidence/server.log"; exit 1; }
    sleep 0.1
done
address=$(sed -n 's/^Cognitive Fabric HTTP API listening on //p' "$evidence/server.log" | head -1)
[[ -n "$address" ]]
base="http://$address/v1"
[[ $(curl --noproxy '*' --silent --show-error --max-time 5 -o "$evidence/unauthorized.json" -w '%{http_code}' "$base/models") == 401 ]]
[[ $(curl --noproxy '*' --silent --show-error --max-time 5 -H "Authorization: Bearer $COGFABRIC_SERVER_API_KEY" -H 'Origin: https://example.invalid' -o "$evidence/origin.json" -w '%{http_code}' "$base/models") == 403 ]]
curl --noproxy '*' --fail --silent --show-error --max-time 5 -H "Authorization: Bearer $COGFABRIC_SERVER_API_KEY" "$base/models" > "$evidence/models.json"
jq -e '(.data|map(.id)|sort)==(["cogfabric-agent","cogfabric-fast","cogfabric-standard","cogfabric-deep","cogfabric-research"]|sort)' "$evidence/models.json" >/dev/null
for kind in arithmetic sql; do
    if [[ "$kind" == arithmetic ]]; then
        goal='calc:6*7'
        answer='42'
    else
        goal='sql:{"tables":[],"query":"SELECT 42 AS answer"}'
        answer='{"columns":["answer"],"rows":[[42]]}'
    fi
    jq -n --arg goal "$goal" '{model:"cogfabric-fast",messages:[{role:"user",content:$goal}]}' > "$evidence/$kind-request.json"
    curl --noproxy '*' --fail --silent --show-error --max-time 20 -H "Authorization: Bearer $COGFABRIC_SERVER_API_KEY" -H 'Content-Type: application/json' --data-binary "@$evidence/$kind-request.json" "$base/chat/completions" > "$evidence/$kind-response.json"
    jq -e --arg answer "$answer" '.choices[0].message.content==$answer' "$evidence/$kind-response.json" >/dev/null
    task=$(jq -r '.id|ltrimstr("chatcmpl-")' "$evidence/$kind-response.json")
    timeout 5 "$core" --home "$state" --json task "$task" > "$evidence/$kind-trace.json"
    jq -e '.task.status=="completed" and any(.events[]; .kind=="tool_worker_process_stopped" and .payload.reaped==true) and ([.events[]|select(.kind=="model_dispatch" or .kind=="worker_process_launch" or .kind=="native_worker_process_launch")]|length)==0' "$evidence/$kind-trace.json" >/dev/null
done
kill -INT "$server_pid"
for _ in {1..100}; do kill -0 "$server_pid" 2>/dev/null || break; sleep 0.1; done
if kill -0 "$server_pid" 2>/dev/null; then echo 'HTTP shutdown deadline exhausted' >&2; exit 1; fi
wait "$server_pid"
server_pid=''
echo 'Installed HTTP smoke passed: authenticated discovery, origin denial, typed arithmetic/SQL workers and graceful shutdown. No model inference tested.'
