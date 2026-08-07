#!/usr/bin/env bash
# package_release.sh — build pscat + pscat-mcp and assemble a
# standalone, installable bundle: the binaries plus the runtime
# assets they need (lib/, fonts/catalog/, scripts/handwrite.sh,
# scripts/photo_etch.sh), with no git checkout or cargo required to
# use it afterward.
#
#   ./scripts/package_release.sh
#   ./scripts/package_release.sh /tmp/out         # custom output dir
#
# The bundle layout is deliberate: pscat/pscat-mcp sit at the bundle
# root so `src/paths.rs`'s exe-sibling resolution finds lib/ and
# fonts/catalog/ with zero configuration — unzip, add the bundle dir
# to PATH, done. $PSCAT_ROOT still works as an explicit override if
# the binaries get moved elsewhere. examples/ and gallery/ are left
# out — nothing at runtime (lib/*.ps, src/) depends on them; they're
# demo material for the checkout, not the installed tool.
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
OUT_BASE="${1:-$ROOT/dist}"

VERSION=$(awk -F'"' '/^version/{print $2; exit}' "$ROOT/Cargo.toml")
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)
NAME="pscat-${VERSION}-${OS}-${ARCH}"
BUNDLE="$OUT_BASE/$NAME"

echo "==> Building pscat and pscat-mcp (release)"
cargo build --release --manifest-path "$ROOT/Cargo.toml"

echo "==> Assembling $BUNDLE"
rm -rf "$BUNDLE"
mkdir -p "$BUNDLE"
cp "$ROOT/target/release/pscat" "$BUNDLE/pscat"
cp "$ROOT/target/release/pscat-mcp" "$BUNDLE/pscat-mcp"
cp -R "$ROOT/lib" "$BUNDLE/lib"
mkdir -p "$BUNDLE/fonts"
cp -R "$ROOT/fonts/catalog" "$BUNDLE/fonts/catalog" # wholesale, license .txt files included
mkdir -p "$BUNDLE/scripts"
cp "$ROOT/scripts/handwrite.sh" "$BUNDLE/scripts/handwrite.sh"
chmod +x "$BUNDLE/scripts/handwrite.sh"
cp "$ROOT/scripts/photo_etch.sh" "$BUNDLE/scripts/photo_etch.sh"
chmod +x "$BUNDLE/scripts/photo_etch.sh"

cat > "$BUNDLE/README.md" <<EOF
# pscat $VERSION ($OS/$ARCH)

A standalone pscat install — no git checkout or cargo needed.

## Use

Add this directory to your \`PATH\`, then run \`pscat\` from anywhere:

    export PATH="\$PWD:\$PATH"
    pscat --png out.png some_file.ps

\`lib/\`, \`fonts/catalog/\`, \`scripts/handwrite.sh\`, and
\`scripts/photo_etch.sh\` are found automatically as long as they stay
next to the \`pscat\` binary (this directory). If you move the binary
elsewhere, set \`PSCAT_ROOT\` to this directory instead.

## What's here

- \`pscat\`, \`pscat-mcp\` — the interpreter and its MCP server
- \`lib/\` — artkit.ps, handscript.ps, etching.ps, the Type 3 font programs, style packs
- \`fonts/catalog/\` — the bundled TrueType/OpenType font catalog
- \`scripts/handwrite.sh\` — the handwritten-note tool
- \`scripts/photo_etch.sh\` — the photo-to-line-etching tool

Full docs, source, and the \`examples/\`/\`gallery/\` demo material:
https://github.com/jeromebanks/postscript_interpreter
EOF

echo "==> Taring"
mkdir -p "$OUT_BASE"
tar -czf "$OUT_BASE/$NAME.tar.gz" -C "$OUT_BASE" "$NAME"

echo "==> Verifying the bundle resolves lib/ and fonts/catalog/ standalone"
# From /tmp (not the bundle, not the checkout) with no $PSCAT_ROOT set,
# so the only thing that can make this work is exe-sibling resolution
# (src/paths.rs) finding lib/ and fonts/catalog/ next to the binary —
# exactly the scenario "unzip + add to PATH" puts a user in.
cd /tmp
FONTS=$(env -u PSCAT_ROOT "$BUNDLE/pscat" --fonts)
echo "$FONTS" | grep -q "Bangers" ||
  { echo "FAIL: fonts/catalog/ did not resolve standalone" >&2; exit 1; }
env -u PSCAT_ROOT "$BUNDLE/pscat" -e '(lib/artkit.ps) run' ||
  { echo "FAIL: lib/artkit.ps did not resolve standalone" >&2; exit 1; }
env -u PSCAT_ROOT "$BUNDLE/pscat" -e '(lib/etching.ps) run' ||
  { echo "FAIL: lib/etching.ps did not resolve standalone" >&2; exit 1; }

# The other realistic install shape: a symlink into some bin/ dir
# pointing back at the bundle (what `brew`, and this project's own
# install.sh, do) rather than the bundle itself being on PATH.
# current_exe() can return the symlink's own path rather than the
# real binary's — caught a real bug here during development — so
# this is worth pinning at packaging time, not just trusting the
# non-symlinked case above.
SYMLINK_DIR=$(mktemp -d)
ln -s "$BUNDLE/pscat" "$SYMLINK_DIR/pscat"
env -u PSCAT_ROOT "$SYMLINK_DIR/pscat" -e '(lib/artkit.ps) run' ||
  { echo "FAIL: lib/artkit.ps did not resolve via a symlinked pscat" >&2; exit 1; }
rm -rf "$SYMLINK_DIR"
cd "$ROOT"

echo "==> Done: $OUT_BASE/$NAME.tar.gz"
du -sh "$OUT_BASE/$NAME.tar.gz"
