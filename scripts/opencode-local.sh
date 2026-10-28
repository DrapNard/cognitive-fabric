#!/usr/bin/env bash
set -Eeuo pipefail

# Start only the Cognitive Fabric HTTP server owned by this invocation, then
# run OpenCode in the current project directory. The upstream inference server
# must already be running. Installed LM Studio models can be loaded via Core.

readonly API_URL="http://127.0.0.1:8234/v1"
readonly SERVER_URL="http://127.0.0.1:8234"
readonly MODEL_ID="cogfabric-agent"
readonly PROVIDER_MODEL="cogfabric/cogfabric-agent"
readonly OUTPUT_LIMIT=1024
readonly CONTEXT_LIMIT=32768

CF_HOME="${COGFABRIC_HOME:-${XDG_DATA_HOME:-${HOME:?HOME is not set}/.local/share}/cogfabric-http}"
SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
if [[ -x "$SCRIPT_DIR/cogfabric" ]]; then
    default_cf_bin="$SCRIPT_DIR/cogfabric"
else
    default_cf_bin='cogfabric'
fi
if [[ -x "$SCRIPT_DIR/cogfabric-server" ]]; then
    default_server_bin="$SCRIPT_DIR/cogfabric-server"
else
    default_server_bin='cogfabric-server'
fi
CF_BIN="${COGFABRIC_BIN:-$default_cf_bin}"
SERVER_BIN="${COGFABRIC_SERVER_BIN:-$default_server_bin}"
OPENCODE_BIN="${OPENCODE_BIN:-opencode}"
SERVER_PID=""
TMP_DIR=""
API_KEY="${COGFABRIC_SERVER_API_KEY:-}"
AUTO_LOAD="${COGFABRIC_OPENCODE_AUTO_LOAD:-1}"
KV_GPU="${COGFABRIC_OPENCODE_KV_GPU:-false}"
NATIVE_POLICY="${COGFABRIC_NATIVE_POLICY:-}"
NATIVE_APPROVED="${COGFABRIC_NATIVE_ACTIVATION_APPROVED:-0}"
NATIVE_KEY_APPROVED="${COGFABRIC_NATIVE_MODEL_KEY_EVIDENCE_APPROVED:-0}"

die() {
    printf 'cognitive-fabric: %s\n' "$*" >&2
    exit 1
}

[[ "$AUTO_LOAD" == 0 || "$AUTO_LOAD" == 1 ]] || die 'COGFABRIC_OPENCODE_AUTO_LOAD must be 0 or 1.'
[[ "$KV_GPU" == true || "$KV_GPU" == false ]] || die 'COGFABRIC_OPENCODE_KV_GPU must be true or false.'
[[ "$NATIVE_APPROVED" == 0 || "$NATIVE_APPROVED" == 1 ]] || die 'COGFABRIC_NATIVE_ACTIVATION_APPROVED must be 0 or 1.'
[[ "$NATIVE_KEY_APPROVED" == 0 || "$NATIVE_KEY_APPROVED" == 1 ]] || die 'COGFABRIC_NATIVE_MODEL_KEY_EVIDENCE_APPROVED must be 0 or 1.'
if [[ -n "$NATIVE_POLICY" ]]; then
    [[ "$NATIVE_APPROVED" == 1 && "$NATIVE_KEY_APPROVED" == 1 ]] || die 'On-demand native activation requires both explicit activation and model-key evidence approval.'
    [[ -f "$NATIVE_POLICY" && ! -L "$NATIVE_POLICY" ]] || die 'The native lifecycle policy must be a regular, non-symlink file.'
elif [[ "$NATIVE_APPROVED" == 1 || "$NATIVE_KEY_APPROVED" == 1 ]]; then
    die 'Native activation approvals require COGFABRIC_NATIVE_POLICY.'
fi

# A remote/background OpenCode process cannot inherit this launcher's ephemeral
# CGF credential. Keep the inference client in this invocation's environment.
for argument in "$@"; do
    [[ "$argument" != -- ]] || break
    case "$argument" in
        --server|--server=*) die 'cogfabric-opencode requires a private OpenCode session; --server cannot inherit its ephemeral CGF token.' ;;
    esac
done

command -v bash >/dev/null 2>&1 || die 'Bash is required.'
for required in jq curl timeout openssl; do
    command -v "$required" >/dev/null 2>&1 || die "Missing dependency: $required."
done

resolve_executable() {
    local requested="$1" resolved
    if [[ "$requested" == */* ]]; then
        [[ -x "$requested" && ! -d "$requested" ]] || die "Executable is not usable: $requested"
        resolved="$(cd -- "$(dirname -- "$requested")" && pwd -P)/$(basename -- "$requested")"
    else
        resolved="$(command -v -- "$requested" 2>/dev/null || true)"
        [[ -n "$resolved" && -x "$resolved" ]] || die "Cannot find executable '$requested'; install it or set its *_BIN environment variable."
    fi
    printf '%s\n' "$resolved"
}

CF_BIN="$(resolve_executable "$CF_BIN")"
SERVER_BIN="$(resolve_executable "$SERVER_BIN")"
OPENCODE_BIN="$(resolve_executable "$OPENCODE_BIN")"

version_output="$(timeout 8 "$OPENCODE_BIN" --version 2>/dev/null || true)"
[[ -n "$version_output" ]] || die 'Could not read the OpenCode version. Install OpenCode v2 or set OPENCODE_BIN.'
if [[ "$version_output" =~ ([0-9]+)\.([0-9]+)\.([0-9]+) ]]; then
    opencode_major="${BASH_REMATCH[1]}"
else
    die "Could not identify the OpenCode version from: $version_output"
fi
(( opencode_major >= 2 )) || die "OpenCode v2 or newer is required; found: $version_output"

[[ -f opencode.json && ! -L opencode.json ]] || die "No regular opencode.json in '$PWD'. Copy /path/to/cognitive-fabric/opencode.example.json to this project, then merge its provider fields if a config already exists. The helper never creates or overwrites project configuration."
jq -e --arg api "$API_URL" --arg model "$PROVIDER_MODEL" \
    --argjson context "$CONTEXT_LIMIT" --argjson output "$OUTPUT_LIMIT" '
      .model as $selected
      | ($selected == $model or any(["none", "low", "medium", "high"][]; $selected == ($model + "#" + .)))
      and .providers.cogfabric.settings.baseURL == $api
      and .providers.cogfabric.package == "@opencode/ai/providers/openai-compatible"
      and ((.providers.cogfabric.env // []) | index("COGFABRIC_SERVER_API_KEY") != null)
      and .providers.cogfabric.models["cogfabric-agent"].capabilities.tools == true
      and .providers.cogfabric.models["cogfabric-agent"].limit.context == $context
      and .providers.cogfabric.models["cogfabric-agent"].limit.output == $output
    ' opencode.json >/dev/null 2>&1 \
    || die "The opencode.json in '$PWD' does not match the Cognitive Fabric v2 provider settings. See docs/OPENCODE.md. It was left untouched."

case "$API_KEY" in
    *$'\n'*|*$'\r'*) die 'COGFABRIC_SERVER_API_KEY must not contain a newline.' ;;
esac
if [[ -n "$API_KEY" && ! "$API_KEY" =~ ^[A-Za-z0-9._~+/=-]{32,4096}$ ]]; then
    die 'COGFABRIC_SERVER_API_KEY must be 32–4096 characters using letters, digits, or . _ ~ + / = -.'
fi

if [[ "$CF_HOME" != /* ]]; then
    CF_HOME="$PWD/$CF_HOME"
fi
mkdir -p -m 700 -- "$CF_HOME" || die "Cannot create private state directory: $CF_HOME"
[[ -d "$CF_HOME" && ! -L "$CF_HOME" ]] || die "State directory must be a real directory, not a symlink: $CF_HOME"
home_mode="$(stat -c '%a' -- "$CF_HOME" 2>/dev/null)" || die "Cannot inspect permissions on $CF_HOME"
(( (8#$home_mode & 077) == 0 )) || die "State directory $CF_HOME must be private (run chmod 700 '$CF_HOME')."

config_file="$CF_HOME/config.json"
[[ -f "$config_file" && ! -L "$config_file" ]] || die "No configured Cognitive Fabric workspace at $CF_HOME. Configure it as described in docs/OPENCODE.md; this helper only loads the model already selected in that configuration."
config_mode="$(stat -c '%a' -- "$config_file" 2>/dev/null)" || die "Cannot inspect permissions on $config_file"
(( (8#$config_mode & 077) == 0 )) || die "Configuration $config_file must be private (run chmod 600 '$config_file')."

TMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/cogfabric-opencode.XXXXXX")" || die 'Could not create a private temporary directory.'
chmod 700 "$TMP_DIR"
umask 077
readonly SERVER_LOG="$TMP_DIR/server.log"
readonly CHECK_LOG="$TMP_DIR/preflight.log"
readonly CURL_CONFIG="$TMP_DIR/curl-auth.conf"

cleanup_server() {
    [[ -n "$SERVER_PID" ]] || return 0
    if kill -0 "$SERVER_PID" 2>/dev/null; then
        kill -INT "$SERVER_PID" 2>/dev/null || true
        for _ in {1..10}; do
            kill -0 "$SERVER_PID" 2>/dev/null || break
            sleep 1
        done
        if kill -0 "$SERVER_PID" 2>/dev/null; then
            kill -TERM "$SERVER_PID" 2>/dev/null || true
            for _ in {1..3}; do
                kill -0 "$SERVER_PID" 2>/dev/null || break
                sleep 1
            done
        fi
        if kill -0 "$SERVER_PID" 2>/dev/null; then
            kill -KILL "$SERVER_PID" 2>/dev/null || true
        fi
    fi
    wait "$SERVER_PID" 2>/dev/null || true
    SERVER_PID=""
}

on_exit() {
    local result=$?
    cleanup_server
    if [[ -n "$TMP_DIR" ]]; then
        rm -f -- "$CURL_CONFIG"
        rm -f -- "$TMP_DIR/config.json" "$TMP_DIR/diagnostics.json" "$TMP_DIR/models.json" \
            "$TMP_DIR/residency.json" "$TMP_DIR/residency.log" "$TMP_DIR/managed-status.json" "$TMP_DIR/load.json"
        if (( result != 0 )) && [[ -s "$CHECK_LOG" ]]; then
            printf 'Private diagnostic log: %s\n' "$CHECK_LOG" >&2
        else
            rm -f -- "$CHECK_LOG"
        fi
        if [[ -s "$SERVER_LOG" ]]; then
            printf 'Private server log: %s\n' "$SERVER_LOG" >&2
        else
            rm -f -- "$SERVER_LOG"
        fi
        # Keep the protected server log for diagnosis; it contains no client key.
        rmdir "$TMP_DIR" 2>/dev/null || true
    fi
    exit "$result"
}
trap on_exit EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

if ! timeout 12 "$CF_BIN" --home "$CF_HOME" --json config-show >"$TMP_DIR/config.json" 2>"$CHECK_LOG"; then
    die "Could not read Cognitive Fabric configuration. See the private diagnostic log: $CHECK_LOG"
fi
jq -e '.backend == "openai-local" and (.max_tokens | type == "number" and . >= 1024) and (.max_tool_calls | type == "number" and . >= 1)' \
    "$TMP_DIR/config.json" >/dev/null 2>&1 \
    || die 'This workspace must use backend openai-local, max_tokens of at least 1024, and max_tool_calls of at least 1. Inspect it with cogfabric --home <path> config-show; no configuration was changed.'

# LM Studio's native inventory reports the context length of each currently
# loaded instance. Check it when that endpoint is available; generic
# OpenAI-compatible providers do not have a common context-inventory API.
configured_api_base="$(jq -r '.api_base // empty' "$TMP_DIR/config.json")"
configured_model="$(jq -r '.model // empty' "$TMP_DIR/config.json")"
if [[ -z "$NATIVE_POLICY" ]]; then
native_authority="$(jq -r '.api_base | try capture("^http://(?<authority>[^/]+)/(?:api/)?v1/?$").authority catch empty' "$TMP_DIR/config.json" 2>/dev/null || true)"
if [[ -n "$native_authority" ]]; then
    native_api_base="http://${native_authority}/api/v1"
    native_args=(--home "$CF_HOME" --json residency-status --api-base "$native_api_base")
    if [[ -n "${COGFABRIC_RESIDENCY_API_KEY:-}" ]]; then
        native_args+=(--authenticate)
    fi
    if timeout 8 "$CF_BIN" "${native_args[@]}" >"$TMP_DIR/residency.json" 2>"$TMP_DIR/residency.log"; then
        if jq -e '.adapter == "lmstudio-native-v1" and (.models | type == "array")' "$TMP_DIR/residency.json" >/dev/null 2>&1; then
            loaded_contexts="$(jq -c --arg model "$configured_model" '[.models[] | if .key == $model then .loaded_instances[] else .loaded_instances[] | select(.id == $model) end | .config.context_length]' "$TMP_DIR/residency.json")"
            loaded_count="$(jq 'length' <<<"$loaded_contexts")"
            if [[ "$AUTO_LOAD" == 1 ]]; then
                if ! timeout 12 "$CF_BIN" --home "$CF_HOME" --json residency-managed-status --api-base "$native_api_base" >"$TMP_DIR/managed-status.json" 2>"$CHECK_LOG"; then
                    die "Could not inspect managed model history. See $CHECK_LOG"
                fi
                jq -e '.managed_lifecycle_blocked == false and .unresolved_provider_operation == false' "$TMP_DIR/managed-status.json" >/dev/null \
                    || die 'Unresolved model lifecycle history requires operator reconciliation; no load or inference was attempted.'
            fi
            if (( loaded_count == 0 )); then
                [[ "$AUTO_LOAD" == 1 ]] || die "Configured model '$configured_model' is unloaded. Load it manually or set COGFABRIC_OPENCODE_AUTO_LOAD=1."
                jq -e --arg model "$configured_model" 'any(.models[]; .key == $model and .type == "llm" and (.loaded_instances | length) == 0)' "$TMP_DIR/residency.json" >/dev/null \
                    || die 'The configured model is not an installed native LLM key; select an installed model in Core configuration. No download was attempted.'
                load_args=(--home "$CF_HOME" --json residency-managed-load --api-base "$native_api_base" --model "$configured_model" --context-length "$CONTEXT_LIMIT" --offload-kv-cache-to-gpu "$KV_GPU" --human-approve)
                if [[ -n "${COGFABRIC_RESIDENCY_API_KEY:-}" ]]; then
                    load_args+=(--authenticate)
                fi
                printf 'Loading configured LM Studio model %s with %s tokens of context (KV cache on GPU: %s).\n' "$configured_model" "$CONTEXT_LIMIT" "$KV_GPU"
                if ! timeout 90 "$CF_BIN" "${load_args[@]}" >"$TMP_DIR/load.json" 2>"$CHECK_LOG"; then
                    die "Model load failed or its outcome is unknown. No retry or unload was attempted; inspect managed residency history and $CHECK_LOG before retrying."
                fi
                jq -e --arg model "$configured_model" --argjson context "$CONTEXT_LIMIT" --argjson gpu "$KV_GPU" '.server_confirmed == true and .receipt.model_key == $model and .receipt.context_length == $context and .receipt.offload_kv_cache_to_gpu == $gpu' "$TMP_DIR/load.json" >/dev/null \
                    || die 'Model load was not confirmed; inspect managed residency history. No retry was attempted.'
                # Re-observe after the confirmed API call, before any inference.
                timeout 8 "$CF_BIN" "${native_args[@]}" >"$TMP_DIR/residency.json" 2>"$CHECK_LOG" \
                    || die 'Could not observe the loaded model; no inference was attempted.'
                loaded_contexts="$(jq -c --arg model "$configured_model" '[.models[] | select(.key == $model) | .loaded_instances[] | .config.context_length]' "$TMP_DIR/residency.json")"
                loaded_count="$(jq 'length' <<<"$loaded_contexts")"
                (( loaded_count > 0 )) || die 'The configured model is still unloaded after the load call.'
            fi
            smallest_context="$(jq 'min' <<<"$loaded_contexts")"
            if (( smallest_context < CONTEXT_LIMIT )); then
                die "Loaded model '$configured_model' reports a $smallest_context-token context, below OpenCode's declared $CONTEXT_LIMIT. Stop it and load/reload it manually with a context of at least $CONTEXT_LIMIT, then rerun. The helper will not change model settings."
            fi
            printf 'LM Studio reports %s loaded instance(s) of %s with at least %s tokens of context.\n' "$loaded_count" "$configured_model" "$smallest_context"
        else
            printf 'Warning: native provider context inventory returned an unrecognized response; actual loaded context is unknown.\n' >&2
        fi
    else
        printf 'Warning: native provider context inventory is unavailable; actual loaded context is unknown. Continuing with the configured %s-token OpenCode declaration.\n' "$CONTEXT_LIMIT" >&2
    fi
else
    printf 'Warning: configured API base %s does not expose the standard LM Studio /v1 path; actual loaded context is unknown. Continuing with the configured %s-token OpenCode declaration.\n' "$configured_api_base" "$CONTEXT_LIMIT" >&2
fi
else
    printf 'On-demand native activation is enabled under the supplied policy; the launcher will not preload models.\n'
fi

diagnostics_args=(--home "$CF_HOME" --json diagnostics)
if [[ -z "$NATIVE_POLICY" ]]; then
    diagnostics_args+=(--probe-inference)
fi
if ! timeout 20 "$CF_BIN" "${diagnostics_args[@]}" >"$TMP_DIR/diagnostics.json" 2>"$CHECK_LOG"; then
    die "The configured upstream inference service did not pass diagnostics. Start the inference server, then retry. Automatic loading requires the native LM Studio API. See docs/OPENCODE.md and the private diagnostic log: $CHECK_LOG"
fi
if [[ -z "$NATIVE_POLICY" ]]; then
    jq -e '.inference_discovery.status == "responded" and .inference_discovery.all_configured_models_advertised == true' \
        "$TMP_DIR/diagnostics.json" >/dev/null 2>&1 \
        || die "The configured upstream server is not advertising every configured model. Start the model server and check its configured model; automatic loading requires the native LM Studio API. See the private diagnostic log: $CHECK_LOG"
fi

# Configure a private curl file so the bearer key does not appear in process
# arguments. It is deleted after the one local service probe.
write_curl_auth() {
    [[ -n "$API_KEY" ]] || return 0
    printf 'header = "Authorization: Bearer %s"\n' "$API_KEY" >"$CURL_CONFIG"
    chmod 600 "$CURL_CONFIG"
}
probe_server() {
    local code
    if [[ -n "$API_KEY" ]]; then
        write_curl_auth
        code="$(curl --config "$CURL_CONFIG" --noproxy '*' --silent --show-error --max-time 3 \
            --output "$TMP_DIR/models.json" --write-out '%{http_code}' "$API_URL/models" 2>/dev/null || true)"
    else
        code="$(curl --noproxy '*' --silent --show-error --max-time 3 \
            --output "$TMP_DIR/models.json" --write-out '%{http_code}' "$API_URL/models" 2>/dev/null || true)"
    fi
    printf '%s' "$code"
}

http_status="$(probe_server)"
if [[ "$http_status" =~ ^2[0-9][0-9]$ ]]; then
    [[ -z "$NATIVE_POLICY" ]] || die 'On-demand activation requires a server started with the supplied policy; an existing server cannot be verified and was left running.'
    jq -e --arg id "$MODEL_ID" '.data | any(.id == $id)' "$TMP_DIR/models.json" >/dev/null 2>&1 \
        || die "An authenticated service is already listening at 127.0.0.1:8234 but does not offer $MODEL_ID. Its process was left running."
    SERVER_PID=""
    printf 'Reusing the authenticated Cognitive Fabric server at %s\n' "$API_URL"
elif [[ "$http_status" == 000 || -z "$http_status" ]]; then
    if [[ -z "$API_KEY" ]]; then
        API_KEY="$(openssl rand -hex 32 2>/dev/null)" || die 'Could not create an ephemeral server key; set COGFABRIC_SERVER_API_KEY or install OpenSSL.'
        [[ "$API_KEY" =~ ^[A-Fa-f0-9]{64}$ ]] || die 'OpenSSL returned an invalid ephemeral key.'
    fi
    export COGFABRIC_SERVER_API_KEY="$API_KEY"
    server_args=(--home "$CF_HOME" --bind 127.0.0.1:8234)
    if [[ -n "$NATIVE_POLICY" ]]; then
        server_args+=(--native-lifecycle-policy "$NATIVE_POLICY" --approve-native-lifecycle --activate-client-native-models --allow-client-model-key-evidence)
    fi
    "$SERVER_BIN" "${server_args[@]}" >"$SERVER_LOG" 2>&1 &
    SERVER_PID=$!
    write_curl_auth
    ready=0
    for _ in {1..25}; do
        if ! kill -0 "$SERVER_PID" 2>/dev/null; then
            die "Cognitive Fabric server exited during startup. Its private log is at $SERVER_LOG"
        fi
        http_status="$(probe_server)"
        if [[ "$http_status" =~ ^2[0-9][0-9]$ ]] && jq -e --arg id "$MODEL_ID" '.data | any(.id == $id)' "$TMP_DIR/models.json" >/dev/null 2>&1; then
            ready=1
            break
        fi
        sleep 1
    done
    [[ "$ready" == 1 ]] || die "Cognitive Fabric server did not become ready at $API_URL. Its private log is at $SERVER_LOG"
    printf 'Started Cognitive Fabric HTTP server on 127.0.0.1:8234 (ephemeral key; not saved).\n'
else
    [[ "$http_status" != 401 && "$http_status" != 403 ]] \
        || die 'A service already responds on 127.0.0.1:8234 but rejected the supplied key. Set the correct COGFABRIC_SERVER_API_KEY; the service was left running.'
    die "A service already responds on 127.0.0.1:8234 (HTTP $http_status), but it could not be authenticated as Cognitive Fabric. Stop or reconfigure that service; this helper left it running."
fi
rm -f -- "$CURL_CONFIG"

export COGFABRIC_SERVER_API_KEY="$API_KEY"
printf 'Starting OpenCode %s from %s\n' "$version_output" "$PWD"
set +e
"$OPENCODE_BIN" --standalone "$@"
opencode_status=$?
set -e
exit "$opencode_status"
