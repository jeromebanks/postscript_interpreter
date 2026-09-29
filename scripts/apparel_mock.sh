#!/usr/bin/env bash
# Composite a transparent apparel PNG onto a light and a dark garment
# colour, side by side, at proof size -- a review aid (issue #147), not
# artwork. It catches what a transparent PNG hides: black ink vanishing
# on a dark shirt, cream wear flecks reading as ink. Also asserts the
# artwork's corner pixels are fully transparent (no accidental white box).
#
#   scripts/apparel_mock.sh ART.png OUT.png [HEIGHT]
set -euo pipefail
[ $# -ge 2 ] || { echo "usage: $0 ART.png OUT.png [HEIGHT]" >&2; exit 2; }
command -v ffmpeg >/dev/null || { echo "ffmpeg is required" >&2; exit 1; }
ART="$1"; OUT="$2"; H="${3:-560}"

# alpha of the top-left and bottom-right pixels, read back from the file
alpha() { # x y
  ffmpeg -v error -i "$ART" -vf "format=rgba,crop=1:1:$1:$2" -f rawvideo -pix_fmt rgba - | tail -c1 | od -An -tu1 | tr -d ' '
}
read -r W HH < <(ffmpeg -hide_banner -i "$ART" 2>&1 | sed -nE 's/.*, ([0-9]+)x([0-9]+)[ ,].*/\1 \2/p' | head -1)
A0="$(alpha 0 0)"; A1="$(alpha $((W - 1)) $((HH - 1)))"
[ "$A0" = 0 ] && [ "$A1" = 0 ] || { echo "$ART: corner alpha $A0/$A1, not transparent" >&2; exit 1; }

ffmpeg -v error -y -i "$ART" -filter_complex "
  [0]scale=-2:$H,split=2[a][b];
  color=c=0xECEEF0:s=1x1[l0]; color=c=0x1C1E26:s=1x1[d0];
  [l0][a]scale2ref=w=iw+40:h=ih+40[lbg][a2]; [lbg][a2]overlay=20:20[lt];
  [d0][b]scale2ref=w=iw+40:h=ih+40[dbg][b2]; [dbg][b2]overlay=20:20[dk];
  [lt][dk]hstack" -frames:v 1 "$OUT"
echo "wrote $OUT (corners transparent, alpha $A0/$A1)"
