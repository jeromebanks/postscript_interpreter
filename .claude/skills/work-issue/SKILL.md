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

If it's closed, stop and tell the user. Otherwise, whether it already
carries `in-progress` or not, run **Claim the issue** below for `<N>`
before doing anything else — an explicit pick can still race a
concurrent invocation targeting the exact same number, whether that's
two sessions racing a brand-new issue or one racing a crashed-or-still
-live prior run of this same one (there's no PR until step 7, so "no
PR yet" alone doesn't mean crashed — it's also the normal state of a
session still mid-run). The one case that skips the claim entirely:
it already carries `in-progress` *and* an open PR claims it (`gh pr
list --state open --json body --jq` scanning for `Closes #<N>`/`Fixes
#<N>`/`Resolves #<N>`) — that's someone genuinely already on it; stop
and tell the user, don't attempt to claim.

If no number was given, **first check for an abandoned in-progress
issue to resume** — this takes priority over picking something fresh,
otherwise a crashed run's issue gets silently skipped forever instead
of finished:

```sh
# in-progress issues with no open PR claiming them = crashed-or-still-live prior runs
gh issue list --state open --label in-progress --json number,title
gh pr list --state open --json body --jq \
  '[.[].body | scan("(?:Closes|Fixes|Resolves) #([0-9]+)")[][0] | tonumber]'
```

For each `in-progress` issue whose number isn't in the open-PR list
(lowest first), run **Claim the issue** below. Resume the first one
that's successfully claimed (reuse its worktree if one exists — step 3
already contemplates this) instead of continuing to the picker below.
If every candidate is live (the claim fails for all of them), fall
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

Take the first candidate not in the open-PR list, then run **Claim the
issue** below for it — even a never-before-touched issue can be raced
by two invocations picking the same lowest-numbered candidate at once.
If the claim fails, stop and report the conflict rather than falling
through to try the next candidate (deliberately not building automatic
next-candidate retry — see Pitfalls). If there are no candidates at
all, report that the backlog is clear (or everything in-flight)
instead of inventing work.

### Claim the issue (atomic lock)

An `in-progress` issue with no open PR is ambiguous on its own — it's
both the normal state of a session still mid-run *and* what a crashed
session leaves behind (#29). A never-before-claimed issue has the same
ambiguity from the other direction: two invocations can both decide
"nobody's on this" at once. A timestamp comparison alone can't
arbitrate either case — two invocations that both read the same stale
(or absent) heartbeat before either writes can both conclude "safe to
proceed" (#35). The lock below is an atomic claim, not just a
staleness read: `mkdir` is atomic for a fresh claim (exactly one
caller can create a given directory name), and `mv` (rename) onto a
not-yet-existing name is atomic for stealing a stale one (exactly one
caller can rename a given source name away — every other racer's `mv`
on that same source fails with "no such file", a clean signal it lost
rather than a false belief it won — verified empirically).

The lock lives in the *main repo's* shared git dir, keyed by issue
number — not the worktree's own git dir, which for a fresh pick
doesn't exist yet at this point in the flow. `--path-format=absolute
--git-common-dir` resolves to the same canonical path from the main
checkout or from inside any worktree (verified) — unlike most paths in
this doc, this one is immune to the cwd-reset pitfall below, so no
`-C` or remembered variable is needed:

```sh
MAINGIT="$(git rev-parse --path-format=absolute --git-common-dir)"
[ -n "$MAINGIT" ] || { echo "RESULT=error: could not resolve the main .git dir"; exit 1; }
LOCKDIR="$MAINGIT/work-issue-lock-<N>"

if mkdir "$LOCKDIR" 2>/dev/null && date +%s > "$LOCKDIR/heartbeat.tmp" && mv "$LOCKDIR/heartbeat.tmp" "$LOCKDIR/heartbeat"; then
  echo "RESULT=claimed (fresh)"
else
  HB="$LOCKDIR/heartbeat"
  ORIG_HB_CONTENT=$(cat "$HB" 2>/dev/null)
  if [ -n "$ORIG_HB_CONTENT" ] && printf '%s' "$ORIG_HB_CONTENT" | grep -qE '^[0-9]+$'; then
    AGE=$(( $(date +%s) - ORIG_HB_CONTENT ))
  else
    # Missing/unreadable heartbeat is ambiguous on its own: it's both a
    # genuinely crashed claim (died mid-truncation) AND what a *different*
    # concurrent invocation's own fresh mkdir looks like for the few
    # milliseconds between its mkdir succeeding and its heartbeat write
    # landing (verified empirically — treating this as unconditionally
    # stale let a second racer steal a lock the first racer had just won,
    # microseconds after winning it). Fall back to the lock dir's own
    # mtime: only a dir that's *also* been sitting untouched past a small
    # grace window is a genuine crash, not an in-flight claim.
    DIR_MTIME=$(stat -f %m "$LOCKDIR" 2>/dev/null || date +%s)
    DIR_AGE=$(( $(date +%s) - DIR_MTIME ))
    if [ "$DIR_AGE" -lt 10 ]; then
      AGE=0
    else
      AGE=999999
    fi
  fi

  if [ "$AGE" -lt 2700 ]; then
    echo "RESULT=live (heartbeat $((AGE / 60)) min old) — do not resume/claim"
  else
    STEAL="$LOCKDIR.stale.$$.$(date +%s)"
    if mv "$LOCKDIR" "$STEAL" 2>/dev/null; then
      MOVED_HB_CONTENT=$(cat "$STEAL/heartbeat" 2>/dev/null)
      if [ "$MOVED_HB_CONTENT" = "$ORIG_HB_CONTENT" ]; then
        if mkdir "$LOCKDIR" 2>/dev/null && date +%s > "$LOCKDIR/heartbeat.tmp" && mv "$LOCKDIR/heartbeat.tmp" "$LOCKDIR/heartbeat"; then
          rm -rf "$STEAL"
          echo "RESULT=claimed (reclaimed a stale lock)"
        else
          rm -rf "$STEAL"
          echo "RESULT=lost a rare 3-way reclaim race — do not resume/claim"
        fi
      else
        # ABA: the object we just moved isn't the stale one we decided
        # to steal -- a faster racer already reclaimed and recreated it
        # between our read and our mv, and we grabbed *their* fresh
        # claim by name instead (mv/rename has no concept of the
        # object's identity, only its path). Put it back and back off.
        mv "$STEAL" "$LOCKDIR" 2>/dev/null
        rm -rf "$STEAL" 2>/dev/null
        echo "RESULT=lost the reclaim race (ABA detected, restored) — do not resume/claim"
      fi
    else
      echo "RESULT=lost the reclaim race to a concurrent invocation — do not resume/claim"
    fi
  fi
fi
```

`RESULT=claimed*`: this invocation owns issue #<N>. Continue (to step
2 for a genuinely fresh pick — `gh issue edit <N> --add-label
in-progress` is harmless to re-run on an issue that already carries
it, for the resume case; straight to step 3 to create or reuse the
worktree).

`RESULT=live*` or `RESULT=lost*`: **stop, don't proceed.** Report to
the user that issue #<N> looks actively worked on (or was just claimed
by a concurrent invocation) instead of silently racing another
session's edits.

**The `ORIG_HB_CONTENT`/`MOVED_HB_CONTENT` comparison after the steal
closes an ABA race the `mv`-is-atomic argument alone doesn't cover**
(found by Codex's review of this PR, then reproduced and fixed —
confirmed empirically): `mv` arbitrates *names*, not *object identity*.
If racer A reads the same stale lock as racer B, but B wins the steal
and recreates a fresh `$LOCKDIR` before A's own (possibly delayed —
scheduler jitter, not just an artificial sleep) `mv` fires, A's `mv`
still succeeds — it moves *B's brand-new claim* out from under it,
because `mv $LOCKDIR $STEAL` only cares that something currently
occupies that name, not which generation of object it is. Without the
content check, A would then believe it legitimately reclaimed a stale
lock and report `claimed`, while B's real claim sits orphaned in `$STEAL`
about to be deleted — two invocations both believing they own the
issue, the exact failure #35 exists to close. The fix: snapshot the
heartbeat's content *before* acting on it, and after the `mv`, confirm
the moved object still has that exact content. A staleness read is
always 45+ minutes old, so it can never coincidentally match a
freshly-written "now" timestamp — a mismatch is unambiguous proof
we grabbed the wrong object. On mismatch, put it back (`mv "$STEAL"
"$LOCKDIR"`) and back off rather than proceeding; the loser's `mkdir`
in the ABA-free case is what makes the restore safe to attempt (the
name is free again, and the low-concurrency threat model here doesn't
need to handle a third racer landing in that exact instant). Verified
by artificially delaying one racer's `mv` past the other's full
reclaim-and-recreate cycle: reproduced the double-claim without this
check, zero double-claims across repeated runs with it.

`stat -f %m` is BSD/macOS syntax (this repo's environment) — if it
ever fails for a reason other than a genuinely missing directory, the
`|| date +%s` fallback makes `DIR_MTIME` "now," i.e. `DIR_AGE` near
zero, i.e. `AGE=0` (live). That's deliberate: when the age can't be
determined, fail toward "don't steal" rather than toward "assume
stale" — the former just delays a legitimate reclaim slightly, the
latter silently reopens the race this whole mechanism exists to close.

The 10s `DIR_AGE` grace window (not to be confused with the 45-minute
staleness threshold above) is chosen to be *short*, not long: it only
needs to outlast the few milliseconds between a legitimate claim's
`mkdir` and its heartbeat write landing — it is not a safety margin
against a slow or interrupted claim. If a claim's `&&` chain is ever
genuinely cut mid-way (the tool call itself killed, not just disk
slowness — see the auto-backgrounding pitfall below), the lock dir
sits empty past 10s and the next invocation correctly reads it as
abandoned and reclaims it. That's the intended behavior for a dead
claim, not a bug — 10s is picked to reclaim a genuinely dead claim
promptly, not to give a live one room to breathe (the 45-minute
threshold already does that job). If the pause is *not* the tool call
being killed but the claimant genuinely still running, just suspended
past 10s between `mkdir` and its heartbeat write, a second invocation
can still steal it and both ends up believing they own it — a known
residual gap tracked in #72, not fully closed here (closing it needs
real compare-and-swap semantics over the whole span, which the
`mkdir`/`mv` primitives alone don't give once content, not just the
name, is what needs arbitrating).

45 minutes is deliberately generous, not tight: PR #30 needed six
Codex review rounds in one legitimate run (~62 min total, each round
itself several minutes) — a false "still live" fully blocks resume,
which is worse than a slower crash-detection window.

**The `mkdir` and its immediately-following heartbeat write must stay
one `&&` chain in a single command, never split across separate tool
calls or steps** — this minimizes how long the dir-exists-but-empty
window is open. That window can't be closed to zero in portable POSIX
shell (there's no atomic "create a populated directory" primitive —
`mv`'s own directory-destination handling nests rather than replacing
an existing target), which is why the `DIR_AGE` fallback above exists
as the real close: even if the window is hit, a lock dir under 10s old
reads as live (`AGE=0`), not stale, so a racer can't steal a claim
that's still
mid-creation. Verified empirically — racing two claims against the
same fresh lock reproduced a double-claim (both racers reporting
`RESULT=claimed`) with only the `&&`-chain protection in place; adding
the `DIR_AGE` fallback and re-running the same race 50 times (30 fresh
-claim races + 20 stale-reclaim races) produced zero double-claims.
Don't remove the `DIR_AGE` fallback thinking the `&&` chain alone is
sufficient — it isn't, and a future edit that "simplifies" it away
reopens exactly the race #35 exists to close.

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

LOCKDIR="$(git rev-parse --path-format=absolute --git-common-dir)/work-issue-lock-<N>"
date +%s > "$LOCKDIR/heartbeat.tmp" && mv "$LOCKDIR/heartbeat.tmp" "$LOCKDIR/heartbeat"
```

The lock/heartbeat lives in the *main repo's* shared git dir, keyed by
issue number (step 1's **Claim the issue** already created it before
this step ran) — not inside this worktree's own git dir, so it
survives independent of whether the worktree itself is later removed.
It never needs a `.gitignore` entry and never shows up in `git status`
or a diff. Refresh it at the end of steps 4, 5, 6, and around each
Codex review round in step 8 — and, since "commit checkpoints" or
"step boundaries" can themselves be 45+ minutes apart during a long
compile or a lengthy single edit, also refresh it before starting any
single operation you expect to take a while (a big `cargo build`/
`cargo test`, an `advisor` call, the Codex review) rather than only
after it finishes — so step 1's claim check can tell a session still
working from one that crashed. Each Bash tool call is a fresh shell —
a `$LOCKDIR` variable set here won't survive to a later step's tool
call — so those refreshes re-derive the path inline rather than
trusting a remembered variable. Write it atomically (temp file + `mv`,
which is atomic on the same filesystem) rather than a bare `>`
redirect: a process that dies between the redirect's truncation and
`date` finishing its write would otherwise leave a heartbeat file that
exists but is empty — step 1's claim check above already treats that
as stale, but only *because* it validates content, not just presence.
Unlike `$WORKTREE_DIR`-derived paths, `--path-format=absolute
--git-common-dir` doesn't need `-C` at all — it resolves to the same
canonical main-repo path regardless of where cwd happens to be,
so it's immune to the cwd-reset pitfall below on its own.

Every refresh site from here through step 8 writes into `$LOCKDIR`
*without* re-`mkdir`ing it — deliberately: the lock dir should already
exist for the entire span between this claim and step 8's terminal
non-merge stops (the only places that release it, and only *after* the
fix-and-re-review loop in step 8 has already concluded — see step 8),
so a refresh finding it missing is a genuine anomaly, not routine
housekeeping. An earlier draft of this fix made refreshes self-heal
with `mkdir -p`, reasoning a refresh "isn't an ownership decision." A
Codex review of this change caught why that's wrong: if the lock is
gone because a *different* invocation legitimately claimed the issue
in the meantime (crash recovery, a stale reclaim, a human re-running
`/work-issue <N>` after this session's own run ended) and this
session's refresh code is still executing for some reason — a stray
retry, a follow-up in the same conversation — `mkdir -p` would silently
recreate the name and overwrite that invocation's heartbeat with this
session's own, with neither side any the wiser that two invocations
are now both editing the same worktree. That's the exact failure #35
exists to close, reintroduced through the back door of "helpful"
self-healing. Let the write fail loudly (the redirect errors with no
such directory) instead — that's the correct signal to stop and
re-verify ownership via **Claim the issue**, not silently patch over it.

**A missing lock dir is the anomaly bare writes catch — a lock dir
that exists but was legitimately reclaimed by someone else in between
is a related, more dangerous case bare writes do *not* catch**: the
directory is present, so the write succeeds and silently overwrites
the new owner's heartbeat, without a mismatch to signal the takeover.
Closing that fully needs an ownership token verified with real
compare-and-swap semantics, which POSIX shell can't express atomically
over the whole read-decide-write span (a token turns the problem into
"don't let the compare-then-write itself get raced," which is the same
shape of gap, just narrower) — tracked as a known residual gap in #72
rather than fixed here:

```sh
LOCKDIR="$(git rev-parse --path-format=absolute --git-common-dir)/work-issue-lock-<N>"
date +%s > "$LOCKDIR/heartbeat.tmp" && mv "$LOCKDIR/heartbeat.tmp" "$LOCKDIR/heartbeat"
```

(the same "re-derive, don't trust a remembered variable" reasoning
already applies to `$WORKTREE_DIR`/`$BRANCH` reused across steps 7-9
below — re-emit the assignment in each new Bash block rather than
assuming it's still set; those two *do* still need `-C`/absoluteness
care, since they resolve to worktree-relative paths, not the
cwd-independent main-repo one above.)

Everything from here on — reading, editing, building, testing,
committing — happens *inside `$WORKTREE_DIR`*, not the original
directory. If a worktree for this issue already exists (a prior
attempt — step 1's claim already confirmed it's stale and reclaimed
it), reuse it rather than erroring or clobbering it: set `WORKTREE_DIR`
to the existing path, and read `BRANCH` from what's actually checked
out there —

```sh
BRANCH="$(git -C "$WORKTREE_DIR" branch --show-current)"
LOCKDIR="$(git rev-parse --path-format=absolute --git-common-dir)/work-issue-lock-<N>"
date +%s > "$LOCKDIR/heartbeat.tmp" && mv "$LOCKDIR/heartbeat.tmp" "$LOCKDIR/heartbeat"
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

Refresh the heartbeat before moving on (re-derive the path and write
atomically, per step 3's note): `H="$(git rev-parse
--path-format=absolute --git-common-dir)/work-issue-lock-<N>/heartbeat"; date +%s > "$H.tmp" && mv "$H.tmp" "$H"`.

## 5. Implement

Follow the conventions already documented for this repo while you
work — they're not restated here to avoid drifting out of sync with
the source of truth:

- AGENTS.md's "Code quality bar" (no `unwrap`/`expect` on
  program-input-derived data, comments explain why not what, tests
  live alongside the code they test).
- Commit at reasonable checkpoints, not one giant commit at the end —
  and refresh the heartbeat at each one (`H="$(git rev-parse
  --path-format=absolute --git-common-dir)/work-issue-lock-<N>/heartbeat";
  date +%s > "$H.tmp" && mv "$H.tmp" "$H"`). Without this, step 4's end and step 5's end are
  the only two heartbeat refreshes bracketing the entire implementation
  phase — for any real code change (edit/build/test cycles, not just a
  doc tweak) that gap alone can exceed the 45-minute staleness
  threshold, making a still-live implementation look crashed to a
  concurrent invocation. Per-commit refreshes narrow that, but don't
  by themselves bound a *single* long-running stretch with no commit
  yet (e.g. one long compile or one big edit in progress) — also
  refresh right before starting the quality-gate build below, and
  again if any single step within it runs long.
- Update `NOTES.md`/`HANDOFF.md`/`README.md` on the branch as
  capabilities land, per AGENTS.md — not as an afterthought.
- If the feature has something a human could actually look at (a new
  art capability, template, chart type, font, etc.), give it a demo
  somewhere it's actually seen — and match the surface's real
  requirements, not just its most visible file:
  - A generative-art piece: add it to `gallery/show.sh`'s `PIECES`
    array *and* the parallel `PAGES`/`SPEEDS` arrays at the same
    index — all three are indexed together under `set -u`, so a
    `PIECES`-only addition leaves `--live` mode aborting on an unbound
    variable the moment it reaches the new entry. Also update
    `gallery/README.md`, *and* render + commit
    `gallery/renders/<name>.png` — `show.sh`'s default (non-`--live`)
    mode silently skips any piece missing that PNG, which defeats the
    whole point.
  - The published site: a card in `site/gallery.html`, backed by a
    PNG in `_site/assets/renders/`. For a gallery piece, that PNG
    comes free from `build_site.sh`'s existing
    `gallery/renders/*.png` wildcard copy — don't *also* add a
    `render` call targeting the same basename, since `render` runs
    after that copy and would overwrite the committed, intentionally
    2×-supersampled still with a plain canonical-size one
    (`gallery/README.md`'s "Re-rendering the stills" explains the
    supersampling). A `render` call in `scripts/build_site.sh` is for
    anything *without* a pre-rendered gallery still — most
    `examples/*.ps` pieces. An `<option>` in `site/playground.html`'s
    picker additionally requires the file be self-contained — no
    `(lib/x.ps) run` of a sibling library, since the wasm build has no
    filesystem — and copied into `build_site.sh`'s own source loop, or
    the playground fetches a 404.

  Copy an existing similar entry's pattern rather than guessing which
  of these apply. An `examples/*.ps` file alone is a regression
  fixture, not a demo; it's fine as the *only* artifact when the
  feature genuinely has no visible surface (an internal fix, a lint
  check, a Rust-only refactor), but don't let that be the default
  reason to skip this.

Refresh the heartbeat before this build (it's often the single
longest-running step of the whole implementation phase):

```sh
H="$(git rev-parse --path-format=absolute --git-common-dir)/work-issue-lock-<N>/heartbeat"
date +%s > "$H.tmp" && mv "$H.tmp" "$H"
cargo build && cargo test && cargo clippy --all-targets && cargo fmt --all -- --check
```

This is the same gate CI runs — clearing it locally first means the
PR isn't the first place a break shows up.

Refresh the heartbeat again once it's clean: `H="$(git rev-parse
--path-format=absolute --git-common-dir)/work-issue-lock-<N>/heartbeat";
date +%s > "$H.tmp" && mv "$H.tmp" "$H"`.

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

Refresh the heartbeat: `H="$(git rev-parse --path-format=absolute
--git-common-dir)/work-issue-lock-<N>/heartbeat"; date +%s > "$H.tmp" && mv "$H.tmp" "$H"`.

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
make a still-live run look stale to a concurrent invocation.

**Run this as a normal blocking Bash call — never pass
`run_in_background: true` on it, and never call `ScheduleWakeup` in
the same turn that launches it.** That exact pairing has twice
produced a review that looked like it finished when it hadn't: once a
0-byte `/tmp/codex-review-<N>.json` behind a task notification that
still said `completed`, once the process reported outright `killed`
(#57, not root-caused — observed only on turns combining the two, not
on plain foreground runs). Let the call block; if the harness
auto-backgrounds it anyway past its own timeout, see the matching
Pitfall below for how to check on it without `ScheduleWakeup`:

```sh
rm -f /tmp/codex-review-<N>.json && \
  H="$(git rev-parse --path-format=absolute --git-common-dir)/work-issue-lock-<N>/heartbeat" && \
  date +%s > "$H.tmp" && mv "$H.tmp" "$H" && \
  cd "$WORKTREE_DIR" && git fetch origin main && \
  test "$(git rev-parse --abbrev-ref HEAD)" = "$BRANCH" && \
  node "$CODEX_SCRIPT" review --wait --json --scope branch --base origin/main \
  > /tmp/codex-review-<N>.json
```

`rm -f` stays first in the chain, exactly as it originally was — if any
later step in the chain fails (heartbeat write, branch assert,
`$WORKTREE_DIR`/`$BRANCH` empty from a cwd/variable reset), the stale
`/tmp/codex-review-<N>.json` from a *prior* round is already gone, so
the "before posting anything" check below can't accept and post that
prior round's output as if it were a review of the current diff — a
short-circuit that isn't `rm -f`-first would silently do exactly that
under `agent-full` merge authority.

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
- `human-only`: stop here. Release the lock (`rm -rf "$(git rev-parse
  --path-format=absolute --git-common-dir)/work-issue-lock-<N>"`) —
  the label already flipped to `in-review` at step 7, so step 1 can't
  re-select this issue via the automatic picker regardless, but
  releasing explicitly here still matters for the explicit-number path
  (`/work-issue <N>` re-run after a human closes this PR unmerged and
  leaves the issue open, for instance). Report the PR as open and
  awaiting merge; don't attempt to merge it.
- `agent-full`: eligible regardless of diff size or files touched.
- `size-and-risk-bar`: eligible only if the diff (excluding doc-only
  files) is under `max_changed_lines` and touches none of
  `sensitive_paths`; otherwise stop here (same lock release as the
  `human-only` case above) and say which condition it missed.

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
rm -rf "$(git rev-parse --path-format=absolute --git-common-dir)/work-issue-lock-<N>"
```

`git worktree remove` can fail with "branch used by worktree" if run
in the wrong order — remove the worktree *before* trying to delete the
local branch, not after, or the branch delete will block on it. The
lock removal is separate and explicit — unlike the old heartbeat file,
the lock now lives in the *main repo's* shared git dir (step 1's
**Claim the issue**), not inside the worktree's own git dir, so
`git worktree remove` no longer cleans it up as a side effect.

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
  call itself (it may, past its own timeout), don't treat that as
  automatically fine — backgrounding of any kind, whether explicitly
  requested (as in #57, where it was `run_in_background: true`) or
  harness-imposed, paired with a same-turn `ScheduleWakeup` is the
  condition under which #57 saw one round produce a 0-byte review file
  behind a "completed" notification, and a separate round terminated
  outright (root cause not confirmed, only the correlation). The job
  is tracked independently
  by `codex-companion.mjs`, so recovering from it doesn't require
  re-running the review — poll `node "$CODEX_SCRIPT" status --all
  --json` or `result <job-id> --json` in a *later* turn (triggered by
  the task's own completion notification or a `Monitor`, never by a
  `ScheduleWakeup` called in the same turn that launched it — see the
  `ScheduleWakeup` pitfall below for why that pairing specifically is
  the danger, not backgrounding alone). If the poll comes back and the
  job is simply gone or the file never materializes, treat it as a
  failed round and re-run the review rather than trusting a partial
  result. **Refresh the heartbeat at each poll**, not just once before
  kicking the review off — this is the
  one operation in the whole flow that can genuinely run past the
  45-minute staleness threshold in a single blocking stretch, and a
  pre-refresh alone doesn't cover that. Other single blocking calls in
  this skill (a `cargo build`/`test` cycle, an `advisor` call) aren't
  separately covered this way — they're not backgrounded/polled, so
  there's no natural point to refresh mid-call; in practice they run
  well under the threshold for this repo's size, but that's an
  observation, not a guarantee, if the flow is reused somewhere the
  build is much slower.
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
  **Specifically, never call `ScheduleWakeup` in the same turn that
  launches the review with `run_in_background: true`** (#57): on two
  separate occasions this exact pairing — never a plain foreground
  `--wait` call — was followed by the review process either vanishing
  (task notification said `completed`, but `/tmp/codex-review-<N>.json`
  was a 0-byte file and `codex-companion.mjs status --all --json`
  showed no record of the job at all) or being actively terminated
  (task notification said `killed`). Not root-caused against the
  harness — flagging the correlation, not a mechanism — but the fix in
  practice is simple: don't background this call on purpose, and don't
  pair it with a same-turn `ScheduleWakeup` if the harness backgrounds
  it on its own.
- **The lock's `mkdir` and its first heartbeat write must be one `&&`
  chain, never split** (step 1's **Claim the issue**) — splitting them
  reopens the exact TOCTOU race #35 exists to close: a second
  invocation's `mkdir` fails, it reads the not-yet-written heartbeat
  as empty/stale, and steals from a run that already believes it won.
  Same reasoning applies to the reclaim path's post-`mv` `mkdir` +
  heartbeat write.
- **The lock lives in the main repo's shared git dir
  (`work-issue-lock-<N>`), not the worktree's** — this is deliberate
  (a fresh pick has no worktree yet to hang a lock off of), but it
  means `git worktree remove` at step 9 no longer cleans it up as a
  side effect the way the old worktree-gitdir heartbeat file did.
  Step 9's explicit `rm -rf` is load-bearing, not decorative.
