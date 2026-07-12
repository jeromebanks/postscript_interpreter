# Gallery

Six pieces of generative art, written in pure PostScript for `pscat` and
using only the operator set the interpreter has today (Stages 1–3): no
`rand`, no `sethsbcolor`, no arrays — so the pieces carry their own
linear-congruential random generator and HSB→RGB converter as PostScript
procedures.

| Piece | File | Technique |
|---|---|---|
| Golden Bloom | `golden_bloom.ps` | Phyllotaxis: 1,300 florets at the golden angle, √-spaced |
| Cathedral Rose | `cathedral_rose.ps` | Maurer rose: chords walking r=sin 6θ in 71° and 97° strides |
| Ember Tree | `ember_tree.ps` | Recursive branching with an LCG; banded dusk gradient; layered sun glow |
| Fern, After Barnsley | `fern.ps` | Chaos game, 48,000 points over four affine maps |
| Silk Waves | `silk_waves.ps` | 66 threads displaced by two interfering sine fields |
| Frost Mandala | `frost_mandala.ps` | Six-fold circle recursion, 11° twist per generation (1,555+ circles) |

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
