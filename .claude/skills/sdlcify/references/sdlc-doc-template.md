# `SDLC.md` template

sdlcify owns this file completely — regenerate it in full on every apply
that touches policy, filling in the frontmatter from Phase 1's answers
(or the existing frontmatter, if this is a re-run and nothing changed).
Don't hand-edit prose around the frontmatter expecting it to survive; if
the user wants to change policy, they edit the frontmatter and re-run
sdlcify, or ask the agent to.

```markdown
---
sdlcify_version: 1
repo: <owner>/<name>
default_branch: <branch>
generated: <ISO date of last apply>
merge_policy:
  mode: size-and-risk-bar   # or: human-only | agent-full
  max_changed_lines: 150     # excluded: doc-only files (README/NOTES/HANDOFF/CHANGELOG)
  sensitive_paths:
    - <path glob>
    - <path glob>
required_status_checks:
  - <ci job name>
branch_protection:
  required_approving_review_count: 0
  enforce_admins: true
  allow_force_pushes: false
  allow_deletions: false
merge_button:
  squash_merge: true
  merge_commit: false
  rebase_merge: false
  delete_branch_on_merge: true
---

# SDLC — <repo name>

This file is generated and owned by the `sdlcify` skill. It's both the
human-readable description of how this repo's software development
lifecycle works *and* the config `sdlcify` reads on every re-run — edit
the frontmatter above to change policy, then re-run `sdlcify` to apply
it to GitHub.

## Lifecycle

1. **Issue first.** Nontrivial work gets a `gh issue create` before any
   code — what and why, even a few sentences.
2. **Feature branch, not `<default_branch>` directly.**
   `git checkout -b feature/<slug>` (`fix/<slug>` for bugs).
3. **Plan, reviewed before implementation starts.** Write the approach
   down; get it reviewed (by `advisor` or an equivalent second opinion)
   before writing code — catch a bad approach while it's still cheap to
   change.
4. **Implement**, commit at coherent checkpoints, not one giant commit.
5. **Quality gate clean before opening a PR**: `<quality-gate command
   for this repo's language>` — the same command CI runs, so the PR
   isn't the first place a break shows up.
6. **Diff reviewed** before the PR is called done — a second look at the
   actual changes, not just the plan.
7. **Open the PR**, referencing the issue (`Closes #N`), with a summary
   and a test plan.
8. **Independent review** — a reviewer with no context from
   implementation (a fresh session, or `/code-review`) checks the diff
   cold. Must come back clean to proceed.
9. **Merge**, per the policy above:
   - `human-only`: a human merges every PR, always.
   - `agent-full`: an agent may merge any PR once CI is green and
     independent review is clean.
   - `size-and-risk-bar`: an agent may merge if CI is green, review is
     clean, the diff is under `max_changed_lines` (excluding doc-only
     files), and touches none of `sensitive_paths`. Otherwise, falls
     back to a human — and the agent should say explicitly which
     condition it missed.
10. **Post-merge cleanup**: delete the remote branch, remove the local
    worktree if one was used, confirm the issue auto-closed.

## Branch protection (enforced on GitHub, not just convention)

`<default_branch>` requires: a PR before merging, the `<ci job name>`
check passing, `<N>` approving review(s), no force-pushes, no deletions.
Admin bypass is `<on/off>` — see the frontmatter above for the exact
values currently applied; this prose is a summary, not the source of
truth.

## Labels

`in-progress` (claimed, being worked), `in-review` (PR open, awaiting
merge decision) — set by whatever skill drives the issue→PR loop for
this repo (see `README.md` / `AGENTS.md` for which one).

## Regenerating this file

Run the `sdlcify` skill again. It re-scans current repo state, diffs
against this file's frontmatter, and only changes what's out of sync —
safe to run as often as you like.
```
