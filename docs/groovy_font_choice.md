# Groovy 60s/70s display face for the Keepin' It Real shirt (issue #155)

**Result: Chewy (primary), Bagel Fat One (heavier alternative).** Neither
is a confirmed identification of the original font. The photo of the
shirt (front: `I'm` / `Keepin' it` / `Real`) does not name the face, and
a ten-year-old Zazzle/GIMP design could have used any free 60s/70s
"hippie" display font, so this is a **look-alike chosen by eye, flagged
for Jerome's review**.

## What the original looks like

Soft, rounded, hand-cut 1960s/70s letters: `R` with a round bowl and a
leg that flares outward, `e` with a slanted crossbar, single-storey-feel
`a`, and an `l` whose top is cut at a slant. Heavy, but with visible
counters. Thin dark outline. (Mixed case: `Real`, not `REAL`.)

## Reference vs finalists

| Original shirt (cropped photo) | Finalists: Gorditas, Bagel Fat One, Chewy, Kavoon |
|---|---|
| ![shirt front](fonts/reference-front.png) | ![finalists](fonts/groovy-finalists.png) |

## Candidates (20 rendered)

`fonts/groovy-candidates.png`: `Real` and `I'm Keepin' it Real` in Caprasimo,
Bagel Fat One, Chango, Modak, Rammetto One, Titan One, Gorditas, Shrikhand,
Sniglet, Lilita One, Bevan, Boogaloo, Galindo, Kavoon, Mochiy Pop One,
Chewy, Chicle, Coiny, Bowlby One, Fascinate.

`fonts/groovy-finalists.png`: Gorditas Bold, Bagel Fat One, Chewy, Kavoon large.

Rejected: Caprasimo, Bevan, Titan One, Chango and Rammetto One read as
sturdy sign-painter faces without the loose hand-cut feel; Modak and
Mochiy Pop One are too inflated (counters vanish); Shrikhand is
italic; Gorditas has slab serifs on the `R`; Boogaloo, Galindo, Chicle
and Coiny are too condensed or thin.

## Choice

- **Chewy** matches the letterform quirks best: flared-leg `R`,
  slanted-crossbar `e`, slanted-top `l`, soft hand-cut edges. It is a bit
  lighter than the shirt's letters, which the outline-plus-mottled-fill
  treatment (`lib/lettering.ps`) makes up for.
- **Bagel Fat One** matches the *weight and blobbiness* best (tiny
  counters, Cooper Black spirit) but its `l` is a plain rounded bar.
  Kept as the heavier alternative.

## Licences

- Chewy: Apache 2.0 (Google Fonts, `LICENSE.txt` alongside the font).
- Bagel Fat One: SIL OFL 1.1 (`OFL.txt` alongside the font).

Both allow embedding in documents and redistribution with the licence
text, as with the other catalog faces.

## Compatibility check

`fonts/groovy-specimen.png` (`examples/groovy_faces.ps`) runs the shirt-front
`hllayout` composition in each face: counters, apostrophes and the
`I'm`/`Keepin' it` tuck against `ea` all resolve, ink boxes agree with the
glyphs. `tests/groovy_faces.rs` asserts both are outline faces resolved
by stem and work through `psyletter` and `hllayout`. Images live in
`docs/fonts/`.
