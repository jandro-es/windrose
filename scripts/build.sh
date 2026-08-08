#!/usr/bin/env bash
#
# build.sh — build a universal (Apple Silicon + Intel) release binary.
#
# What:  Builds windrose for aarch64-apple-darwin and x86_64-apple-darwin,
#        then joins them with lipo into a single binary at dist/windrose that
#        runs natively on both kinds of Mac.
#
# Why:   Windrose is for whatever Mac someone happens to own, including older
#        Intel machines where the answer is mostly "use a cloud service". One
#        binary means one download and no wrong-architecture mistakes.
#
# Note:  Shell scripts here are verified by running them, not by unit tests.
#        The Rust test suite covers the program; this covers the packaging.
#
# Usage: ./scripts/build.sh          build the universal binary
#        ./scripts/build.sh --help   show this message

set -euo pipefail

TARGETS=(aarch64-apple-darwin x86_64-apple-darwin)
BINARY=windrose

usage() {
    sed -n '2,/^$/p' "$0" | sed 's/^# \{0,1\}//'
}

for arg in "$@"; do
    case "$arg" in
        --help | -h)
            usage
            exit 0
            ;;
        *)
            echo "Unknown option: $arg" >&2
            echo "Try --help." >&2
            exit 2
            ;;
    esac
done

if [[ "$(uname -s)" != "Darwin" ]]; then
    echo "This builds a macOS universal binary and needs lipo, so it only runs on a Mac." >&2
    exit 1
fi

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

# Fail early with an actionable message rather than mid-build with a cryptic
# one. A missing target is the most common reason this script fails.
installed="$(rustup target list --installed)"
for target in "${TARGETS[@]}"; do
    if ! grep -qx "$target" <<< "$installed"; then
        echo "Missing Rust target: $target" >&2
        echo "Install it with: rustup target add $target" >&2
        exit 1
    fi
done

for target in "${TARGETS[@]}"; do
    echo "Building for ${target}…"
    cargo build --release --target "$target"
done

mkdir -p dist
inputs=()
for target in "${TARGETS[@]}"; do
    inputs+=("target/$target/release/$BINARY")
done

lipo -create -output "dist/$BINARY" "${inputs[@]}"

echo
echo "Built dist/$BINARY"
file "dist/$BINARY"
echo
echo "Size: $(du -h "dist/$BINARY" | cut -f1)"
