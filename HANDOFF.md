# HANDOFF.md — state of the interpreter and how to continue

Written 2026-07-13 at the completion of Stages 6–7; last updated
2026-08-15. Stages 8–20 are all complete: spool/halftone/Gallery II
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
has the full story). Also done: issue #20, axial/radial gradient
(shading) fill support — the `shfill` operator (`src/shading.rs`,
`src/ops/shading.rs`, `Gfx::shfill`), `ShadingType` 2/3 with
`FunctionType` 2/3, native SVG `<linearGradient>`/`<radialGradient>`
export, a PDF average-color-fill approximation, and `lib/artkit.ps`'s
"gradients" section (`gradfn`/`axialsh`/`radialsh`/`gradfill`) on top of it
(NOTES.md's entry has the full story). Also done: issue #9, a tiling/tessellation
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
filled rows (NOTES.md's entry has the full story). Also done: issue
#14, a data-visualization chart library — `lib/dataviz.ps` (also no
dependency on artkit or graph.ps): `setdvframe`/`dvmapy`/`dvcatx` map
a value domain and category count onto a device viewport for
`barchart`/`linechart`/`areachart`; `setscatterframe` (the same 8-arg
shape as graph.ps's `setframe`) backs `scatterchart`; `piechart` draws
pie wedges or, given a nonzero inner radius, donuts; `dvaxes` frames
the categorical charts. `examples/dataviz.ps` is a six-panel specimen
sheet; the gallery piece Field Notes pairs a bar chart and a line
chart on one shared category axis with a species-mix donut (NOTES.md's
entry has the full story, including two bug classes deliberately
tested for — a donut filled solid to the center, and a pie wedge
sweeping the wrong direction — both invisible to a plain ink-count
check). Also done: issue #15, a photo-to-line-etching utility — a
third sibling library, `lib/etching.ps` (no dependency on artkit,
graph.ps, or dataviz.ps): `et-dims` walks a JPEG's own marker segments
to read width/height/component count without decoding it; `et-draw`
opens the same file through `/DCTDecode` (a general filter here, so a
PS program can `readstring` decoded samples directly, bypassing the
`image` operator entirely — confirmed empirically before writing any
of this, which is what kept the whole feature to a PS library with
zero new Rust) and hatches it into a line engraving: parallel strokes
whose width tracks local darkness, plus a perpendicular crosshatch
pass in the shadows — quantized into a few width buckets with one
stroke per constant-bucket run, not one stroke per sample, since
stroke count (not sample count) is what actually costs render time.
`scripts/photo_etch.sh` wraps it end to end; `examples/etching_demo.ps`
is the specimen sheet (NOTES.md's entry has the full story, including
a real upside-down-photo bug the coverage tests caught: PostScript's
y runs bottom-up but decoded JPEG rows run top-down, so the first
version of `et-hatch`'s sample lookup rendered every photo flipped
vertically until that mapping got a `ph y sub`). Also done: issue #16,
paragraph/flowing-text layout for artkit — `lib/artkit.ps`'s
`tfwrap`/`tfdrawline`/`tfflow`/`tfblock`/`tfcols`: greedy word-wrap,
four alignments (including `/justify`'s stretch-to-margin, skipped on
a paragraph's last line), and columns, all built on `tfflow` taking a
`boundsproc` (`{y -> x0 x1}`, called per line) so a region's width can
vary with height instead of being locked to a rectangle — `tfblock`/
`tfcols` are just `tfflow` with a constant-width boundsproc.
`examples/paragraph_layout.ps` and the gallery piece `The Compositor's
Proof` (`gallery/compositors_proof.ps`, a motto flowed into a circular
medallion via a hand-written boundsproc) demonstrate it (NOTES.md's
entry has the full story, including two real bugs caught before
merge — one by `advisor` plan review, one by actually rendering the
example, that a pixel-adjacent-but-not-quite regression test had
missed). Also done: issue #17, a self-check/lint mode for
agent-driven rendering — `src/lint.rs` (`--lint` on the CLI, wired
into `pscat-mcp`'s `render_postscript`) flags a blank page, an
unbalanced `gsave`, and operand-/dict-stack leaks; `error_report`
gained a `Line: N` source-line attribution, scoped to the top-level
program only (see the "Deliberate deviations" list below for why)
(NOTES.md's entry has the full story, including two false-positive
traps `advisor` caught before any code existed, and two real,
previously undetected operand-stack leaks — in `lib/artkit.ps`'s
`tfdrawline` `/justify` and `lib/etching.ps`'s `et-hatch` — that
running `--lint` over `examples/`/`gallery/` found and this issue
fixed alongside the feature). Also done: issue #18, parameterized page
templates -- a fourth sibling library, `lib/pagekit.ps` (depends on
`artkit.ps`, unlike `graph.ps`/`dataviz.ps`/`etching.ps`): `pgcard`/
`pgletter`/`pgcertificate`/`pginvitation`/`pgposter`, each
`x y w h dict pgNAME` filling a region from a content dict of optional
keys and returning `tfblock`'s leftover-text contract.
`examples/template_*.ps` is one specimen per template (NOTES.md's
entry has the full story, including two real bugs `advisor`'s plan
review caught before any template code existed -- artkit's `fitfont`
enlarging as well as shrinking, and two of artkit's mood palettes not
actually being ordered dark-to-light despite looking like they were --
and one real implementation bug `--lint` caught on the first rendered
example, a copy-paste-misaligned `tfblock` call in `pgletter`). Also
done: issue #19, noise and flow-field procedures for artkit —
`noiseinit`/`noise2` (2D gradient/Perlin noise off a `srand`-shuffled
permutation table), `curl2` (turns any `{x y -> n}` scalar field into
a unit-vector flow via its normalized perpendicular gradient), and
`advect` (traces a particle through a `{x y -> dx dy}` field as
`lineto`s). All three use plain global scratch, no caller proc gets a
private dict opened for it during `exec` — an original draft did wrap
`curl2`/`advect` (they take a caller-supplied field proc, so the
gasket/carpet/hexgrid nested-composition gotcha applies), but a
cross-model (Codex) review at the PR stage found that auto-wrapping
silently swallows any ordinary (non-nesting) field proc's own plain
`def`-based state; switched to `gasket`/`carpet`'s own precedent
instead (library stays unwrapped, caller wraps their own nested call).
`examples/noise.ps` and the gallery piece `Lodestone`
(`gallery/lodestone.ps`, 1,400 `advect`-traced iron filings curling
around a jittered rock) demonstrate it (NOTES.md's entry has the full
story, including that Codex review — it also caught `curl2`'s
docstring overclaiming exact divergence-freedom for its normalized
output, measured at ~-0.27 for one test field, now documented
accurately — and two real bugs caught empirically before either
reached a permanent test: `and 255` vs. `mod` for negative lattice
coordinates, and a field proc that doesn't consume its `x y` silently
leaking stack values instead of erroring — the same class of bug
`--lint` also caught directly in `examples/noise.ps`'s first draft).
Also done: issue #21, sweep/contact-sheet rendering — `--sweep-seed`
(overrides every `srand` call transparently, via a new
`Interp::set_seed_override`/`seed_override_fired` pair checked in
`ops/arith.rs`'s `srand`, so found art with a hardcoded `N srand` line
sweeps unmodified) and `--sweep NAME=` (predefines `/NAME` in userdict
via a second `run_source` call before the real one, for a source that
opts in to read it — no source-line-shift text-mangling); a new
`src/contact_sheet.rs` composites same-sized frames into one grid PNG,
capped at the same 8000px-per-side ceiling `--page` already enforces.
`examples/sweep_demo.ps` demonstrates both mechanisms (NOTES.md's
entry has the full story, including why a plain `pscat-mcp` tool
wasn't added -- a documented scope cut, not an oversight). Also done:
issue #39, a machine-readable catalog of agent-usable art
capabilities -- `--capabilities` on the CLI and `describe_art_
capabilities` on `pscat-mcp` both print one JSON payload covering
fonts, the Type 3 program faces, artkit's mood palettes, the
`pagekit.ps` templates, and artkit's/the style packs' major
procedures (`src/capabilities.rs`). Fonts are the one section built
dynamically, off the same `font::catalog_entries()` `--fonts` and
`findfont` resolution already use, so that section can't drift from
what's actually installed; everything else is hand-maintained (no
docstring convention in PostScript for this module to parse) but kept
honest by `tests/capabilities.rs`, which loads each `.ps` source into
a real `Interp` and checks the name set both ways -- every cataloged
name still exists, and every name a source file actually defines is
either cataloged or on an explicit internal-helper allowlist. See
CAPABILITIES.md for the payload shape and how to register a new
capability (NOTES.md's entry has the full story). Also done: issue
#48, deterministic scatter and distribution primitives for artkit --
the area-shaped counterpart to `alongpath`: `screct`/`scpath` build a
region (a rectangle, or the current path flattened and closed the way
`fill` sees it -- `clippath scpath` for the clip region),
`scin`/`scarea` interrogate one, and `scatter` places a
caller-supplied mark across it by fixed count or by density, with a
`/Weight` procedure for non-uniform distributions, seeded
scale/rotation variation, exact `/MinSpacing` (a sparse hash grid in
an ordinary PostScript dict, cells of MinSpacing/1.5 so at most one
mark per cell, 5x5 neighborhood -- not an O(n^2) scan), and a hard
deposit budget. Containment is a real crossing test over the captured
edges, so candidates outside a shape are *rejected* rather than drawn
and clipped. Three scratch prefixes (`sc-`/`sq-`/`si-`) rather than
one, because the natural way to write a non-uniform scatter is a
`/Weight` proc that calls `scin` from inside scatter's own loop.
`examples/scatter.ps` and the gallery piece `Firefly Census`
(`gallery/firefly_census.ps`, a night meadow where every mark on the
page is scattered and none placed by hand) demonstrate it (NOTES.md's
entry has the full story, including why `/Seed` saves and restores the
caller's random stream with `rrand` instead of just calling `srand`,
and why gs agrees on scatter's counts but not its placements). Also
done: issue #49, a reusable hatching and cross-hatching library — a
sixth sibling, `lib/hatchkit.ps` (no dependency on `artkit.ps` or any
other sibling): one operator, `hatch`, fills whatever region is
currently *clipped* with parallel line strokes, leaning entirely on
the graphics state's own `clip` rather than reimplementing
point-in-polygon math — lines sweep past the region's real boundary
and get cut to shape by the ambient clip, so concave and
self-intersecting regions clip exactly as cleanly as convex ones.
Single or multi-angle (`/Angles`, one full layered pass per angle —
cross-hatching is just two calls 90 degrees apart), seeded
width/wobble/dropout/length variation, and an optional `/Density`
callback (clamped into `[0,1]` regardless of what it returns, and
bounded by two independent deterministic-from-`/BBox`/`/Spacing`
pre-flight checks — `/MaxLines` and `/MaxSamples` — computed and
enforced before any drawing or RNG draw, so a pathological `/Spacing`
or callback can change tone but never spawn unbounded geometry).
`examples/hatching.ps` is a three-panel specimen sheet (flat shading,
a `/Density`-driven tonal band that reads as a curved lit sphere from
perfectly straight strokes, and layered cross-hatching). Tag-migrated
into the doc-comment capability catalog (issue #94) from the start,
rather than added to `build.rs`'s `LEGACY_FILES` (NOTES.md's entry has
the full story, including two real implementation bugs a `--headless`
render actually caught rather than reasoning about the PostScript: an
`hkclipseg` initial range too narrow to ever draw a full-length line,
and `/Density`'s own value stored under a bare name that silently
auto-executed the caller's callback mid-setup — the exact footgun this
file's own default-wrapping helper exists to avoid, applied
inconsistently to a second name in the same file).
Also done: issue #50, density-driven stippling and point-shading — a
seventh sibling, `lib/stipplekit.ps` (`@requires: (lib/artkit.ps) run`,
tag-migrated from the start like hatchkit). One operator, `stipple`, a
thin convenience layer over `scatter` (issue #48) rather than a second
placement engine — the region operand and every option not about
density (`/Count`, `/MinSpacing`, `/Seed`, `/Tries`, `/Budget`,
`/Scale`, `/Rotate`) are `scatter`'s own, forwarded unchanged.
`/Density` is a plain number (forwarded verbatim as `scatter`'s own
`/Density`) or a `{x y -> w}` relative-tone callback paired with a
required `/MaxDensity` (peak marks per area) — internally `stipple`
hands `scatter` `/MaxDensity` as its own `/Density` (driving `Count`)
and the callback, unwrapped, as `scatter`'s own `/Weight` (driving
shape): two existing mechanisms recombined, no new placement
arithmetic. The realized total deliberately tracks that peak times the
region's area, not the field's own spatial integral — `scatter`'s
retry-until-accepted `/Tries` loop makes the two diverge, a design bug
the advisor caught before any code existed (NOTES.md's entry has the
full story, including a second, independent bug hit during manual
smoke testing: the exact same bare-name auto-execution footgun
hatchkit's own entry above describes, in a brand-new file that didn't
reuse hatchkit's code and so didn't inherit its fix). Default `/Mark`
is a filled circle sized by `scatter`'s own `/Scale` range (no new
size-variation vocabulary); a caller-supplied `/Mark` overrides it
entirely for real point-shading. `examples/stippling.ps` is a
three-panel specimen (constant density, a callback-driven sparse-to-
dense tonal ramp, point-shading with a custom rotated-cross mark); no
gallery/site entry, matching hatchkit's own precedent that a primitive
gets an `examples/` specimen, not a gallery card.
Also done: issue #53, reusable halftone screens and misregistration
offsets — an eighth sibling, `lib/halftonekit.ps` (no sibling
dependencies: a halftone is a deterministic lattice, not random
placement, so routing it through `scatter` would add noise the medium
is defined by not having). One operator, `halftone`, fills the current
clip with a dot, line, or cross-line screen (`/Screen`, compared with
the language's own `eq` — a `(dot)` string selects the same screen as
`/dot`, verified identical in Ghostscript): per-cell tone from a
number or `{x y -> w}` callback (clamped before `sqrt` ever sees it),
`/Frequency` in cells per inch, per-layer `/Offset` via an
unconditional `translate`, and a single deterministic `/MaxCells`
pre-flight budget (one tone call and a fixed 1-or-2 marks per cell, so
one count bounds callbacks, geometry, and ink together — hatchkit's
two-budget shape exists only because its per-line sample counts vary).
`examples/halftone.ps` is a four-panel specimen ending in a
misregistered two-plate spread; no gallery/site entry, same primitive
precedent (NOTES.md's entry has the full story, including two real
bugs smoke-rendering caught: a two-operand `exch` on a one-operand
operator, and a normal-vector/count name collision that parked the
whole lattice at (560, 540)).
Also done: issue #51, paper/canvas/print-surface textures — a ninth
sibling, `lib/surfacekit.ps` (`@requires: (lib/artkit.ps) run`,
tag-migrated from the start). Five presets: `grain`/`fiber`/`scuff`/
`misreg` are thin `scatter` wrappers (the same "no new placement
engine" choice `stipplekit.ps` made); `weave` is its own grid-and-`scin`
loop with two independent pre-flight budgets (`/MaxThreads` on cell
count, `/MaxEdgeSamples` on cell-count-times-edge-count for a `scpath`
path region — mirroring `hatchkit.ps`'s own `/MaxLines`/`/MaxSamples`
shape). New `/Color`/`/Strength` options, each preset's call wrapped in
its own `gsave`/`grestore` since every default mark sets color.
`examples/surfacekit.ps` is a six-panel specimen sheet (NOTES.md's
entry has the full story, including why the "scratches and scuffs"
preset is named `scuff` rather than `scratch` — this codebase already
uses "scratch" as a term of art for private working state throughout
every sibling library's own docs, and five review rounds' worth of
auto-execution and unguarded-numeric-conversion hazards a caller-
supplied value could trigger before its type was checked).
Also done: issue #52, woodcut/linocut/engraving mark presets — a
tenth sibling, `lib/printkit.ps`, composing `hatchkit.ps`'s `hatch`,
`artkit.ps`'s `scatter`, and (optionally, `/Paper true`)
`surfacekit.ps`'s `grain` into three presets over one shared options
dict (`/Scale`/`/Density`/`/Roughness`/`/Seed`/`/Color`/`/Budget`/
`/Paper`/`/Angle`). Deliberately breaks from its siblings' `region opts
NAME` convention — each preset takes the *current path* directly (like
`hatch` itself) and manages its own `clip` internally, since
reconstructing an exact `clip`-able path back out of a stored region's
flattened edge soup would be real machinery for no benefit over the
path the caller already has. `examples/printkit.ps` is a four-panel
specimen sheet; the gallery piece *Nightfall, Three Cuts*
(`gallery/nightfall_triptych.ps`) uses all three presets in one
moonlit scene, a deliberate departure from the "primitive gets a
specimen, not a gallery card" precedent its own siblings set, since
issue #52 explicitly asked for a gallery composition (NOTES.md's
entry has the full story, including three design-review findings
`advisor` caught before any code existed — never trust `hatch`'s own
`pathbbox` default across a call that changes the current path first,
clip the true path before flattening one via `scpath`, and a
`/Budget` default that matches `hatch`'s/`scatter`'s own rather than
silently lowering either's ceiling).
Written for whichever model
picks the project up next — read this after `CLAUDE.md` and before
touching code. `ROADMAP.md` has the task list with model routing;
`NOTES.md` has per-stage histories; this file is the *orientation*.

## Where things stand

**623 tests across 40 suites, clippy clean.** Stages 1–19 are done,
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
- Stroke width ≈ √|det CTM| (anisotropic pens wrong); `showpage`
  doesn't erase; ints are i64; `flattenpath` uses a fixed
  quarter-pixel tolerance (`setflat` not modeled — chord counts
  differ from gs, shapes agree).
- `shfill` (issue #20): `FunctionType` restricted to 2 (exponential)
  and 3 (stitching) — 0 (sampled) and 4 (calculator) unsupported,
  since those would need the same `Frame::PostOp` reentrancy
  `Separation`'s tint transform uses, which axial/radial gradients
  don't otherwise need. `ColorSpace` restricted to `/DeviceGray`/
  `/DeviceRGB`/`/DeviceCMYK` (no Indexed/Separation, no
  array-of-functions form). `/Range` is honored only on the top-level
  Function a shading dict names directly, not recursively per
  stitching leg. `/Extend` is validated but always behaves as
  `[true true]` — the `gsave <path> clip shfill grestore` idiom
  already bounds the painted region, so `false`'s "transparent beyond
  the axis" is an edge case that idiom never hits. SVG export gets
  real `<linearGradient>`/`<radialGradient>`; PDF export approximates
  a shading as a flat fill in the ramp's *position-weighted* average
  color (no pattern-colorspace machinery). SVG's two-circle radial
  model only renders faithfully when the focal circle sits entirely
  inside the outer one (`distance(centers) + focal_r <= outer_r`) — an
  off-center `ShadingType` 3 that fails that (valid PostScript; no
  such constraint exists there) isn't detected or worked around, so
  its SVG export can visibly diverge from the raster.
- `setalpha`/`currentalpha` and `setblendmode`/`currentblendmode`
  (issue #47) are **pscat extensions, not PLRM operators** — real
  Ghostscript has no PostScript-callable alpha operator at all
  (`.setfillconstantalpha`, `.setopacityalpha`,
  `.setstrokeconstantalpha`, `setalpha` are all `where`-undefined on
  gs 10.x; gs's transparency lives inside its PDF interpreter). Two
  consequences worth knowing before touching either:
    * **Alpha does not reach `image`/`imagemask`.** `src/image.rs`
      blits samples straight into the pixmap rather than going through
      `Gfx::paint()`, so a translucent `image` silently paints opaque.
      Documented at the field, the operator, README and the tests;
      closing it means teaching the image blitter about the graphics
      state, not just adding a call.
    * **A program using them will not render the same under plain `gs
      file.ps`.** The verification route for alpha-bearing content is
      `--pdf` plus gs rasterizing that PDF back (`tests/pdf.rs`'s
      `alpha_survives_the_round_trip_through_gs`), which is stronger
      than the `ghostscript_accepts_*` pattern, not weaker — it
      compares pixels rather than exit status. `lib/paintkit.ps`'s
      watercolor section carries a flatten-against-white fallback for
      the `gs file.ps` path; see `docs/WATERCOLOR.md`.
  Blend modes are deliberately just `Normal`/`Multiply`, on pscat's own
  two-variant enum rather than a re-export of `tiny_skia::BlendMode`:
  each variant has to map to tiny-skia, SVG's `mix-blend-mode` *and*
  PDF's `ExtGState /BM`, and an exhaustive match on a local enum is
  what makes adding a third mode fail to compile until all three
  exporters learn about it.
- `error_report`'s `Line: N` (issue #17) is best-effort, not exact
  source attribution: no `Object` carries a source position, so it
  reports the line of the most recent token scanned directly from the
  *top-level* program (`Lexer`'s `is_main` flag) — never a
  `run`-loaded library file, an eexec stream, or an executable
  string, which would misattribute to the wrong source entirely — and
  stays sticky across procedure calls, so an error deep inside a
  previously defined procedure gets the call site's line, not the
  procedure's definition site.

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
4. Watercolor (issue #47, gated on the architecture spike in issue
   #46): read `docs/WATERCOLOR.md` before starting — it recommends a
   small `alpha: f32` field on `GraphicsState` (already prototyped as
   `pub(crate)`-only in `src/gfx.rs`'s `tests::
   watercolor_prototype_b_alpha_sample`, not yet a public operator) as
   the primary mechanism, plus the SVG/PDF export work that prototype
   deliberately left undone, and names a real gs-portability gap (no
   PostScript-callable alpha operator in gs 10.07.1) to document rather
   than paper over.
5. Shrinking PS-library-only coupling to Rust/CI/gs (architecture
   spike in issue #92): **read `docs/PS_LIBRARY_COUPLING.md` in full
   before starting either of its two remaining follow-ups (a phased
   PS-native verification path, CI diff-shape detection) — do not work
   from a summary of it, including this one.** That document itself
   needed eight rounds of cross-model review to converge (mostly the
   same failure mode each time: a correction made in one section not
   propagating to a shorter restatement elsewhere, including in an
   earlier version of this very HANDOFF.md entry) — a short paraphrase
   here is exactly the kind of restatement that risks going stale the
   next time the source document is revised. Read the whole thing.
   The first follow-up, a doc-comment-driven capabilities catalog, is
   done (issue #94): new `% @kind:`/`% @summary:`/`% @example:`/
   `% @param:`/`% @internal`/`% @requires:` doc-comment tags in
   `lib/*.ps`, parsed at build time by the new `build.rs` into
   `src/capabilities.rs`'s catalog — see `build.rs`'s own module docs
   for the tag grammar and NOTES.md's issue #94 entry for the full
   story. `lib/paintkit.ps` and `lib/hatchkit.ps` (issue #49 — a
   brand-new file, so it started tagged rather than being added to
   `LEGACY_FILES`) are migrated so far; migrating the rest of
   `lib/*.ps` — `artkit.ps`/`pagekit.ps`/the four style packs/
   `handscript.ps`/`hangul.ps` — is itchy-when-you-get-to-it follow-up
   work, not a filed issue. The mechanism already
   handles `Template`/`Dial` generically (same `/name ... def`
   discovery as `Procedure`) — including `lib/styles/*.ps`'s
   `/name /othername def` shape (a Dial bound to another name literal,
   e.g. `/spmetal /brass def`) and `bind def`, both needing a real
   redesign of the definition-name tokenizer to get right, not a
   one-line fix — two rounds of Codex review on PR #97 caught two
   different ways a "which literal is the name" heuristic broke on
   real `lib/styles/steampunk.ps` code; see `build.rs`'s
   `find_top_level_defs` docs and NOTES.md's issue #94 entry for the
   full story before touching that function again.
   Two kinds still need new discovery logic before their file can
   migrate: `Palette` (`Palettes /name [...] put` dict-literal
   mutations, not `def` bindings) and `Type3Face` (`/Name Dict
   definefont pop`, not `/name ... def` either — `@kind: Type3Face` is
   explicitly rejected by `build.rs` until this exists, so
   `handscript.ps`/`hangul.ps` can't migrate yet regardless of
   `Palette`). Migrating a file needs no new test either —
   `tests/capabilities.rs`'s `every_migrated_file_names_
   match_the_catalog_exactly` cross-checks every file
   `capabilities::migrated_files()` reports, generically.
   The second follow-up, a PS-native verification path, has its
   **Phase A** done (issue #95): `%%SelfTest` doc-comment blocks run by
   `pscat --selftest`, plus a strict `--lint` mode that actually fails
   the process, run against rendering drivers under `selftest/drivers/`.
   `./scripts/selftest.sh` runs both, and CI runs it as its own step.
   **Read `docs/SELFTEST.md` before adding self-tests or a driver** —
   same reasoning as above: it records the assertion vocabulary, why
   there is deliberately no "assert something raised" form, and (with
   `docs/GS_CHECK_INVENTORY.md`) exactly which defect classes stay
   uncovered and why. Two pieces remain open, both filed:
   **Phase B** (#134), a pixel-sample operator for the
   geometry/measurement class Phase A structurally can't reach, and the
   `ghostscript_accepts_*` extraction (#135), which #95 inventoried (25
   drivers today, not the decision record's 16) rather than performed.

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
  per Interp, process-lifetime object) — genuinely harmless for the
  usual one-`Interp`-per-process pattern, but it also leaks `userdict`
  (and everything a program stored there), since systemdict holds a
  strong reference to it too. A caller that constructs many `Interp`s
  in one process run must call `Interp::break_permanent_dict_cycle(self)`
  on each one instead of just dropping it, or that stops being
  bounded — confirmed empirically via `Rc::downgrade`/`strong_count`,
  not just reasoned about (NOTES.md's issue #21 entry has the story).
  It takes `self` by value specifically so a caller can't accidentally
  keep using an `Interp` after its systemdict has been emptied — that
  would be a compile error, not a silent `undefined` at runtime. Two
  call sites do this today: issue #21's `--sweep-seed`/`--sweep` loop
  (`main.rs::run_sweep`, one call per frame) and `--spool`'s per-job
  loop (`window.rs::poll_spool`, via `mem::replace` to get the
  outgoing job's `Interp` out of the struct field before the new one
  overwrites it). Any future caller that constructs more than one
  `Interp` in a long-running or high-iteration process should do the
  same.
- `build.rs`'s `% @...` tag scanner models the depth-0 operand stack
  with a **two-slot window**, so it only understands `def` whose key is
  exactly two objects back. `/name systemdict /other known def` is read
  as defining `/other` (issue #47 hit this; same family as issue #104's
  open parser gaps). Write such a definition probe-first —
  `systemdict /other known /name exch def`, all on one line so the tag
  block stays directly above the `def` — or teach the scanner operator
  arities, which is a real change, not a tweak.
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
./scripts/selftest.sh                     # PS-native checks (no gs, no Rust)
cargo run -- examples/postcard.ps         # watch it draw
./gallery/show.sh --live                  # the fun regression suite
```
