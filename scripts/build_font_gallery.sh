#!/usr/bin/env bash
# build_font_gallery.sh — render one card PNG per available font face for
# site/fonts.html: the eight Type3 lib/fonts/ faces (their natural
# materials, colors lifted from examples/font_library*.ps) and the 46
# fonts/catalog/ families (the standard 35 plus the display shelf, same
# label/font/sample data as examples/font_catalog.ps). Deterministic
# (each rand-driven face is srand-seeded) and self-contained: run it any
# time to regenerate, nothing here is checked in.
#
#   ./scripts/build_font_gallery.sh _site/assets/fonts
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
OUT=${1:?usage: build_font_gallery.sh OUTDIR}
mkdir -p "$OUT/type3" "$OUT/catalog"

cargo build --release --manifest-path "$ROOT/Cargo.toml" --quiet
BIN="$ROOT/target/release/pscat"
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

card() { # slug page(WxH) subdir ps-body-on-stdin
  local slug=$1 page=$2 sub=$3
  cat >"$TMP/$slug.ps"
  "$BIN" --page "$page" --png "$OUT/$sub/$slug.png" "$TMP/$slug.ps" >/dev/null
}

# ---------------------------------------------------------------- Type3
card neon 440x130 type3 <<'EOF'
%!PS
(lib/fonts/neon.ps) run
11 srand
0.03 0.03 0.06 setrgbcolor clippath fill
/Neon findfont 54 scalefont setfont
36 62 moveto (NEON) show
NeonDict /work get /Tube [0.1 0.9 0.85] put
/Neon findfont 16 scalefont setfont
36 26 moveto (bent glass and noble gas) show
showpage
EOF

card marquee 440x130 type3 <<'EOF'
%!PS
(lib/fonts/marquee.ps) run
11 srand
0.10 0.04 0.05 setrgbcolor clippath fill
/Marquee findfont 46 scalefont setfont
0 setgray
36 60 moveto (MARQUEE) show
/Marquee findfont 14 scalefont setfont
36 24 moveto (two shows nightly) show
showpage
EOF

card constellation 480x130 type3 <<'EOF'
%!PS
(lib/fonts/constellation.ps) run
11 srand
0.012 0.02 0.07 setrgbcolor clippath fill
/Constellation findfont 44 scalefont setfont
26 62 moveto (CONSTELLATION) show
/Constellation findfont 15 scalefont setfont
26 24 moveto (stars in ancient order) show
showpage
EOF

card lapidary 440x130 type3 <<'EOF'
%!PS
(lib/fonts/lapidary.ps) run
11 srand
0.93 0.91 0.87 setrgbcolor clippath fill
/Lapidary findfont 50 scalefont setfont
0 setgray
36 60 moveto (LAPIDARY) show
/Lapidary findfont 14 scalefont setfont
36 24 moveto (cut in stone) show
showpage
EOF

card circuitry 440x130 type3 <<'EOF'
%!PS
(lib/fonts/circuitry.ps) run
11 srand
0.04 0.15 0.10 setrgbcolor clippath fill
CircuitryDict /work get /Hole [0.04 0.15 0.10] put
/Circuitry findfont 40 scalefont setfont
0 setgray
36 60 moveto (CIRCUITRY) show
/Circuitry findfont 14 scalefont setfont
36 24 moveto (copper routes a word) show
showpage
EOF

card stitchwork 440x130 type3 <<'EOF'
%!PS
(lib/fonts/stitchwork.ps) run
11 srand
0.96 0.93 0.86 setrgbcolor clippath fill
/Stitchwork findfont 38 scalefont setfont
0 setgray
36 60 moveto (STITCHWORK) show
/Stitchwork findfont 14 scalefont setfont
36 24 moveto (every x a kiss of floss) show
showpage
EOF

card confetti 440x130 type3 <<'EOF'
%!PS
(lib/fonts/confetti.ps) run
11 srand
0.10 0.08 0.14 setrgbcolor clippath fill
/Confetti findfont 48 scalefont setfont
0 setgray
36 60 moveto (CONFETTI) show
/Confetti findfont 14 scalefont setfont
36 24 moveto (thrown and still falling) show
showpage
EOF

card handscript 440x130 type3 <<'EOF'
%!PS
(lib/handscript.ps) run
0.99 0.98 0.95 setrgbcolor clippath fill
<< /Text (no two letters ever match)
   /Size 28 /Width 440 /Height 130 /Margin 26 /Leading 1.4
   /Ink [0.13 0.14 0.34] /Paper /plain /Seed 1985 >> hs-write
showpage
EOF

# ------------------------------------------------------------- catalog
# label | findfont name | sample text | point size
cat <<'TABLE' | while IFS='|' read -r label fn sample size; do
Helvetica|Helvetica|Hamburgefonstiv 123|22
Times-Roman|Times-Roman|Hamburgefonstiv 123|22
Courier|Courier|Hamburgefonstiv 12|20
Palatino-Roman|Palatino-Roman|Hamburgefonstiv 123|22
Bookman-Light|Bookman-Light|Hamburgefonstiv 123|22
NewCenturySchlbk|NewCenturySchlbk-Roman|Hamburgefonstiv 123|22
AvantGarde-Book|AvantGarde-Book|Hamburgefonstiv 123|22
ZapfChancery|ZapfChancery-MediumItalic|Hamburgefonstiv 123|22
Helvetica-Narrow|Helvetica-Narrow|Hamburgefonstiv 12345|22
Symbol|Symbol|abgdepfjklmnwq|22
ZapfDingbats|ZapfDingbats|abcdefgHIJKLMN|22
EBGaramond|EBGaramond-Regular|Hamburgefonstiv|24
LibreBaskerville|LibreBaskerville-Regular|Hamburgefonstiv|24
PlayfairDisplay|PlayfairDisplay-Regular|Hamburgefonstiv|24
Lora|Lora-Regular|Hamburgefonstiv|24
Cinzel|Cinzel-Regular|HAMBURGEFONS|22
Poppins|Poppins-Regular|Hamburgefonstiv|24
Jost|Jost-Regular|Hamburgefonstiv|24
Oswald|Oswald-Regular|Hamburgefonstiv|24
AtkinsonHyperlegible|AtkinsonHyperlegible-Regular|Hamburgefonstiv|22
ZillaSlab|ZillaSlab-Regular|Hamburgefonstiv|24
AlfaSlabOne|AlfaSlabOne-Regular|Hamburgefonst|22
JetBrainsMono|JetBrainsMono-Regular|Hamburgefonst|20
GreatVibes|GreatVibes-Regular|Hamburgefonstiv|26
Pacifico|Pacifico-Regular|Hamburgefonstiv|24
Caveat|Caveat-Regular|Hamburgefonstiv|26
HomemadeApple|HomemadeApple-Regular|Hamburgefonst|20
BebasNeue|BebasNeue-Regular|HAMBURGEFONSTIV|22
AbrilFatface|AbrilFatface-Regular|Hamburgefonstiv|22
Monoton|Monoton-Regular|HAMBURGE|22
Limelight|Limelight-Regular|Hamburgefonstiv|22
Bungee|Bungee-Regular|HAMBURGE|22
UnifrakturMaguntia|UnifrakturMaguntia|Hamburgefonstiv|24
PirataOne|PirataOne-Regular|Hamburgefonstiv|22
Rye|Rye-Regular|Hamburgefonst|22
Creepster|Creepster-Regular|Hamburgefonstiv|24
PressStart2P|PressStart2P-Regular|HAMBURGE|16
VT323|VT323-Regular|Hamburgefonstiv|26
SpecialElite|SpecialElite-Regular|Hamburgefonstiv|22
Orbitron|Orbitron-Regular|Hamburgefonst|20
Audiowide|Audiowide-Regular|Hamburgefonst|20
StardosStencil|StardosStencil-Regular|Hamburgefonstiv|22
Bangers|Bangers-Regular|Hamburgefonstiv|24
ComicNeue|ComicNeue-Regular|Hamburgefonstiv|24
MedievalSharp|MedievalSharp|Hamburgefonstiv|22
PermanentMarker|PermanentMarker-Regular|Hamburgefonst|22
TABLE
  slug=$(echo "$label" | tr 'A-Z ' 'a-z-' | tr -cd 'a-z0-9-')
  card "$slug" 440x90 catalog <<EOF
%!PS
0.98 0.97 0.94 setrgbcolor clippath fill
0 setgray
/Helvetica findfont 10 scalefont setfont
24 68 moveto ($label) show
/$fn findfont $size scalefont setfont
24 22 moveto ($sample) show
showpage
EOF
done

n=$(find "$OUT" -name '*.png' | wc -l | tr -d ' ')
echo "font gallery: $n cards in $OUT"
