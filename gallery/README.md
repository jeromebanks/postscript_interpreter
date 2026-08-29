# Gallery

Generative art, written in pure PostScript for `pscat` and using only
the operator set the interpreter had when each piece was made. The six
originals date to Stage 3 — no `rand`, no `sethsbcolor`, no arrays — so
they carry their own linear-congruential random generator and HSB→RGB
converter as PostScript procedures. Ring of Type is the Stage 6 piece
and leans on what that stage added: fonts, `stringwidth`, `charpath`.
Hundred Lines opens Gallery II (Stage 10): it reuses the Stage 12
/HandScript dynamic handwriting font wholesale, pulling two knobs the
letter demo left alone — the jitter amplitude and pen width are
overridden per line through the font's scratch dict.

| Piece | File | Technique |
|---|---|---|
| Golden Bloom | `golden_bloom.ps` | Phyllotaxis: 1,300 florets at the golden angle, √-spaced |
| Cathedral Rose | `cathedral_rose.ps` | Maurer rose: chords walking r=sin 6θ in 71° and 97° strides |
| Ember Tree | `ember_tree.ps` | Recursive branching with an LCG; banded dusk gradient; layered sun glow |
| Fern, After Barnsley | `fern.ps` | Chaos game, 48,000 points over four affine maps |
| Silk Waves | `silk_waves.ps` | 66 threads displaced by two interfering sine fields |
| Frost Mandala | `frost_mandala.ps` | Six-fold circle recursion, 11° twist per generation (1,555+ circles) |
| Ring of Type | `ring_of_type.ps` | One sentence circling 11 shrinking rings, set glyph by glyph; charpath ampersand |
| Hundred Lines | `hundred_lines.ps` | The Stage 12 /HandScript dynamic font writing punishment lines on a chalkboard — same sentence nine times, no two letters alike, jitter climbing line by line |
| Hortus Machinalis | `hortus.ps` | (Stage 19) A herbarium plate: three L-system plants grown by turtle, dried blossoms stamped along the plant's own path with `pathforall`, Palatino letterpress |
| Woven Labyrinth | `woven_labyrinth.ps` | Sébastien Truchet's 1704 two-triangle tile, randomly quarter-turned 256 times (artkit's `truchet`) — the single motif chains into a continuous flowing quilt pattern |
| Infinite Descent | `infinite_descent.ps` | A Poincaré-disk {7,3} hyperbolic tessellation (artkit's `httile`) — one heptagon reflected across its own edges out to four generations, 232 tiles colored in rings by generation |
| Recursive Peaks | `recursive_peaks.ps` | A night mountain range built entirely from artkit's fractals section: `gasket`-subdivided low-poly peaks shaded by altitude, a `carpet`-driven sparse starfield, and `edgepoly` koch/quadkoch snowflakes floating overhead |
| Ripple Range | `ripple_range.ps` | (issue #13) Two decaying ripple sources summed into a height field and swept row by row under `graph.ps`'s `project3` camera — each row an occluding filled ridge, back-to-front painter's algorithm |
| Field Notes | `field_notes.ps` | (issue #14) A marsh census styled as a naturalist's field journal: `dataviz.ps`'s `barchart` and `linechart` sharing one category axis (weekly sightings against a temperature trend), a species-mix donut with a hand-drawn legend, all lettered in the Stage 12 /HandScript font |
| The Compositor's Proof | `compositors_proof.ps` | (issue #16) A printer's proof sheet for artkit's paragraph-flow section: a motto set inside a round medallion via `tfflow` and a hand-written circle boundsproc, a justified body paragraph via `tfblock`, and a two-column colophon via `tfcols` that genuinely spills from the first column into the second |
| Lodestone | `lodestone.ps` | (issue #19) A naturalist's demonstration plate: 1,400 `advect`-traced iron filings swirling around a jittered rock, following a `curl2` flow field built from a hand-composed potential — coherent `noise2` texture plus a radial term, whose perpendicular gradient curls a purely radial field into the concentric loops real filings make around a magnet |
| Plum Branch in Ink | `plum_branch.ps` | (issues #40/#41) A sumi-e plum-blossom branch where variable width is the whole point: every stroke — trunk, forking branches, grass at the foot, even the background wash — is one bezier or turtle centerline walked by artkit's `walkpath` and filled by paintkit's `pkribbon`, a quadratic pressure curve narrowing each branch to a brush-lifted point, independent start/end tapers on the grass blades, jitter for a loaded-ink edge |
| Flying White: Reeds at Dusk | `flying_white_reeds.ps` | (issue #80) A wetland at dusk built around paintkit's `pkdry` dry-bristle brush (issue #43): reed stalks each a loaded base and a drier, more broken tip sharing an endpoint, plus a broken reflection on the water and thin dry-brush mist in the sky — the same primitive at three different scales, over a banded `mix3` dusk-palette gradient |
| Fugitive Pigments | `fugitive_pigments.ps` | (issue #84) A poetry broadside — an original poem on impermanence, typeset in Palatino under a title spray-stenciled through its own `charpath`. The paintkit brushes are the whole illustration: the milky way is `pkspray` cloud and star-dust passes along two shared bezier centerlines, the moon wears a spray halo, the horizon glow is spray hugging a line, the reeds are `pknib` tapers with spray-dab seed heads, and the field is `pkdry` broken texture |
| Mochi in Denim Blue | `mochi_denim_blue.ps` | (issue #100) A close-cropped portrait study from `references/mochi.jpg`: broad overlapping `pkoil` masses model an orange-sable Pomeranian against a scraped denim-blue canvas, `pkdry` breaks the silhouette into a furry edge, and selectively crisp eyes and nose hold the likeness |
| First Rain | `first_rain.ps` | (issue #47) A river valley in layered watercolor: paintkit's `pkwash` over a `pkpaper` ground, painted the way a watercolor is — palest and wettest first, then five receding ridges each darker, drier and tighter than the one behind it, every color composited by the renderer over the paint already down rather than guessed in advance. The rain veil is a wash with its edge pooling turned off; the reeds are `pkribbon` drawn under the same wash alpha |

## Reference portrait study

`references/mochi.jpg` is the original 3000×4000 photograph used for
**Mochi in Denim Blue**. The painting preserves the reference's most
recognizable anchors — upright ears, orange-sable and cream markings,
oversized reflective eyes, compact charcoal nose, and surrounding denim
blue — while deliberately simplifying individual hairs into directional
loaded-brush masses. The photograph's close framing becomes a centered
canvas portrait; the exact clothing folds become a cross-woven painted
ground. The result is an interpretation, not a traced or photorealistic
copy: `pkoil` supplies the wet planes and bristle ridges, while `pkdry`
softens selected contour passages like a fan brush dragged through wet
paint.

## Viewing the gallery

```sh
./gallery/show.sh          # rendered stills, one at a time (Quick Look)
./gallery/show.sh --live   # each piece draws itself in a pscat window;
                           # close the window to advance
```

Or run any piece by hand — page sizes match each file's `%%BoundingBox`,
and `--speed` (interpreter steps per frame) sets the drawing tempo:

```sh
cargo run --release -- --page 700x700 gallery/golden_bloom.ps
cargo run --release -- --page 620x820 --speed 400 gallery/ember_tree.ps
cargo run --release -- --page 620x800 --speed 3000 gallery/fern.ps
```

The fern wants a high speed (it plots 48k dots); the tree is nice slow.
One quirk worth knowing: pieces built from a few large `stroke`s (the
rose, the waves) appear in big steps — paint happens at `stroke`/`fill`
time, exactly like real PostScript.

## Re-rendering the stills

`gallery/renders/*.png` are 2× supersampled: each was rendered with
`2 2 scale` prepended and a doubled `--page`, e.g.:

```sh
{ printf '2 2 scale\n'; cat gallery/golden_bloom.ps; } > /tmp/hi.ps
cargo run --release -- --page 1400x1400 --png gallery/renders/golden_bloom.png /tmp/hi.ps
```

Every image is deterministic: same file, same pixels. Change the `/seed`
in the tree or fern and a different individual grows.
