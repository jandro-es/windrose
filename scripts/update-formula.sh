#!/usr/bin/env bash
#
# update-formula.sh — render the Homebrew formula for a released version.
#
# What:  Fills packaging/homebrew/windrose.rb.template with a version and the
#        SHA-256 checksums of that release's two tarballs, writing
#        packaging/homebrew/windrose.rb. Copy that file into the tap repo.
#
# Why:   The formula is maintained by hand because cargo-dist's generated one
#        does not install man pages, so `man windrose` would not work after
#        `brew install`. Hand-maintained must not mean hand-edited: getting a
#        checksum wrong breaks every install with an error that looks like a
#        corrupted download, so the checksums are computed, never typed.
#
# Note:  Verified by running it. --local renders from the tarballs that
#        `dist build` leaves in target/distrib, which is how to test a formula
#        before publishing anything.
#
# Usage: ./scripts/update-formula.sh v0.1.0            from the published release
#        ./scripts/update-formula.sh v0.1.0 --local    from target/distrib
#        ./scripts/update-formula.sh --help

set -euo pipefail

REPO="jandro-es/windrose"
ARM_ARTIFACT="windrose-aarch64-apple-darwin.tar.xz"
INTEL_ARTIFACT="windrose-x86_64-apple-darwin.tar.xz"

usage() {
    sed -n '2,/^$/p' "$0" | sed 's/^# \{0,1\}//'
}

tag=""
local_mode=false

for arg in "$@"; do
    case "$arg" in
        --help | -h)
            usage
            exit 0
            ;;
        --local)
            local_mode=true
            ;;
        v*)
            tag="$arg"
            ;;
        *)
            echo "Unknown argument: $arg" >&2
            echo "Try --help." >&2
            exit 2
            ;;
    esac
done

if [[ -z "$tag" ]]; then
    usage >&2
    exit 2
fi

version="${tag#v}"
if [[ ! "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    echo "Expected a tag like v1.2.3 — got \"$tag\"." >&2
    exit 2
fi

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

template="packaging/homebrew/windrose.rb.template"
output="packaging/homebrew/windrose.rb"

if [[ ! -f "$template" ]]; then
    echo "Missing template: $template" >&2
    exit 1
fi

# Prints the SHA-256 of one artifact, either from the local build or from the
# published release.
checksum_of() {
    artifact="$1"

    if [[ "$local_mode" == true ]]; then
        path="target/distrib/$artifact"
        if [[ ! -f "$path" ]]; then
            echo "Missing $path — run 'dist build --artifacts=local' first." >&2
            exit 1
        fi
        shasum -a 256 "$path" | cut -d' ' -f1
        return
    fi

    url="https://github.com/$REPO/releases/download/$tag/$artifact"
    tmp="$(mktemp)"
    # --fail so a 404 stops here rather than hashing an HTML error page and
    # producing a formula that fails for every user instead.
    if ! curl --proto '=https' --tlsv1.2 --fail --silent --location \
        --output "$tmp" "$url"; then
        rm -f "$tmp"
        echo "Could not download $url" >&2
        echo "Is $tag published, and did its release finish building?" >&2
        exit 1
    fi
    shasum -a 256 "$tmp" | cut -d' ' -f1
    rm -f "$tmp"
}

echo "Rendering the formula for $tag"
if [[ "$local_mode" == true ]]; then
    echo "  (checksums from target/distrib — for testing, not for the tap)"
fi

sha_arm="$(checksum_of "$ARM_ARTIFACT")"
sha_intel="$(checksum_of "$INTEL_ARTIFACT")"

echo "  Apple Silicon: $sha_arm"
echo "  Intel:         $sha_intel"

sed -e "s|@VERSION@|$version|g" \
    -e "s|@SHA_ARM@|$sha_arm|g" \
    -e "s|@SHA_INTEL@|$sha_intel|g" \
    "$template" > "$output"

# A leftover placeholder means the template gained one this script does not
# know about, which would ship a formula that cannot install.
if grep -q '@[A-Z_]*@' "$output"; then
    echo "Unfilled placeholder left in $output:" >&2
    grep -n '@[A-Z_]*@' "$output" >&2
    rm -f "$output"
    exit 1
fi

echo
echo "Wrote $output"
echo
echo "Next:"
# `brew audit <path>` and `brew install <path>` are both disabled in current
# Homebrew — formulae must live in a tap. `brew style` still takes a path.
echo "  1. Check it:   brew style $output"
echo "  2. Publish it: copy to the tap repo as Formula/windrose.rb, then"
echo "                 git commit -m \"windrose $version\" && git push"
echo "  3. Try it:     brew install jandro-es/tap/windrose && man windrose"
