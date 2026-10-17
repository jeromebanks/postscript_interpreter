## Tag-driven publishing pipeline, resumable stages, release environment (issue #602, 2026-10-16)

Part of release epic #562 (567b). This PR is owner-merged. The procedure is
`docs/dev/RELEASING.md`.

## What shipped

- `scripts/release.sh X.Y.Z` opens the release PR. It runs from a clean,
  up-to-date `main` and checks the version is newer, the tag is absent and
  the font index is pinned (#616). It bumps `Cargo.toml`, pscat's
  `Cargo.lock` entry (edited in place, so nothing else re-resolves) and any
  plugin, npm or `server.json` manifest. It runs git-cliff (`cliff.toml`;
  the commits are PR titles, grouped by leading area word, `fonts-*` tags
  excluded), commits `release: X.Y.Z` and opens the PR. `--no-push` stops
  after the local commit.
- `scripts/release_stages.py` is the publishing engine. Every stage asks
  the registry first: on a definite not-found it publishes; on the same
  artifact it skips; on a different or unverifiable artifact it halts
  (exit 3); on a transport or auth error it fails (exit 1, retryable).
  Its subcommands:
  - `manifest`: `SHA256SUMS` and `release-manifest.json`, recording
    version, tag, commit and digests;
  - `run --stage ...`: the real or `--fake DIR` backends, `--plan` for a
    read-only run;
  - `rehearse`: the acceptance scenarios, run on real artifacts;
  - `check-pin` and `bump-manifests`.
- `.github/workflows/release.yml`, rewritten: `verify`, then `binaries`
  (5 targets), then `assemble` (manifest, rehearsal, read-only plan against
  the real registries), then `draft`, then `publish` in the `release`
  environment. A pull request touching the pipeline, or a manual dispatch,
  is a dry run through `assemble`.
- `scripts/package_release.sh` changes:
  - new options `--target`, `--no-default-features`, `--notices` and
    `--no-notices`;
  - archives are named by target triple, and Windows gets a `.zip`;
  - archives ship `LICENSE-MIT`, `LICENSE-APACHE`, `LICENSE-OFL`,
    `fonts/LICENSE`, `THIRD_PARTY_NOTICES.md`, `licenses/assets.json` and
    `THIRD_PARTY_LICENSES.html` (cargo-about, from `about.toml` and
    `about.hbs`);
  - the archive listing is checked for all of those;
  - a cross-built binary the host cannot run is reported, not failed.
- `scripts/check_workflows.py` (+ tests) enforces:
  - every action is SHA-pinned;
  - `id-token: write` appears only in a `release` environment job (and the
    `github-pages` deploy);
  - non-`GITHUB_TOKEN` secrets are used only in `release` environment jobs;
  - no `pull_request_target`;
  - the release workflows grant per-job, least-privilege permissions.

  Every workflow's actions are now pinned. `pages.yml` moved its OIDC grant
  from workflow level to the deploy job. CI has a new `release-policy` job
  (no Rust build).
- Docs:
  - `docs/dev/RELEASING.md`;
  - the `release` issue template;
  - the `SDLC.md` release lifecycle section, plus the matching `release:`
    frontmatter key and template section in the sdlcify skill. Only the
    prose was applied by hand, because sdlcify's apply phase writes branch
    protection, which an agent must not change.
  - `RELEASING-FONTS.md` (#616 guard, environment);
  - the getting-started install section (stale `fonts/catalog/` text,
    archive table, headless Linux);
  - `THIRD_PARTY_NOTICES.md`;
  - HANDOFF.

## Decisions

- **Linux musl binaries are headless.** A static musl binary cannot `dlopen`
  libX11/libwayland, so the window could never open. glibc with the window
  is the alternative, recorded in RELEASING.md for the owner to revisit.
- **macOS x86_64 is cross-compiled on the arm64 runner.** GitHub is retiring
  its Intel images. The bundle check runs it when Rosetta is there and says
  so when it is not.
- **Identity across reruns.** The binaries are not bit-reproducible. A draft
  that already carries a `release-manifest.json` is therefore judged by that
  manifest, after its provenance (tag, version, commit) is checked. A rerun
  with rebuilt binaries adopts a complete draft instead of halting. crates.io
  compares the API checksum first, then the file contents plus the
  `.cargo_vcs_info.json` sha1, so a repackaged `.crate` from the same commit
  matches.
- **The draft is created before the approval** (`contents: write`, no
  environment), so the owner reviews the draft and then approves. Only
  `publish` holds `id-token: write`.
- **The bootstrap is resolved by the resume logic.** The owner's manual first
  `cargo publish` from the tag is just "already published, same commit and
  contents", so the `crates` stage skips it.
- **Plugin stage.** No plugin exists yet (#569). The stage checks that every
  `.claude-plugin/*.json` carries the release version, which `release.sh`
  sets. Pointing a marketplace at a release ref is #569's design.

## Deferred

- #635 build-provenance attestations for the archives
- #636 Dependabot to keep the SHA pins current
- #637 `pscat --version`
- #638 an automated post-publish install check
