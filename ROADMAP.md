# ROADMAP.md — Stages 5+ and the path to broad spec coverage

Stages 1–4 (see `NOTES.md`) delivered the language core, live-window
graphics, control flow, and a robustness pass. This file plans everything
after, with **model routing per task** so work can continue on whatever
Claude model is at hand. Read `AGENTS.md` for working conventions before
starting any task here.

## Model routing — how to read the tags

Each task is tagged with the *least capable* model expected to do it well.
A more capable model can always take a smaller task; the reverse invites
rework.

- **[haiku]** — mechanical, well-specified, existing pattern to follow:
  single operators with clear PLRM semantics, test tables, doc sync,
  CLI flags. The `src/ops/` modules are deliberately formulaic; copying
  the shape of `ops/arith.rs` is most of the job.
- **[sonnet]** — the default workhorse: multi-operator features, new
  execution-stack frame types, rendering features with tiny-skia,
  anything where the design is settled and the work is careful
  implementation plus tests.
- **[opus]** — cross-cutting or architecturally risky: changes that touch
  the object model, the execution machine's control flow, font/file
  machinery, or performance refactors guided by profiling.
- **[opus+review]** — do the design writeup first, get it reviewed (by a
  person or a stronger session) before implementing. Reserved for the
  few features flagged in `ARCHITECTURE.md` as able to force rewrites.

**Escalation rule (applies to every task):** if a `[haiku]`/`[sonnet]`
task turns out to require changing `src/object.rs`, `src/interp.rs`'s
frame machinery, or error semantics, *stop and record the finding in
`NOTES.md`* rather than improvising — that's the tripwire that the task
was mis-tagged and needs an `[opus]` pass.

**Every task, regardless of model:** `cargo test` green, clippy clean,
new operators get `tests/eval.rs`-style cases, rendering changes get
pixel or golden tests, and behavior questions are settled by the PLRM
first and `gs` second (`tests/golden.rs` shows the comparison pattern).

---

## Stage 5 — Run found PostScript (data structures + error recovery)

**✅ COMPLETE (2026-07-12)** — all twelve items shipped; see `NOTES.md`
for the summary and the deviations list (errordict custom handlers and
error-time operand restoration remain open, folded into future work).

Goal: real `.ps` files from the internet execute. They lean on arrays,
strings, `forall`, and `stopped` constantly. Demonstrable by a corpus of
found files rendering side-by-side with Ghostscript.

1. **Generalized dict keys** — `Dict` today is keyed by name text only;
   the PLRM allows any object (numbers as-is, strings→names). Introduce a
   proper key type in `object.rs`. Do this *first*; get/put build on it.
   [opus] (small but it's an object-model change)
2. **Array/string element ops** — `get put length getinterval putinterval
   copy` (polymorphic), `array string aload astore`. Watch reference
   semantics: intervals share storage in real PostScript — decide and
   document subarray representation (offset+len view vs. copy). The view
   design needs the [opus] pass; the operators on top are [sonnet].
3. **`forall`** — new loop-frame type over array/string/dict; follow the
   `Frame::For` pattern in `interp.rs` exactly. [sonnet]
4. **Type conversions** — `cvi cvr cvn cvs cvx cvlit type xcheck`.
   Formulaic; `cvs` reuses `Object::text()`. [haiku]
5. **Dict conveniences** — `<< >>` construction (they're already lexed as
   names), `known where store currentdict countdictstack cleardictstack
   maxlength`. [haiku]
6. **`stop`/`stopped`** — a stop-context frame on the execution stack;
   errors become catchable. This changes how `run_source` unwinds, so
   design against `interp.rs` carefully. [opus]
7. **errordict + `$error` + `handleerror`** — standard error reporting on
   top of #6; `PsError::name()` already matches PLRM names. [sonnet]
8. **String search ops** — `search anchorsearch token`. `token` reuses
   the lexer on a string body. [sonnet]
9. **`clip`/`clippath`/`initclip`** — tiny-skia `Mask` from the current
   path; clip state joins `GraphicsState` (gsave/grestore already clone
   it). Even-odd variant `eoclip` too. [sonnet]
10. **Matrix operands** — `matrix identmatrix currentmatrix setmatrix
    concat concatmatrix transform itransform dtransform idtransform
    invertmatrix rotate/scale/translate` with-matrix forms. Depends on
    task 2 (matrices are 6-element arrays). [sonnet]
11. **Missing small ops** — `rand srand rrand` (an LCG — port it back
    out of `gallery/*.ps`!), `min`-less `max`-less math extras `cvrs`,
    `bitshift`, `setdash` (tiny-skia StrokeDash), `sethsbcolor/
    currenthsbcolor` (port `sethsb` from the gallery), `currentrgbcolor
    currentgray currentlinewidth`, `//name` immediate names in the
    lexer, ASCII85 string literals. [haiku]
12. **Found-file corpus test** — collect ~10 real-world PS/EPS files
    (public domain), wire into `tests/golden.rs`'s gs comparison,
    document per-file status. [sonnet]

## Stage 6 — Text and fonts

Goal: `(Hello, LaserWriter) show` — and the gallery gains typography.
This is the emotional payoff stage; it's sequenced after Stage 5 because
font machinery uses dicts/arrays heavily.

**✅ COMPLETE (2026-07-13)** — all seven tasks; see `FONTS.md` for the
architecture and `NOTES.md` for the summaries.

1. ✅ **Font architecture writeup** — `FONTS.md`: font dicts are real
   Dicts with `FID` as the registry seam; bundled Liberation faces via
   `ttf-parser` back the standard names; `setfont` caches FontMatrix,
   `show` reads Encoding live. [opus+review]
2. ✅ **Font dict plumbing** — `findfont scalefont makefont setfont
   currentfont definefont selectfont`, FontMatrix composition. [sonnet]
3. ✅ **`show` and metrics** — `show stringwidth charpath ashow
   widthshow awidthshow` (`kshow` deferred to task 5 — it needs the
   same in-show procedure execution as BuildChar). [sonnet]
4. ✅ **Encodings** — StandardEncoding/ISOLatin1Encoding vectors,
   `Encoding` array remapping (read live per glyph). [haiku]
5. ✅ **Type 3 fonts** — the show family became a `Frame::Show` on the
   execution stack (one glyph per step); `BuildChar`/`BuildGlyph` run
   in sealed glyph contexts; `setcachedevice(2)`/`setcharwidth`;
   `kshow` (deferred from task 3) rode along. [opus]
6. ✅ **Type 1 fonts** — eexec decryption (via the Stage 7 file/filter
   machinery, done first for exactly this reason), `src/type1.rs`
   charstring interpreter (hints ignored, flex, seac, OtherSubrs),
   verified by generating a complete PFA and cross-checking gs accepts
   the same bytes. [opus]
7. ✅ **Glyph cache** — measured first, per the task: face-parse cache
   (OnceLock) took stringwidth 2×; show is rasterization-bound at
   ~330k glyphs/sec, so the bitmap cache is deferred until something
   feels slow (same doctrine as name interning). [sonnet]

## Stage 7 — Images and filters

Goal: EPS files with embedded images render; `image`/`imagemask` work.

**✅ COMPLETE (2026-07-16)** — see `NOTES.md` and `HANDOFF.md`.
`examples/postcard.ps` demonstrates the whole pipeline inline and sits
in the golden suite. DCTDecode landed 07-16, after the 07-13 wrap.

1. ✅ **File objects + `currentfile`** — `Value::File` sharing one read
   cursor with the scanner (`src/file.rs`); `read readstring
   readhexstring readline closefile bytesavailable status`, exec/token
   on files, `run`, read-only `file`. [opus]
2. ✅ **Filter framework + easy decoders** — `filter`; on-demand
   ASCIIHex/ASCII85/RunLength + the eexec cipher; a filter consumes
   exactly the source bytes its consumer needed. [sonnet]
3. ✅ **Compression decoders** — FlateDecode (`flate2`), LZWDecode
   (hand-rolled), DCTDecode (`zune-jpeg`; buffers one whole JPEG but
   marker-aware, consuming exactly through EOI). [sonnet]
4. ✅ **`image`/`imagemask`/`colorimage`** — `Frame::Image`, both
   operand forms, gray/RGB/CMYK 1/2/4/8-bit, Decode arrays, proc/file/
   string sources, filter-chain draining, minimal `setcolorspace`.
   Gap: MultipleDataSources colorimage (limitcheck). [opus]

## Stage 8 — VM fidelity and performance

Goal: the semantics that separate a toy from an implementation.

**✅ COMPLETE (2026-07-16)** — save/restore, interning, benches, color
spaces, Level 2 odds and ends, corpus round 2 (the gs classics render
block-identical), `--pstack-on-error`. The one open sliver is task
6's `--interactive` windowed REPL (design note in NOTES.md).

1. ✅ **`save`/`restore`** (2026-07-16) — object-granularity
   copy-on-write journaling; design writeup and gs pins in `VM.md`,
   deviations documented there (no invalidrestore stack scan; files
   not closed by restore). `savetype` objects, `vmstatus`,
   `grestoreall` included. [opus+review]
2. ✅ **Name interning** (2026-07-16) — `src/name.rs`: `PsName`
   (interned id + shared text, `Deref<Target=str>`), thread-local
   interner, dicts keyed by id with a one-multiply hasher. fib 27
   214ms→137ms, fern 375ms→258ms; remaining gap vs gs is the frame
   loop's per-element RefCell borrow + Object clone (future lever).
   [opus]
3. ✅ **Benchmark suite** (2026-07-16) — `benches/perf.rs`, four
   workloads (fib/defloop/sierpinski/fern), best-of-three wall clock
   under `cargo bench`. [haiku]
4. ✅ **Color spaces** (2026-07-16) — Indexed (string lookups) and
   Separation (tint transform via the new `Frame::PostOp`
   continuation), `setcolor`, spaces in the gsave snapshot;
   `setcmykcolor` predated this. Gaps in `ops/color.rs` docs. [sonnet]
5. ✅ **`packedarray`, `usertime`/`realtime`, `languagelevel`, resource
   category basics** (2026-07-16) — `ops/level2.rs`; Font category
   delegates to findfont/definefont; writes journaled. [sonnet]
6. **Interactive niceties** — ✅ `--pstack-on-error` (REPL + headless,
   2026-07-16). The `--interactive` window+REPL flag remains: design
   note in NOTES.md (stdin thread → EventLoopProxy user events →
   run_str between frames); deferred until the windowed path can be
   exercised for real. [sonnet]

## Stage 9 — Output targets

**✅ COMPLETE (2026-07-16)** — multi-page showpage (lazy erase), --dpi,
SVG export, PDF export; every target verified against gs or a browser
rasterization of our own output.

1. ✅ **Multi-page documents** (2026-07-16) — showpage snapshots the
   page, full initgraphics (gs-pinned), *lazy* erase (canvas keeps the
   finished page until the next mark — the window keeps its picture);
   copypage/initgraphics ops; `--png` numbers multi-page output;
   arrow-key page browsing in the window. [sonnet]
2. ✅ **PDF export** (2026-07-16) — `--pdf`; design note in
   `src/pdf.rs`: the paint-pipeline-mirror recorder (as proven by SVG)
   beat both re-execution (side effects, rand) and a retained display
   list (no third consumer yet). Content streams in our device space
   (one flip cm per page), per-element q…Q clip chains, images as
   Flate RGB XObjects, imagemasks as /ImageMask stencils, text as
   outlines. Oracle: gs rasterizes our PDFs and they block-match our
   canvas. [opus+review]
3. ✅ **Print-resolution rendering** (2026-07-16) — `--dpi`, verified
   pixel-identical against gs -r144. [haiku]
4. ✅ **SVG export** (2026-07-16) — `--svg`; `src/svg.rs` mirrors the
   paint pipeline (device-space paths 1:1, glyphs as outlines, clip
   chains as nested clipPath groups, images as embedded base64 PNG
   with the exact rasterizer transform); one document per page.
   Verified by rasterizing the output in a browser against the
   canvas. [sonnet]

## Stage 10 — The LaserWriter experience (stretch/fun)

- ✅ **Spool mode** (2026-07-17): `--spool DIR` — the window idles
  like the printer in the corner of the lab, polls the directory
  (400ms), and renders each new `.ps`/`.eps` in a fresh interpreter;
  files must hold size+mtime across two polls (no half-copied jobs),
  startup contents don't reprint, rewrites do. `src/spool.rs` is the
  testable watcher; port listening deferred until wanted. [sonnet]
- **Halftone screens**: classic `setscreen`-style dots for an authentic
  300dpi-printer look as an optional render mode. [sonnet]
- **Gallery II**: new pieces exploiting Stage 5/6 features (text art,
  clipped compositions, image-based collage). Any model with taste — the
  constraint-driven format in `gallery/README.md` is the brief. [any]
  - ✅ *Hundred Lines* (2026-07-17) — `gallery/hundred_lines.ps`, the
    /HandScript chalkboard: the Stage 12 font writing punishment
    lines, jitter and pen width overridden per line through the
    font's scratch dict. More pieces welcome; the slot stays open.

## Stage 11 — Performance parity with Ghostscript

Goal: know exactly where pscat stands against gs on speed, memory,
and overall resource usage — with a repeatable harness, not anecdotes
— and close the gaps worth closing.

**✅ COMPLETE (2026-07-17)** — see NOTES.md for the full findings.
Headline: pscat beats gs on startup (4ms vs 19ms), memory (3–5×
smaller on every workload), save/restore (8×), and every rendering
page (5–7×); defloop is tied. Remaining gaps (fib ~2.3×, fern ~1.9×
net of startup) are per-element machine-loop costs; the justified
fix (Drop moved from Object to PsArray, ~8–20% across workloads)
landed, the unjustified one (name-lookup cache — built two ways,
measured, slower on def-heavy code) was reverted with its numbers
recorded.

1. ✅ **Comparison harness** — `benches/vs_gs.rs` (wall + peak RSS,
   startup row for netting). [sonnet]
2. ✅ **Investigation** — `sample` profiles of fib and fern; findings
   in NOTES.md. Fern is interpreter-bound, not raster-bound. [opus]
3. ✅ **Targeted optimizations** — `Drop` relocation (kept, with
   numbers); lookup cache (rejected, with numbers); the remaining
   levers (bytecode-style bodies, per-name binding slots, NaN
   boxing) are recorded as future representation changes. Memory
   needed nothing: smallest footprint of the two by far. [opus]

## Stage 12 — Handwritten text (dynamic glyph generation)

Goal: `(hello) show` in a handwriting font where every glyph instance
is generated fresh with small random perturbations — jittered control
points, wobbling baseline, varying slant and stroke width — so
repeated characters never render identically and a page reads like a
human actually wrote it.

**✅ COMPLETE (2026-07-17)** — `examples/handwriting.ps`: the
/HandScript Type 3 font, pure PostScript riding on existing
machinery. Verified in both interpreters (gs runs the same file).

1. ✅ **Stroke-skeleton font data** — single-stroke letterforms
   (full lowercase, digits, punctuation) as flat polylines on a
   1000-unit grid, smoothed at draw time into quadratic-style curves
   so corners round the way a moving pen rounds them. [sonnet]
2. ✅ **The dynamic font** — BuildChar jitters every point through
   the interpreter's own `rand` (±16 units), adds per-glyph slant
   (upright-to-eager bias), baseline drift, and pen-pressure width
   variation, then strokes with round caps. Type 3 glyphs are
   deliberately uncached, so every `show` re-rolls; the seeded LCG
   keeps whole pages reproducible (`tests/handwriting.rs` pins
   determinism *and* that no two glyph instances match). BuildChar
   defs go in a scratch dict — gs makes font dicts read-only at
   definefont, and the file runs there too. [sonnet]
3. ✅ **Demo** — the example page is a handwritten letter to
   Ghostscript on ruled paper; structure tests assert ink, page
   count, per-instance variation, determinism, and gs acceptance
   (pixel comparison excluded like the other rand-driven art). A
   Gallery II entry can reuse the font wholesale. [any]

---

## Standing gaps not tied to a stage

- `NOTES.md` records per-stage deviations; the current standing ones are
  stroke-width √|det CTM| approximation (revisit if anisotropic `scale`
  art appears), REPL single-window separation, and int width being i64
  rather than 32-bit.
- Error-operand restoration: PLRM error handlers see the operand stack
  *as it was before the failed operator*; we currently leave partial
  pops. Fold into Stage 5 task 7. [sonnet]
- `gs` remains the behavioral oracle: any semantics disagreement gets a
  minimal repro in `tests/` before it gets a fix.

## Suggested cadence

Each stage: architecture note (if tagged for one) → implementation in
PLRM-grouped chunks with tests → gs cross-check → `NOTES.md` summary +
commit per chunk. Stage 5 is the next thing to start; its task 1 and
task 6 are the two [opus] gates — everything else in the stage can
proceed on [sonnet]/[haiku] once those two land.
