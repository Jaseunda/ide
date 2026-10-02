#!/usr/bin/env bash
# Computes the IDE version as MAJOR.MINOR.PATCH:
#   MAJOR — manual only (edit the VERSION file at the repo root)
#   MINOR — auto-increments when the ide repo or the partial submodule changed
#           since the previous invocation
#   PATCH — auto-increments on every invocation
#
# Usage:
#   bump-version.sh build       print the version for a build (state only)
#   bump-version.sh release     print the version and persist it to VERSION
#   bump-version.sh changelog <version>
#                               append a release entry to CHANGELOG.md
#
# The version is printed on stdout; progress goes to stderr. The build counter
# lives in .version-state at the repo root, which is not committed.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
VERSION_FILE="$REPO_ROOT/VERSION"
STATE_FILE="$REPO_ROOT/.version-state"
CHANGELOG_FILE="$REPO_ROOT/CHANGELOG.md"

main_sha="$(git -C "$REPO_ROOT" rev-parse HEAD)"
partial_sha="$(git -C "$REPO_ROOT/partial" rev-parse HEAD)"

state_mm=""
state_main_sha=""
state_partial_sha=""
state_patch=""
if [ -f "$STATE_FILE" ]; then
    # shellcheck disable=SC1090
    source "$STATE_FILE"
fi

version="$(cat "$VERSION_FILE" 2>/dev/null || echo "0.1.0")"
version="${version//$'\r'/}"
version="$(echo "$version" | tr -d '[:space:]')"
major="${version%%.*}"
rest="${version#*.}"
minor="${rest%%.*}"

if [ -z "${state_mm:-}" ] || [ "$major.$minor" != "$state_mm" ]; then
    # The VERSION file was edited by hand; take its numbers and reset the patch.
    patch=0
elif [ "$main_sha" != "${state_main_sha:-}" ] || [ "$partial_sha" != "${state_partial_sha:-}" ]; then
    minor=$((minor + 1))
    patch=0
else
    patch=$(( ${state_patch:--1} + 1 ))
fi

next_version="$major.$minor.$patch"

state_tmp="$(mktemp "$REPO_ROOT/.version-state.XXXXXX")"
{
    echo "state_mm=\"$major.$minor\""
    echo "state_main_sha=\"$main_sha\""
    echo "state_partial_sha=\"$partial_sha\""
    echo "state_patch=\"$patch\""
} > "$state_tmp"
mv "$state_tmp" "$STATE_FILE"

case "${1:-}" in
    build)
        echo "$next_version"
        ;;
    release)
        printf '%s\n' "$next_version" > "$VERSION_FILE"
        echo "$next_version"
        ;;
    changelog)
        release_version="${2:-}"
        if [ -z "$release_version" ]; then
            echo "usage: bump-version.sh changelog <version>" >&2
            exit 1
        fi
        range="$(git -C "$REPO_ROOT" describe --tags --abbrev=0 2>/dev/null || echo "90731bdec0")"
        entry="$(mktemp "$REPO_ROOT/CHANGELOG.md.XXXXXX")"
        {
            printf '## %s (%s)\n\n' "$release_version" "$(date +%Y-%m-%d)"
            git -C "$REPO_ROOT" log --pretty=format:'- %h %s' "$range..HEAD"
            printf '\n\n'
            if [ -f "$CHANGELOG_FILE" ]; then
                cat "$CHANGELOG_FILE"
            fi
        } > "$entry"
        mv "$entry" "$CHANGELOG_FILE"
        ;;
    *)
        echo "usage: bump-version.sh build|release|changelog <version>" >&2
        exit 1
        ;;
esac
