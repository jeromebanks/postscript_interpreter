#!/usr/bin/env bash
# release.sh X.Y.Z — open the release PR for pscat X.Y.Z (issue #602).
#
#   ./scripts/release.sh 0.6.0                    # branch, bump, changelog, push, open the PR
#   ./scripts/release.sh 0.6.0 --no-push          # stop after the local commit (a dry run)
#   ./scripts/release.sh 0.6.0 --no-push --allow-unpinned   # dry runs only, see below
#
# From an up-to-date, clean `main` it:
#   1. refuses unless X.Y.Z is newer than Cargo.toml's version, the tag vX.Y.Z
#      does not exist yet, and src/fontpacks/pinned.rs pins a font index (the
#      first fonts-* release must be published and pinned first: #616,
#      docs/dev/RELEASING-FONTS.md);
#   2. creates the branch release/vX.Y.Z;
#   3. sets the version in Cargo.toml, pscat's Cargo.lock entry, and every
#      plugin/npm/MCP Registry manifest that exists
#      (`python3 scripts/release_stages.py bump-manifests`);
#   4. prepends the release's section to CHANGELOG.md with git-cliff
#      (cliff.toml);
#   5. commits "release: X.Y.Z", pushes the branch and opens the release PR
#      with the checklist from docs/dev/RELEASING.md.
#
# Merging that PR publishes nothing. Pushing the tag does: after the merge,
#   git switch main && git pull && git tag -a vX.Y.Z -m "pscat X.Y.Z" && git push origin vX.Y.Z
# starts .github/workflows/release.yml (docs/dev/RELEASING.md).
#
# --allow-unpinned skips the pin check so the script can be exercised before
# the first font release (scripts/test_release_stages.py does). The release
# workflow refuses an unpinned tag regardless.
set -euo pipefail

die() { echo "release.sh: $*" >&2; exit 1; }

VERSION="${1:-}"
[[ -n "$VERSION" && "$VERSION" != -* ]] || die "usage: scripts/release.sh X.Y.Z [--no-push] [--allow-unpinned]"
shift
PUSH=1
CHECK_PIN=1
for arg in "$@"; do
  case "$arg" in
    --no-push) PUSH=0 ;;
    --allow-unpinned) CHECK_PIN=0 ;;
    *) die "unknown option $arg" ;;
  esac
done

[[ "$VERSION" =~ ^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]] || die "$VERSION is not X.Y.Z"
TAG="v$VERSION"
BRANCH="release/$TAG"

ROOT=$(git rev-parse --show-toplevel)
cd "$ROOT"

CURRENT=$(python3 -c 'import sys; sys.path.insert(0, "scripts"); import release_stages as r; print(r.cargo_version())')
python3 - "$CURRENT" "$VERSION" <<'EOF' || die "$VERSION is not newer than the current version $CURRENT"
import sys
cur, new = (tuple(int(x) for x in v.split(".")) for v in sys.argv[1:3])
sys.exit(0 if new > cur else 1)
EOF

[[ -z "$(git status --porcelain --untracked-files=no)" ]] || die "the working tree has uncommitted changes"
[[ "$(git branch --show-current)" == main ]] || die "run this from main (on $(git branch --show-current))"
if [[ -z "${RELEASE_SKIP_FETCH:-}" ]]; then
  git fetch --quiet --tags origin main
  [[ "$(git rev-parse HEAD)" == "$(git rev-parse origin/main)" ]] || die "main is not up to date with origin/main"
  [[ -z "$(git ls-remote --tags origin "refs/tags/$TAG")" ]] || die "the tag $TAG already exists on origin"
fi
! git rev-parse -q --verify "refs/tags/$TAG" >/dev/null || die "the tag $TAG already exists"
! git rev-parse -q --verify "refs/heads/$BRANCH" >/dev/null || die "the branch $BRANCH already exists"

if [[ "$CHECK_PIN" == 1 ]]; then
  python3 scripts/release_stages.py check-pin || die "refusing to release with no pinned font index"
fi
command -v git-cliff >/dev/null || die "git-cliff is not installed (cargo install git-cliff, or brew install git-cliff)"

git switch --quiet -c "$BRANCH"
echo "==> Setting the version to $VERSION"
CHANGED=()
while IFS= read -r f; do CHANGED+=("$f"); done < <(python3 scripts/release_stages.py bump-manifests "$VERSION")
printf '    %s\n' "${CHANGED[@]}"

echo "==> CHANGELOG.md"
if [[ -f CHANGELOG.md ]]; then
  git cliff --unreleased --tag "$TAG" --prepend CHANGELOG.md
else
  git cliff --unreleased --tag "$TAG" --output CHANGELOG.md
fi
grep -q "^## \[$VERSION\]" CHANGELOG.md || die "git-cliff wrote no section for $VERSION"

git add -- "${CHANGED[@]}" CHANGELOG.md
git commit --quiet -m "release: $VERSION"
echo "==> Committed release: $VERSION on $BRANCH"

if [[ "$PUSH" == 0 ]]; then
  echo "Not pushed (--no-push). Review with: git show --stat HEAD"
  exit 0
fi

git push -u origin "$BRANCH"
BODY=$(cat <<EOF
Release pscat $VERSION. Opened by \`scripts/release.sh $VERSION\`. The release
procedure is [docs/dev/RELEASING.md](docs/dev/RELEASING.md).

This PR only sets the version (Cargo.toml, Cargo.lock, any plugin/npm
manifests) and adds the CHANGELOG.md section. Edit the changelog wording here
if needed. **Merging publishes nothing.** After the merge, the owner pushes the
tag \`$TAG\`, and that starts \`release.yml\`.

## Release checklist

- [ ] \`src/fontpacks/pinned.rs\` pins the font index this release ships with
- [ ] CI is green, including \`semver\` (the summary says whether this version number fits the API change)
- [ ] The MCP safety tests (\`tests/mcp.rs\`) and the \`mcp\` Inspector check pass
- [ ] README's install section lists only the channels this release ships
- [ ] SECURITY.md, THIRD_PARTY_NOTICES.md and docs/STABILITY.md are accurate
- [ ] CHANGELOG.md reads well
- [ ] After merging: tag, approve the \`release\` environment, then run the outsider install smoke test (RELEASING.md)
EOF
)
gh pr create --base main --head "$BRANCH" --title "release: $VERSION" --body "$BODY"
