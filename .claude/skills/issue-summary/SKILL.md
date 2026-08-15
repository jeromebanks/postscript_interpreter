---
name: issue-summary
description: Show a dashboard of recent GitHub issue state for this repo — in-progress, in-review, open-and-untouched, and recently closed, with PR links and CI/review status where applicable. Use whenever the user says "/issue-summary", "what's the state of the backlog", "what's in progress", "what issues are open", or wants a quick view of recent issue/PR activity without scrolling raw `gh issue list`/`gh pr list` output.
---

# issue-summary — dashboard of recent issue state

This is a thin wrapper: it runs `scripts/issue_summary.sh` and shows
its output verbatim. The script does all the actual `gh`/`jq` work
(fetching issues and PRs, matching PRs to issues via their `Closes
#N`/`Fixes #N`/`Resolves #N` body text, grouping by status) — there is
no reasoning to do here, and no reason to re-derive any of it by
hand. If the script's output looks wrong, fix the script; don't
paper over it by reasoning about the data in this skill instead.

## Run it

```sh
./scripts/issue_summary.sh
```

Pass `--closed N` to change how many recently-closed issues are shown
(default 10):

```sh
./scripts/issue_summary.sh --closed 20
```

Print the script's stdout to the user as-is (it's already formatted
markdown, grouped into In Progress / In Review / Open / Recently
Closed). Requires `gh` authenticated against this repo's remote.
