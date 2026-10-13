#!/usr/bin/env bash
# Optional live integration: OpenCode v2 + a configured local tool-capable model.
set -euo pipefail
umask 077
server_binary=$(realpath "${1:?Usage: opencode-smoke.sh /path/to/cogfabric-server}")
for program in opencode jq openssl timeout rg; do command -v "$program" >/dev/null; done
case "${COGFABRIC_MANAGED_INFERENCE:-false}" in true|false) ;; *) echo "COGFABRIC_MANAGED_INFERENCE must be true or false" >&2; exit 1 ;; esac
root=$(mktemp -d /tmp/cogfabric-opencode-smoke.XXXXXX)
server_pid=
cleanup() {
  if [[ -n "$server_pid" ]]; then
    kill -INT "$server_pid" 2>/dev/null || true
    for _ in {1..50}; do kill -0 "$server_pid" 2>/dev/null || break; sleep 0.1; done
    kill -TERM "$server_pid" 2>/dev/null || true
    wait "$server_pid" 2>/dev/null || true
  fi
}
trap cleanup EXIT
mkdir "$root/fabric" "$root/workspace"
export COGFABRIC_SERVER_API_KEY
COGFABRIC_SERVER_API_KEY=$(openssl rand -hex 32)
# These configure a fresh workspace; they never change the operator's saved config.
jq -n --arg model "${COGFABRIC_MODEL:-qwen/qwen3-8b}" --arg api "${COGFABRIC_API_BASE:-http://127.0.0.1:1234/v1}" --argjson managed "${COGFABRIC_MANAGED_INFERENCE:-false}" \
  '{backend:"openai-local",model:$model,api_base:$api,inference_process:$managed,max_calls:4,max_cycles:2,max_tokens:2048,max_input_bytes:128000,max_tool_calls:4,max_wall_seconds:90,timeout_seconds:80,reasoning_effort:"none",temperature:0}' > "$root/fabric/config.json"
"$server_binary" --home "$root/fabric" --bind 127.0.0.1:0 > "$root/server.log" 2>&1 &
server_pid=$!
for _ in {1..100}; do
  if rg -q '^Cognitive Fabric HTTP API listening on ' "$root/server.log"; then break; fi
  kill -0 "$server_pid" 2>/dev/null || { cat "$root/server.log"; exit 1; }
  sleep 0.1
done
address=$(sed -n 's/^Cognitive Fabric HTTP API listening on //p' "$root/server.log" | head -1)
[[ -n "$address" ]]
jq -n --arg url "http://$address/v1" '{
  model:"cogfabric/cogfabric-agent",
  providers:{cogfabric:{name:"Cognitive Fabric",env:["COGFABRIC_SERVER_API_KEY"],package:"@opencode/ai/providers/openai-compatible",settings:{baseURL:$url},models:{"cogfabric-agent":{name:"Cognitive Fabric Agent",capabilities:{tools:true,input:["text"],output:["text"]},limit:{context:32768,output:2048}}}}},
  permissions:[{action:"*",resource:"*",effect:"deny"},{action:"read",resource:"fixture.txt",effect:"allow"},{action:"edit",resource:"fixture.txt",effect:"allow"}],
  default_agent:"fabric-test",agents:{"fabric-test":{mode:"primary",system:"Read fixture.txt, replace Status: before with Status: after, read it again and report the observed status. Only modify fixture.txt. Do not use shell, network, subagents or external paths.",steps:5}}
}' > "$root/workspace/opencode.json"
printf 'Cognitive Fabric OpenCode integration fixture.\nStatus: before\n' > "$root/workspace/fixture.txt"
export XDG_DATA_HOME="$root/data" XDG_CONFIG_HOME="$root/config" XDG_CACHE_HOME="$root/cache" XDG_STATE_HOME="$root/state"
(cd "$root/workspace" && timeout --kill-after=5 150 opencode run --standalone --agent fabric-test --model cogfabric/cogfabric-agent --format json \
  'Read fixture.txt, change Status: before to Status: after, then read it again and report the observed status.') > "$root/opencode.jsonl" 2> "$root/opencode.stderr"
jq -s -e '
  [ .[] | select(.type=="tool_use") | .part ] as $tools |
  ($tools | map(select(.tool=="read")) | length)>=2 and
  ($tools | map(select(.tool=="edit" or .tool=="write")) | length)>=1 and
  ($tools | all(.state.status=="completed" and (.tool=="read" or .tool=="edit" or .tool=="write"))) and
  ([ .[] | select(.type=="error") ] | length)==0 and
  ([ .[] | select(.type=="text") | .part.text ] | any(contains("Status: after")))
' "$root/opencode.jsonl" >/dev/null
[[ $(cat "$root/workspace/fixture.txt") == $'Cognitive Fabric OpenCode integration fixture.\nStatus: after' ]]
if [[ "${COGFABRIC_MANAGED_INFERENCE:-false}" == true ]]; then
  command -v sqlite3 >/dev/null
  for _ in {1..50}; do
    read -r launched reaped admitted < <(sqlite3 -init /dev/null -batch -noheader -list -separator ' ' -readonly "$root/fabric/fabric.sqlite3" \
      "SELECT (SELECT count(*) FROM events WHERE kind='native_worker_process_launch'), (SELECT count(*) FROM events WHERE kind='native_worker_process_stopped' AND json_extract(payload,'$.reaped')=1), (SELECT count(*) FROM events WHERE kind='agent_admission');")
    [[ "$launched" -gt 0 && "$launched" == "$reaped" && "$launched" == "$admitted" ]] && break
    sleep 0.1
  done
  [[ "$launched" -gt 0 && "$launched" == "$reaped" && "$launched" == "$admitted" ]]
fi
printf 'OpenCode read/edit/read smoke passed. Evidence: %s\n' "$root"
