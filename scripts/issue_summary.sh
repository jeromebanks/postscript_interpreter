#!/usr/bin/env bash
# issue_summary.sh — script-backed dashboard of recent issue state:
# in-progress, in-review, open-and-untouched, and recently closed.
#
#   ./scripts/issue_summary.sh [--closed N]
#
# --closed N   how many most-recently-closed issues to show (default 10)
#
# This is the same in-progress/in-review/open split `work-issue`
# already re-derives ad hoc every run for its own picking/resuming
# logic (see .claude/skills/work-issue/SKILL.md step 1) — this gives a
# standalone, cheap view of it without spending model tokens on
# formatting/aggregation, which is pure `gh`/`jq` work with no
# judgment calls involved.
#
# Requires `gh` authenticated against this repo's remote, and `jq`.
set -euo pipefail

CLOSED_LIMIT=10
while [ $# -gt 0 ]; do
  case "$1" in
    --closed)
      if [ $# -lt 2 ]; then
        echo "usage: $0 [--closed N]" >&2
        exit 1
      fi
      CLOSED_LIMIT="$2"
      shift 2
      ;;
    *)
      echo "usage: $0 [--closed N]" >&2
      exit 1
      ;;
  esac
done

case "$CLOSED_LIMIT" in
  ''|*[!0-9]*)
    echo "error: --closed requires a positive integer, got '$CLOSED_LIMIT'" >&2
    exit 1
    ;;
esac

REPO=$(gh repo view --json nameWithOwner --jq .nameWithOwner)

OPEN_JSON=$(gh issue list --state open --limit 200 \
  --json number,title,url,updatedAt,labels)

PR_JSON=$(gh pr list --state open --limit 200 \
  --json number,url,title,body,updatedAt,reviewDecision,statusCheckRollup)

# `gh issue list --state closed` has no "sort by closedAt" of its own
# to page against, and any bounded "most recently updated" window can
# still miss a genuine recent closure that a heavily-commented older
# issue displaces — so this fetches the whole closed set (bounded at
# CLOSED_FETCH_CAP as a sanity ceiling, not a recency heuristic) and
# lets jq pick the true N most-recently-*closed* out of all of it.
CLOSED_FETCH_CAP=1000
CLOSED_JSON=$(gh issue list --state closed --limit "$CLOSED_FETCH_CAP" \
  --json number,title,url,closedAt,stateReason)

echo "# Issue summary — ${REPO}"
echo "_generated $(date -u +%Y-%m-%dT%H:%M:%SZ)_"
echo

jq -n -r \
  --argjson open "$OPEN_JSON" \
  --argjson prs "$PR_JSON" \
  --argjson closed "$CLOSED_JSON" \
  --argjson closed_limit "$CLOSED_LIMIT" \
'
# issue number -> {pr, url, ci, review}, from PRs whose body says
# Closes/Fixes/Resolves #N (same convention work-issue itself writes
# into every PR it opens).
def pr_lookup:
  [ $prs[] | . as $pr |
    ( $pr.body // "" | [scan("(?:Closes|Fixes|Resolves) #([0-9]+)"; "gi")]
      | map(.[0] | tonumber) ) as $nums |
    $nums[] | {
      key: (. | tostring),
      value: {
        pr: $pr.number,
        url: $pr.url,
        ci: (
          # statusCheckRollup mixes two GraphQL union members: modern
          # CheckRun entries report outcome via .conclusion, legacy
          # commit-status (StatusContext) entries via .state instead —
          # normalize to one field before judging pass/fail.
          ($pr.statusCheckRollup | map(.conclusion // .state)) as $outcomes |
          # Explicit success-like/failing-like sets, not "anything
          # other than SUCCESS counts as pending" — a completed but
          # SKIPPED/NEUTRAL CheckRun (conditional or continue-on-error
          # jobs) or a STALE one is a terminal result, not one still
          # queued or running, so it must land in one bucket or the
          # other, not fall through to pending by default.
          if ($outcomes | length) == 0 then "no checks"
          elif ($outcomes | any(. == "FAILURE" or . == "ERROR" or . == "CANCELLED" or . == "TIMED_OUT" or . == "STARTUP_FAILURE" or . == "ACTION_REQUIRED" or . == "STALE")) then "failing"
          elif ($outcomes | all(. == "SUCCESS" or . == "NEUTRAL" or . == "SKIPPED")) then "passing"
          else "pending" end
        ),
        review: (
          if $pr.reviewDecision == "APPROVED" then "approved"
          elif $pr.reviewDecision == "CHANGES_REQUESTED" then "changes requested"
          elif $pr.reviewDecision == "REVIEW_REQUIRED" then "review required"
          else "pending" end
        )
      }
    }
  ] | from_entries;

def has_label($l): [.labels[].name] | index($l) != null;

def fmt_date($d): $d[0:10];

def issue_line:
  "- #\(.number)  \(.title)  _(updated \(fmt_date(.updatedAt)))_";

def pr_suffix($lookup):
  ($lookup[.number | tostring]) as $p |
  if $p then "  \n  PR #\($p.pr) — CI: \($p.ci), review: \($p.review)  \($p.url)"
  else "" end;

(pr_lookup) as $lookup |

($open | map(select(has_label("in-progress")))) as $in_progress |
($open | map(select(has_label("in-review")))) as $in_review |
($open | map(select((has_label("in-progress") or has_label("in-review")) | not))) as $untouched |

"## In Progress (\($in_progress | length))",
"",
(if ($in_progress | length) == 0 then "_none_" else
  ($in_progress[] | issue_line + pr_suffix($lookup)) end),
"",
"## In Review (\($in_review | length))",
"",
(if ($in_review | length) == 0 then "_none_" else
  ($in_review[] | issue_line + pr_suffix($lookup)) end),
"",
"## Open (\($untouched | length))",
"",
(if ($untouched | length) == 0 then "_none_" else
  ($untouched[] | issue_line + pr_suffix($lookup)) end),
"",
($closed | sort_by(.closedAt) | reverse | .[0:$closed_limit]) as $recent |

"## Recently Closed (\($recent | length))",
"",
(if ($recent | length) == 0 then "_none_" else
  ($recent[] | "- #\(.number)  \(.title)  _(closed \(fmt_date(.closedAt)), \(.stateReason // "unknown" | ascii_downcase))_") end)
'
