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
block-identical), `--pstack-on-error`. Task 6's last sliver, the
`--interactive` windowed REPL, landed 2026-07-18.

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
6. ✅ **Interactive niceties** — `--pstack-on-error` (REPL + headless,
   2026-07-16); `--interactive` / `-i` (2026-07-18), the windowed
   REPL, built exactly as the design note planned: a stdin reader
   thread ships raw lines through `EventLoopProxy` user events, the
   event loop owns the interpreter and runs each complete chunk on
   the normal frame budget (so a pasted fractal still draws live).
   Line accumulation (`...>` continuation) moved to `src/repl.rs`,
   shared with the terminal REPL and unit-tested there. An optional
   file argument runs first as a prelude; EOF drains queued input
   before exiting, so piped sessions work. [sonnet]

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
- ✅ **Halftone screens** (2026-07-17): `--halftone` — `src/halftone.rs`
  screens the finished raster (window + `--png`; vector targets stay
  contone): euclidean dots on a 45° lattice, black dots to 50% then
  white corner holes, coverage tracking darkness (a naive 1−r²
  threshold overshoots midtones — caught by the coverage test). The
  `setscreen` *operator* (per-color screens, spot procedures) stays
  future work if a found file ever needs it. [sonnet]
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

## Stage 13 — handwrite: string in, handwritten PNG out

**✅ COMPLETE (2026-07-17)** — all three items; see `NOTES.md`. The
README's "Handwrite" section documents usage; the options-dict
schema is in `lib/handscript.ps`'s header.

Goal: `./scripts/handwrite.sh "any text"` produces a PNG of the text
written in the Stage 12 /HandScript font, word-wrapped across
multiple lines the way a person fills a page — and the machinery is
reusable, not buried in the script.

1. **Reusable PostScript library** — `lib/handscript.ps`: the
   /HandScript font plus a dict-driven layout API (`hs-write` draws,
   `hs-linecount` measures; one options dict documented in the file
   header controls text, size, column width, margins, leading,
   jitter, pen width, ink color, ruled/plain paper, seed). Business
   logic lives in structured PostScript so other applications can
   embed the file wholesale; it must run in gs unchanged. Word wrap
   is greedy on skeleton advances (jitter never moves a line break);
   embedded newlines force breaks. [sonnet]
2. **Bash wrapper** — `scripts/handwrite.sh`: appearance options as
   CLI flags mapping 1:1 onto the options dict, plus `--dpi`,
   `--halftone`, and `-o` passthroughs. Auto-sizes the page height
   by running a headless `hs-linecount` pre-pass, then renders the
   PNG with pscat. Input is lowercased (the font is lowercase-only);
   `\`, `(`, `)` escaped into the PostScript string. [sonnet]
3. **Tests + docs** — wrap-engine tests through the interpreter
   (width shrinks → line count grows; newlines force breaks; ink is
   deterministic under a fixed seed; gs accepts the library), and a
   README section documenting script usage and library reuse.
   [sonnet]

---

## Stage 14 — pscat for agents: skill, MCP server, CLI polish

**✅ COMPLETE (2026-07-18)** — all four tasks; NOTES.md has the
summary (the design decision: MCP shells out to the CLI so
interpreter stdout can't corrupt the protocol channel).

Goal: any coding agent — Claude Code, Codex, OpenClaw, Hermes — can
pick up pscat as a *tool*: render PostScript, produce handwritten
notes, and debug programs, without reading the whole repo first. Two
integration surfaces, because agents differ: instruction files for
agents that read docs (skills / AGENTS.md), and an MCP server for
agents that call tools.

1. **CLI polish for programmatic callers** — `pscat -` reads the
   program from stdin (pipe-friendly: `generate | pscat --png out.png -`);
   audit that every mode has clean exit codes and stderr/stdout
   discipline (errors → stderr, artifacts announced on stdout).
   [haiku]
2. **Claude Code skill** — `.claude/skills/pscat/SKILL.md`: when to
   reach for pscat (render/preview/debug PostScript, handwritten
   notes via `scripts/handwrite.sh`, spool/halftone modes), the
   command recipes, and the pitfalls (showpage vs trailing art,
   `--page` vs `--dpi`, HandScript is lowercase-only). Skills are
   markdown with frontmatter — the same file doubles as reference
   for any doc-reading agent; AGENTS.md gets a short "using pscat
   as a tool" pointer so Codex finds it too. [sonnet]
3. **MCP server** — `pscat-mcp`, a second binary speaking MCP over
   stdio (JSON-RPC 2.0: initialize, tools/list, tools/call). Tools:
   `render_postscript` (source in; PNG image content or SVG/PDF out,
   page size / dpi / halftone options), `handwrite` (text in,
   handwritten-note PNG out, the handwrite.sh options), and
   `eval_postscript` (source in, prints and operand stack out — the
   debugging loop). The server shells out to the pscat CLI rather
   than linking the interpreter, so interpreter stdout (`=`, `print`)
   can never corrupt the protocol channel, and the tools stay in
   lock-step with the CLI. serde_json is an acceptable dependency
   for the binary. Integration test drives the real binary over
   stdio. [sonnet]
4. **Client wiring docs** — README "For agents" section: one-line
   registration for each client (`claude mcp add pscat -- <path>`,
   `codex mcp add`, OpenClaw/Hermes generic MCP config JSON), plus
   what each tool returns. [haiku]

## Stage 15 — the font library: artistic Type 3 faces

**✅ COMPLETE (2026-07-18)** — all four faces shipped
(`lib/fonts/{neon,marquee,constellation,lapidary}.ps`), plus the
four-band specimen (`examples/font_library.ps`, render at
`lib/fonts/specimen.png`) and `tests/fontlib.rs` (loads/inks/case
mapping/seeded reproducibility/gs accepts). NOTES.md records the two
craft findings: doubled skeleton points pin sharp corners through
the midpoint smoothing, and rand's low bits correlate across draws
(burnt-out bulbs came in runs until the draw moved to high bits).

Goal: `lib/fonts/` — a library of original display fonts, each a
self-contained pure-PostScript file (the `lib/handscript.ps`
doctrine: loading defines the font and draws nothing, runs unchanged
in gs, embeddable wholesale). Each font is a *concept*, not a
digitization — Type 3 BuildChar is a program, so the letterforms can
do things outline formats can't. Candidate faces (pick the stunning
ones, drop the merely cute; every shipped font covers at least A–Z,
digits, and basic punctuation):

- **/Lapidary** — chiseled Roman capitals: every stroke cut twice
  (dark incision offset from a lit face) so the letters read as
  carved into the page. [sonnet]
- **/Constellation** — letters as star charts: bright stars of
  varying magnitude at the skeleton's anchor points, hairline
  great-circle segments between them, faint scatter of field stars —
  best on a midnight ground. [sonnet]
- **/Marquee** — theater-sign bulbs: evenly spaced glowing dots
  along the stroke skeletons, warm halos, occasional burnt-out bulb
  (rand, seeded). [sonnet]
- **/Neon** — glass-tube script: each stroke drawn as layered
  strokes of descending width and rising brightness over a dark
  ground, round caps everywhere, the glow done purely with
  overdraw. [sonnet]

Shared machinery: one capital skeleton set (polyline anchors, the
HandScript CharDefs pattern) may be duplicated into each file —
self-containment beats deduplication here, as documented in
handscript.ps. Deliverables per font: the library file, a specimen
line in a combined `examples/font_library.ps` showcase page, and a
render for the README. Wrap-up: gs accepts every font (pinned by
test, same policy as handwriting), plus a gallery-quality showcase
render. [any model with taste, per gallery/README.md's brief]

---

## Stage 16 — pscat in the browser: WASM + JS library

Goal: the live window, but it's a `<canvas>`. Compile the
interpreter core to `wasm32-unknown-unknown` and ship a small
JavaScript library that renders and *executes* PostScript in the
browser — including step-driven execution, so a page can watch a
program draw exactly the way the winit window does.

1. **Make the core cross-compile** — gate the desktop-only modules
   (`window.rs`, `spool.rs` — winit/softbuffer and std::fs have no
   business in wasm) behind `#[cfg(not(target_arch = "wasm32"))]`;
   shim the two wall clocks (`usertime`/`realtime` use
   `Instant`/`SystemTime`, which panic on wasm32-unknown-unknown —
   return 0 there, documented deviation); add `cdylib` to the crate
   types. Everything else is already pure Rust (tiny-skia, flate2's
   Rust backend, ttf-parser, zune-jpeg; fonts are include_bytes).
   No wasm-bindgen: the export surface is a hand-rolled C ABI over
   byte buffers, in keeping with this repo's no-framework habit.
   [sonnet]
2. **The wasm API** (`src/wasm.rs`, cfg'd to wasm32) — exports:
   alloc/dealloc, `ps_begin(src, w, h)`, `ps_step(n)` (1 = more
   work, 0 = done, -1 = error and the estack is cleared, session
   continues), `ps_pixels`/`ps_width`/`ps_height` (the live RGBA
   canvas), `ps_error` (last error report), `ps_run` (begin + drive
   to completion). One interpreter per module instance,
   thread-local. [sonnet]
3. **The JS library** (`web/pscat.js`, ES module, no dependencies) —
   `Pscat.load(wasmUrl)` → instance with `run(source)`,
   `begin(source)` / `step(n)`, `paintTo(canvas)` (putImageData of
   the live pixmap), `error`. Plus `web/index.html`: a demo page
   with editor textarea, canvas, speed slider, and a
   requestAnimationFrame loop — the watch-it-draw window, in a
   browser tab. `scripts/build_wasm.sh` builds and copies the .wasm
   next to the JS. [sonnet]
4. **Tests + docs** — a smoke test that (when the wasm target and
   node are installed, skipping gracefully otherwise, gs-test
   style) builds the wasm, instantiates it under node, runs a
   program, and checks pixels land; README "In the browser"
   section; skill note. [sonnet]

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
