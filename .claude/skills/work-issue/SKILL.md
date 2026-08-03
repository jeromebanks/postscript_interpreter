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
- **No open PR**: don't assume crashed — an in-progress issue with no
  PR yet is *also* the normal state of a session still mid-run (there
  is no PR until step 7). Run the **live-run check** below before
  resuming. If it comes back stale, this is a crashed or interrupted
  prior run — resume it rather than halting. Check for
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

If any `in-progress` issue's number isn't in the open-PR list, run the
same live-run check as the explicit-number path above before treating
it as resumable, then resume the lowest-numbered one that comes back
stale (reuse its worktree if one exists) instead of continuing to the
picker below. If every candidate's heartbeat is still live, fall
through to the picker below rather than resuming a running session.

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

### Live-run check (heartbeat)

An `in-progress` issue with no open PR is ambiguous on its own — it's
both the normal state of a session still mid-run *and* what a crashed
session leaves behind. Steps 3, 4, 5, 6, and 8 each refresh a
heartbeat timestamp in the worktree at their checkpoints; use it here
to tell the two apart before resuming, rather than inferring
abandonment purely from "no PR yet" (which used to misclassify a live
run — see #29):

```sh
HEARTBEAT="$(git -C "../<repo>-issue-<N>" rev-parse --git-dir 2>/dev/null)/work-issue-heartbeat"
if [ -f "$HEARTBEAT" ]; then
  AGE=$(( $(date +%s) - $(cat "$HEARTBEAT") ))
else
  AGE=999999   # no heartbeat: worktree never created, or crashed before step 3's first write
fi
```

- `AGE` under 2700 (45 min): still live. **Stop — don't resume.**
  Report to the user that issue #<N> looks actively worked on
  (heartbeat `$((AGE / 60))` min old) instead of silently racing a
  live session's edits.
- `AGE` at or above 2700, or no heartbeat file at all: stale — safe to
  resume.

45 minutes is deliberately generous, not tight: PR #30 needed six
Codex review rounds in one legitimate run (~62 min total, each round
itself several minutes) — a false "still live" fully blocks resume,
which is worse than a slower crash-detection window.

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

HEARTBEAT="$(git -C "$WORKTREE_DIR" rev-parse --git-dir)/work-issue-heartbeat"
date +%s > "$HEARTBEAT"
```

The heartbeat file lives in the worktree's git-dir (under the main
repo's `.git/worktrees/<name>/`), not the tracked working tree — it
never needs a `.gitignore` entry and never shows up in `git status` or
a diff. Refresh it at the end of steps 4, 5, 6, and around each Codex
review round in step 8, so the live-run check above can tell a session
still working from one that crashed. Each Bash tool call is a fresh
shell — a `$HEARTBEAT` variable set here won't survive to a later
step's tool call — so those refreshes re-derive the path inline rather
than trusting a remembered variable:

```sh
date +%s > "$(git -C "$WORKTREE_DIR" rev-parse --git-dir)/work-issue-heartbeat"
```

(same reasoning already applies to `$WORKTREE_DIR`/`$BRANCH` reused
across steps 7-9 below — re-emit the assignment in each new Bash block
rather than assuming it's still set.)

Everything from here on — reading, editing, building, testing,
committing — happens *inside `$WORKTREE_DIR`*, not the original
directory. If a worktree for this issue already exists (a prior
attempt — the live-run check above already confirmed it's stale),
reuse it rather than erroring or clobbering it: set `WORKTREE_DIR` to
the existing path, and read `BRANCH` from what's actually checked out
there —

```sh
BRANCH="$(git -C "$WORKTREE_DIR" branch --show-current)"
HEARTBEAT="$(git -C "$WORKTREE_DIR" rev-parse --git-dir)/work-issue-heartbeat"
date +%s > "$HEARTBEAT"
```

— instead of recomputing a fresh `SLUG`/`BRANCH` from the issue title.
A freshly regenerated slug can differ from a prior run's (wording
tweak, truncation difference) even though the worktree itself is being
correctly reused, and every later branch assertion/push/cleanup in
this skill trusts `$BRANCH` — pointing it at a name that doesn't match
what's actually checked out silently targets the wrong branch instead
of erroring (#29).

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

Refresh the heartbeat before moving on (re-derive the path, per step
3's note): `date +%s > "$(git -C "$WORKTREE_DIR" rev-parse --git-dir)/work-issue-heartbeat"`.

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

Refresh the heartbeat: `date +%s > "$(git -C "$WORKTREE_DIR" rev-parse --git-dir)/work-issue-heartbeat"`.

## 6. Get the implementation reviewed before marking done

Once the quality gate is clean, call `advisor` again — same tool, but
now it's reviewing the actual diff and decisions made while
implementing, not just the plan. This is the check before the issue
gets marked done, mirroring the plan review in step 4: catch problems
while they're still on a branch nobody's looked at yet, not after the
PR is open.

Point it explicitly at the categories the Codex review in step 8 keeps
catching that this pass tends to miss: state mutated across
control-flow branches (e.g. font/graphics-state changes mid-loop that
only bite on a later iteration), multi-byte/encoding edge cases, and
implicit side effects (a call that looks read-only but isn't). Naming
these up front costs nothing and has caught real bugs cheaper than
waiting for the several-minutes-to-an-hour Codex round trip in step 8
to find them (PR #30: a coordinate-collision bug in exactly this
category survived the full unit test suite and needed a dedicated
Codex round to catch).

If the advisor surfaces something worth fixing, fix it, re-run the
quality gate, and use your judgment on whether it's worth one more
advisor pass — don't loop indefinitely chasing a clean bill of health
on genuinely subjective feedback.

Refresh the heartbeat: `date +%s > "$(git -C "$WORKTREE_DIR" rev-parse --git-dir)/work-issue-heartbeat"`.

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

**If this branch's diff touches
`.claude/skills/work-issue/SKILL.md` itself**, note that explicitly in
the PR/issue comments posted below: this session loaded `SKILL.md`
before the change existed, so this very step-8 pass is running the
*old* review logic even while reviewing a diff that may have changed
it (confirmed happening for real in #26/#27 — the PR that added Codex
review wasn't itself reviewed by Codex, because the fallback path was
all the session had loaded). A human reading the comment then knows to
re-check step 8's behavior against what actually merged, rather than
trusting this run's self-report of "clean."

**Review must come from a different model family than whichever one
implemented the change.** This skill only ever runs as Claude, so its
review dispatches to Codex — never a same-family blank-context `Agent`
review, which shares training and blind spots with the implementer no
matter how little context it's given. (Symmetric rule, stated once:
if a Codex-driven equivalent of this skill is ever built, its step 8
dispatches to a blank-context Claude `Agent` instead — no runtime
detection needed, since each skill only ever runs as the model it's
written for.)

Locate the Codex plugin's runtime script rather than assuming
`$CLAUDE_PLUGIN_ROOT` — that env var is populated for plugin-owned
commands, not project skills like this one, so it may be unset or
point somewhere unrelated:

```sh
CODEX_SCRIPT=$(find "$HOME/.claude/plugins" -path "*/codex/scripts/codex-companion.mjs" 2>/dev/null | head -1)
```

If nothing is found, or `node "$CODEX_SCRIPT" status --json` shows the
runtime isn't authenticated (run the `codex:setup` skill to check),
don't silently skip cross-model review — fall back to a blank-context
Claude `Agent` review (self-contained prompt: nothing but the PR
number and what to check; it fetches `gh pr view`/`gh pr diff` itself,
it doesn't inherit this session's view of them) and say explicitly in
the PR/issue comment (below) that this was a same-family fallback, not
policy, so a human knows to expect a real Codex pass once the runtime
is fixed.

Otherwise, run the Codex review from inside `$WORKTREE_DIR` — cwd can
reset between tool calls (see Pitfalls), and a review run from the
wrong directory reviews the wrong tree and still returns a confident
verdict, so assert the branch before trusting the output. Refresh the
heartbeat right before kicking it off — this is the longest-blocking
step (minutes to tens of minutes per round) and the one most likely to
make a still-live run look stale to a concurrent invocation:

```sh
date +%s > "$(git -C "$WORKTREE_DIR" rev-parse --git-dir)/work-issue-heartbeat" && \
rm -f /tmp/codex-review-<N>.json && \
  cd "$WORKTREE_DIR" && git fetch origin main && \
  test "$(git rev-parse --abbrev-ref HEAD)" = "$BRANCH" && \
  node "$CODEX_SCRIPT" review --wait --json --scope branch --base origin/main \
  > /tmp/codex-review-<N>.json
```

`rm -f` first so a short-circuited chain (the branch assert failing, or
`$WORKTREE_DIR`/`$BRANCH`/`$CODEX_SCRIPT` coming up empty from a
cwd/variable reset — see Pitfalls) leaves *no* file behind rather than
leaving a prior round's stale review sitting at that path where the
posting step below would read and post it as if it were current.

This blocks (expect several minutes — it's a real model pass, it reads
around the diff for context) and, on success, writes a JSON object
shaped `{review, target, codex: {status, stderr, stdout, reasoning}}`
— **not** `review-output.schema.json`'s `{verdict, findings[], ...}`;
that schema belongs to a different internal mode and does not describe
this command's actual output (confirmed empirically — an earlier draft
of this skill assumed it did, and `jq`-ing for `.verdict`/`.findings`
silently returned `null` and would have posted nothing useful). The
real content is `.codex.stdout`: free-form markdown — a short summary
paragraph, then a "Full review comments:" section with bullets each
tagged by priority (`[P1]`, `[P2]`, ...) inline in the prose, not
structured fields. The mapping is `P0`=critical, `P1`=high,
`P2`=medium, `P3`=low — stated here so it doesn't need re-deriving by
reasoning every run. Treat the tagging itself as a rough heuristic,
not a contract — it's model-generated text, not a schema, and its
exact shape can drift between runs.

**Read the whole thing yourself and decide what needs fixing —
don't gate merge on a `[P1]`-only regex.** A finding describing a real
defect must be fixed or given an explicit stated reason it's not being
fixed ("not a bug", "out of scope, tracked separately"), regardless of
which priority tag Codex put on it; an unexplained skip isn't
acceptable — `SDLC.md`'s independent-review step requires coming back
*clean*, not merely *reviewed*, and priority tags from a free-text
reviewer aren't reliable enough to auto-waive anything. Use `[P1]` (or
worse) as a signal for "definitely stop and look," not as the entire
policy.

**Before posting anything, check whether the run actually succeeded.**
If `.codex.status` is non-zero, or `.codex.stdout` is empty/null, the
run failed rather than approved — don't post it as if it were a clean
review; treat it the same as "Codex runtime unavailable" below (skip
straight to the Fallback path, don't post from this file at all).

```sh
jq -e '.codex.status == 0 and (.codex.stdout // "" | length > 0)' /tmp/codex-review-<N>.json
```

Only once that passes: **post the review on both the PR and the
issue, unconditionally — including a clean pass with nothing to
fix.** This is the actual point of cross-model review: a durable
record a human can read without re-running anything, not just a
merge/no-merge gate.

```sh
gh pr comment <PR#> --body "$(jq -r '"## Codex review\n\n" + .codex.stdout' /tmp/codex-review-<N>.json)"
gh issue comment <N> --body "Codex review posted on <PR URL> — <one-line: clean, or what's being fixed/dispositioned>."
```

If anything needs fixing: fix it, **commit and `git push` the fix**,
then re-run the quality gate and re-run the Codex review on the
updated diff (repeat the whole block above, including posting the new
review as a fresh comment — don't overwrite the prior one) before
continuing. Pushing before re-reviewing matters here specifically:
`--scope branch` diffs local git state, so a fix that's only committed
locally makes the *next local review* look clean while the actual PR
on GitHub — what CI tests and what merge would actually ship — still
points at the old, unfixed commit. Use your judgment on how many
rounds are worth it on genuinely subjective feedback.

Once the review is clean — nothing left unfixed without a stated
reason — proceed to merge-eligibility below.

**Fallback path** (Codex runtime unavailable/unauthenticated, or
`.codex.status` non-zero / empty output above): use the `Agent` tool
with a self-contained prompt that gives it nothing but the PR number
and what to check (title/body via `gh pr view`, diff via `gh pr diff`
— the agent fetches these itself, it doesn't inherit this session's
view of them). Post its review to the PR and issue under the same
policy as above — unconditionally, including a clean result — but
via `gh pr comment`/`gh issue comment` with the agent's own report
text directly as the body; there's no `/tmp/codex-review-<N>.json` in
this path, so the `jq` mechanics above don't apply. Note in both
comments that this was a same-family fallback. Same rule as above: fix
or explicitly disposition everything it raises, commit and push before
any re-review, before continuing.

With the review clean (nothing left unfixed without a stated reason,
via whichever path ran), decide merge eligibility from the policy:
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
- **A Codex review at step 8 legitimately takes several minutes** — it
  reads around the diff (NOTES.md, HANDOFF.md, prior commits) rather
  than just pattern-matching the patch. Don't mistake a long-running
  `--wait` call for a hang. If the harness auto-backgrounds the Bash
  call itself (it may, past its own timeout), that's fine — the review
  job is tracked independently by `codex-companion.mjs`; poll `node
  "$CODEX_SCRIPT" status --all --json` or `result <job-id> --json`
  rather than re-running the review.
- **`status --all --json` can report a job as `"running"` long after
  the underlying `codex` process has actually died.** Confirmed while
  building this step: the process exited cleanly (macOS's unified log
  showed a normal exit-handler sequence, not a crash) mid-review, but
  the job tracker kept reporting `running` with a stale `updatedAt`
  for 10+ minutes until manually `cancel`led — nothing timed it out on
  its own. If a `--wait` call seems stuck, don't just trust `status`;
  cross-check the reported `pid` is actually alive (`kill -0 <pid>`)
  before waiting longer, and `cancel` + fall back if it isn't. Also
  note `status --all --json`'s own output is occasionally truncated/
  malformed mid-write (a transient parse failure, not evidence of
  anything) — retry the status call once before treating a parse error
  as a signal.
- **Test the diff scope on something small before trusting it on a
  real PR.** `--scope working-tree` against a directory full of
  unrelated large/binary files (confirmed while building this step —
  a pile of untracked art assets) made a review stall for 6+ minutes
  reading files that had nothing to do with the change; `--scope
  branch --base origin/main` against just the PR's actual commits is
  the intended shape and stayed on-task.
- **`ScheduleWakeup` is for `/loop`, not for polling a backgrounded
  Codex review job.** Observed misuse: once self-corrected mid-run
  (wasted a cycle), once a stale fallback wakeup fired after the PR
  was already merged and the worktree cleaned up. To wait on a
  `codex-companion.mjs` review job, poll it directly (`status --all
  --json` / `result <job-id> --json`, per the pitfall above) or via
  `Monitor`/background-task notifications — not `ScheduleWakeup`.
