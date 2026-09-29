#!/usr/bin/env bash
# Render every piece of an apparel configuration (lib/apparel.ps) as its
# own artwork file, plus a reduced-size proof sheet and a manifest.
#
#   scripts/apparel_export.sh examples/apparel_shirt.ps out/shirt
#
# Writes, per configured piece (front, back, sleeve-left, sleeve-right --
# whichever the config defines):
#   <name>-<piece>.png|svg|pdf   full-size artwork, at the config's DPI
# and, once per run:
#   <name>-proof.png             every piece at reduced size, print areas
#                                and margins outlined (a check, not art)
#   <name>-manifest.json         sizes, DPI, background behaviour, fonts
#                                (+ licences) -- what a consumer needs to
#                                pick a product and check a print
#                                service's then-current requirements
#
# The config is the single source of truth: page size and DPI are asked of
# it (ApparelMode /size), never repeated here, and the renderer is plain
# `pscat --page --dpi --png/--svg/--pdf` -- this script only sequences it.
# Artwork is exported with --transparent unless the config sets an RGB
# /Background (a mock, not garment art). Set PSCAT to use another binary.
set -euo pipefail

[ $# -eq 2 ] || { echo "usage: $0 CONFIG.ps OUTDIR" >&2; exit 2; }
CONFIG="$(cd "$(dirname "$1")" && pwd)/$(basename "$1")"
mkdir -p "$2"
OUT="$(cd "$2" && pwd)"
# (lib/...) run resolves against the cwd, so pin it to the repo root.
cd "$(dirname "$0")/.."
PSCAT="${PSCAT:-target/release/pscat}"
[ -x "$PSCAT" ] || PSCAT="target/debug/pscat"
[ -x "$PSCAT" ] || { echo "build pscat first (cargo build --release)" >&2; exit 1; }
command -v jq >/dev/null || { echo "jq is required for the manifest" >&2; exit 1; }

# config-with-a-mode: the prelude picks what the config's `apmain` does
ps() { # mode [piece]
  { printf '/ApparelMode /%s def /ApparelPiece /%s def\n' "$1" "${2:-Front}"; cat "$CONFIG"; }
}
NAME="$(ps manifest | "$PSCAT" --headless - | jq -r .name)"
PIECES="$(ps list | "$PSCAT" --headless -)"
[ -n "$PIECES" ] || { echo "config defines no pieces" >&2; exit 1; }

DPI="$(ps manifest | "$PSCAT" --headless - | jq -r .dpi)"
PIECE_JSON=()
while read -r PIECE STEM; do
  read -r WPT HPT WPX HPX BG < <(ps size "$PIECE" | "$PSCAT" --headless -)
  BASE="$OUT/$NAME-$STEM"
  FLAGS=()
  [ "$BG" = transparent ] && FLAGS+=(--transparent)
  # The raster gets --dpi. The vector files do not: --dpi scales the
  # *device* (an SVG/PDF rendered at 300 dpi declares a 50-inch page), so
  # they are written in a second, plain 72-dpi run where 1 unit = 1 point.
  ps draw "$PIECE" | "$PSCAT" --page "${WPT}x${HPT}" --dpi "$DPI" ${FLAGS[@]+"${FLAGS[@]}"} \
    --png "$BASE.png" -
  ps draw "$PIECE" | "$PSCAT" --page "${WPT}x${HPT}" ${FLAGS[@]+"${FLAGS[@]}"} \
    --svg "$BASE.svg" --pdf "$BASE.pdf" -
  PDF_BOX="$(grep -a -o 'MediaBox \[[^]]*\]' "$BASE.pdf" | head -1 | tr -dc '0-9. ' | xargs)"
  SVG_W="$(sed -nE 's/.*<svg [^>]*width="([0-9.]+)" height="([0-9.]+)".*/\1/p' "$BASE.svg" | head -1)"
  SVG_H="$(sed -nE 's/.*<svg [^>]*width="([0-9.]+)" height="([0-9.]+)".*/\2/p' "$BASE.svg" | head -1)"
  [ "$PDF_BOX" = "0 0 $WPT $HPT" ] && [ "$SVG_W" = "$WPT" ] && [ "$SVG_H" = "$HPT" ] || {
    echo "$BASE vector size mismatch (pdf '$PDF_BOX', svg ${SVG_W}x${SVG_H}, want ${WPT}x${HPT}pt)" >&2; exit 1; }
  # what pscat actually wrote, not what we asked for
  KIND="$(file "$BASE.png")"
  ACT_W="$(sed -E 's/.*PNG image data, ([0-9]+) x ([0-9]+).*/\1/' <<<"$KIND")"
  ACT_H="$(sed -E 's/.*PNG image data, ([0-9]+) x ([0-9]+).*/\2/' <<<"$KIND")"
  COLOR="$(sed -E 's/.*, [0-9]+-bit\/color ([A-Za-z]+),.*/\1/' <<<"$KIND")"
  [ "$ACT_W" = "$WPX" ] && [ "$ACT_H" = "$HPX" ] || {
    echo "$BASE.png is ${ACT_W}x${ACT_H}, expected ${WPX}x${HPX}" >&2; exit 1; }
  PIECE_JSON+=("$(jq -n --arg piece "$PIECE" --arg stem "$STEM" --arg color "$COLOR" \
    --arg base "$(basename "$BASE")" --argjson w "$ACT_W" --argjson h "$ACT_H" \
    --argjson vw "$WPT" --argjson vh "$HPT" \
    '{piece:$piece, files:{png:($base+".png"), svg:($base+".svg"), pdf:($base+".pdf")},
      png_actual_pixels:[$w,$h], png_color_type:$color,
      vector_declared_points:[$vw,$vh]}')")
  echo "$PIECE: ${WPX}x${HPX}px (${WPT}x${HPT}pt @ ${DPI}dpi, $BG)"
done <<<"$PIECES"

# font provenance: catalog faces have a directory with the font + licence
faces="$(ps manifest | "$PSCAT" --headless - | jq -r '[.pieces[].runs[].face]|unique|.[]')"
FONTS_JSON="[]"
for face in $faces; do
  dir="fonts/catalog/$face"
  if [ -d "$dir" ]; then
    files="$(cd "$dir" && ls | grep -Ei '\.(ttf|otf|ttc)$' | jq -R . | jq -s .)"
    lic="$(cd "$dir" && ls | grep -Ei '^(OFL|LICENSE)' | head -1 || true)"
    kind="see-file"
    [ -n "$lic" ] && grep -qi "SIL Open Font" "$dir/$lic" && kind="OFL-1.1"
    [ -n "$lic" ] && grep -qi "Apache License" "$dir/$lic" && kind="Apache-2.0"
    FONTS_JSON="$(jq -c --arg face "$face" --arg dir "$dir" --arg lic "${lic:+$dir/$lic}" \
      --arg kind "$kind" --argjson files "$files" \
      '. + [{face:$face, source:"catalog", files:($files|map($dir+"/"+.)), license_file:$lic, license:$kind}]' <<<"$FONTS_JSON")"
  else
    FONTS_JSON="$(jq -c --arg face "$face" '. + [{face:$face, source:"builtin"}]' <<<"$FONTS_JSON")"
  fi
done

# merge the measured per-piece results into the manifest
printf '%s\n' "${PIECE_JSON[@]}" | jq -s '.' > "$OUT/.measured.json"
ps manifest | "$PSCAT" --headless - | jq \
  --slurpfile measured "$OUT/.measured.json" --argjson fonts "$FONTS_JSON" \
  --arg proof "$NAME-proof.png" '
  .pieces |= map(. as $p | . + ($measured[0][] | select(.piece == $p.piece)))
  | . + {
      proof: $proof,
      fonts: $fonts,
      output_notes: {
        png: "RGBA; pixels outside the artwork have alpha 0 when background is transparent",
        svg: "width/height are unitless user units = points (1/72 in), not pixels; no backdrop rect when background is transparent; /transition fills stay native gradients",
        pdf: "MediaBox is in points (1/72 in); no page background is painted (never had one); /transition fills are a flat average colour",
        vector_scale: "SVG and PDF are written without --dpi so their declared size is the physical size in points; only the PNG is dpi-scaled",
        mottled_flecks: "the /mottled wear flecks are painted cream, not knocked out; on a dark garment they print as ink",
        piece_sides: "SleeveLeft/SleeveRight are labels chosen by the config; this template does not say whether they mean the wearer side or the viewer side",
        transparent_background: "produced by pscat --transparent (issue #146); without it PNG and SVG carry an opaque white page"
      }
    }' > "$OUT/$NAME-manifest.json"
rm -f "$OUT/.measured.json"

# the proof: the same config, reduced -- default mode, plain 72 dpi page
ps proof | "$PSCAT" --page 1400x760 --png "$OUT/$NAME-proof.png" -
echo "wrote $OUT/$NAME-manifest.json and $NAME-proof.png"
