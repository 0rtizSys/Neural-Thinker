#!/bin/sh
# Fails if anything from the private (paid services) code could reach the
# public repository. Run by the pre-commit and pre-push hooks and by CI.
#
#   sh scripts/check-private.sh            check the index (what would be committed)
#   sh scripts/check-private.sh --history  also check every commit reachable from any ref
#
# Private files carry this marker so that a copy placed anywhere else in the
# tree is caught too. It is split here so this script does not match itself.
MARKER="NT-PRIVATE-""DO-NOT-PUBLISH"
PRIVATE_DIR="src/private"
# Files allowed to mention the marker: this script and the docs that explain it.
EXCLUDES=":(exclude)scripts/check-private.sh :(exclude)docs/PRIVATE_SERVICES.md"

cd "$(git rev-parse --show-toplevel)" || exit 2
status=0

fail() {
    echo "check-private: $1" >&2
    status=1
}

# 1. The private folder must stay ignored.
if ! git check-ignore --no-index -q "$PRIVATE_DIR/probe.rs"; then
    fail "$PRIVATE_DIR/ is not ignored; restore the rule in .gitignore"
fi

# 2. Nothing under the private folder may be tracked or staged.
tracked=$(git ls-files --cached -- "$PRIVATE_DIR")
if [ -n "$tracked" ]; then
    fail "private files are tracked or staged (run: git rm -r --cached $PRIVATE_DIR):"
    echo "$tracked" | sed 's/^/    /' >&2
fi

# 3. No tracked file may carry the private marker.
# shellcheck disable=SC2086
marked=$(git grep --cached -l -I -F "$MARKER" -- . $EXCLUDES)
if [ -n "$marked" ]; then
    fail "files containing the private marker are tracked or staged:"
    echo "$marked" | sed 's/^/    /' >&2
fi

# 4. Public code must not depend on the private code.
# shellcheck disable=SC2086
refs=$(git grep --cached -n -I -E 'mod +private|path *= *"[^"]*private|neural[-_]thinker[-_](private|pro)' -- '*.rs' '*.toml' $EXCLUDES)
if [ -n "$refs" ]; then
    fail "public code refers to the private crate:"
    echo "$refs" | sed 's/^/    /' >&2
fi

if [ "$1" = "--history" ]; then
    # 5. No commit on any ref may ever have added a private path or the marker.
    in_history=$(git log --all --format='%h' --name-only -- "$PRIVATE_DIR" | grep -v '^$')
    if [ -n "$in_history" ]; then
        fail "private paths exist in git history; they must be purged before pushing:"
        echo "$in_history" | sed 's/^/    /' >&2
    fi
    # shellcheck disable=SC2086
    marked_history=$(git log --all --format='%h %s' -S "$MARKER" -- . $EXCLUDES)
    if [ -n "$marked_history" ]; then
        fail "commits in history add or remove the private marker:"
        echo "$marked_history" | sed 's/^/    /' >&2
    fi
fi

if [ "$status" -eq 0 ]; then
    echo "check-private: OK"
fi
exit "$status"
