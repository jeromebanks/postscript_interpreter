# Keepin' It Real shirt — front and back print artwork (issue #147)

A clean, editable recreation of Jerome's original shirt (GIMP design,
printed by Zazzle 10+ years ago), built on `lib/apparel.ps` (#146),
`lib/headline.ps` (#145) and `lib/lettering.ps` (#144). Nothing is traced
from the photos; they were used only as the visual reference.

## Files

| | |
|---|---|
| `examples/keepin_it_real_shirt.ps` | the editable source/config (chosen front + back) |
| `examples/keepin_it_real_front_alt.ps` | rejected second front composition |
| `art/keepin-it-real/keepin-it-real-{front,back}.{png,svg,pdf}` | print artwork |
| `art/keepin-it-real/keepin-it-real-manifest.json` | sizes, DPI, fonts + licences, output notes |
| `art/keepin-it-real/*proof.png`, `mock-*.png` | review proofs (proof sheet; light/dark garment mocks) |

## Exact render command

```sh
cargo build --release
scripts/apparel_export.sh examples/keepin_it_real_shirt.ps art/keepin-it-real
scripts/apparel_mock.sh art/keepin-it-real/keepin-it-real-front.png art/keepin-it-real/mock-front.png 480   # needs ffmpeg
```

Renders are deterministic (same seed → identical output).

## Recorded settings

- **Seed** 7. **DPI** 300.
- **Front**: 12 × 14 in (864 × 1008 pt) → 3600 × 4200 px. **Back**: 12 × 5 in
  (864 × 360 pt) → 3600 × 1500 px. Margin 0.5 in / 0.4 in.
- **Fonts**: front `AlfaSlabOne` (OFL, `fonts/catalog/AlfaSlabOne`); back
  `Courier-Bold` and `Courier-BoldOblique` (built-in Liberation Mono, OFL).
- **Background**: transparent. PNG is RGBA with alpha 0 outside the art
  (`apparel_mock.sh` asserts the corner pixels; the SVG has no backdrop rect;
  the PDF paints no page fill). No white box.
- Front: mottled `Real` (red/green/blue, `/Wear 0.15`, thin dark outline),
  patchwork `Keepin' it` / `I'm`. Back: solid black, no outline, no wear.

## Front composition: A chosen over B

- **A** (chosen): `Real` fills the width; `Keepin' it` and `I'm` tuck against
  the `ea` so the big R rises up the left — the original's arrangement.
- **B** (`alt-front-*.png`): loose script (`Pacifico`) with the phrase
  stacked and centered above. Rejected: the capital R loses its bulk and
  counters, and the shape no longer matches the photo.

Checked at 300 dpi (crops of the R, its two counters, the apostrophes in
`I'm` / `Keepin'`, and the edges): counters stay open, apostrophes stand
clear of neighbouring letters, all ink is inside the margin (also enforced
by `tests/apparel.rs`). `Keepin' it` is ~4.3 in wide with ~0.6 in caps at
print size — readable.

## Deviations from the original (please review)

- **Face**: no catalog face is a true rounded, loose 60s/70s (Cooper-like)
  display face. `AlfaSlabOne` is the closest heavy, soft-cornered choice; the
  original's letters are rounder and the `l` shorter. Swap in a better face
  by changing `/Font` in the config.
- **Colour**: the original is a soft tie-dye cloud; this library's treatment
  is hard-edged red/green/blue patches with specks. Its supporting words are
  also multicolour and slightly smaller relative to `Real` than the photo.
- **Back**: matches the photo (Courier Bold, oblique `Phoney`, three centered
  lines, capital P). Line spacing is by eye.
- Spacing/sizes were judged from two phone photos of a creased shirt.

## Light vs dark garments

Designed for **light garments**, like the original. On a dark shirt the front
still reads (colours hold; the near-black contour merges with the fabric) but
the cream wear flecks print as ink, and the **black back text disappears**
(see `mock-back.png`). For a dark garment, change the back's `/Palette` and
add a light outline first.

## Vendor notes

Vendor product choice and upload stay outside this repo. Check the chosen
Printify product's current artwork requirements (size, DPI, format,
transparency) against the manifest before uploading; the PNGs are ready
to use as-is.
