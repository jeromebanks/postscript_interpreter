#!/usr/bin/env bash
# gh_comment_upsert.sh — post or update a PR/issue comment keyed by an
# HTML-comment marker, instead of `gh pr comment --edit-last`.
#
# `--edit-last` scopes to the authenticated user's most recent comment
# with no content awareness. When more than one CI job posts as
# github-actions[bot] on the same PR (e.g. `test` and
# `perf-regression`), each job's `--edit-last` grabs whichever comment
# the *other* job posted most recently and overwrites it — the two
# jobs fight over one comment instead of each keeping its own updated
# in place. A marker-based lookup avoids that: each caller owns the
# one comment that starts with its marker AND was posted by
# github-actions[bot] — the author check matters because comments are
# public, so a human comment that happens to start with the same
# marker text must not be matched (and silently overwritten on the
# next run) or mistaken for this job's own comment.
#
#   ./scripts/gh_comment_upsert.sh <marker> <number> <body-file>
#
# <marker> should be a distinct HTML comment, e.g. '<!-- perf-regression -->'.
# <number> is a PR or issue number (same endpoint either way).
# Requires GH_TOKEN and GITHUB_REPOSITORY in the environment (both set
# automatically inside GitHub Actions).
set -euo pipefail

MARKER="$1"
NUMBER="$2"
BODY_FILE="$3"

TMP=$(mktemp)
trap 'rm -f "$TMP"' EXIT
{
  printf '%s\n\n' "$MARKER"
  cat "$BODY_FILE"
} >"$TMP"

id=$(gh api "repos/${GITHUB_REPOSITORY}/issues/${NUMBER}/comments" --paginate \
  --jq "[.[] | select(.user.login == \"github-actions[bot]\" and (.body | startswith(\"$MARKER\")))] | last | .id // empty")

if [ -n "$id" ]; then
  gh api --method PATCH "repos/${GITHUB_REPOSITORY}/issues/comments/${id}" -F body=@"$TMP"
else
  gh api --method POST "repos/${GITHUB_REPOSITORY}/issues/${NUMBER}/comments" -F body=@"$TMP"
fi
