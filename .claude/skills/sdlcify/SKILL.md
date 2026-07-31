---
name: sdlcify
description: Idempotently bring a repo up to Crackpot Industries' standard SDLC -- branch protection, merge-button rules, repo description, GitHub Actions CI, linters, a sane .gitignore, and CLAUDE.md/AGENTS.md/README.md links into a generated SDLC.md that records the policy. Use whenever the user says "/sdlcify", "sdlcify this repo", "set up the SDLC here", "configure this repo's development process", or "fix/audit this repo's SDLC setup" -- for a brand-new empty folder (bootstrap) or a legacy repo that predates the process (retrofit). Safe to re-run: only changes what's out of compliance with the policy recorded in SDLC.md.
---

# sdlcify — make a repo's SDLC real, not just written down

This skill configures a repo's *infrastructure* for the issue → branch →
PR → review → merge → cleanup lifecycle: GitHub-side settings, CI,
linters, ignore rules, and the docs that describe the process. It does
**not** implement the day-to-day "pick up an issue and ship a PR" loop —
that's a separate, per-repo execution skill (this repo's is
`.claude/skills/work-issue/`). If that skill is missing when sdlcify
runs, say so in the final report as a follow-up item; don't try to
author it as part of this skill. sdlcify's job ends at "the rails are in
place and documented in `SDLC.md`."

**Idempotency is the whole point.** Every phase below is
detect-current-state → compute-target-state → diff → apply-only-the-delta.
A second run against a fully-compliant repo should report "already
compliant" everywhere and touch nothing. Never regenerate or overwrite
something just because you're re-running — only act on an actual diff.

## Phase 0 — orient

```sh
git rev-parse --is-inside-work-tree 2>&1   # in a repo at all?
git remote get-url origin 2>&1              # has a GitHub remote?
gh repo view --json nameWithOwner,defaultBranchRef,description,\
squashMergeAllowed,mergeCommitAllowed,rebaseMergeAllowed,deleteBranchOnMerge 2>&1
```

- No `.git` yet → this is a bootstrap (empty new folder). Ask the user
  once ("what's this project?") if there's nothing to infer from, then
  `git init`, and skip anything below that needs an existing commit
  history.
- `.git` exists but no `origin` / `gh repo view` fails → local-only repo.
  Run every *file-level* phase (gitignore, CI workflow, linters, docs,
  `SDLC.md`) but skip every *GitHub-API* phase (branch protection, merge
  button, repo description, labels) — note in the final report that
  those are pending on `gh repo create` and a re-run.
- Both exist → full run, all phases apply.

Detect the language/toolchain by the first marker file found (repo root,
non-recursive is enough for the common case):

| Marker | Language | Quality-gate command | CI setup step |
|---|---|---|---|
| `Cargo.toml` | Rust | `cargo build && cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt --all -- --check` | `dtolnay/rust-toolchain@stable` + `Swatinem/rust-cache@v2` |
| `package.json` | Node/TS | `npm ci && npm run build --if-present && npm test --if-present && npm run lint --if-present` | `actions/setup-node@v4` |
| `pyproject.toml` / `requirements.txt` | Python | `pip install -e .[dev] && pytest && ruff check . && ruff format --check .` | `actions/setup-python@v5` |
| `go.mod` | Go | `go build ./... && go test ./... && go vet ./... && test -z "$(gofmt -l .)"` | `actions/setup-go@v5` |

More than one marker present (rare) → ask which is primary rather than
guessing. No marker at all → docs-only repo; skip the CI/linter phases
entirely (nothing to build or test), everything else still applies.

Full profile details (gitignore lines, CI job skeletons per language)
are in `references/language-profiles.md` — read it once you know which
language you're targeting, rather than loading all four up front.

## Phase 1 — load or establish policy

Policy lives in `SDLC.md` at the repo root, as a YAML frontmatter block
the skill both reads and writes — it's the source of truth, not just
documentation. **If `SDLC.md` already exists, parse its frontmatter and
skip straight to Phase 2 with that as target state** — don't re-ask.
This is what makes re-runs (e.g. "we changed our merge policy, apply
it") idempotent and driven by editing one file rather than re-answering
questions.

If `SDLC.md` doesn't exist yet, this is a first run — ask the policy
questions below, then write it (template: `references/sdlc-doc-template.md`).
Recommended defaults come from this repo's own dogfooding session
(`jeromebanks/postscript_interpreter`, 2026-07-31) — offer them as the
first/recommended option, not as unilateral choices:

1. **Merge authority** — who/what can merge a compliant PR (CI green,
   independent review clean)?
   - *Recommended*: agent may self-merge below a size/risk bar (default
     150 changed lines excluding doc files, and none of a
     language-appropriate "sensitive paths" list — ask the user to name
     any paths that always need a human, e.g. the execution core,
     CI/workflow files, dependency manifests); above the bar, falls
     back to human-merge.
   - Alternatives: human merges always; agent has unconditional merge
     authority once CI+review are clean.
2. **Required status check name(s)** — identify the single workflow that
   runs the quality-gate command (there may be others — a Pages deploy,
   a release build — that aren't gates; don't require those). If more
   than one workflow looks like a candidate gate, list them and ask
   rather than guessing. Within that workflow, the check name is the
   job's `name:` field if it has one, **otherwise the job key itself**
   (e.g. a job written as `jobs: test:` with no `name:` registers its
   check as `test` — that's this repo's `ci.yml` today).
3. **Admin bypass on branch protection** — on (recommended for a
   solo/small-team repo; lets you self-rescue) or off (protection binds
   everyone, no exceptions).
4. **Merge button shape** — squash-only + delete-branch-on-merge
   (recommended: linear history, no stale branches) vs. leave all three
   merge types open.

Use `AskUserQuestion` for these — they're genuine policy calls, not
things to infer. Batch them into one call.

**Never set `required_approving_review_count` to anything but `0`
without asking first.** A solo-maintainer repo where the agent opens
PRs under the owner's own credentials cannot satisfy a ≥1-approval
requirement — GitHub refuses self-approval — and that misconfiguration
silently bricks merging with no obvious error at protection-setup time.
If the user wants real required-approval review (multi-contributor
repo), confirm they have a second reviewing identity before setting it
above 0.

## Phase 2 — scan current state

Read-only. For each item below, capture current vs. target and hold the
diff — don't print anything yet, don't mutate anything yet.

- **`.gitignore`**: which target lines (language profile + universal OS/
  editor cruft — `.DS_Store`, `*.swp`, `*.swo`) are already present vs.
  missing. Never mind ordering or lines already there for other reasons.
- **CI workflow** (`.github/workflows/ci.yml` or equivalent): present at
  all? If present, does it trigger on `push: [main]` and `pull_request`,
  and does it run the full quality-gate command? **If it exists, treat
  it as authored, not generated — don't overwrite it.** Only flag gaps
  (e.g. missing a trigger) in the report; let the user decide whether to
  hand-edit. Only *generate from scratch* when the file is absent.
- **Linters**: for Rust this is satisfied by the CI quality-gate command
  already including `clippy --all-targets -- -D warnings` and
  `fmt --all -- --check` — no extra file needed. Other languages: check
  for the profile's expected config file (`.eslintrc*`/`ruff.toml`/etc.);
  note as missing if absent, but don't invent a config with opinions the
  user hasn't confirmed — flag it, offer to scaffold a minimal default,
  don't silently create one with `apply_all` below.
- **Docs** (`CLAUDE.md`, `AGENTS.md`, `README.md`): does each contain the
  managed block (delimited by `<!-- sdlcify:managed:start -->` /
  `<!-- sdlcify:managed:end -->`)? If the file doesn't exist, target is
  "create it containing just the managed block" (README additionally
  gets a minimal skeleton — title + one-line description — if it's
  totally absent, since a README with *only* an SDLC block would be a
  strange first thing to ship). If the file exists without the block,
  target is "append the block," leaving all existing content untouched.
  If the block exists, target is "block content matches the current
  template" — diff just the block's contents, never anything outside
  the markers. **If the file already has hand-authored SDLC content
  outside where the block would go** (e.g. this repo's `AGENTS.md` has
  a full "Software development lifecycle" section) — don't let the
  managed block create a second, competing description. The block
  should say `SDLC.md` is the authoritative source, and Phase 5's
  report must flag the pre-existing section by name so the user can
  fold or trim it by hand. Silently ending up with two descriptions of
  "how PRs work" in one file is the failure the managed-block design
  exists to prevent — don't let it happen anyway by only handling the
  empty-file case carefully.
- **Labels**: `gh label list` — do `in-progress` / `in-review` exist?
- **Branch protection**: `gh api repos/{o}/{r}/branches/{default}/protection`
  (404 means unprotected — that's a normal "current state," not an
  error). Diff against the Phase 1 policy.
- **Merge button + description**: from the Phase 0 `gh repo view` call
  already made — diff `squashMergeAllowed`/`mergeCommitAllowed`/
  `rebaseMergeAllowed`/`deleteBranchOnMerge` and `description` (only
  flag description as a gap if it's empty; never overwrite a
  human-written one without asking).
- **Execution skill**: does a `.claude/skills/<something>/SKILL.md`
  exist that looks like the issue→branch→PR loop (grep skill
  descriptions for "issue" + "branch" + "PR")? Record present/absent
  only — sdlcify doesn't create this (see the top-of-file boundary
  note).

## Phase 3 — report the diff, get confirmation

Print a compact table: item → current → target → action (none / create
/ update / API call). Two confirmation tiers, not one:

- **File-level changes** (gitignore, CI generation, docs, `SDLC.md`,
  linter scaffolds): one combined go-ahead covers all of them — these
  land as uncommitted working-tree changes the user reviews via `git
  diff` before committing anything themselves. Never auto-commit.
- **GitHub API changes** (branch protection, merge button, repo
  description, labels): confirm explicitly and separately, every run
  that has a nonempty diff here — even on a repo sdlcify has configured
  before. These are exactly the "shared system, hard to reverse"
  actions that warrant asking each time, not just on first setup.

If every item's diff is empty, skip both confirmations and report
"already compliant" — don't ask the user to approve doing nothing.

## Phase 4 — apply

Only the items with a nonempty diff, in this order:

1. **`.gitignore`** — append missing lines under a `# sdlcify: common
   ignores` heading if that heading isn't already there; otherwise
   append missing lines under the existing one. Don't touch anything
   else in the file.
2. **CI workflow** — generate only if absent, from the language profile
   skeleton. Never regenerate an existing one.
3. **Linter scaffold** — only if the user confirmed a minimal default is
   wanted (Phase 2 flags this as a judgment call, not an auto-apply).
4. **Docs managed blocks** — write/update the block in each of
   `CLAUDE.md`, `AGENTS.md`, `README.md` per the Phase 2 target,
   preserving everything outside the markers byte-for-byte.
5. **`SDLC.md`** — regenerate in full from `references/sdlc-doc-template.md`
   with the Phase 1 policy filled in. This file is fully sdlcify-owned;
   safe to overwrite wholesale every run (that's what makes editing its
   frontmatter and re-running the mechanism for policy changes).
6. **Labels** — `gh label create in-progress --color FBCA04 --description
   "Actively being worked on" 2>/dev/null || true` (and same for
   `in-review`) — idempotent by construction, `||true` absorbs
   "already exists."
7. **Branch protection** — use a JSON body via heredoc, not `gh api -f`/
   `-F` flags: the `required_status_checks.checks[]` array can't be
   expressed through the flag syntax, so the flag form isn't a viable
   primary path here.
   ```sh
   gh api --method PUT "repos/{owner}/{repo}/branches/{default}/protection" \
     -H "Accept: application/vnd.github+json" \
     --input - <<EOF
   {
     "required_status_checks": {
       "strict": true,
       "checks": [{"context": "<ci job name>"}]
     },
     "enforce_admins": <true|false per policy>,
     "required_pull_request_reviews": {
       "required_approving_review_count": <policy value>,
       "dismiss_stale_reviews": false,
       "require_code_owner_reviews": false
     },
     "restrictions": null,
     "allow_force_pushes": false,
     "allow_deletions": false
   }
   EOF
   ```
   **Confirmed working** (`jeromebanks/postscript_interpreter`,
   2026-07-31): this exact payload shape applies cleanly —
   `required_approving_review_count: 0` survives without coercion or
   rejection, `enforce_admins: false` round-trips correctly for
   admin-bypass-on, and the GET-back matches the PUT byte-for-byte on
   every field. No longer treat this as a guess. **Still always follow
   with a GET on the same endpoint** and diff the result against what
   was requested before reporting success — a payload confirmed once
   doesn't guarantee every GitHub API version/plan combination accepts
   it identically. A live push-rejection test (attempting a real direct
   push to confirm GitHub actually blocks it) was *not* performed during
   verification — the GET match was treated as sufficient evidence, since
   a real test requires an actual ref-updating push attempt, which is a
   separate risk of its own. If you want stronger proof on a future run,
   that's an explicit call to make with the user, not a default action.
8. **Merge button + description** —
   ```sh
   gh repo edit --enable-squash-merge=<policy> --enable-merge-commit=<policy> \
     --enable-rebase-merge=<policy> --delete-branch-on-merge=<policy>
   gh repo edit --description "<only if currently empty and user supplied one>"
   ```

## Phase 5 — final report

One table: item, before, after (or "unchanged"). Explicitly call out:
- Any GitHub-API field that didn't apply as requested (from the Phase 4
  step 7 verify-GET).
- Whether an execution skill (work-issue-equivalent) is present; if not,
  say that's the next thing to add, not something this run did.
- Any file-level diff left uncommitted, and that committing it is the
  user's call.
- Any doc where a hand-authored SDLC section now sits alongside the
  managed block pointing at `SDLC.md` (Phase 2's duplication check) —
  name the file and section so the user can reconcile it, don't bury
  this in the table.

## Pitfalls

- **`required_approving_review_count` ≥ 1 on a single-identity repo
  deadlocks merging.** Covered in Phase 1 — don't let a "recommended
  defaults" skim past this; it's the one setting that bricks the whole
  workflow silently.
- **Don't overwrite hand-authored CI, docs outside the managed block, or
  a non-empty repo description.** The whole reason for the managed-block
  markers and the "only generate if absent" rule is that this skill will
  run against repos with real, valuable existing content (this repo's
  `AGENTS.md`/`HANDOFF.md`/`ci.yml` predate sdlcify and are good) — never
  treat "sdlcify doesn't recognize this content" as license to replace
  it.
- **`SDLC.md` is the one file that's fully owned and safe to
  regenerate.** Don't apply the same caution there — if you find
  yourself diffing its prose instead of just rewriting it from the
  template + policy, that's unnecessary care in the wrong place.
- **A 404 on the branch-protection GET is normal**, not a failed call —
  it means "currently unprotected." Don't treat it as an error to work
  around.
- **Bootstrap (empty folder) runs skip more than retrofit runs do** —
  no commit history, possibly no remote yet. Don't force those phases;
  report what's pending and why.
