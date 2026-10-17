# `SDLC.md` template

sdlcify owns this file completely — regenerate it in full on every apply
that touches policy, filling in the frontmatter from Phase 1's answers
(or the existing frontmatter, if this is a re-run and nothing changed).
Policy fields that change the prose (e.g. `require_up_to_date`) have their
paragraph in the template below; regenerate from the template, never
re-derive the prose from memory, or the paragraph is lost on the next run.
Don't hand-edit prose around the frontmatter expecting it to survive; if
the user wants to change policy, they edit the frontmatter and re-run
sdlcify, or ask the agent to.

```markdown
---
sdlcify_version: 1
repo: <owner>/<name>
default_branch: <branch>
generated: <ISO date of last apply>
quality_gate: <optional; the gate command, exactly as CI runs it. Omit to use the language default>
merge_policy:
  mode: size-and-risk-bar   # or: human-only | agent-full
  max_changed_lines: 150     # excluded: doc-only files (README/NOTES/HANDOFF/CHANGELOG)
  sensitive_paths:
    - <path glob>
    - <path glob>
  owner_merge_file: .github/CODEOWNERS   # optional, any mode: PRs touching a path it lists are owner-merged
required_status_checks:
  - <ci job name>
branch_protection:
  required_approving_review_count: 0
  require_up_to_date: true   # GitHub's required_status_checks.strict; true when absent
  enforce_admins: true
  allow_force_pushes: false
  allow_deletions: false
merge_button:
  squash_merge: true
  merge_commit: false
  rebase_merge: false
  delete_branch_on_merge: true
release:                     # optional; absent means no release section
  process_doc: <path to the release procedure>
  environment: <protected deployment environment that publishes>
  tag_pattern: "v*"
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
5. **Quality gate clean before opening a PR**: `<quality_gate, or the
   language default>` — the same checks CI runs, so the PR isn't the
   first place a break shows up. (CI runs them as separate steps, in its
   own order; the string is what to run locally, with the same result.)
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
   - **Owner-merge paths (any mode, when `merge_policy.owner_merge_file` is
     set):** a PR that touches a path matched by that file is never merged by
     an agent; it is treated as `human-only`. See "Who merges" below.
10. **Post-merge cleanup**: delete the remote branch, remove the local
    worktree if one was used, confirm the issue auto-closed.

## Branch protection (enforced on GitHub, not just convention)

`<default_branch>` requires: a PR before merging, the `<ci job name>`
check passing, `<N>` approving review(s), no force-pushes, no deletions.
Admin bypass is `<on/off>` — see the frontmatter above for the exact
values currently applied; this prose is a summary, not the source of
truth.

<!-- Include this paragraph only when require_up_to_date is false. -->
**Up-to-date branches are not required** (`require_up_to_date: false`,
GitHub's `required_status_checks.strict`). A PR may merge while it is
behind `<default_branch>`, so a PR's last green CI run is not re-run
against a moving `<default_branch>`. Strict mode forces an
update-branch-and-rerun cycle (one full CI run) on almost every PR when
several agents merge in parallel. The safety net is CI on every push to
`<default_branch>`: after merging, the merger checks that the push run for
its merge commit went green and stops and reports if it did not. The
intended long-term replacement is a merge queue, which needs an
organization-owned repo; when this repo moves to one, switch to the queue
and add a `merge_group:` trigger to the CI workflow.

<!-- When require_up_to_date is true (or absent), say instead: "A PR must
be up to date with `<default_branch>` before it merges (`strict` status
checks)." -->

<!-- Include this section only when merge_policy.owner_merge_file is set. -->
## Who merges

Whatever `merge_policy.mode` says, **agents don't self-merge PRs that touch
`<owner_merge_file>` paths; the owner merges those.** The file lists the paths
that change what gets built, shipped or trusted (CI workflows, dependency
manifests, release scripts, licence files, plugin manifests, and the file
itself). The agent finds out whether a PR touches one by running
`python3 scripts/codeowners_check.py --base origin/<default_branch>`
(exit status 1, and the matching paths on stdout, means it does); the check
reads the file as it is on `<default_branch>` and in the PR, so a PR cannot
exempt itself by deleting its own line. The agent still opens the PR, runs
the independent review and waits for CI, then stops and reports the PR as
awaiting the owner's merge.
The rule in force is the one on `<default_branch>`: the agent reads `merge_policy.owner_merge_file`
from `SDLC.md` as it is on `<default_branch>`, so a PR that removes the field is still
owner-merged, and the PR that first adds the field is judged by the policy before it.

This is enforced by the agent's skill, **not by GitHub**. Turning on "Require
review from Code Owners" would need at least one required approval, which a
repo whose agents open PRs under the owner's own account cannot satisfy (see
the `required_approving_review_count` pitfall in the `sdlcify` skill), so
`required_pull_request_reviews.require_code_owner_reviews` stays `false` and
CODEOWNERS only requests the owner's review.

<!-- Include this section only when the `release` frontmatter key is set.
Fill <process_doc>, <environment>, <tag_pattern> from it. Added in #602. -->
## Release lifecycle

Releases follow `<process_doc>` (`release.process_doc`). In outline:

1. **Release PR.** A release script branches from an up-to-date
   `<default_branch>`, bumps every manifest version, updates the changelog,
   and opens the PR. It goes through the lifecycle above like any PR. (When
   `merge_policy.owner_merge_file` covers the manifests, the owner merges it.)
2. **Tag.** The owner pushes a `<tag_pattern>` tag (`release.tag_pattern`) on
   the merge commit. A tag ruleset makes release tags immutable.
3. **Pipeline.** The release workflow verifies and builds, then creates a
   draft release. Publishing waits in the protected `<environment>`
   environment (`release.environment`) for the owner's approval, the only
   recurring manual step. Only that environment can obtain publishing
   credentials (trusted publishing over OIDC). Pull-request CI cannot.
4. **Resume, never overwrite.** Each publishing stage skips an identical
   artifact and halts on a different one. A failed release is resumed by
   re-running its failed jobs.
5. **Hotfix.** A `fix/` PR into `<default_branch>`, then a patch release.
6. **Yank or deprecate, never delete.** The fix ships as a new version.

sdlcify only documents this. It does not create the environment, the tag
ruleset or the registry trusted publishers; `<process_doc>` lists those as
owner steps.

## Labels

`in-progress` (claimed, being worked), `in-review` (PR open, awaiting
merge decision) — set by whatever skill drives the issue→PR loop for
this repo (see `README.md` / `AGENTS.md` for which one), and removed by
it once the merge is verified.

## Regenerating this file

Run the `sdlcify` skill again. It re-scans current repo state, diffs
against this file's frontmatter, and only changes what's out of sync —
safe to run as often as you like.
```
