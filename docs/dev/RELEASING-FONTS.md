# Releasing font packs

The font catalog (`fonts/catalog/`) ships to installed pscat builds as
downloadable font packs on their own release train, separate from
pscat's `v*` releases (issue #599). User-facing docs:
[`docs/guides/fonts.md`](../guides/fonts.md#font-packs-the-catalog-outside-a-checkout).

## The pieces

| Piece | What it is |
|---|---|
| `fonts/packs.json` | Which pack each catalog family ships in, with its font files. Compiled into pscat (a missing face names its pack). `cargo test` checks it against `fonts/catalog/`. |
| `licenses/assets.json` | Each family's SPDX id, copyright line and licence files (#563). The builder copies these into every pack's manifest. Nothing is relicensed. |
| `scripts/font_packs.py` | `check`, `build --tag T --out DIR [--pack NAME]`, `pin --index FILE`. |
| `scripts/font_pack_smoke.sh` | Installs built packs into a scratch HOME outside the checkout and checks them: the missing-pack errors, refusal of a corrupted pack, verification, pinned status, pixel identity with `fonts/catalog/`, and remove/reinstall. |
| `.github/workflows/release-fonts.yml` | Builds, pins a throwaway pscat, runs the smoke test, uploads the packs, and (on a real release only) publishes. |
| `src/fontpacks/pinned.rs` | The index tag and SHA-256 this pscat installs by default. |

## Formats

- **Pack**: `<pack>-<tag>.tar.zst`, a plain ustar archive (regular files
  only, sorted, mtime 0) compressed with `zstd -19`. It holds
  `<Family>/<file>` for the fonts and their upstream licence files,
  copied byte for byte, plus `manifest.json`.
- **`manifest.json`**: `manifest_schema` (now 1), `pack`, `tag`,
  `description`, and `files`, where each file has `path`, `kind` (`font` or
  `license`), `license` (SPDX), `copyright`, `sha256`, `size`, and for
  fonts `faces` (the findfont name, the file stem).
- **`font-index.json`**: `index_schema` (now 1), `tag`, and `packs`, where
  each pack has `name`, `file`, `size`, `sha256`, `manifest_schema`,
  `files` and `description`.
- **`SHA256SUMS`**: covers the packs and the index.

`manifest_schema` and `index_schema` are versioned separately from both
release trains. pscat refuses a schema newer than it reads, and says a
newer pscat may be needed. Bump a schema only for an incompatible
change.

## Cutting a font release

Needs the protected `release` environment and the repository variable
`FONTS_RELEASE_ENABLED=true`, both owner steps in
[`RELEASING.md`](RELEASING.md#one-time-setup-owner) (steps 1 and 4). Until
then, tag pushes build and smoke-test but do not publish, and the dry run
below is the whole process. The publish job runs in the `release`
environment, so it waits for the owner's approval like an interpreter
release.

1. Dry run: Actions → release-fonts → Run workflow, or any PR touching
   the packs. Download the `font-packs-*` artifact if you want to look
   at it.
2. Tag and push: `git tag fonts-2026.10 && git push origin fonts-2026.10`.
   The format is CalVer, `fonts-YYYY.MM`; a fix in the same month is
   `fonts-YYYY.MM.N`.
3. The publish job creates the GitHub Release with the packs, the index
   and `SHA256SUMS`. It refuses if a release for the tag already exists.

**Tags and their assets are immutable.** Never delete and re-push a
`fonts-*` tag, and never re-upload an asset. A pscat release may already
pin the old index's SHA-256, and every install of it would then fail its
checksum. Fix forward with a new tag.

## Pinning a pscat release

Each pscat release pins one font index, so a plain `pscat fonts
install` always fetches the same bytes:

1. Download the release's `font-index.json`:
   `gh release download fonts-2026.10 -p font-index.json`.
2. `python3 scripts/font_packs.py pin --index font-index.json`. This
   rewrites `PINNED_INDEX` in `src/fontpacks/pinned.rs` with the tag and
   the file's SHA-256. Never edit the constant by hand.
3. Commit it in the release PR. Check with `pscat fonts install` from a
   scratch `PSCAT_HOME`, then `pscat fonts list`, which should say
   `pinned`.

While `PINNED_INDEX` is `None` (every build until the first font
release), a plain `pscat fonts install` tells the user to choose
`--version` or `--from`. It never falls back to `latest`. An interpreter
release with `None` is refused (#616): `scripts/release.sh` will not open the
release PR, and the release workflow's `verify` job fails a `v*` tag whose
`PINNED_INDEX` is `None` or whose pinned `font-index.json` does not hash to
the pinned SHA-256 (`python3 scripts/release_stages.py check-pin`).

## Transport

`pscat fonts install` downloads with the system `curl` (HTTPS only, for
redirects too). That is pscat's only network access, and it happens only
when the user runs that command. Integrity comes from the SHA-256 chain,
not from TLS. The reasons for curl over an HTTP crate are in
`src/fontpacks/install.rs` (`Curl`).
