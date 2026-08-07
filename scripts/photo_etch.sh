#!/usr/bin/env bash
# photo_etch.sh — photo in, line-engraving out (issue #15).
#
#   ./scripts/photo_etch.sh photo.jpg
#   ./scripts/photo_etch.sh --max-dim 800 --angle 20 -o etching.svg photo.jpg
#
# Renders a tone-driven line engraving of a JPEG photo (lib/etching.ps)
# — the reproduction technique old newspapers and books actually used
# before halftone screens, not edge detection. A headless et-dims
# pre-pass reads the photo's own dimensions (from its SOF marker, no
# decode), sizing the output page to match its aspect ratio; the real
# render then reads and hatches it. All business logic lives in the
# PostScript library; this script only builds the options dict and
# runs pscat twice, same shape as scripts/handwrite.sh.
#
# JPEG input only — this interpreter has no PNG decode path in
# PostScript (see lib/etching.ps's header comment for why that's not
# a gap).
set -euo pipefail

usage() {
  cat <<'EOF'
usage: photo_etch.sh [options] photo.jpg

  -o, --out PATH     output file; format from extension: .png/.svg/.pdf
                      (default etching.png)
      --max-dim N     longest output page dimension, points (default 480)
      --spacing N     device units between hatch lines (default 3)
      --angle DEG     primary hatch angle (default -15)
      --threshold2 F  darkness above which crosshatch kicks in, 0..1
                      (default 0.55)
      --ink R,G,B     stroke color, components 0..1 (default 0.08,0.07,0.05)
      --paper R,G,B   background color, components 0..1 (default 0.96,0.94,0.88)
      --dpi N         render resolution (default 72)
  -h, --help          this text
EOF
}

ROOT=$(cd "$(dirname "$0")/.." && pwd)
LIB="$ROOT/lib/etching.ps"

# BIN resolution covers two layouts this same script file runs from
# unmodified: an installed bundle (pscat sits at the bundle root, this
# script one level down in scripts/) and a dev checkout (built via
# cargo, below) — same fallback chain as scripts/handwrite.sh.
if [[ -x "$ROOT/pscat" ]]; then
  BIN="$ROOT/pscat"
elif [[ -x "$ROOT/target/release/pscat" ]]; then
  BIN="$ROOT/target/release/pscat"
elif command -v pscat >/dev/null 2>&1; then
  BIN="$(command -v pscat)"
else
  BIN="$ROOT/target/release/pscat"
fi

PHOTO="" OUT="etching.png"
MAXDIM=480 SPACING=3 ANGLE=-15 THRESHOLD2=0.55
INK="0.08,0.07,0.05" PAPER="0.96,0.94,0.88" DPI=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    -h|--help) usage; exit 0 ;;
    -o|--out) OUT=$2; shift 2 ;;
    --max-dim) MAXDIM=$2; shift 2 ;;
    --spacing) SPACING=$2; shift 2 ;;
    --angle) ANGLE=$2; shift 2 ;;
    --threshold2) THRESHOLD2=$2; shift 2 ;;
    --ink) INK=$2; shift 2 ;;
    --paper) PAPER=$2; shift 2 ;;
    --dpi) DPI=$2; shift 2 ;;
    -*) echo "photo_etch.sh: unknown option: $1" >&2; usage >&2; exit 1 ;;
    *)
      if [[ -n "$PHOTO" ]]; then
        echo "photo_etch.sh: more than one photo argument" >&2
        exit 1
      fi
      PHOTO=$1; shift ;;
  esac
done

[[ -n "$PHOTO" ]] || { echo "photo_etch.sh: no photo given" >&2; usage >&2; exit 1; }
[[ -f "$PHOTO" ]] || { echo "photo_etch.sh: no such file: $PHOTO" >&2; exit 1; }

[[ -x "$BIN" ]] || cargo build --release --manifest-path "$ROOT/Cargo.toml"

# Absolute path: the generated .ps lives under mktemp -d, so a
# relative photo path would resolve against the wrong directory (the
# skill's own cwd-reset pitfall, same failure shape — silently opens
# nothing rather than erroring).
PHOTO_ABS="$(cd "$(dirname "$PHOTO")" && pwd)/$(basename "$PHOTO")"
ESCAPED_PHOTO=$(printf '%s' "$PHOTO_ABS" | sed -e 's/\\/\\\\/g' -e 's/(/\\(/g' -e 's/)/\\)/g')

TMP=$(mktemp -d "${TMPDIR:-/tmp}/photo_etch.XXXXXX")
trap 'rm -rf "$TMP"' EXIT

# Pre-pass: read width/height straight from the JPEG's own SOF marker
# (et-dims doesn't decode), so the output page matches the photo's
# aspect ratio instead of distorting it.
{
  cat "$LIB"
  echo "<< /Photo ($ESCAPED_PHOTO) >> et-dims"
  echo "/n exch def /h exch def /w exch def"
  echo "w == h == n =="
} > "$TMP/dims.ps"
read -r W H N <<< "$("$BIN" --headless "$TMP/dims.ps" | tr '\n' ' ')"

if (( W >= H )); then
  PAGE_W=$MAXDIM
  PAGE_H=$(awk -v w="$W" -v h="$H" -v m="$MAXDIM" 'BEGIN { printf "%d", (m * h / w) + 0.5 }')
else
  PAGE_H=$MAXDIM
  PAGE_W=$(awk -v w="$W" -v h="$H" -v m="$MAXDIM" 'BEGIN { printf "%d", (m * w / h) + 0.5 }')
fi

{
  cat "$LIB"
  cat <<EOF
<< /Photo ($ESCAPED_PHOTO)
   /PageWidth $PAGE_W
   /PageHeight $PAGE_H
   /Spacing $SPACING
   /Angle $ANGLE
   /Threshold2 $THRESHOLD2
   /Ink [ ${INK//,/ } ]
   /Paper [ ${PAPER//,/ } ]
>> et-draw showpage
EOF
} > "$TMP/render.ps"

OUTFLAG=--png
case "$OUT" in
  *.svg) OUTFLAG=--svg ;;
  *.pdf) OUTFLAG=--pdf ;;
  *.png) OUTFLAG=--png ;;
  *) echo "photo_etch.sh: --out must end in .png, .svg, or .pdf, got: $OUT" >&2; exit 1 ;;
esac

# shellcheck disable=SC2086  # DPI is deliberately word-split
"$BIN" --page "${PAGE_W}x${PAGE_H}" ${DPI:+--dpi "$DPI"} \
  "$OUTFLAG" "$OUT" "$TMP/render.ps"
