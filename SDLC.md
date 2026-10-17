---
sdlcify_version: 1
repo: jeromebanks/postscript_interpreter
default_branch: main
generated: 2026-10-16
quality_gate: "PSCAT_PROFILE=ci ./scripts/selftest.sh && cargo test --profile ci && cargo clippy --all-targets -- -D warnings && cargo fmt --all -- --check"
merge_policy:
  mode: agent-full
  owner_merge_file: .github/CODEOWNERS
required_status_checks:
  - test
branch_protection:
  required_approving_review_count: 0
  require_up_to_date: false
  enforce_admins: false
  allow_force_pushes: false
  allow_deletions: false
merge_button:
  squash_merge: true
  merge_commit: false
  rebase_merge: false
  delete_branch_on_merge: true
release:
  process_doc: docs/dev/RELEASING.md
  environment: release
  tag_pattern: "v*"
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
5. **Quality gate clean before opening a PR**: `PSCAT_PROFILE=ci
   ./scripts/selftest.sh && cargo test --profile ci && cargo clippy
   --all-targets -- -D warnings && cargo fmt --all -- --check` — the
   same checks CI runs (`.github/workflows/ci.yml`), so the PR isn't the
   first place a break shows up. The string is `quality_gate` in the
   frontmatter above.
6. **Diff reviewed** before the PR is called done — a second look at the
   actual changes, not just the plan.
7. **Open the PR**, referencing the issue (`Closes #N`), with a summary
   and a test plan.
8. **Independent review** — a reviewer with no context from
   implementation (a fresh session, or `/code-review`) checks the diff
   cold. Must come back clean to proceed.
9. **Merge — agent-full policy**: an agent may merge any PR once CI is
   green and independent review is clean, regardless of diff size or
   which files it touches, **except** a PR that touches a path in
   `.github/CODEOWNERS` (`merge_policy.owner_merge_file`), which the owner
   merges: see "Who merges" below. There is no size/risk bar — if
   that turns out to be too permissive (e.g. after a merge touching
   `src/interp.rs` lands without a human look), tighten
   `merge_policy.mode` to `size-and-risk-bar` in the frontmatter above
   and re-run `sdlcify`.
10. **Post-merge cleanup**: delete the remote branch, remove the local
    worktree if one was used, confirm the issue auto-closed.

## Branch protection (enforced on GitHub, not just convention)

`main` requires: a PR before merging, the `test` check passing, 0
required approving reviews (single-maintainer repo — GitHub can't
satisfy self-approval, see the `sdlcify` skill's pitfalls section for
why), no force-pushes, no deletions. Admin bypass is **on**
(`enforce_admins: false`) — the repo owner can still push past
protection in an emergency.

**Up-to-date branches are not required** (`require_up_to_date: false`,
GitHub's `required_status_checks.strict`). A PR may merge while it is
behind `main`, so a PR's last green CI run is not re-run against a moving
`main`. Strict mode forced an update-branch-and-rerun cycle (about 3
minutes of CI) on almost every PR once several agents merged in
parallel. The safety net is CI on every push to `main`: after merging, the
agent checks that the push-to-main run for its merge commit went green and
stops and reports if it did not (`.claude/skills/work-issue/`). The
intended long-term replacement is a merge queue, which needs an
organization-owned repo; when this repo moves to one, switch to the queue
and add a `merge_group:` trigger to `.github/workflows/ci.yml`.

## Who merges

Whatever `merge_policy.mode` says, **agents don't self-merge PRs that touch
`.github/CODEOWNERS` paths; the owner merges those.** The file lists the paths
that change what gets built, shipped or trusted (CI workflows, dependency
manifests, release scripts, licence files, plugin manifests, and the file
itself). The agent finds out whether a PR touches one by running
`python3 scripts/codeowners_check.py --base origin/main` (exit status 1, and the
matching paths on stdout, means it does); the check reads the file as it is on
`main` and in the PR, so a PR cannot exempt itself by deleting its own line.
The agent still opens the PR, runs the independent review and waits for CI,
then stops and reports the PR as awaiting the owner's merge.
The rule in force is the one on `main`: the agent reads `merge_policy.owner_merge_file` from
`SDLC.md` as it is on `main`, so a PR that removes the field is still owner-merged, and the PR
that first adds the field is judged by the policy before it.

This is enforced by the agent's skill, **not by GitHub**. Turning on "Require
review from Code Owners" would need at least one required approval, which a
repo whose agents open PRs under the owner's own account cannot satisfy (see
the `required_approving_review_count` pitfall in the `sdlcify` skill), so
`required_pull_request_reviews.require_code_owner_reviews` stays `false` and
CODEOWNERS only requests the owner's review.

## Release lifecycle

Releases follow `docs/dev/RELEASING.md` (`release.process_doc`). In outline:

1. **Release PR.** `scripts/release.sh X.Y.Z` branches `release/vX.Y.Z` from
   an up-to-date `main`, bumps every manifest version, prepends the
   `CHANGELOG.md` section (git-cliff), and opens the PR. It goes through the
   lifecycle above like any PR. It touches CODEOWNERS paths, so the owner
   merges it.
2. **Tag.** The owner pushes `vX.Y.Z` (`release.tag_pattern`) on the merge
   commit. A tag ruleset makes release tags immutable.
3. **Pipeline.** `.github/workflows/release.yml` verifies and builds, then
   creates a draft release. Publishing waits in the protected `release`
   environment (`release.environment`) for the owner's approval, the only
   recurring manual step. Only that environment can obtain publishing
   credentials (trusted publishing over OIDC). Pull-request CI cannot.
4. **Resume, never overwrite.** Each publishing stage skips an identical
   artifact and halts on a different one. A failed release is resumed by
   re-running its failed jobs.
5. **Hotfix.** A `fix/` PR into `main`, then a patch release. Releases are cut
   from `main` only.
6. **Yank or deprecate, never delete.** `cargo yank`, a "Deprecated" note on
   the GitHub Release, `npm deprecate`. The fix ships as a new version.

## Labels

`in-progress` (claimed, being worked), `in-review` (PR open, awaiting
merge decision) — set by `.claude/skills/work-issue/`, which removes both
once the merge is verified, so closed issues carry neither.

## Regenerating this file

Run the `sdlcify` skill again. It re-scans current repo state, diffs
against this file's frontmatter, and only changes what's out of sync —
safe to run as often as you like.
