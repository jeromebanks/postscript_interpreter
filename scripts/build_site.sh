#!/usr/bin/env bash
# build_site.sh — assemble the GitHub Pages site into _site/ (Stage 17).
#
#   ./scripts/build_site.sh
#   python3 -m http.server -d _site       # preview locally
#
# Pieces: the authored pages (site/), the wasm build + JS library,
# playground example sources (self-contained files only — the wasm
# build has no filesystem), and PNG renders of every gallery piece
# and example, produced by pscat itself.
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
OUT="$ROOT/_site"

rm -rf "$OUT"
mkdir -p "$OUT/assets/renders" "$OUT/examples"

# 1. the authored pages
cp "$ROOT"/site/*.html "$ROOT"/site/style.css "$OUT/"

# 2. the browser build
"$ROOT/scripts/build_wasm.sh"
cp "$ROOT/web/pscat.wasm" "$ROOT/web/pscat.js" "$OUT/"

# 3. playground example sources
for f in examples/sierpinski.ps examples/koch_snowflake.ps \
         examples/golden_spiral.ps examples/specimen.ps \
         examples/testcard.ps examples/handwriting.ps \
         gallery/fern.ps gallery/golden_bloom.ps \
         gallery/cathedral_rose.ps gallery/hundred_lines.ps; do
  cp "$ROOT/$f" "$OUT/examples/"
done

# 4. renders — gallery stills are pre-rendered in the repo; examples
# render here at their canonical sizes (BoundingBox where declared).
cp "$ROOT"/gallery/renders/*.png "$OUT/assets/renders/"
cp "$ROOT/lib/fonts/specimen.png" "$OUT/assets/renders/font_library.png"
cp "$ROOT/lib/fonts/specimen2.png" "$OUT/assets/renders/font_library2.png"
cp "$ROOT/fonts/catalog/specimen-1.png" "$OUT/assets/renders/font_catalog1.png"
cp "$ROOT/fonts/catalog/specimen-2.png" "$OUT/assets/renders/font_catalog2.png"
cp "$ROOT"/lib/styles/specimen_*.png "$OUT/assets/renders/"

# 5. the font gallery — one card per face (site/fonts.html)
"$ROOT/scripts/build_font_gallery.sh" "$OUT/assets/fonts"

cargo build --release --manifest-path "$ROOT/Cargo.toml"
BIN="$ROOT/target/release/pscat"
render() { # file WxH
  "$BIN" --page "$2" --png "$OUT/assets/renders/$(basename "$1" .ps).png" "$ROOT/$1"
}
render examples/sierpinski.ps 500x500
render examples/koch_snowflake.ps 500x500
render examples/golden_spiral.ps 500x500
render examples/testcard.ps 612x792
render examples/specimen.ps 612x792
render examples/postcard.ps 612x792
render examples/type3_ransom.ps 612x792
render examples/handwriting.ps 612x792

echo "site assembled: $OUT ($(du -sh "$OUT" | cut -f1))"
