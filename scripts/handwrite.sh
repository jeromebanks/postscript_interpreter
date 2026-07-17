#!/usr/bin/env bash
# handwrite.sh — string in, handwritten PNG out (Stage 13).
#
#   ./scripts/handwrite.sh "meet me at the bandshell at nine"
#   ./scripts/handwrite.sh --size 48 --paper ruled -o note.png "buy milk"
#
# Renders the text in the /HandScript dynamic font (lib/handscript.ps),
# word-wrapped across lines like a person filling a page. The page
# height is auto-sized: a headless hs-linecount pre-pass measures the
# wrap, then the real render runs at exactly the height the text needs.
# All business logic lives in the PostScript library; this script only
# builds the options dict and runs pscat twice.
#
# The font is lowercase-only, so input is lowercased. Unknown
# characters advance invisibly rather than erroring.
set -euo pipefail

usage() {
  cat <<'EOF'
usage: handwrite.sh [options] "text to write"

  -o, --out PATH     output PNG (default handwritten.png)
      --size N       font size in points (default 36)
      --width N      page width in points (default 612)
      --height N     page height in points (default: auto-sized to fit)
      --margin N     page margin in points (default 54)
      --leading F    baseline spacing, multiple of size (default 1.5)
      --jitter N     ink wobble in glyph units, 0=calm (default 16)
      --pen N        stroke width in glyph units (default 22)
      --ink R,G,B    ink color, components 0..1 (default 0.13,0.14,0.34)
      --paper STYLE  plain | ruled (default plain)
      --seed N       rand seed; same seed, same page (default 1985)
      --dpi N        render resolution (default 72)
      --halftone     screen the output like a mono laser printer
  -h, --help         this text

Newlines in the text force line breaks; blank lines are kept.
EOF
}

ROOT=$(cd "$(dirname "$0")/.." && pwd)
LIB="$ROOT/lib/handscript.ps"
BIN="$ROOT/target/release/pscat"

TEXT="" OUT="handwritten.png"
SIZE=36 WIDTH=612 HEIGHT="" MARGIN=54 LEADING=1.5
JITTER=16 PEN=22 INK="0.13,0.14,0.34" PAPER=plain SEED=1985
DPI="" HALFTONE=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    -h|--help) usage; exit 0 ;;
    -o|--out) OUT=$2; shift 2 ;;
    --size) SIZE=$2; shift 2 ;;
    --width) WIDTH=$2; shift 2 ;;
    --height) HEIGHT=$2; shift 2 ;;
    --margin) MARGIN=$2; shift 2 ;;
    --leading) LEADING=$2; shift 2 ;;
    --jitter) JITTER=$2; shift 2 ;;
    --pen) PEN=$2; shift 2 ;;
    --ink) INK=$2; shift 2 ;;
    --paper) PAPER=$2; shift 2 ;;
    --seed) SEED=$2; shift 2 ;;
    --dpi) DPI=$2; shift 2 ;;
    --halftone) HALFTONE="--halftone"; shift ;;
    --) shift; TEXT=${1:-}; shift $# ;;
    -*) echo "handwrite.sh: unknown option: $1" >&2; usage >&2; exit 1 ;;
    *)
      if [[ -n "$TEXT" ]]; then
        echo "handwrite.sh: more than one text argument (quote your string)" >&2
        exit 1
      fi
      TEXT=$1; shift ;;
  esac
done

[[ -n "$TEXT" ]] || { echo "handwrite.sh: no text given" >&2; usage >&2; exit 1; }
case "$PAPER" in plain|ruled) ;; *)
  echo "handwrite.sh: --paper must be plain or ruled, got: $PAPER" >&2; exit 1 ;;
esac

[[ -x "$BIN" ]] || cargo build --release --manifest-path "$ROOT/Cargo.toml"

# Lowercase (the font has no capitals) and escape \ ( ) for the
# PostScript string literal. Embedded newlines pass through — a
# PostScript string may span lines.
ESCAPED=$(printf '%s' "$TEXT" \
  | tr '[:upper:]' '[:lower:]' \
  | sed -e 's/\\/\\\\/g' -e 's/(/\\(/g' -e 's/)/\\)/g')

opts_dict() { # $1 = height to declare
  cat <<EOF
/HandOpts <<
  /Text ($ESCAPED)
  /Size $SIZE
  /Width $WIDTH
  /Height $1
  /Margin $MARGIN
  /Leading $LEADING
  /Jitter $JITTER
  /Pen $PEN
  /Ink [ ${INK//,/ } ]
  /Paper /$PAPER
  /Seed $SEED
>> def
EOF
}

TMP=$(mktemp -d "${TMPDIR:-/tmp}/handwrite.XXXXXX")
trap 'rm -rf "$TMP"' EXIT

if [[ -z "$HEIGHT" ]]; then
  # Pre-pass: how many lines will the wrap produce? (Height doesn't
  # affect wrapping, so a placeholder is fine here.)
  { cat "$LIB"; opts_dict 792; echo "HandOpts hs-linecount ="; } > "$TMP/count.ps"
  LINES=$("$BIN" --headless "$TMP/count.ps")
  HEIGHT=$(awk -v n="$LINES" -v m="$MARGIN" -v s="$SIZE" -v l="$LEADING" \
    'BEGIN { if (n < 1) n = 1
             h = 2*m + 1.05*s + (n-1)*l*s
             printf "%d", (h == int(h)) ? h : int(h) + 1 }')
fi

{ cat "$LIB"; opts_dict "$HEIGHT"; echo "HandOpts hs-write showpage"; } > "$TMP/render.ps"
# shellcheck disable=SC2086  # DPI/HALFTONE are deliberately word-split
"$BIN" --page "${WIDTH}x${HEIGHT}" ${DPI:+--dpi "$DPI"} $HALFTONE \
  --png "$OUT" "$TMP/render.ps"
