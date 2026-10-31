#!/bin/sh
# Install Cognitive Fabric from a selected source revision without sudo.
set -eu

repository_url='https://github.com/DrapNard/cognitive-fabric.git'
default_ref='develop/v1.0'
source_override=''
prefix=${HOME:?HOME must be set}/.local
ref=$default_ref
replace=0
nvml_gpu=0
language_model=''
reasoning_model=''
role_models=''
role_model_count=0
api_base='http://127.0.0.1:1234/v1'
jev_r_only=0
domain_experts=0
domain_expert_manifest=''
domain_expert_workflow_version=''
native_on_demand=0
language_context=16384
language_context_set=0
parallelism=2
expert_parallelism=2

usage() {
    cat <<'USAGE'
Usage: setup.sh [--prefix PATH] [--ref GIT_REF_OR_COMMIT] [--source PATH] [--replace] [--nvml-gpu]
                [--language-model MODEL --reasoning-model MODEL | --jev-r-only]
                [--role-model ROLE=MODEL ...]
                [--api-base URL] [--domain-experts]
                [--domain-expert-manifest MODEL_MANIFEST]
                [--domain-expert-workflow-version VERSION] [--native-on-demand]
                [--language-context TOKENS]
                [--parallelism 1..4] [--expert-parallelism 1..4]

Build and install cogfabric, cogfabric-chat, and cogfabric-server from source.
Also install the cogfabric-opencode and cogfabric-opencode-on-demand helpers.
The default prefix is $HOME/.local and the default source ref is develop/v1.0.
--source installs from an existing local checkout without network access.
--replace allows replacing existing binaries with the same package version.
--nvml-gpu builds the CLI and HTTP server with optional NVML observation support.
Supplying --language-model with --reasoning-model configures Qwen as a
language-only route and shares the selected reasoning backbone across Jev-R
roles in the CLI and OpenCode state directories. --domain-experts additionally
registers the included workflow manifests; by default they share the
qwen3.6-35b-a3b reasoning model and standard LM Studio API base. An optional
--domain-expert-manifest registers an exact operator-provided model manifest
and binds new workflow versions to that model. Its API base must match
--api-base, it must differ from the language-only model, and its declared
context must be at least 16384 tokens. Bump
--domain-expert-workflow-version when intentionally changing the immutable
expert-model binding.
Repeat --role-model ROLE=MODEL to route planner, solver, explorer, critic or
reflector to a specific model; unassigned roles use --reasoning-model.
--native-on-demand writes an unapproved native lifecycle policy draft for the
OpenCode home and installs cogfabric-opencode-on-demand. Running that explicit
command approves model activation on first admitted use; it does not preload.
The draft assigns 16384 tokens to all routes by default. Set
--language-context TOKENS to override only the language model context (1-32768).
Supplying --language-model with --jev-r-only instead disables reasoning-role
model calls. Configuration and registration do not load models or run inference.
Width defaults are 2 and 2.
USAGE
}

while [ "$#" -gt 0 ]; do
    case $1 in
        --prefix)
            [ "$#" -ge 2 ] || { echo 'Missing value for --prefix.' >&2; exit 2; }
            prefix=$2
            shift 2
            ;;
        --ref)
            [ "$#" -ge 2 ] || { echo 'Missing value for --ref.' >&2; exit 2; }
            ref=$2
            shift 2
            ;;
        --source)
            [ "$#" -ge 2 ] && [ -n "$2" ] || { echo 'Missing value for --source.' >&2; exit 2; }
            source_override=$2
            shift 2
            ;;
        --replace)
            replace=1
            shift
            ;;
        --nvml-gpu)
            nvml_gpu=1
            shift
            ;;
        --language-model)
            [ "$#" -ge 2 ] && [ -n "$2" ] || { echo 'Missing value for --language-model.' >&2; exit 2; }
            language_model=$2
            shift 2
            ;;
        --reasoning-model)
            [ "$#" -ge 2 ] && [ -n "$2" ] || { echo 'Missing value for --reasoning-model.' >&2; exit 2; }
            reasoning_model=$2
            shift 2
            ;;
        --role-model)
            [ "$#" -ge 2 ] && [ -n "$2" ] || { echo 'Missing value for --role-model.' >&2; exit 2; }
            role_model_count=$((role_model_count + 1))
            [ "$role_model_count" -le 5 ] || { echo 'At most five --role-model options are supported.' >&2; exit 2; }
            case $2 in *'='*) ;; *) echo '--role-model requires ROLE=MODEL.' >&2; exit 2 ;; esac
            if [ -n "$role_models" ]; then
                role_models="$role_models
$2"
            else
                role_models=$2
            fi
            shift 2
            ;;
        --api-base)
            [ "$#" -ge 2 ] && [ -n "$2" ] || { echo 'Missing value for --api-base.' >&2; exit 2; }
            api_base=$2
            shift 2
            ;;
        --jev-r-only)
            jev_r_only=1
            shift
            ;;
        --domain-experts)
            domain_experts=1
            shift
            ;;
        --domain-expert-manifest)
            [ "$#" -ge 2 ] && [ -n "$2" ] || { echo 'Missing value for --domain-expert-manifest.' >&2; exit 2; }
            domain_expert_manifest=$2
            shift 2
            ;;
        --domain-expert-workflow-version)
            [ "$#" -ge 2 ] && [ -n "$2" ] || { echo 'Missing value for --domain-expert-workflow-version.' >&2; exit 2; }
            domain_expert_workflow_version=$2
            shift 2
            ;;
        --native-on-demand)
            native_on_demand=1
            shift
            ;;
        --language-context)
            [ "$#" -ge 2 ] || { echo 'Missing value for --language-context.' >&2; exit 2; }
            language_context=$2
            language_context_set=1
            shift 2
            ;;
        --parallelism)
            [ "$#" -ge 2 ] || { echo 'Missing value for --parallelism.' >&2; exit 2; }
            parallelism=$2
            shift 2
            ;;
        --expert-parallelism)
            [ "$#" -ge 2 ] || { echo 'Missing value for --expert-parallelism.' >&2; exit 2; }
            expert_parallelism=$2
            shift 2
            ;;
        --help|-h)
            usage
            exit 0
            ;;
        *)
            echo "Unknown argument: $1" >&2
            usage >&2
            exit 2
            ;;
    esac
done

if [ "$language_context_set" -eq 1 ] && [ "$native_on_demand" -ne 1 ]; then
    echo '--language-context requires --native-on-demand.' >&2
    exit 2
fi
case $language_context in
    ''|*[!0-9]*) echo '--language-context must be an integer from 1 through 32768.' >&2; exit 2 ;;
esac
if [ "$language_context" -lt 1 ] || [ "$language_context" -gt 32768 ]; then
    echo '--language-context must be an integer from 1 through 32768.' >&2
    exit 2
fi

if [ "$jev_r_only" -eq 1 ] || [ -n "$language_model" ] || [ -n "$reasoning_model" ] || [ "$domain_experts" -eq 1 ] || [ -n "$domain_expert_manifest" ] || [ -n "$domain_expert_workflow_version" ] || [ "$role_model_count" -gt 0 ] || [ "$native_on_demand" -eq 1 ]; then
    if [ -z "$language_model" ]; then
        echo 'A configured network requires --language-model MODEL.' >&2
        exit 2
    fi
    if [ "$jev_r_only" -eq 1 ] && [ -n "$reasoning_model" ]; then
        echo '--jev-r-only conflicts with --reasoning-model.' >&2
        exit 2
    fi
    if [ "$jev_r_only" -eq 1 ] && [ "$role_model_count" -gt 0 ]; then
        echo '--jev-r-only conflicts with --role-model.' >&2
        exit 2
    fi
    if [ "$jev_r_only" -ne 1 ] && [ -z "$reasoning_model" ]; then
        echo 'Model-assisted routing requires --reasoning-model MODEL; use --jev-r-only for compiled solvers.' >&2
        exit 2
    fi
    if [ "$jev_r_only" -eq 1 ] && [ "$api_base" != 'http://127.0.0.1:1234/v1' ]; then
        echo '--api-base applies only to model-assisted routing.' >&2
        exit 2
    fi
    if [ "$native_on_demand" -eq 1 ] && { [ "$jev_r_only" -eq 1 ] || [ "$api_base" != 'http://127.0.0.1:1234/v1' ]; }; then
        echo '--native-on-demand requires model-assisted routing with the standard loopback LM Studio API base.' >&2
        exit 2
    fi
    if [ "$domain_experts" -eq 1 ] && { [ "$jev_r_only" -eq 1 ] || [ "$reasoning_model" != 'qwen3.6-35b-a3b' ] || [ "$api_base" != 'http://127.0.0.1:1234/v1' ]; }; then
        echo '--domain-experts requires model-assisted routing with qwen3.6-35b-a3b at http://127.0.0.1:1234/v1.' >&2
        exit 2
    fi
    if { [ -n "$domain_expert_manifest" ] || [ -n "$domain_expert_workflow_version" ]; } && [ "$domain_experts" -ne 1 ]; then
        echo '--domain-expert-manifest and --domain-expert-workflow-version require --domain-experts.' >&2
        exit 2
    fi
    if [ -n "$domain_expert_manifest" ]; then
        case $domain_expert_manifest in
            /*) ;;
            *) domain_expert_manifest=$PWD/$domain_expert_manifest ;;
        esac
        [ -f "$domain_expert_manifest" ] && [ ! -L "$domain_expert_manifest" ] || {
            echo '--domain-expert-manifest must name an existing regular file, not a symlink.' >&2
            exit 2
        }
        domain_expert_manifest_size=$(wc -c <"$domain_expert_manifest" | tr -d '[:space:]')
        [ "$domain_expert_manifest_size" -le 16000 ] || {
            echo '--domain-expert-manifest exceeds the 16000-byte model registry limit.' >&2
            exit 2
        }
        command -v jq >/dev/null 2>&1 || {
            echo 'jq is required to read and register --domain-expert-manifest.' >&2
            exit 1
        }
        domain_expert_manifest_api_base=$(jq -er '.api_base' "$domain_expert_manifest") || {
            echo '--domain-expert-manifest must contain a valid API base.' >&2
            exit 2
        }
        domain_expert_manifest_context=$(jq -er '.context_tokens | select(. >= 16384)' "$domain_expert_manifest") || {
            echo '--domain-expert-manifest must declare at least 16384 context tokens for the OpenCode route.' >&2
            exit 2
        }
        if [ "$domain_expert_manifest_api_base" != "$api_base" ]; then
            echo "The expert model manifest API base must exactly match $api_base." >&2
            exit 2
        fi
        domain_expert_manifest_model=$(jq -er '.model' "$domain_expert_manifest") || {
            echo '--domain-expert-manifest must declare a model key.' >&2
            exit 2
        }
        if [ "$domain_expert_manifest_model" = "$language_model" ]; then
            echo 'The Jev+ expert model must differ from the language-only model.' >&2
            exit 2
        fi
    fi
    if [ -n "$domain_expert_workflow_version" ]; then
        command -v jq >/dev/null 2>&1 || {
            echo 'jq is required to validate --domain-expert-workflow-version.' >&2
            exit 1
        }
        jq -en --arg version "$domain_expert_workflow_version" \
            '$version | test("^(0|[1-9][0-9]*)\\.(0|[1-9][0-9]*)\\.(0|[1-9][0-9]*)$")' >/dev/null || {
            echo '--domain-expert-workflow-version must be canonical MAJOR.MINOR.PATCH.' >&2
            exit 2
        }
    fi
    case $parallelism in 1|2|3|4) ;; *) echo '--parallelism must be 1, 2, 3, or 4.' >&2; exit 2 ;; esac
    case $expert_parallelism in 1|2|3|4) ;; *) echo '--expert-parallelism must be 1, 2, 3, or 4.' >&2; exit 2 ;; esac
fi

if [ "$domain_experts" -eq 1 ]; then
    command -v jq >/dev/null 2>&1 || {
        echo 'jq is required for exact model-registry checks with --domain-experts.' >&2
        exit 1
    }
fi

for program in git cargo rustc; do
    command -v "$program" >/dev/null 2>&1 || {
        echo "Required program not found: $program" >&2
        echo 'Install Git and the Rust toolchain before running this setup script.' >&2
        exit 1
    }
done

# Keep git's ref argument from being interpreted as an option or option bundle.
if [ -z "$source_override" ]; then
    case $ref in
        ''|-*|*[!A-Za-z0-9._/-]*)
            echo 'Use a branch/tag name or a 40/64-character hexadecimal commit ID for --ref.' >&2
            exit 2
            ;;
    esac
    case $ref in
        *..*|*/|*//*)
            echo 'Invalid --ref value.' >&2
            exit 2
            ;;
    esac
else
    [ -d "$source_override" ] || { echo "Local source directory does not exist: $source_override" >&2; exit 2; }
    source_override=$(CDPATH= cd "$source_override" && pwd -P)
    [ -f "$source_override/Cargo.toml" ] && [ -f "$source_override/Cargo.lock" ] \
        || { echo 'Local source must contain Cargo.toml and Cargo.lock.' >&2; exit 2; }
fi

case $prefix in
    ''|-*)
        echo 'Use a non-empty prefix path that does not begin with a dash.' >&2
        exit 2
        ;;
esac

data_home=${XDG_DATA_HOME:-${HOME:?HOME must be set}/.local/share}
cli_home=${COGFABRIC_CLI_HOME:-$data_home/cogfabric-rust}
opencode_home=${COGFABRIC_HOME:-$data_home/cogfabric-http}
case $data_home in /*) ;; *) data_home=$PWD/$data_home ;; esac
case $cli_home in /*) ;; *) cli_home=$PWD/$cli_home ;; esac
case $opencode_home in /*) ;; *) opencode_home=$PWD/$opencode_home ;; esac
native_policy_file=$opencode_home/native-policy.json
if [ "$native_on_demand" -eq 1 ] && { [ -e "$native_policy_file" ] || [ -L "$native_policy_file" ]; }; then
    echo "Refusing to replace the existing native policy: $native_policy_file" >&2
    exit 1
fi

if [ ! -d "$prefix" ]; then
    mkdir -p "$prefix"
fi
prefix=$(CDPATH= cd "$prefix" && pwd)
mkdir -p "$prefix/bin"

if [ "$replace" -ne 1 ]; then
    for binary in cogfabric cogfabric-chat cogfabric-server cogfabric-opencode cogfabric-opencode-on-demand; do
        if [ -e "$prefix/bin/$binary" ] || [ -L "$prefix/bin/$binary" ]; then
            echo "An executable already exists at $prefix/bin/$binary." >&2
            echo 'Re-run with --replace after reviewing the source if you intend to replace it.' >&2
            exit 1
        fi
    done
fi

workdir=$(mktemp -d "${TMPDIR:-/tmp}/cogfabric-setup.XXXXXX") || {
    echo 'Could not create a temporary build directory.' >&2
    exit 1
}
cleanup() {
    rm -rf "$workdir"
}
trap cleanup EXIT
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM

if [ -n "$source_override" ]; then
    source_dir=$source_override
    printf 'Building local source: %s\n' "$source_dir"
else
    source_dir=$workdir/source
    mkdir "$source_dir"
    git -C "$source_dir" init --quiet
    git -C "$source_dir" remote add origin "$repository_url"
    echo "Fetching Cognitive Fabric source ref: $ref"
    git -C "$source_dir" fetch --quiet --depth 1 origin "$ref"
    git -C "$source_dir" checkout --quiet --detach FETCH_HEAD
    printf 'Building source commit: '
    git -C "$source_dir" rev-parse --short=12 HEAD
fi
cd "$source_dir"

install_core() {
    if [ "$nvml_gpu" -eq 1 ] && [ "$replace" -eq 1 ]; then
        cargo install --features nvml-gpu --bins --locked --path "$source_dir" --root "$prefix" --force
    elif [ "$nvml_gpu" -eq 1 ]; then
        cargo install --features nvml-gpu --bins --locked --path "$source_dir" --root "$prefix"
    elif [ "$replace" -eq 1 ]; then
        cargo install --bins --locked --path "$source_dir" --root "$prefix" --force
    else
        cargo install --bins --locked --path "$source_dir" --root "$prefix"
    fi
}

install_core
if [ "$nvml_gpu" -eq 1 ] && [ "$replace" -eq 1 ]; then
    cargo install --features nvml-gpu --locked --path "$source_dir/crates/cogfabric-server" --root "$prefix" --force
elif [ "$nvml_gpu" -eq 1 ]; then
    cargo install --features nvml-gpu --locked --path "$source_dir/crates/cogfabric-server" --root "$prefix"
elif [ "$replace" -eq 1 ]; then
    cargo install --locked --path "$source_dir/crates/cogfabric-server" --root "$prefix" --force
else
    cargo install --locked --path "$source_dir/crates/cogfabric-server" --root "$prefix"
fi

configure_network_for_home() {
    config_home=$1
    if [ "$jev_r_only" -eq 1 ]; then
        "$prefix/bin/cogfabric" --home "$config_home" configure-network \
            --language-model "$language_model" --jev-r-only \
            --parallelism "$parallelism" --expert-parallelism "$expert_parallelism"
        return
    fi
    set -- --home "$config_home" configure-network \
        --language-model "$language_model" --reasoning-model "$reasoning_model"
    newline=$(printf '\nx')
    newline=${newline%x}
    old_ifs=$IFS
    IFS=$newline
    for assignment in $role_models; do
        [ -n "$assignment" ] && set -- "$@" --role "$assignment"
    done
    IFS=$old_ifs
    set -- "$@" --parallelism "$parallelism" --expert-parallelism "$expert_parallelism"
    "$prefix/bin/cogfabric" "$@"
}

register_model_if_exact() {
    config_home=$1
    model_manifest=$2
    registered_models=$("$prefix/bin/cogfabric" --home "$config_home" --json models)
    if printf '%s\n' "$registered_models" | jq -e --slurpfile expected "$model_manifest" \
        'any(.[]; .manifest.id == $expected[0].id and .manifest.version == $expected[0].version)' >/dev/null; then
        if ! printf '%s\n' "$registered_models" | jq -e --slurpfile expected "$model_manifest" \
            'any(.[]; .manifest == $expected[0])' >/dev/null; then
            echo "Refusing to replace a different immutable model manifest: $model_manifest" >&2
            return 1
        fi
        if ! printf '%s\n' "$registered_models" | jq -e --slurpfile expected "$model_manifest" \
            'any(.[]; .manifest == $expected[0] and .enabled == true)' >/dev/null; then
            "$prefix/bin/cogfabric" --home "$config_home" model-enable \
                "$(jq -r '.id' "$model_manifest")" "$(jq -r '.version' "$model_manifest")"
        else
            printf 'Keeping identical registered model %s@%s in %s\n' \
                "$(jq -r '.id' "$model_manifest")" "$(jq -r '.version' "$model_manifest")" "$config_home"
        fi
    else
        "$prefix/bin/cogfabric" --home "$config_home" model-register "$model_manifest"
    fi
}

prepare_domain_expert_model() {
    config_home=$1
    if [ -n "$domain_expert_manifest" ]; then
        expert_manifest_path=$domain_expert_manifest
        expert_skill_version=${domain_expert_workflow_version:-1.1.0}
    else
        expert_manifest_path="$source_dir/examples/qwen36_model_manifest.json"
        expert_skill_version=${domain_expert_workflow_version:-1.0.0}
    fi
    register_model_if_exact "$config_home" "$expert_manifest_path"
    expert_model_id=$(jq -er '.id' "$expert_manifest_path")
    expert_model_version=$(jq -er '.version' "$expert_manifest_path")
    registered_models=$("$prefix/bin/cogfabric" --home "$config_home" --json models)
    expert_registration=$(printf '%s\n' "$registered_models" | jq -cer \
        --arg id "$expert_model_id" --arg version "$expert_model_version" \
        '[.[] | select(.manifest.id == $id and .manifest.version == $version)] |
         if length == 1 and .[0].enabled == true then .[0].manifest
         else error("selected expert model is absent, ambiguous, or disabled") end') || {
        echo "The selected expert model is not enabled in $config_home." >&2
        exit 1
    }
    expert_model_key=$(printf '%s\n' "$expert_registration" | jq -er '.model')
    expert_model_api_base=$(printf '%s\n' "$expert_registration" | jq -er '.api_base')
    if [ "$expert_model_key" = "$language_model" ]; then
        echo 'The Jev+ expert model must differ from the language-only model.' >&2
        exit 1
    fi
    if [ "$expert_model_api_base" != "$api_base" ]; then
        echo "The selected expert model API base must exactly match $api_base." >&2
        exit 1
    fi
    if [ -n "$domain_expert_model_key" ] && [ "$domain_expert_model_key" != "$expert_model_key" ]; then
        echo 'The CLI and OpenCode homes resolve the expert registration to different model keys.' >&2
        exit 1
    fi
    domain_expert_model_key=$expert_model_key
}

if [ -n "$language_model" ]; then
    domain_expert_model_key=''
    if [ "$domain_experts" -eq 1 ]; then
        for config_home in "$cli_home" "$opencode_home"; do
            prepare_domain_expert_model "$config_home"
        done
    fi
    for config_home in "$cli_home" "$opencode_home"; do
        configure_network_for_home "$config_home"
        if [ "$jev_r_only" -ne 1 ]; then
            "$prefix/bin/cogfabric" --home "$config_home" configure-local \
                --model "$reasoning_model" \
                --api-base "$api_base" \
                --reasoning-effort default
            if [ "$domain_experts" -eq 1 ]; then
                for expert in \
                    'data.sql.s1:data_sql' \
                    'git.s1:git' \
                    'linux.s1:linux' \
                    'reverse.s1:reverse' \
                    'rust.s1:rust' \
                    'security.s1:security' \
                    'trainer.s1:trainer' \
                    'unity.s1:unity'; do
                    skill_id=${expert%%:*}
                    manifest_name=${expert#*:}
                    manifest_path="$source_dir/examples/${manifest_name}_qwen36_workflow_manifest.json"
                    if [ -n "$domain_expert_manifest" ]; then
                        expert_workflow_manifest="$workdir/${manifest_name}_workflow_manifest.json"
                        jq --arg model_id "$expert_model_id" \
                            --arg model_version "$expert_model_version" \
                            --arg workflow_version "$expert_skill_version" \
                            '.version = $workflow_version | .model = {"id":$model_id,"version":$model_version}' \
                            "$manifest_path" >"$expert_workflow_manifest"
                        manifest_to_register=$expert_workflow_manifest
                    else
                        manifest_to_register=$manifest_path
                    fi
                    "$prefix/bin/cogfabric" --home "$config_home" skill-register "$manifest_to_register"
                    "$prefix/bin/cogfabric" --home "$config_home" skill-activate "$skill_id" "$expert_skill_version"
                done
            fi
        fi
    done
    if [ "$native_on_demand" -eq 1 ]; then
        policy_tmp=$(mktemp "$opencode_home/.native-policy.XXXXXX") || {
            echo 'Could not create a private native policy draft.' >&2
            exit 1
        }
        set -- --api-base http://127.0.0.1:1234/api/v1 --context-length 16384 \
            --model-context "$language_model=$language_context"
        if [ -n "$domain_expert_model_key" ]; then
            set -- "$@" --extra-model "$domain_expert_model_key"
        fi
        if ! "$prefix/bin/cogfabric" --home "$opencode_home" --json native-policy-draft "$@" >"$policy_tmp"; then
            rm -f "$policy_tmp"
            echo 'Could not draft the on-demand policy; no provider operation was attempted.' >&2
            exit 1
        fi
        chmod 600 "$policy_tmp"
        if [ -e "$native_policy_file" ] || [ -L "$native_policy_file" ]; then
            rm -f "$policy_tmp"
            echo "Refusing to replace the existing native policy: $native_policy_file" >&2
            exit 1
        fi
        if ! ln "$policy_tmp" "$native_policy_file"; then
            rm -f "$policy_tmp"
            echo "Could not create the native policy without replacing an existing file: $native_policy_file" >&2
            exit 1
        fi
        rm -f "$policy_tmp"
        printf 'Saved unapproved on-demand policy draft: %s\n' "$native_policy_file"
    fi
    echo 'Network configuration saved. Models were not loaded and no inference was run.'
fi

helper_tmp=$(mktemp "$prefix/bin/.cogfabric-opencode.XXXXXX")
cp "$source_dir/scripts/opencode-local.sh" "$helper_tmp"
chmod 755 "$helper_tmp"
mv -f "$helper_tmp" "$prefix/bin/cogfabric-opencode"
helper_tmp=$(mktemp "$prefix/bin/.cogfabric-opencode-on-demand.XXXXXX")
cp "$source_dir/scripts/opencode-on-demand.sh" "$helper_tmp"
chmod 755 "$helper_tmp"
mv -f "$helper_tmp" "$prefix/bin/cogfabric-opencode-on-demand"
printf '\nInstalled commands:\n  %s\n  %s\n  %s\n  %s\n  %s\n' "$prefix/bin/cogfabric" "$prefix/bin/cogfabric-chat" "$prefix/bin/cogfabric-server" "$prefix/bin/cogfabric-opencode" "$prefix/bin/cogfabric-opencode-on-demand"
echo "Add $prefix/bin to PATH if it is not already present."
