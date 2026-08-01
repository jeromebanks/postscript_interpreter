---
name: work-issue
description: Run this repo's issue -> feature branch -> PR -> review -> merge -> cleanup SDLC (SDLC.md) end to end for one GitHub issue -- pick or accept an issue, label it in-progress, create a feature branch and worktree, implement it, open a PR, independently review it, and merge per SDLC.md's policy. Use whenever the user says "work on issue N", "implement the next ticket/issue", "pick up the next backlog item", "/work-issue", or similar -- this is the standard way backlog issues in this repo get turned into shipped code, not an ad hoc one-off.
---

# work-issue — one issue, start to finish

This is the repo's SDLC automation: take one open GitHub issue from the
backlog all the way to merged, following `SDLC.md`'s lifecycle exactly
(issue -> feature branch -> plan review -> implement -> quality gate ->
diff review -> PR -> independent review -> merge -> cleanup). Merge
authority is whatever `SDLC.md`'s `merge_policy` frontmatter currently
says — read it fresh at step 8, don't assume from memory, since it's
the kind of thing that gets changed by re-running `sdlcify`. Expect to
revise this skill as real usage surfaces rough edges.

Argument: an optional issue number. `/work-issue 16` works that issue;
`/work-issue` with no argument picks the next un-implemented one.

## 1. Pick the issue

If an issue number was given, confirm it's open:

```sh
gh issue view <N> --json state,title,labels
```

If it's closed, stop and tell the user. If it carries `in-progress`,
check whether there's already an open PR claiming it (`gh pr list
--state open --json body --jq` scanning for `Closes #<N>`/`Fixes
#<N>`/`Resolves #<N>`):
- **Open PR exists**: stop and tell the user — someone (or another run
  of this skill) is genuinely already on it.
- **No open PR**: this is a crashed or interrupted prior run, not a
  live claim — resume it rather than halting. Check for
  `../<repo>-issue-<N>` (step 3 already contemplates reusing an
  existing worktree); if it exists, pick up from wherever it left off
  (uncommitted changes, a branch with commits but no PR, etc. are all
  normal mid-run states, not corruption). This matters for the
  unattended-loop case specifically: a session crash shouldn't
  permanently deadlock the backlog on one issue.

If no number was given, **first check for an abandoned in-progress
issue to resume** — this takes priority over picking something fresh,
otherwise a crashed run's issue gets silently skipped forever instead
of finished:

```sh
# in-progress issues with no open PR claiming them = crashed prior runs
gh issue list --state open --label in-progress --json number,title
gh pr list --state open --json body --jq \
  '[.[].body | scan("(?:Closes|Fixes|Resolves) #([0-9]+)")[][0] | tonumber]'
```

If any `in-progress` issue's number isn't in the open-PR list, resume
the lowest-numbered one of those (same as the explicit-number path
above — reuse its worktree if one exists) instead of continuing to the
picker below.

Otherwise, find the lowest-numbered open issue that isn't already
labeled `in-progress` and isn't already the target of an open PR:

```sh
# candidates: open, not in-progress, lowest number first
gh issue list --state open --search "-label:in-progress" \
  --json number,title,labels --jq 'sort_by(.number)'

# cross-check: numbers already claimed by an open PR's "Closes #N"/"Fixes #N"
gh pr list --state open --json body --jq \
  '[.[].body | scan("(?:Closes|Fixes|Resolves) #([0-9]+)")[][0] | tonumber]'
```

Take the first candidate not in the open-PR list. If there are no
candidates at all, report that the backlog is clear (or everything
in-flight) instead of inventing work.

## 2. Mark it in-progress

Labels are the status signal this skill uses (not assignees — this
repo has one main contributor, and a label reads clearly in `gh issue
list` at a glance). Create the labels the first time they're needed;
`gh label create` errors if a label already exists, so don't let that
stop the run:

```sh
gh label create in-progress --color FBCA04 \
  --description "Actively being worked on" 2>/dev/null || true
gh label create in-review --color 0E8A16 \
  --description "Implementation done, PR open, awaiting human review" \
  2>/dev/null || true

gh issue edit <N> --add-label in-progress
```

## 3. Branch and worktree

Base the new branch on up-to-date `main` rather than whatever the
current worktree's local `main` happens to be at — the current
worktree may have uncommitted changes of its own, which is fine, since
`git worktree add` doesn't touch it.

```sh
git fetch origin main

REPO_DIR=$(basename "$(git rev-parse --show-toplevel)")
SLUG=<kebab-case slug of the issue title, ~40 chars, no leading/trailing hyphens>
PREFIX=feature   # or "fix" if the issue carries the `bug` label — AGENTS.md's convention
BRANCH="${PREFIX}/<N>-${SLUG}"
WORKTREE_DIR="../${REPO_DIR}-issue-<N>"

git worktree add -b "$BRANCH" "$WORKTREE_DIR" origin/main
```

Everything from here on — reading, editing, building, testing,
committing — happens *inside `$WORKTREE_DIR`*, not the original
directory. If a worktree for this issue already exists (a prior
attempt), reuse it rather than erroring or clobbering it — ask the
user if it looks stale.

## 4. Plan, and get the plan reviewed before touching code

Read the issue as the spec: `gh issue view <N>`. This repo's issues
are written to leave the implementation approach open ("left to the
implementer") — that's intentional, not a gap to fill by asking. Make
the design calls yourself, the way AGENTS.md already asks of any
nontrivial work here: "make judgment calls on genuine coin-flip
decisions and note them; don't stall waiting for input on things
you're equipped to decide." Do flag anything that's genuinely hard to
reverse.

Write the plan out — what you're going to change, which files, the
approach and why, and anything you're deliberately *not* doing — then
call the `advisor` tool. `advisor` forwards this whole session
(everything since you picked up the issue) to a stronger reviewer with
no other context, so the plan needs to actually be written down first;
don't call it with nothing but the issue text and your intentions
still in your head. This is the point in the loop where a bad
approach is cheapest to catch — before any code exists, not after.

Take the advisor's feedback seriously: if it flags a problem with the
approach, revise the plan (and call `advisor` again if the revision is
substantial) before writing any implementation code. Don't proceed to
step 5 on a plan the advisor pushed back on without addressing why.

## 5. Implement

Follow the conventions already documented for this repo while you
work — they're not restated here to avoid drifting out of sync with
the source of truth:

- AGENTS.md's "Code quality bar" (no `unwrap`/`expect` on
  program-input-derived data, comments explain why not what, tests
  live alongside the code they test).
- Commit at reasonable checkpoints, not one giant commit at the end.
- Update `NOTES.md`/`HANDOFF.md`/`README.md` on the branch as
  capabilities land, per AGENTS.md — not as an afterthought.

Before moving on, the quality bar from AGENTS.md must be clean *in the
worktree*:

```sh
cargo build && cargo test && cargo clippy --all-targets && cargo fmt --all -- --check
```

This is the same gate CI runs — clearing it locally first means the
PR isn't the first place a break shows up.

## 6. Get the implementation reviewed before marking done

Once the quality gate is clean, call `advisor` again — same tool, but
now it's reviewing the actual diff and decisions made while
implementing, not just the plan. This is the check before the issue
gets marked done, mirroring the plan review in step 4: catch problems
while they're still on a branch nobody's looked at yet, not after the
PR is open.

If the advisor surfaces something worth fixing, fix it, re-run the
quality gate, and use your judgment on whether it's worth one more
advisor pass — don't loop indefinitely chasing a clean bill of health
on genuinely subjective feedback.

## 7. Open the PR, update the issue

```sh
git push -u origin "$BRANCH"
gh pr create --title "<summary>" --body "$(cat <<'EOF'
## Summary
<what changed and why, in a few bullets>

Closes #<N>

## Test plan
- [x] cargo build && cargo test && cargo clippy --all-targets && cargo fmt --all -- --check
<+ whatever else was actually run/verified>
EOF
)"
```

`Closes #<N>` is what lets merging the PR close the issue automatically
— that's the only thing that should close it.

Flip the issue's status and leave a trail back to the PR:

```sh
gh issue edit <N> --remove-label in-progress --add-label in-review
gh issue comment <N> --body "Opened <PR URL>."
```

## 8. Independent review, then merge per `SDLC.md`'s policy

Read `SDLC.md`'s frontmatter fresh (`merge_policy.mode` and, if
`size-and-risk-bar`, `max_changed_lines`/`sensitive_paths`) — this is
config, not something to remember from a prior run or from having
written the skill.

Run an independent review of the PR — a reviewer with no context from
implementing it, cold on the diff. **Invoking the `review` skill
directly in this session doesn't satisfy that**: this session wrote
the code and carries the whole implementation history, so it isn't
independent no matter which skill it runs. Use the `Agent` tool
instead, with a self-contained prompt that gives it nothing but the PR
number and what to check (title/body via `gh pr view`, diff via `gh pr
diff` — the agent fetches these itself, it doesn't inherit this
session's view of them) — same spirit as the `review` skill's own
instructions, just run by a subagent with a blank slate rather than
this one. Must come back with nothing blocking to proceed; if it
flags something, fix it, re-run the quality gate, and get a fresh
independent pass on the updated diff before continuing.

If review is clean, decide merge eligibility from the policy:
- `human-only`: stop here. Report the PR as open and awaiting merge;
  don't attempt to merge it.
- `agent-full`: eligible regardless of diff size or files touched.
- `size-and-risk-bar`: eligible only if the diff (excluding doc-only
  files) is under `max_changed_lines` and touches none of
  `sensitive_paths`; otherwise stop here and say which condition it
  missed, same as the `human-only` case.

If eligible, merge:

```sh
gh pr view <PR#> --json mergeable,mergeStateStatus,statusCheckRollup
```

If `mergeStateStatus` is `BEHIND` (branch protection's `strict` status
check requires the tested commit to reflect current `main` — a PR
opened a while ago, or after another PR merged first, will hit this),
update and wait for CI before merging:

```sh
gh pr update-branch <PR#>
gh pr checks <PR#> --watch   # or poll; wait for the re-triggered run
```

Then merge using the shape recorded in `SDLC.md`'s `merge_button`
block (this repo: squash, delete-branch-on-merge):

```sh
gh pr merge <PR#> --squash --delete-branch
```

If CI fails on the updated branch, don't force past it — treat that
as a blocker the same as a failed review, and report it rather than
merging anyway.

## 9. Post-merge cleanup

```sh
gh issue view <N> --json state   # confirm Closes #<N> auto-closed it
git worktree remove "$WORKTREE_DIR" 2>&1 || true
git -C "$(git rev-parse --show-toplevel)" branch -d "$BRANCH" 2>&1 || true
git -C "$(git rev-parse --show-toplevel)" fetch origin --prune
```

`git worktree remove` can fail with "branch used by worktree" if run
in the wrong order — remove the worktree *before* trying to delete the
local branch, not after, or the branch delete will block on it.

Report back: issue number and title, branch name, PR URL and its merge
commit, and confirmation the issue closed and the worktree/branch were
cleaned up. If step 8 stopped short of merging (`human-only`, or over
the size/risk bar), report that explicitly instead — branch name,
worktree path, and the PR URL, with the worktree left in place since
there may be follow-up commits before a human merges it.

## Pitfalls

- **Don't work in the main checkout.** The whole point of the worktree
  is that the branch's changes are physically separate from wherever
  this skill was invoked from. If you catch yourself editing files
  outside `$WORKTREE_DIR`, stop.
- **Shell cwd can reset to the invoking directory between tool calls**
  (confirmed in the first real run of this skill — it happened twice
  in one session). Don't rely on an earlier `cd` into `$WORKTREE_DIR`
  still holding; either prefix every shell command with `cd
  "$WORKTREE_DIR" &&` or use absolute paths throughout, for cargo
  invocations *and* for any PostScript that does `(lib/artkit.ps) run`
  or similar relative-path loads. A cwd reset there fails silently —
  it just quietly loads `main`'s copy of the file instead of the
  worktree's — which is a much worse failure mode than an error.
- **An issue with no clear implementer scope is normal, not a bug.**
  This repo's issue-writing convention deliberately omits
  implementation detail (see step 4) — that's not a sign the issue is
  underspecified.
- **Both advisor calls (steps 4 and 6) need something written down
  first.** `advisor` reviews the session transcript, not your
  intentions — if the plan or the summary of changes never got put
  into words, there's nothing substantive for it to react to.
- **Label creation is idempotent by construction** (`|| true`) — don't
  let "label already exists" abort the run.
- **If `cargo fmt --all -- --check` or clippy fails**, fix it before
  opening the PR — don't open a PR you know CI will reject.
- **A PR opened a while ago (or after another PR merged first) will
  likely show `mergeStateStatus: BEHIND`** at step 8, because branch
  protection's `strict` status check requires the tested commit to
  reflect current `main` — the CI run from when the PR was opened no
  longer counts. This is normal, not a failure: `gh pr update-branch`,
  wait for CI to re-run on the updated branch, then merge. Confirmed
  hitting this on the very first agent-merged PR in this repo (#22).
- **Remove the worktree before deleting the local branch, not after**
  (step 9's order matters) — `git branch -d` fails with "branch used
  by worktree" if the worktree still references it. `git worktree
  remove` first, then the branch delete, then `fetch --prune` to clear
  the now-dead remote-tracking ref.
