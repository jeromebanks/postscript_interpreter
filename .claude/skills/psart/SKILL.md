---
name: psart
description: Make generative art with the pscat PostScript interpreter — the artkit library (turtle, L-systems, palettes, path brushes), the font catalog, and the render-look-refine loop. Use when asked to create art, posters, generative pieces, or decorative graphics in PostScript.
---

# psart — making art with pscat

You have a full PostScript interpreter (`cargo build --release`;
binary `target/release/pscat`), a curated font catalog, seven Type 3
display faces, and `lib/artkit.ps` — a generative-art toolkit. Art
here is *written*, rendered, looked at, and refined.

## The loop

1. Write the piece as a `.ps` file (start from a gallery piece or the
   sketch below).
2. `pscat --page WxH --png out.png piece.ps` — then **look at the
   PNG**. Really look: composition, color, balance, whether the idea
   reads.
3. Refine and re-render. Small seeds, big differences: `N srand` at
   the top makes every run reproducible (same seed, same picture —
   the house doctrine).
4. When it's done, check it in Ghostscript too:
   `gs -dBATCH -dNOPAUSE -sDEVICE=png16m -g<W>x<H> -o gs.png piece.ps`

## The toolkit (`(lib/artkit.ps) run` from the repo root)

- **Random**: `n chance` (0..n-1), `j jit` (-j..j), `frnd` (0..1),
  `arr oneof`. All flow from `srand`.
- **Palettes**: `/dusk /ember /tide /meadow /carnival /parchment
  /nocturne /stone` — `/name palpick` → r g b; `/name pal` → the
  five-color array. `[rgb] [rgb] t mix3` lerps; `[rgb] k shade`
  darkens (k<1) or lifts toward white (k>1).
- **Turtle**: `x y heading thome`, `d fd` (draws), `d hop` (pen up),
  `a tl` / `a tr` (turn; NOT lt/rt — those are comparisons),
  `tpush`/`tpop` (pose stack). Build path, then `stroke` yourself.
- **L-systems**: `(axiom) << (F) 0 get (F[+F]F[-F]F) >> depth lsys`
  → expanded string; `(str) step angle ldraw` drives the turtle
  (F draw, f hop, + left, - right, [ ] push/pop; other letters are
  free no-op symbols).
- **Path brushes** (the superpower): `pitch {x y ang ...} alongpath`
  stamps a proc at even arc-length along the **current path** —
  including text via `charpath`. `(string) pathtext` sets type along
  any path, glyph by glyph, rotated to the tangent. Wrap stamps in
  `gsave newpath ... grestore` so the walked path survives.
- **Shapes/text/layout**: `ngon`, `star`, `rrect`; `(s) cx y
  showctr`, `(s) w fitfont`; `x y w h cols rows {x y w h ...} grid`.
- **Tiling**: `x0 y0 v1 v2 n1 n2 {x y ...} lattice` (the general point
  walk — any basis, oblique included); `hex`/`tri` shapes; `hexgrid`/
  `trigrid` (the other two regular tessellations, built on `lattice`);
  `truchet` — walks a square grid like `grid` but rotates each cell a
  random quarter-turn first, so one motif reads as a flowing maze
  (`gallery/woven_labyrinth.ps` is the worked example). Calling
  `hex`/`tri` from inside a `hexgrid`/`trigrid` stamp (the obvious
  thing to do) needs the inner call wrapped in its own `N dict begin
  ... end` — they share scratch names, same as `grid`/`ngon` already
  do; artkit.ps's tiling-section header has the details.

## Style packs (`lib/styles/`, load after artkit)

Four motif libraries, one per aesthetic — each registers three
palettes into artkit's `Palettes` and adds path builders (you paint
them) and painted stamps (self-contained). Every file's header lists
its full API; `examples/style_<name>.ps` is a worked specimen poster
for each.

- **steampunk** — `gear` (path, bored hub), `rivet`, `pipe`, `gauge`,
  `plateframe`; palettes /brass /verdigris /boiler; `/spmetal` dial
  picks the stamps' metal. Pair with Rye + SpecialElite.
- **psychedelic** — `rays`, `blob` (wobble circle — nest and cycle
  colors), `spiral`, `wavy`, `kaleido` (n-fold repeat about a
  center), `t rainbow`; /acid /blacklight /sherbet. Monoton on an
  `arcn` path via `pathtext` is the move.
- **scifi** — `glowstroke` (the neon seller: halo/mid/core),
  `starfield`, `planet` + `planetring` (`/sfworld` dial),
  `hudcorners`, `reticle`, `hexfield`, `gridfloor` (synthwave
  perspective floor — glowstroke it); /void /hologram /synthwave.
  Orbitron, Audiowide, VT323, PressStart2P.
- **toon** — the cel-cartoon look: `celfill` (flat fill + fat ink
  outline, the foundation), `burst`, `bubble` (speech, tail toward a
  point), `speedlines`, `dotfill` (halftone in the current path, path
  survives), `eye`, `dripbox` (slime title slab); /saturday
  /latenight /pastelpop; `/tnink` dial sets the line color. Bangers,
  ComicNeue, PermanentMarker.

Scratch prefixes sp-/py-/sf-/tn- join artkit's reserved list — don't
reuse them in your own procs.

## Type is material

- `pscat --fonts` lists everything. The standard 35 all resolve
  (Palatino, Bookman, ZapfChancery, Symbol, ZapfDingbats…) plus ~35
  display families: `/GreatVibes-Regular`, `/Bangers`,
  `/UnifrakturMaguntia`, `/PressStart2P-Regular`, `/Creepster-Regular`…
- The Type 3 program-faces (`lib/fonts/*.ps`, load with `(file) run`)
  — /Neon, /Marquee, /Constellation, /Lapidary, /Circuitry,
  /Stitchwork, /Confetti — take dials through their scratch dicts
  (each file's header documents them).
- Text as geometry: `(word) true charpath` then fill/stroke/clip
  it — or walk it with `alongpath`.

## Composition habits that keep pieces good

- Pick a palette and stay in it; one accent color earns more than six.
- Ground first (`clippath fill`), then depth back-to-front; overdraw
  (glow, halo, shadow) is layered strokes, no alpha needed.
- Odd counts, asymmetry, and a little `jit` beat perfect grids.
- Big shapes read; a thousand tiny marks are texture, not subject.
- Deterministic art is debuggable art: seed everything, and when a
  region misbehaves, render just that region at `--page` scale.

## A sketch to start from

```postscript
%!PS-Adobe-3.0
(lib/artkit.ps) run
11 srand
/dusk pal 0 get aload pop setrgbcolor clippath fill   % ground
newpath 306 200 90 thome
(F) << (F) 0 get (FF-[-F+F+F]+[+F-F-F]) >> 3 lsys 6 22.5 ldraw
/dusk pal 4 get aload pop setrgbcolor 1.2 setlinewidth stroke
/Palatino-Italic findfont 18 scalefont setfont
/dusk pal 3 get aload pop setrgbcolor
(grown, not drawn) 306 60 showctr
showpage
```

`gallery/hortus.ps` is the worked example (herbarium plate:
L-systems + alongpath blossoms + letterpress); `gallery/README.md`
is the brief for gallery-quality pieces, and `examples/` has the
font showcases.
