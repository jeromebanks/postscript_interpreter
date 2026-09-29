# Keepin' It Real shirt — front and back print artwork (issue #147)

A clean, editable recreation of Jerome's original shirt (GIMP design,
printed by Zazzle 10+ years ago), built on `lib/apparel.ps` (#146),
`lib/headline.ps` (#145) and `lib/lettering.ps` (#144). Nothing is traced
from the photos; they were used only as the visual reference.

## Files

| | |
|---|---|
| `examples/keepin_it_real_shirt.ps` | the editable source/config (front in Chewy + back) |
| `examples/keepin_it_real_shirt_bagel.ps` | same config, Bagel Fat One front; exports in `art/keepin-it-real-bagel/` |
| `art/keepin-it-real/font-comparison.png` | reference photo crop above both fronts (Chewy, Bagel Fat One) |
| `gallery/keepin_it_real.ps` | gallery piece: both fronts + back, see `gallery/README.md` |
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

- **Seed** 11 (plasma field; was 7 for the AlfaSlabOne/mottled pass). **DPI** 300.
- **Front**: 12 × 14 in (864 × 1008 pt) → 3600 × 4200 px. **Back**: 12 × 5 in
  (864 × 360 pt) → 3600 × 1500 px. Margin 0.5 in / 0.4 in.
- **Fonts**: front `Chewy-Regular` (Apache 2.0) or, in the `-bagel` variant, `BagelFatOne-Regular` (OFL), see `docs/groovy_font_choice.md`; back
  `Courier-Bold` and `Courier-BoldOblique` (built-in Liberation Mono, OFL).
- **Background**: transparent. PNG is RGBA with alpha 0 outside the art
  (`apparel_mock.sh` asserts the corner pixels; the SVG has no backdrop rect;
  the PDF paints no page fill). No white box.
- Front: GIMP-style plasma (`/Treatment /plasma`, `lib/fillkit.ps`, red/green/blue palette,
  `/Turbulence 2 /Spread 0.01`, `/Wear 0.15`, thin dark outline) on all three runs. Back: solid black, no outline, no wear.

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

- **Face**: Chewy is a look-alike, not the original font (unidentifiable
  from the photo); its bracketed serifs are vestigial. Bagel Fat One is
  heavier and rounder but its counters (the R, the `a`) are tight at print
  size. Both fronts are exported with identical layout, palette and seed;
  **Jerome to pick** (the default `art/keepin-it-real/` is Chewy). See
  `art/keepin-it-real/font-comparison.png`.
- **Colour**: plasma is smooth and soft like the original but lower in
  contrast and greener/less red-topped than the photo, which is more
  tie-dye with grain; the original's red top / blue-green bottom is only
  loosely echoed. All three runs use the same plasma, so the supporting
  words are multicolour like the photo.
- **Layout**: `Keepin' it` is offset a little higher (`/Offset [8 30]`) than
  the AlfaSlabOne pass because Chewy's taller ascenders collided at the old
  offset (`headline-runs-collide`).
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
