---
name: work-issue
description: Run this repo's simple issue -> feature branch -> PR SDLC (AGENTS.md's "Software development lifecycle" section) end to end for one GitHub issue -- pick or accept an issue, label it in-progress, create a feature branch and worktree, implement it, then open a PR and hand it back for human review. Use whenever the user says "work on issue N", "implement the next ticket/issue", "pick up the next backlog item", "/work-issue", or similar -- this is the standard way backlog issues in this repo get turned into PRs, not an ad hoc one-off.
---

# work-issue — one issue, start to finish

This is the repo's first-pass SDLC automation: take one open GitHub
issue from the backlog to an open PR, following AGENTS.md's
"Software development lifecycle" section exactly (issue -> feature
branch -> PR, quality bar before opening the PR, never merge). It's
deliberately simple — no batching, no config, no auto-merge. Expect to
revise this skill as real usage surfaces rough edges.

Argument: an optional issue number. `/work-issue 16` works that issue;
`/work-issue` with no argument picks the next un-implemented one.

## 1. Pick the issue

If an issue number was given, confirm it's open and not already being
worked:

```sh
gh issue view <N> --json state,title,labels
```

If it's closed, or already carries the `in-progress` label, stop and
tell the user rather than plowing ahead — that's a sign someone (or a
previous run of this skill) is already on it.

If no number was given, find the lowest-numbered open issue that
isn't already labeled `in-progress` and isn't already the target of an
open PR:

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
— that's the only thing that should close it. This skill never merges
and never asks to; per AGENTS.md, "opening the PR is the deliverable,"
full stop. Leave it for a human.

Then flip the issue's status and leave a trail back to the PR:

```sh
gh issue edit <N> --remove-label in-progress --add-label in-review
gh issue comment <N> --body "Opened <PR URL>."
```

Report back: issue number and title, branch name, worktree path, and
the PR URL. Mention that the worktree is left in place on purpose (for
review, follow-up commits, or CI-triggered fixes) — cleaning it up
with `git worktree remove <path>` is a manual step for once the PR is
actually merged, not something this skill does for you.

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
