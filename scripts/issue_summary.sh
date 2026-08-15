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

# `gh issue list --state closed` has no "sort by closedAt" of its own,
# so over-fetch and let jq pick the N most-recently-*closed* below —
# sorting by anything else (e.g. last-updated) can let a stale issue
# that merely got a late comment displace a genuinely recent closure.
CLOSED_FETCH=$(( CLOSED_LIMIT > 60 ? CLOSED_LIMIT : 60 ))
CLOSED_JSON=$(gh issue list --state closed --limit "$CLOSED_FETCH" \
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
          if ($pr.statusCheckRollup | length) == 0 then "no checks"
          elif ($pr.statusCheckRollup | map(.conclusion) | any(. == "FAILURE" or . == "CANCELLED" or . == "TIMED_OUT" or . == "STARTUP_FAILURE" or . == "ACTION_REQUIRED")) then "failing"
          elif ($pr.statusCheckRollup | map(.conclusion) | all(. == "SUCCESS")) then "passing"
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
  ($untouched[] | issue_line) end),
"",
($closed | sort_by(.closedAt) | reverse | .[0:$closed_limit]) as $recent |

"## Recently Closed (\($recent | length))",
"",
(if ($recent | length) == 0 then "_none_" else
  ($recent[] | "- #\(.number)  \(.title)  _(closed \(fmt_date(.closedAt)), \(.stateReason // "unknown" | ascii_downcase))_") end)
'
