#!/usr/bin/env bash
#
# release.sh — run the gates, bump the version, and push the release.
#
# What:  Runs the same checks CI does (fmt, clippy, tests, man-page drift),
#        then bump-version.sh, then pushes the commit and the tag. Pushing the
#        tag is what starts the release pipeline on GitHub.
#
# Why:   Releasing is the one operation where a mistake is public and awkward
#        to undo. Running the gates first means a broken commit never gets a
#        tag, and doing it in one script means the steps cannot be reordered
#        or half-remembered.
#
# Note:  This pushes to the remote and starts a public release. It asks for
#        confirmation first, and --dry-run stops before anything leaves the
#        machine. Shell scripts here are verified by running them, not by
#        unit tests.
#
# Usage: ./scripts/release.sh patch             release a patch version
#        ./scripts/release.sh current           release the version as it
#                                               stands — for a first release
#        ./scripts/release.sh minor --dry-run   check and bump, but do not push
#        ./scripts/release.sh --help            show this message

set -euo pipefail

usage() {
    sed -n '2,/^$/p' "$0" | sed 's/^# \{0,1\}//'
}

part=""
dry_run=false

for arg in "$@"; do
    case "$arg" in
        --help | -h)
            usage
            exit 0
            ;;
        --dry-run)
            dry_run=true
            ;;
        major | minor | patch | current)
            part="$arg"
            ;;
        *)
            echo "Unknown argument: $arg" >&2
            echo "Try --help." >&2
            exit 2
            ;;
    esac
done

if [[ -z "$part" ]]; then
    usage >&2
    exit 2
fi

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

branch="$(git rev-parse --abbrev-ref HEAD)"
if [[ "$part" == "current" ]]; then
    echo "Releasing the current version from branch $branch."
else
    echo "Releasing a $part version from branch $branch."
fi
echo

echo "Checking formatting…"
cargo fmt --check

echo "Checking lints…"
cargo clippy --all-targets -- -D warnings

echo "Running tests…"
cargo test --quiet

echo "Checking the manual pages are current…"
./scripts/generate-man.sh --check

echo
echo "All checks passed."

./scripts/bump-version.sh "$part"

tag="$(git describe --tags --abbrev=0)"

if [[ "$dry_run" == true ]]; then
    echo
    echo "Dry run: stopping before the push."
    echo "$tag exists locally. Undo it with:"
    echo "  git tag -d $tag && git reset --hard HEAD~1"
    exit 0
fi

# Pushing the tag publishes a release, which is awkward to take back.
echo
read -r -p "Push $tag and publish the release? [y/N] " answer
case "${answer:-}" in
    y | Y | yes | Yes) ;;
    *)
        echo "Stopped. $tag exists locally but nothing was pushed."
        echo "Undo it with: git tag -d $tag && git reset --hard HEAD~1"
        exit 0
        ;;
esac

git push
git push --tags

echo
echo "Release $tag started — watch GitHub Actions."
