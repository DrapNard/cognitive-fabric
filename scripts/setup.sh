#!/bin/sh
# Install Cognitive Fabric from a selected source revision without sudo.
set -eu

repository_url='https://github.com/DrapNard/cognitive-fabric.git'
default_ref='develop/v1.0'
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
parallelism=2
expert_parallelism=2

usage() {
    cat <<'USAGE'
Usage: setup.sh [--prefix PATH] [--ref GIT_REF_OR_COMMIT] [--replace] [--nvml-gpu]
                [--language-model MODEL --reasoning-model MODEL | --jev-r-only]
                [--role-model ROLE=MODEL ...]
                [--api-base URL] [--domain-experts]
                [--parallelism 1..4] [--expert-parallelism 1..4]

Build and install cogfabric, cogfabric-chat, and cogfabric-server from source.
Also install the cogfabric-opencode startup helper.
The default prefix is $HOME/.local and the default source ref is develop/v1.0.
--replace allows replacing existing binaries with the same package version.
--nvml-gpu builds the CLI and HTTP server with optional NVML observation support.
Supplying --language-model with --reasoning-model configures Qwen as a
language-only route and shares the selected reasoning backbone across Jev-R
roles in the CLI and OpenCode state directories. --domain-experts additionally
registers the included Qwen3.6-backed workflow manifests; it requires the
repository's qwen3.6-35b-a3b model key and the standard LM Studio API base.
Repeat --role-model ROLE=MODEL to route planner, solver, explorer, critic or
reflector to a specific model; unassigned roles use --reasoning-model.
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

if [ "$jev_r_only" -eq 1 ] || [ -n "$language_model" ] || [ -n "$reasoning_model" ] || [ "$domain_experts" -eq 1 ] || [ "$role_model_count" -gt 0 ]; then
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
    if [ "$domain_experts" -eq 1 ] && { [ "$jev_r_only" -eq 1 ] || [ "$reasoning_model" != 'qwen3.6-35b-a3b' ] || [ "$api_base" != 'http://127.0.0.1:1234/v1' ]; }; then
        echo '--domain-experts requires model-assisted routing with qwen3.6-35b-a3b at http://127.0.0.1:1234/v1.' >&2
        exit 2
    fi
    case $parallelism in 1|2|3|4) ;; *) echo '--parallelism must be 1, 2, 3, or 4.' >&2; exit 2 ;; esac
    case $expert_parallelism in 1|2|3|4) ;; *) echo '--expert-parallelism must be 1, 2, 3, or 4.' >&2; exit 2 ;; esac
fi

for program in git cargo rustc; do
    command -v "$program" >/dev/null 2>&1 || {
        echo "Required program not found: $program" >&2
        echo 'Install Git and the Rust toolchain before running this setup script.' >&2
        exit 1
    }
done

# Keep git's ref argument from being interpreted as an option or option bundle.
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

case $prefix in
    ''|-*)
        echo 'Use a non-empty prefix path that does not begin with a dash.' >&2
        exit 2
        ;;
esac

if [ ! -d "$prefix" ]; then
    mkdir -p "$prefix"
fi
prefix=$(CDPATH= cd "$prefix" && pwd)
mkdir -p "$prefix/bin"

if [ "$replace" -ne 1 ]; then
    for binary in cogfabric cogfabric-chat cogfabric-server cogfabric-opencode; do
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

source_dir=$workdir/source
mkdir "$source_dir"
git -C "$source_dir" init --quiet
git -C "$source_dir" remote add origin "$repository_url"
echo "Fetching Cognitive Fabric source ref: $ref"
git -C "$source_dir" fetch --quiet --depth 1 origin "$ref"
git -C "$source_dir" checkout --quiet --detach FETCH_HEAD
printf 'Building source commit: '
git -C "$source_dir" rev-parse --short=12 HEAD
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

if [ -n "$language_model" ]; then
    data_home=${XDG_DATA_HOME:-${HOME:?HOME must be set}/.local/share}
    cli_home=${COGFABRIC_CLI_HOME:-$data_home/cogfabric-rust}
    opencode_home=${COGFABRIC_HOME:-$data_home/cogfabric-http}
    for config_home in "$cli_home" "$opencode_home"; do
        configure_network_for_home "$config_home"
        if [ "$jev_r_only" -ne 1 ]; then
            "$prefix/bin/cogfabric" --home "$config_home" configure-local \
                --model "$reasoning_model" \
                --api-base "$api_base" \
                --reasoning-effort default
            if [ "$domain_experts" -eq 1 ]; then
                "$prefix/bin/cogfabric" --home "$config_home" model-register \
                    "$source_dir/examples/qwen36_model_manifest.json"
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
                    "$prefix/bin/cogfabric" --home "$config_home" skill-register "$manifest_path"
                    "$prefix/bin/cogfabric" --home "$config_home" skill-activate "$skill_id" 1.0.0
                done
            fi
        fi
    done
    echo 'Network configuration saved. Models were not loaded and no inference was run.'
fi

helper_tmp=$(mktemp "$prefix/bin/.cogfabric-opencode.XXXXXX")
cp "$source_dir/scripts/opencode-local.sh" "$helper_tmp"
chmod 755 "$helper_tmp"
mv -f "$helper_tmp" "$prefix/bin/cogfabric-opencode"
printf '\nInstalled commands:\n  %s\n  %s\n  %s\n  %s\n' "$prefix/bin/cogfabric" "$prefix/bin/cogfabric-chat" "$prefix/bin/cogfabric-server" "$prefix/bin/cogfabric-opencode"
echo "Add $prefix/bin to PATH if it is not already present."
