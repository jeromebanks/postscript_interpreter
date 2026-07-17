#!/usr/bin/env bash
# Display the gallery, one piece at a time.
#
#   ./gallery/show.sh          step through the rendered PNGs; close each
#                              Quick Look window (or press q) to advance
#   ./gallery/show.sh --live   run each .ps in a live pscat window instead,
#                              so you watch every piece draw itself;
#                              close the window to move on
set -euo pipefail
cd "$(dirname "$0")/.."

PIECES=(golden_bloom cathedral_rose ember_tree fern silk_waves frost_mandala ring_of_type hundred_lines)
PAGES=(700x700 700x700 620x820 620x800 700x480 700x700 700x700 560x620)
SPEEDS=(150 60 400 3000 150 150 200 250)

if [[ "${1:-}" == "--live" ]]; then
  BIN=target/release/pscat
  [[ -x "$BIN" ]] || cargo build --release
  for i in "${!PIECES[@]}"; do
    echo ">> ${PIECES[$i]}  (close the window for the next piece)"
    "$BIN" --page "${PAGES[$i]}" --speed "${SPEEDS[$i]}" "gallery/${PIECES[$i]}.ps"
  done
else
  for p in "${PIECES[@]}"; do
    img="gallery/renders/$p.png"
    if [[ ! -f "$img" ]]; then
      echo "!! missing $img — see gallery/README.md to re-render" >&2
      continue
    fi
    echo ">> $p"
    if command -v qlmanage >/dev/null 2>&1; then
      # macOS Quick Look: blocks until the preview window closes
      qlmanage -p "$img" >/dev/null 2>&1
    else
      (xdg-open "$img" 2>/dev/null || open "$img") &
      read -r -p "   press enter for the next piece... "
    fi
  done
fi
echo "fin."
