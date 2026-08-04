# HANDOFF.md — state of the interpreter and how to continue

Written 2026-07-13 at the completion of Stages 6–7; last updated
2026-07-24. Stages 8–20 are all complete: spool/halftone/Gallery II
(10), perf parity (11), handwriting (12), the handwrite tool (13),
agent integration — skill, `pscat-mcp`, `pscat -` (14), the
font library `lib/fonts/` — /Neon, /Marquee, /Constellation,
/Lapidary + specimen (15), the browser build — wasm + web/pscat.js (16), the Pages site + pipeline — site/, scripts/build_site.sh, .github/workflows/pages.yml (17), and the font catalog —
`fonts/catalog/` runtime loader completing the standard 35, plus
/Circuitry, /Stitchwork, /Confetti (18), the art toolkit —
pathforall/flattenpath, lib/artkit.ps, the psart skill, Hortus
Machinalis (19), and the style packs — `lib/styles/`
steampunk/psychedelic/scifi/toon motif libraries over artkit, with
specimen posters in `examples/style_*.ps` (20) — plus
`--interactive` (the windowed
REPL, Stage 8's last sliver) and the Level 2 rect operators. Also
done, unplanned/post-roadmap: Stage 21, standalone installability —
`src/paths.rs` (a consolidated, tested resource-root resolver used
by the font catalog, `run`/`file`, and `pscat-mcp`'s handwrite path),
`scripts/package_release.sh`, and `.github/workflows/release.yml`,
so pscat can be installed and run without a git checkout; and Stage
22, Korean/Japanese/Thai fonts — a `CatalogEncoding::Unicode` face
variant (Noto Sans KR/JP/Thai) whose `show` decodes UTF-8 and maps
codepoints straight to glyphs via `cmap`, since Hangul/kanji exceed
the 256-slot Encoding model everything else uses (see FONTS.md's
"Unicode-mode catalog faces" addendum and NOTES.md's Stage 22 entry);
and Stage 23, the Stage-22 follow-up issues (#3, #5) — gallery/site
entries for the four international faces, `.ttc` as a recognized
catalog extension, and a second Korean face (Nanum Brush Script,
brush-calligraphy style) on the same Unicode-mode mechanism (NOTES.md's
Stage 23 entry). Also done: issue #12, circular/curved-text procedures
in artkit (`ctext`/`ctextctr`), and issue #8, PDF document metadata —
`%%Title:`/`%%For:` DSC header comments now populate the exported
PDF's `/Info` dict, verified against gs's own pdfwrite behavior on the
same comments; also fixed a real pre-existing bug found while building
its demo example, where `stroke`'s PDF recording was silently
no-op'd whenever `--pdf` was used without `--svg` (NOTES.md's entry
has the full story). Also done: issue #9, a tiling/tessellation
library for artkit — `lattice`/`hex`/`tri`/`hexgrid`/`trigrid`/
`truchet`, `examples/tiling.ps`, and the gallery piece `Woven
Labyrinth` (NOTES.md's entry has the full story). Also done: issue #6,
a procedural jittered-stroke Hangul face (jamo composition) —
`lib/hangul.ps`'s `/HangulScript`, built on a new interpreter-side
extension (`/UnicodeBuildChar true`, FONTS.md's "Unicode-mode Type 3
BuildChar" addendum) that lets a Type 3 `BuildChar` receive a full
Unicode codepoint instead of a byte; Unicode's Hangul-syllable
arithmetic decomposes each codepoint into its jamo, which compose from
just 14 atomic consonant stroke shapes plus a handful of vowel
primitives (NOTES.md's entry has the full story). Also done: issue #10,
hyperbolic geometry in the Poincare disk — `lib/artkit.ps`'s
`hpoint`/`hpolar`/`horthocircle`/`hreflect`/`hgeo`/`hpoly`/`httile`
(a breadth-first-reflection generator for regular {p,q} tessellations),
`examples/hyperbolic.ps`, and the gallery piece `Infinite Descent`
(NOTES.md's entry has the full story, including two real bugs a
standalone Python prototype caught before either reached PostScript: a
backwards circumradius convention, and a dedup-tolerance formula that
silently capped tile growth with depth). Also done: issue #11, fractal
/ self-similar-geometry procedures for artkit — `edgefractal`/
`edgepoly` (generalized Koch-style edge replacement, any turn-delta
generator, presets `/koch`/`/quadkoch` in `FractalGens`) and `gasket`/
`carpet` (Sierpinski-style area subdivision, triangle and square),
`examples/fractals.ps`, and the gallery piece `Recursive Peaks`
(NOTES.md's entry has the full story, including why `gasket`/`carpet`
ended up iterative rather than recursive — a recursive version leaves
every ancestor's dict frame open exactly when a leaf invokes the
caller's proc, silently breaking a plain `def`-based counter). Also
done: issue #13, 2D/3D function-graphing procedures — a new sibling
library, `lib/graph.ps` (no dependency on artkit either way):
`setframe`/`gmapx`/`gmapy`/`gmoveto`/`glineto` map a data-space domain
onto a device-space viewport, `plotfn`/`plotparam`/`plotpolar` sample
curves into the current path, `axes` frames them; `setview`/
`project3` add an azimuth/elevation camera and `plotsurface`/
`surfacerow`/`surfacecol`/`axes3` a `z=f(x,y)` wireframe mesh (no
hidden-surface removal — a documented scope cut). `examples/graphing.ps`
is the specimen sheet; the gallery piece Ripple Range demonstrates the
one-piece-specific trick that gets occlusion anyway: back-to-front
filled rows (NOTES.md's entry has the full story).
Written for whichever model
picks the project up next — read this after `CLAUDE.md` and before
touching code. `ROADMAP.md` has the task list with model routing;
`NOTES.md` has per-stage histories; this file is the *orientation*.

## Where things stand

**356 tests across 35 suites, clippy clean.** Stages 1–19 are done,
including Stage 8's last sliver, the `--interactive` windowed REPL
(`-i`; stdin reader thread → `EventLoopProxy` user events → chunks
run on the frame budget; line accumulation shared with the terminal
REPL via `src/repl.rs`). Stage 8 delivered: **save/restore** as
object-granularity copy-on-write journaling (`VM.md` is the design
doc and gs-pin record — read it before touching any operator that
mutates array/dict contents; new mutators call
`Interp::journal_array`/`journal_dict` first, strings exempt by
spec); **name interning** (`src/name.rs`, fib 214→137ms);
**Indexed/Separation color spaces** (tint transforms run via the
generic `Frame::PostOp` continuation — copy that pattern for any
operator needing a procedure's result); **Level 2 odds and ends**
(`ops/level2.rs`); and **corpus round 2** (`tests/corpus.rs`): tiger,
golfer, escher, colorcir, doretree render block-identical to gs.
Stage 9 delivered the output targets: real multi-page `showpage`
(lazy erase — the window keeps the finished page), `--dpi`, and
`--svg`/`--pdf` export (paint-pipeline mirrors; the PDF is verified
by gs rasterizing it back). The interpreter runs found PostScript
with data structures, error recovery, text in three font technologies
(bundled TrueType via ttf-parser, Type 3 glyph procedures, Type 1
charstrings), file objects and decode filters, and sampled images.
Everything is golden-tested against Ghostscript (`tests/golden.rs`
renders eight examples in both and compares block-downsampled output).

## The three ideas that explain most of the code

1. **The machine is an explicit frame stack** (`src/interp.rs`,
   `Frame` enum). Anything that must interleave with PostScript
   execution is a frame: loops, `stopped` boundaries, the show family
   (`Frame::Show` → `font::ShowCtx`), images (`Frame::Image` →
   `image::ImageCtx`). Frame order **is** program order — that's why
   `stringwidth` can push its result at frame-pop, why Type 3
   BuildChar and kshow procs nest arbitrarily, and why the live window
   renders glyph-by-glyph. If a new feature needs to run a PostScript
   procedure mid-operator (patterns, `cshow`, resource callbacks),
   copy the ShowCtx pattern; do NOT call the machine recursively.

2. **Files share one read cursor** (`src/file.rs`). The scanner pulls
   bytes through a `FileHandle`; `currentfile` returns the same
   handle; filters are files layered on files, decoding *on demand* so
   a filter consumes exactly the source bytes its consumer needed.
   This single property is what makes `currentfile eexec` (Type 1),
   `N RD <binary>` (charstrings), `currentfile buf readhexstring`
   prologs, and inline image data all work. Two conventions were
   pinned against gs and must not regress:
   - a token terminated by whitespace consumes that one delimiter
     character (`lexer::eat_token_delimiter`);
   - `image` drains a filter-chain source layer-by-layer to each EOD
     marker on completion.

3. **Every glyph source feeds one transform pipeline.** Glyph-space
   outline → FontMatrix (composed by scalefont/makefont, cached at
   setfont) → CTM at the pen → device `PsPath` → the ordinary
   fill/clip machinery. TTF outlines (`font::outline_glyph`), Type 1
   charstrings (`font::type1_glyph` + `src/type1.rs`), and Type 3
   procedures (CTM set so BuildChar draws in glyph space) all end in
   the same place. A new glyph source (Type 42-from-program, CFF)
   should too.

## Deliberate deviations (all documented in place)

- `FID` is integertype; unknown `findfont` substitutes Helvetica
  (gs-style) instead of erroring. Symbol/ZapfDingbats have real URW
  faces since Stage 18 — via the runtime catalog (`fonts/catalog/`,
  `font::resolve`), so wasm and catalog-less installs still
  substitute.
- `charpath` on Type 3 advances without capturing outlines; its bool
  operand is ignored for outline fonts.
- Type 1 hints are ignored entirely; no `/Metrics`, no CID/CFF.
- Access control (`executeonly` etc.) is not modeled — identities.
- Filter parameter dicts accepted and ignored; no encode filters.
- Images: nearest-neighbor, no `/Interpolate`, no 12-bit samples, no
  MultipleDataSources colorimage (limitcheck). DCTDecode buffers one
  whole JPEG (marker-aware, stops exactly at EOI); CCITTFax absent;
  Adobe inverted-CMYK JPEGs untested.
- errordict handlers not consulted; error-time operand-stack
  restoration not done (PLRM handlers see pre-error operands).
- eexec's systemdict push isn't restored on error unwinds.
- Stroke width ≈ √|det CTM| (anisotropic pens wrong); `showpage`
  doesn't erase; ints are i64; `flattenpath` uses a fixed
  quarter-pixel tolerance (`setflat` not modeled — chord counts
  differ from gs, shapes agree).

## Working conventions that matter (beyond AGENTS.md)

- **gs is the oracle.** Every semantics question in Stages 5–7 was
  settled by running the snippet in gs first and pinning the answer in
  a test comment. Keep doing that; it caught real design errors twice
  (glyph caching by encoded name; the token-delimiter rule).
- The `.notdef`/substitution philosophy: found files should *render*,
  not error, wherever the PLRM allows latitude — but record every
  such choice in the module docs and NOTES.md.
- Commit per coherent chunk with stage-prefixed messages; update
  README/NOTES/ROADMAP in the same or a closing commit.

## Next work, in recommended order

1. **More Gallery II pieces** — the slot stays open by design;
   `gallery/README.md` is the brief, and Hundred Lines
   (`gallery/hundred_lines.ps`) shows the /HandScript font reused.
   The Stage 15 faces (`lib/fonts/`) and the Stage 20 style packs
   (`lib/styles/` — steampunk, psychedelic, scifi, toon, with
   specimen posters in `examples/style_*.ps`) are ready-made
   material — a neon nocturne, a star-chart page, a carved
   inscription, an orrery of brass.
2. Leftovers when they itch: the remaining fib/fern machine-loop gap
   vs gs (Stage 11 findings in NOTES.md — the untaken levers are
   representation changes), errordict handlers, error-time operand
   restoration, DSC-comment tolerance and more corpus files,
   CCITTFax. (`rectfill`/`rectstroke`/`rectclip` landed 2026-07-18,
   found missing by `--interactive`'s first session.)
3. Worth knowing before touching export: SVG (`src/svg.rs`) and PDF
   (`src/pdf.rs`) both mirror the paint pipeline at the same seams
   in `Gfx` (fill, stroke, glyph fill, erase, prepare_paint,
   end_page, plus the image hook in `image.rs`). A third export
   target should promote those seams into a neutral display list
   first — the design note in `src/pdf.rs` says why that wasn't
   done at two.

## Gotchas for the next implementer

- `PsArray` has a custom `Drop` (iterative teardown of last-handle
  storage; it moved off `Object` in Stage 11 for speed, so
  `obj.value` is movable these days). Don't reintroduce a
  `Drop for Object` — profiling showed it taxing every popped
  operand (~18% of fib).
- `Interp` split-borrows fields in `next_item` (`estack`, `dstack`,
  `gfx`, `ostack`); new frame types plug in there and must decide
  their action before any stack mutation (the `Action` enum).
- Frames that hold external state (glyph contexts' graphics-state
  snapshot, paint suppression) must clean up in **three** unwind
  paths: normal pop, `do_stop`, and `unwind_all`. `ShowCtx::cleanup`
  is the pattern; `Frame::Image` deliberately holds nothing.
- `exit` must not cross Scanner/StopMark/Show/Image/PostOp frames
  (`invalidexit`) — extend that match if you add frame kinds.
- The systemdict self-reference is an intentional Rc cycle (one leak
  per Interp, process-lifetime object).
- tiny-skia `Transform::from_row(sx, ky, kx, sy, tx, ty)` matches the
  PS matrix order `[a b c d tx ty]` — `ops/matrix.rs` documents it;
  don't rediscover this the hard way.
- Test Interps at 100×100; golden runs at 500×500/r72. Metric
  assertions use 1e-3 tolerances where the pen is f32.

## Verification checklist before calling any future stage done

```sh
cargo test            # all suites, golden included (needs gs installed)
cargo clippy --all-targets
cargo fmt --check
cargo run -- examples/postcard.ps         # watch it draw
./gallery/show.sh --live                  # the fun regression suite
```
