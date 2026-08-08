#!/usr/bin/env bash
#
# generate-man.sh — regenerate the manual pages in man/ and check for drift.
#
# What:  Runs `windrose gen-man man/`, which writes windrose.1 plus one page
#        per visible subcommand, then reports whether the committed pages
#        differ from what the current code produces.
#
# Why:   The pages are generated from the clap definitions in src/cli.rs, so
#        they go stale the moment a flag or description changes. Committing
#        them keeps them available to packagers without a build step, and this
#        check is what stops the committed copies drifting out of date. CI runs
#        it for exactly that reason.
#
# Usage: ./scripts/generate-man.sh          regenerate and report drift
#        ./scripts/generate-man.sh --check  fail (exit 1) if pages are stale
#        ./scripts/generate-man.sh --help   show this message

set -euo pipefail

usage() {
    sed -n '2,/^$/p' "$0" | sed 's/^# \{0,1\}//'
}

check_only=false
for arg in "$@"; do
    case "$arg" in
        --help | -h)
            usage
            exit 0
            ;;
        --check)
            check_only=true
            ;;
        *)
            echo "Unknown option: $arg" >&2
            echo "Try --help." >&2
            exit 2
            ;;
    esac
done

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

cargo run --quiet -- gen-man man/

# Outside a git repository there is nothing to compare against, and that is
# not a failure — the pages have still been generated.
if ! git rev-parse --git-dir > /dev/null 2>&1; then
    echo "Man pages written to man/ (not a git repository, so no drift check)."
    exit 0
fi

if git diff --quiet -- man/ && [ -z "$(git ls-files --others --exclude-standard man/)" ]; then
    echo "Man pages are up to date."
    exit 0
fi

if [ "$check_only" = true ]; then
    echo "Man pages are out of date. Run ./scripts/generate-man.sh and commit man/." >&2
    git --no-pager diff --stat -- man/ >&2
    exit 1
fi

echo "Man pages updated — commit them."
git --no-pager diff --stat -- man/
