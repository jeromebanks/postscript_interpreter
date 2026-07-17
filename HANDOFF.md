# HANDOFF.md — state of the interpreter and how to continue

Written 2026-07-13 at the completion of Stages 6–7; last updated
2026-07-16 with Stage 8 complete. Written for whichever model
picks the project up next — read this after `CLAUDE.md` and before
touching code. `ROADMAP.md` has the task list with model routing;
`NOTES.md` has per-stage histories; this file is the *orientation*.

## Where things stand

**223 tests across 20 suites, clippy clean.** Stages 1–8 are done
(Stage 8's one open sliver: the `--interactive` windowed REPL,
design note in NOTES.md). Stage 8 delivered: **save/restore** as
object-granularity copy-on-write journaling (`VM.md` is the design
doc and gs-pin record — read it before touching any operator that
mutates array/dict contents; new mutators call
`Interp::journal_array`/`journal_dict` first, strings exempt by
spec); **name interning** (`src/name.rs`, fib 214→137ms);
**Indexed/Separation color spaces** (tint transforms run via the
generic `Frame::PostOp` continuation — copy that pattern for any
operator needing a procedure's result); **Level 2 odds and ends**
(`ops/level2.rs`); and **corpus round 2** (`tests/corpus.rs`): tiger,
golfer, escher, colorcir, doretree render block-identical to gs. The interpreter runs found PostScript
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
  (gs-style) instead of erroring; Symbol has no real face.
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
  doesn't erase; ints are i64.

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

1. **Stage 9 task 1, multi-page** ([sonnet]): `showpage` advances a
   page counter, window gains page navigation, `--png` writes
   out-001.png etc.; resolve the Stage 2 "showpage doesn't erase"
   deviation properly.
2. **Stage 9 task 3, `--dpi`** ([haiku]): decouple page points from
   device pixels — the CTM already supports it.
3. **Stage 9 task 4, SVG export** ([sonnet]): the path/paint pipeline
   maps nearly 1:1.
4. **Stage 9 task 2, PDF export** ([opus+review]): design note first
   (display list vs. re-execution).
5. Leftovers when they itch: `--interactive` (note in NOTES.md),
   remaining perf (frame loop's per-element RefCell borrow + clone,
   noted in benches/perf.rs), errordict handlers, DSC tolerance as
   the corpus grows.

## Gotchas for the next implementer

- `Object` has a custom `Drop` (iterative teardown) — you cannot move
  out of `obj.value`; match on `&obj.value` and clone the Rc.
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
