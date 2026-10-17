#!/usr/bin/env bash
# package_release.sh — build pscat + pscat-mcp and assemble a standalone,
# installable archive: the binaries, the runtime assets they need (lib/),
# and the licence files, with no git checkout or cargo needed to use it.
# The handwrite, typewrite and photo-etch tools are `pscat` subcommands
# (issue #600), so the bundle carries no scripts.
#
#   ./scripts/package_release.sh                           # host target, into dist/
#   ./scripts/package_release.sh /tmp/out                  # custom output dir
#   ./scripts/package_release.sh --target x86_64-unknown-linux-musl --no-default-features
#   ./scripts/package_release.sh --notices THIRD_PARTY_LICENSES.html
#
# Options:
#   --target TRIPLE          cross-build for TRIPLE (default: the host's)
#   --no-default-features    build without the live window (the `window`
#                            feature); the release's Linux musl builds use it,
#                            see docs/dev/RELEASING.md
#   --notices FILE           the cargo-about dependency notices to bundle as
#                            THIRD_PARTY_LICENSES.html. Without it the script
#                            runs `cargo about generate` itself (about.toml),
#                            and fails if cargo-about is not installed.
#   --no-notices             leave the dependency notices out (local builds
#                            only; the release workflow always bundles them)
#   --out DIR                output directory (default: dist/)
#
# Output: DIR/pscat-<version>-<target>.tar.gz (.zip for Windows targets),
# holding one directory of the same name. The bundle layout is deliberate:
# pscat/pscat-mcp sit at the bundle root so `src/paths.rs`'s exe-sibling
# resolution finds lib/ with zero configuration — unpack, add the bundle dir
# to PATH, done. $PSCAT_HOME (or $PSCAT_ROOT) still works as an explicit
# override if the binaries get moved elsewhere. examples/ and gallery/ are
# left out: nothing at runtime depends on them. The font catalog is not
# bundled either: it ships as separately versioned font packs that `pscat
# fonts install` downloads (issue #599).
#
# Licences (issue #563, #602): LICENSE-MIT and LICENSE-APACHE (the code),
# LICENSE-OFL and fonts/LICENSE (the Liberation faces and the Courier
# Prime-derived lib/typewriter_outlines.ps compiled into the binaries),
# THIRD_PARTY_NOTICES.md with licenses/assets.json, and the cargo-about
# notices for every Rust dependency linked into the binaries.
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)

TARGET=""
FEATURES=()
NOTICES=""
WANT_NOTICES=1
OUT_BASE="$ROOT/dist"
while [[ $# -gt 0 ]]; do
  case "$1" in
    --target) TARGET="$2"; shift 2 ;;
    --no-default-features) FEATURES+=(--no-default-features); shift ;;
    --notices) NOTICES="$2"; shift 2 ;;
    --no-notices) WANT_NOTICES=0; shift ;;
    --out) OUT_BASE="$2"; shift 2 ;;
    -*) echo "unknown option $1" >&2; exit 2 ;;
    *) OUT_BASE="$1"; shift ;;
  esac
done
mkdir -p "$OUT_BASE"
OUT_BASE=$(cd "$OUT_BASE" && pwd)
if [[ -n "$NOTICES" ]]; then
  NOTICES="$(cd "$(dirname "$NOTICES")" && pwd)/$(basename "$NOTICES")"
fi

HOST=$(rustc -vV | sed -n 's/^host: //p')
TARGET="${TARGET:-$HOST}"
VERSION=$(awk -F'"' '/^version/{print $2; exit}' "$ROOT/Cargo.toml")
NAME="pscat-${VERSION}-${TARGET}"
BUNDLE="$OUT_BASE/$NAME"
EXE=""
WINDOWS=0
case "$TARGET" in *windows*) EXE=".exe"; WINDOWS=1 ;; esac
HEADLESS=0
[[ ${#FEATURES[@]} -gt 0 ]] && HEADLESS=1
# On Windows runners `python3` can be the Microsoft Store stub; `python` is real.
if [[ "$WINDOWS" == 1 ]]; then PY=$(command -v python || command -v python3); else PY=$(command -v python3 || command -v python); fi

echo "==> Building pscat and pscat-mcp (release, $TARGET${FEATURES[*]:+, ${FEATURES[*]}})"
cargo build --release --locked --target "$TARGET" ${FEATURES[@]+"${FEATURES[@]}"} --manifest-path "$ROOT/Cargo.toml"
BIN_DIR="$ROOT/target/$TARGET/release"

if [[ "$WANT_NOTICES" == 1 && -z "$NOTICES" ]]; then
  command -v cargo-about >/dev/null ||
    { echo "cargo-about is not installed: install it (cargo install cargo-about), pass --notices FILE, or --no-notices" >&2; exit 1; }
  NOTICES="$OUT_BASE/THIRD_PARTY_LICENSES.html"
  echo "==> Dependency notices (cargo about generate)"
  (cd "$ROOT" && cargo about generate --locked --all-features --fail about.hbs -o "$NOTICES")
fi

echo "==> Assembling $BUNDLE"
rm -rf "$BUNDLE"
mkdir -p "$BUNDLE/licenses" "$BUNDLE/fonts"
cp "$BIN_DIR/pscat$EXE" "$BUNDLE/pscat$EXE"
cp "$BIN_DIR/pscat-mcp$EXE" "$BUNDLE/pscat-mcp$EXE"
cp -R "$ROOT/lib" "$BUNDLE/lib"
for f in LICENSE-MIT LICENSE-APACHE LICENSE-OFL THIRD_PARTY_NOTICES.md; do
  cp "$ROOT/$f" "$BUNDLE/$f"
done
cp "$ROOT/licenses/assets.json" "$BUNDLE/licenses/assets.json"
cp "$ROOT/fonts/LICENSE" "$BUNDLE/fonts/LICENSE"
if [[ "$WANT_NOTICES" == 1 ]]; then
  [[ -s "$NOTICES" ]] || { echo "FAIL: no dependency notices at $NOTICES" >&2; exit 1; }
  cp "$NOTICES" "$BUNDLE/THIRD_PARTY_LICENSES.html"
fi

# shellcheck disable=SC2016 # literal text for the README, not expansions
if [[ "$WINDOWS" == 1 ]]; then
  PATH_HINT='    set PATH=%CD%;%PATH%          (cmd)
    $env:PATH = "$PWD;$env:PATH"  (PowerShell)'
else
  PATH_HINT='    export PATH="$PWD:$PATH"'
fi
if [[ "$HEADLESS" == 1 ]]; then
  WINDOW_NOTE="This build is headless: it renders to PNG, SVG and PDF, but has no live
window (\`pscat file.ps\` with no output flag, \`--interactive\`, \`--spool\`).
A fully static Linux binary cannot load the system's X11/Wayland libraries.
For the window, build from source: \`cargo install pscat\`."
else
  WINDOW_NOTE="\`pscat file.ps\` with no output flag opens a live window and draws the
program as it runs."
fi

cat > "$BUNDLE/README.md" <<EOF
# pscat $VERSION ($TARGET)

A standalone pscat install — no git checkout or cargo needed.

## Use

Add this directory to your \`PATH\`, then run \`pscat\` from anywhere:

$PATH_HINT
    pscat --png out.png some_file.ps

$WINDOW_NOTE

\`lib/\` is found automatically as long as it stays
next to the \`pscat\` binary (this directory). If you move the binary
elsewhere, set \`PSCAT_HOME\` to this directory instead. (The
libraries are also compiled into \`pscat\`, so \`lib/\` keeps working
even then.)

## Fonts

The twelve Liberation faces (Helvetica, Times, Courier) are built in.
The rest of the catalog (Symbol, ZapfDingbats, Palatino and the other
standard-35 faces, the display faces, Korean/Japanese/Thai) comes as
font packs:

    pscat fonts install      # the packs this pscat release pins
    pscat fonts list

A program that asks for a face from a pack that is not installed stops
with an error naming the pack.

## What's here

- \`pscat\`, \`pscat-mcp\` — the interpreter and its MCP server
- \`lib/\` — artkit.ps, handscript.ps, etching.ps, the Type 3 font programs, style packs
- \`LICENSE-MIT\`, \`LICENSE-APACHE\` — pscat's licence (MIT OR Apache-2.0, your choice)
- \`LICENSE-OFL\`, \`fonts/LICENSE\` — the licence of the fonts compiled into pscat
- \`THIRD_PARTY_NOTICES.md\`, \`licenses/assets.json\` — what else is in pscat, and under which terms
- \`THIRD_PARTY_LICENSES.html\` — the licences of the Rust libraries linked into the binaries

The handwritten-note, typewritten-page and photo-to-line-etching tools are
subcommands of the binary: \`pscat handwrite\`, \`pscat typewrite\` and
\`pscat photo-etch\` (add \`--help\`). They need nothing but \`pscat\`.

Full docs, source, and the \`examples/\`/\`gallery/\` demo material:
https://github.com/jeromebanks/postscript_interpreter
EOF

echo "==> Archiving"
if [[ "$WINDOWS" == 1 ]]; then
  ARCHIVE="$OUT_BASE/$NAME.zip"
  rm -f "$ARCHIVE"
  (cd "$OUT_BASE" && "$PY" - "$NAME" "$NAME.zip" <<'EOF'
import os, sys, zipfile
src, dest = sys.argv[1], sys.argv[2]
with zipfile.ZipFile(dest, "w", zipfile.ZIP_DEFLATED) as z:
    for d, dirs, files in os.walk(src):
        dirs.sort()
        for f in sorted(files):
            p = os.path.join(d, f)
            z.write(p, p.replace(os.sep, "/"))
EOF
  )
else
  ARCHIVE="$OUT_BASE/$NAME.tar.gz"
  tar -czf "$ARCHIVE" -C "$OUT_BASE" "$NAME"
fi

# The archive itself holds the binaries and licence files (a check on the
# archive, not just the directory it was made from).
if [[ "$WINDOWS" == 1 ]]; then
  LISTING=$("$PY" -c 'import sys, zipfile; print("\n".join(zipfile.ZipFile(sys.argv[1]).namelist()))' "$ARCHIVE" | tr -d '\r')
else
  LISTING=$(tar -tzf "$ARCHIVE")
fi
REQUIRED=(LICENSE-MIT LICENSE-APACHE LICENSE-OFL THIRD_PARTY_NOTICES.md licenses/assets.json fonts/LICENSE
  README.md lib/artkit.ps "pscat$EXE" "pscat-mcp$EXE")
[[ "$WANT_NOTICES" == 1 ]] && REQUIRED+=(THIRD_PARTY_LICENSES.html)
for f in "${REQUIRED[@]}"; do
  grep -qx "$NAME/$f" <<<"$LISTING" || { echo "FAIL: $NAME/$f is not in the archive" >&2; exit 1; }
done

# ---------------------------------------------------------------------------
# Run the bundle. From outside the checkout with no $PSCAT_HOME, so exe-sibling
# resolution (src/paths.rs) must find lib/ next to the binary: the scenario
# "unpack + add to PATH" puts a user in. The binary also carries an embedded
# lib/, so the lib checks assert on the resolver trace that the bundle's copy
# served it.
# ---------------------------------------------------------------------------
bare() { env -u PSCAT_ROOT -u PSCAT_HOME -u PSCAT_LIB PSCAT_TRACE_RESOURCES=1 "$@"; }
PSCAT="$BUNDLE/pscat$EXE"
cd "${TMPDIR:-/tmp}"
if ! bare "$PSCAT" -e 'quit' >/dev/null 2>&1; then
  if [[ "$TARGET" != "$HOST" ]]; then
    echo "==> WARNING: cannot run $TARGET binaries on $HOST; the archive is checked but its binaries were not run"
    echo "==> Done: $ARCHIVE"
    exit 0
  fi
  echo "FAIL: $PSCAT does not run" >&2
  exit 1
fi

echo "==> Verifying the bundle resolves lib/ standalone, and needs font packs"
# With an empty HOME (so no font packs the packager installed), a pack
# face must stop with an error naming its pack, never substitute.
# (The program does not start with "/": Git Bash on Windows would rewrite
# an argument that looks like a POSIX path before pscat.exe saw it.)
EMPTY_HOME=$(mktemp -d)
OUT=$(HOME="$EMPTY_HOME" XDG_DATA_HOME="$EMPTY_HOME" APPDATA="$EMPTY_HOME" bare "$PSCAT" -e '0 pop /Bangers findfont pop' 2>&1) &&
  { echo "FAIL: /Bangers resolved with no font packs installed" >&2; exit 1; }
grep -q "is in the extras font pack" <<<"$OUT" ||
  { echo "$OUT" >&2; echo "FAIL: missing face did not name its font pack" >&2; exit 1; }
rm -rf "$EMPTY_HOME"
for lib in lib/artkit.ps lib/etching.ps; do
  TRACE=$(bare "$PSCAT" -e "($lib) run" 2>&1) || true
  grep -q "resource $lib <- exe-dir" <<<"$TRACE" ||
    { echo "$TRACE" >&2; echo "FAIL: $lib did not resolve from the bundle" >&2; exit 1; }
done

# The three tool subcommands need nothing but the binary (issue #600): no
# scripts/ beside it, no checkout, no shell. The photo is test input from
# the checkout, not an install dependency.
TOOLS=$(mktemp -d)
printf 'bundle check\n' > "$TOOLS/note.txt"
bare "$PSCAT" handwrite -o "$TOOLS/h.png" "bundle check" >/dev/null ||
  { echo "FAIL: pscat handwrite did not run from the bundle" >&2; exit 1; }
bare "$PSCAT" typewrite -o "$TOOLS/t.png" "$TOOLS/note.txt" >/dev/null ||
  { echo "FAIL: pscat typewrite did not run from the bundle" >&2; exit 1; }
bare "$PSCAT" photo-etch -o "$TOOLS/e.png" "$ROOT/tests/data/sphere.jpg" >/dev/null ||
  { echo "FAIL: pscat photo-etch did not run from the bundle" >&2; exit 1; }
for f in h t e; do
  [[ -s "$TOOLS/$f.png" ]] || { echo "FAIL: a tool subcommand wrote no PNG ($f)" >&2; exit 1; }
done
rm -rf "$TOOLS"

# The other realistic install shape: a symlink into some bin/ dir pointing
# back at the bundle (what `brew` does) rather than the bundle itself being
# on PATH. current_exe() can return the symlink's own path rather than the
# real binary's — caught a real bug here during development — so this is
# worth pinning at packaging time. Not on Windows (no plain symlinks there).
if [[ "$WINDOWS" == 0 ]]; then
  SYMLINK_DIR=$(mktemp -d)
  ln -s "$PSCAT" "$SYMLINK_DIR/pscat"
  TRACE=$(bare "$SYMLINK_DIR/pscat" -e '(lib/artkit.ps) run' 2>&1) || true
  grep -q "resource lib/artkit.ps <- exe-dir" <<<"$TRACE" ||
    { echo "FAIL: lib/artkit.ps did not resolve via a symlinked pscat" >&2; exit 1; }
  rm -rf "$SYMLINK_DIR"
fi
cd "$ROOT"

echo "==> Done: $ARCHIVE"
du -sh "$ARCHIVE"
