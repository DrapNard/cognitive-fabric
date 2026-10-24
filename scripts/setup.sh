#!/bin/sh
# Install Cognitive Fabric from a selected source revision without sudo.
set -eu

repository_url='https://github.com/DrapNard/cognitive-fabric.git'
default_ref='develop/v1.0'
prefix=${HOME:?HOME must be set}/.local
ref=$default_ref
replace=0
nvml_gpu=0

usage() {
    cat <<'USAGE'
Usage: setup.sh [--prefix PATH] [--ref GIT_REF_OR_COMMIT] [--replace] [--nvml-gpu]

Build and install cogfabric, cogfabric-chat, and cogfabric-server from source.
Also install the cogfabric-opencode startup helper.
The default prefix is $HOME/.local and the default source ref is develop/v1.0.
--replace allows replacing existing binaries with the same package version.
--nvml-gpu builds the core CLI with the optional NVML observation feature.
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
if [ "$replace" -eq 1 ]; then
    cargo install --locked --path "$source_dir/crates/cogfabric-server" --root "$prefix" --force
else
    cargo install --locked --path "$source_dir/crates/cogfabric-server" --root "$prefix"
fi

helper_tmp=$(mktemp "$prefix/bin/.cogfabric-opencode.XXXXXX")
cp "$source_dir/scripts/opencode-local.sh" "$helper_tmp"
chmod 755 "$helper_tmp"
mv -f "$helper_tmp" "$prefix/bin/cogfabric-opencode"
printf '\nInstalled commands:\n  %s\n  %s\n  %s\n  %s\n' "$prefix/bin/cogfabric" "$prefix/bin/cogfabric-chat" "$prefix/bin/cogfabric-server" "$prefix/bin/cogfabric-opencode"
echo "Add $prefix/bin to PATH if it is not already present."
