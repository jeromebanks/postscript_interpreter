---
sdlcify_version: 1
repo: jeromebanks/postscript_interpreter
default_branch: main
generated: 2026-07-31
merge_policy:
  mode: agent-full
required_status_checks:
  - test
branch_protection:
  required_approving_review_count: 0
  enforce_admins: false
  allow_force_pushes: false
  allow_deletions: false
merge_button:
  squash_merge: true
  merge_commit: false
  rebase_merge: false
  delete_branch_on_merge: true
---

# SDLC — postscript_interpreter

This file is generated and owned by the `sdlcify` skill
(`.claude/skills/sdlcify/`). It's both the human-readable description of
how this repo's software development lifecycle works *and* the config
`sdlcify` reads on every re-run — edit the frontmatter above to change
policy, then re-run `sdlcify` to apply it to GitHub.

## Lifecycle

1. **Issue first.** Nontrivial work gets a `gh issue create` before any
   code — what and why, even a few sentences.
2. **Feature branch, not `main` directly.** `git checkout -b
   feature/<slug>` (`fix/<slug>` for bugs).
3. **Plan, reviewed before implementation starts.** Write the approach
   down; get it reviewed (`advisor` or an equivalent second opinion)
   before writing code — catch a bad approach while it's still cheap to
   change.
4. **Implement**, commit at coherent checkpoints, not one giant commit.
5. **Quality gate clean before opening a PR**: `cargo build && cargo
   test && cargo clippy --all-targets -- -D warnings && cargo fmt --all
   -- --check` — the same command CI runs, so the PR isn't the first
   place a break shows up.
6. **Diff reviewed** before the PR is called done — a second look at the
   actual changes, not just the plan.
7. **Open the PR**, referencing the issue (`Closes #N`), with a summary
   and a test plan.
8. **Independent review** — a reviewer with no context from
   implementation (a fresh session, or `/code-review`) checks the diff
   cold. Must come back clean to proceed.
9. **Merge — agent-full policy**: an agent may merge any PR once CI is
   green and independent review is clean, regardless of diff size or
   which files it touches. There is currently no size/risk bar and no
   sensitive-paths carve-out — if that turns out to be too permissive
   (e.g. after a merge touching `src/interp.rs` or `Cargo.toml` lands
   without a human look), tighten `merge_policy.mode` to
   `size-and-risk-bar` in the frontmatter above and re-run `sdlcify`.
10. **Post-merge cleanup**: delete the remote branch, remove the local
    worktree if one was used, confirm the issue auto-closed.

## Branch protection (enforced on GitHub, not just convention)

`main` requires: a PR before merging, the `test` check passing, 0
required approving reviews (single-maintainer repo — GitHub can't
satisfy self-approval, see the `sdlcify` skill's pitfalls section for
why), no force-pushes, no deletions. Admin bypass is **on**
(`enforce_admins: false`) — the repo owner can still push past
protection in an emergency.

## Labels

`in-progress` (claimed, being worked), `in-review` (PR open, awaiting
merge decision) — set by `.claude/skills/work-issue/`.

## Regenerating this file

Run the `sdlcify` skill again. It re-scans current repo state, diffs
against this file's frontmatter, and only changes what's out of sync —
safe to run as often as you like.
