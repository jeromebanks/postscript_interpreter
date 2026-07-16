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

1. **`save`/`restore`** — full VM snapshot semantics. The one feature
   flagged since Stage 1 as able to reach into the object model
   (copy-on-write vs. generation-stamped journaling — decide on paper
   first). `gsave` state, dict contents, and array contents all roll
   back. [opus+review]
2. **Name interning** — replace `Rc<str>` name keys with interned
   symbols; benchmark first (`fib 27` vs gs is the existing yardstick,
   ~3× gap attributed to hashing). Touches lexer, object model, dicts.
   [opus]
3. **Benchmark suite** — `benches/` with the fib/loop/sierpinski/fern
   workloads so #2 and future perf work have regression cover. [haiku]
4. **Color spaces** — `setcmykcolor` (and real CMYK→RGB), Indexed,
   Separation as they appear in found files. [sonnet]
5. **`packedarray`, `usertime`/`realtime`, `languagelevel`, resource
   category basics** (`defineresource findresource`). [sonnet]
6. **Interactive niceties** — `pstack`-on-error option in the REPL, an
   `--interactive` flag that opens the window *and* a REPL together
   (needs a second thread or event-loop integration — small design
   note first). [sonnet]

## Stage 9 — Output targets

1. **Multi-page documents** — `showpage` advances a page counter; window
   gains page navigation; `--png` writes `out-001.png` etc. Resolve the
   Stage 2 "showpage doesn't erase" deviation properly. [sonnet]
2. **PDF export** — replay executed page content into a PDF (either via
   a recording display list or by re-running per page). Design note
   first: display list vs. re-execution. [opus+review]
3. **Print-resolution rendering** — `--dpi` flag decoupling page points
   from device pixels (the CTM already supports it; it's mostly CLI and
   window scaling). [haiku]
4. **SVG export** — the path/paint pipeline maps almost 1:1; cheap win
   for sharing gallery pieces as vectors. [sonnet]

## Stage 10 — The LaserWriter experience (stretch/fun)

- **Spool mode**: watch a directory (or listen on a port) and render
  whatever lands there, page by page, like the printer in the corner of
  the lab. [sonnet]
- **Halftone screens**: classic `setscreen`-style dots for an authentic
  300dpi-printer look as an optional render mode. [sonnet]
- **Gallery II**: new pieces exploiting Stage 5/6 features (text art,
  clipped compositions, image-based collage). Any model with taste — the
  constraint-driven format in `gallery/README.md` is the brief. [any]

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
