#!/usr/bin/env bash
#
# bump-version.sh — raise the version, close the changelog entry, tag it.
#
# What:  Reads the current version from Cargo.toml, works out the next one,
#        rewrites Cargo.toml, refreshes Cargo.lock via `cargo check`,
#        regenerates the manual pages (they embed the version), moves
#        everything under CHANGELOG's [Unreleased] heading into a dated
#        release section, commits as `chore: release vX.Y.Z`, and creates the
#        annotated tag vX.Y.Z.
#
# Why:   The version lives in three places that must agree — Cargo.toml, the
#        changelog, and the git tag. Doing it by hand is how they drift apart,
#        and cargo-dist keys its whole release off the tag.
#
# Note:  This is local only. Nothing is pushed; see release.sh for that.
#        Shell scripts here are verified by running them, not by unit tests.
#
# Usage: ./scripts/bump-version.sh patch    0.1.0 -> 0.1.1
#        ./scripts/bump-version.sh minor    0.1.0 -> 0.2.0
#        ./scripts/bump-version.sh major    0.1.0 -> 1.0.0
#        ./scripts/bump-version.sh current  release 0.1.0 as it stands
#        ./scripts/bump-version.sh --help   show this message
#
#        "current" exists for a first release: the version in Cargo.toml has
#        never been published, so bumping it would skip it entirely and imply
#        an earlier release that was withdrawn.

set -euo pipefail

usage() {
    sed -n '2,/^$/p' "$0" | sed 's/^# \{0,1\}//'
}

if [[ $# -ne 1 ]]; then
    usage >&2
    exit 2
fi

case "$1" in
    --help | -h)
        usage
        exit 0
        ;;
    major | minor | patch | current)
        part="$1"
        ;;
    *)
        echo "Expected major, minor, patch or current — got \"$1\"." >&2
        echo "Try --help." >&2
        exit 2
        ;;
esac

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

# A dirty tree would sweep unrelated changes into the release commit, and the
# tag would then point at something nobody reviewed.
if ! git diff --quiet || ! git diff --cached --quiet; then
    echo "Working tree has uncommitted changes. Commit or stash them first." >&2
    git --no-pager status --short >&2
    exit 1
fi

# The first `version =` after [package] — a plain grep would find dependency
# versions further down the file.
current="$(awk '/^\[package\]/ { in_pkg = 1; next }
                /^\[/          { in_pkg = 0 }
                in_pkg && /^version *= *"/ {
                    match($0, /"[^"]+"/)
                    print substr($0, RSTART + 1, RLENGTH - 2)
                    exit
                }' Cargo.toml)"

if [[ ! "$current" =~ ^([0-9]+)\.([0-9]+)\.([0-9]+)$ ]]; then
    echo "Could not read a semantic version from Cargo.toml (found \"$current\")." >&2
    exit 1
fi
major="${BASH_REMATCH[1]}"
minor="${BASH_REMATCH[2]}"
patch="${BASH_REMATCH[3]}"

case "$part" in
    major) major=$((major + 1)); minor=0; patch=0 ;;
    minor) minor=$((minor + 1)); patch=0 ;;
    patch) patch=$((patch + 1)) ;;
    current) ;;  # release the version already in Cargo.toml
esac
next="$major.$minor.$patch"
tag="v$next"

if git rev-parse -q --verify "refs/tags/$tag" > /dev/null; then
    echo "Tag $tag already exists." >&2
    exit 1
fi

if [[ "$part" == "current" ]]; then
    echo "Releasing $next as it stands (no version change)"
else
    echo "Bumping $current -> $next"
fi

# Only the [package] version, for the same reason as the read above.
awk -v next_version="$next" '
    /^\[package\]/ { in_pkg = 1; print; next }
    /^\[/          { in_pkg = 0 }
    in_pkg && !done && /^version *= *"/ {
        sub(/"[^"]+"/, "\"" next_version "\"")
        done = 1
    }
    { print }
' Cargo.toml > Cargo.toml.tmp && mv Cargo.toml.tmp Cargo.toml

# Refreshes Cargo.lock so the release commit contains a matching lockfile.
cargo check --quiet

# The manual pages carry the version in their .TH header and VERSION section,
# so they go stale the moment Cargo.toml changes. Regenerating them here keeps
# the release commit self-consistent — without this, every release would fail
# the man-page drift check at the start of the *next* one.
cargo run --quiet -- gen-man man/

today="$(date +%F)"
awk -v version="$next" -v today="$today" '
    /^## \[Unreleased\]/ {
        print
        print ""
        print "## [" version "] - " today
        seen = 1
        next
    }
    { print }
    END {
        if (!seen) {
            print "Could not find an [Unreleased] heading in CHANGELOG.md" > "/dev/stderr"
            exit 1
        }
    }
' CHANGELOG.md > CHANGELOG.md.tmp && mv CHANGELOG.md.tmp CHANGELOG.md

git add Cargo.toml Cargo.lock CHANGELOG.md man/
git commit --quiet -m "chore: release $tag"
git tag --annotate "$tag" --message "$tag"

echo "Committed and tagged $tag."
echo "Nothing has been pushed. Use scripts/release.sh, or push yourself:"
echo "  git push && git push --tags"
