# NOTES.md — stage summaries

Newest first. Per `AGENTS.md`, each stage ends with a summary here: what
was built, tradeoffs made, what's explicitly deferred.

## Stage 14 — pscat for agents (2026-07-18)

Two integration surfaces, because agents differ. Docs-readers:
`.claude/skills/pscat/SKILL.md` (Claude Code loads it as a skill;
it's plain markdown for everyone else — Codex reaches it via the
AGENTS.md pointer). Tool-callers: `pscat-mcp`, an MCP stdio server
(`src/bin/pscat_mcp.rs`) with three tools — `render_postscript`
(PNG images inline / SVG text / PDF to out_path; partial renders
returned *with* the error text), `handwrite` (the handwrite.sh
options), `eval_postscript` (prints back, or error name + operand
post-mortem).

The one design decision worth recording: the server **shells out to
the pscat CLI** instead of linking the interpreter. PostScript
programs write to stdout (`=`, `print`, `pstack`) and stdout *is*
the JSON-RPC channel — in-process, one `=` would corrupt the
protocol. Subprocessing also keeps tools in lock-step with the CLI
and adds crash isolation. serde_json came in for the binary (first
new dependency since zune-jpeg); hand-rolled base64 (same as
svg.rs). CLI grew `pscat -` (program from stdin) with the arg parser
special-casing a bare `-`; `tests/cli.rs` pins the CLI contract,
`tests/mcp.rs` drives the real server binary over stdio.

## `--interactive` — the windowed REPL (2026-07-18)

Stage 8 task 6's deferred sliver, built exactly as its design note
planned: `pscat -i` (or `--interactive`) is the terminal REPL and
the live window at once — type PostScript at the prompt, watch it
draw. A stdin reader thread ships raw lines into the event loop
through `winit::EventLoopProxy` user events and never touches
interpreter state; the loop owns the interpreter (the
ARCHITECTURE.md threading rule) and runs each complete chunk through
the existing `begin_source`/`step_n` frame budget, so a pasted
fractal draws live at `--speed`, not in one frozen gulp.

Mechanics: line accumulation (complete chunk vs `...>` continuation)
moved from `main.rs` into `src/repl.rs` (`LineBuffer` +
`source_is_complete`), shared by both REPLs and unit-tested. Chunks
queue while one runs; errors print via `error_report` and the
session continues (REPL semantics — `step_n` already clears the
estack and keeps operand stack + canvas). A file argument runs first
as a prelude (load `lib/handscript.ps`, then explore by hand). EOF
sets a drain flag rather than exiting: everything queued still runs,
so `printf '...' | pscat -i` works end to end. `quit`, Ctrl-D, or
closing the window ends the session. All window modes now share one
`EventLoop<UserEvent>`; only interactive sends events.

Found immediately by using it: `rectfill` was still undefined — fixed
the same day, below.

## rectfill / rectstroke / rectclip (2026-07-18)

The Level 2 rectangle conveniences, prompted by `--interactive`'s
very first session. gs pins (all in `tests/stage5_gfx.rs`): rectfill
and rectstroke paint inside an implicit gsave — current path, point,
and graphics state survive; rectclip intersects the clip and leaves
the current path *empty*; negative width/height are corner-defined;
the flat-array form takes 4n numbers (empty = no-op for fill/stroke,
clips-everything-away for rectclip; other lengths typecheck); and
for rectstroke a 6-element array on top is *always* the matrix
operand, never a rect list — it concats after the path is built, so
it shapes the pen, not the rectangles. Not supported: the PLRM's
encoded-number-string form (a string typechecks, same as gs gives a
plain string). Known deviation, inherited not new: gs strokes a
`[3 0 0 1 0 0]`-matrix pen anisotropically; our √|det CTM| width
approximation draws it uniform (standing gap in ROADMAP.md).

## Stage 13 — handwrite: string → PNG (2026-07-17)

`./scripts/handwrite.sh "any text"` → a PNG of the text written in
/HandScript, word-wrapped like a person filling a page, page height
auto-sized to the text. Appearance options are CLI flags (size,
width/height, margin, leading, jitter, pen, ink RGB, plain/ruled
paper, seed, dpi, halftone, output path).

**Where the logic lives** — `lib/handscript.ps`, deliberately: the
font (the Stage 12 definition with the gallery's /W pen-width
adaptation) plus a dict-driven layout API in a `HSLayout` dict.
Public entries: `opts hs-write` (draws; caller shows the page) and
`opts hs-linecount` (measures, drawing nothing). One options dict,
schema documented in the file header, every key defaulted except
/Text. The library draws nothing on load and runs unchanged in gs
(pinned by test), so it can be embedded wholesale in other
applications; the bash script is only argument parsing, string
escaping (lowercase + `\()` escapes), and two pscat invocations.

**The wrap engine** measures with skeleton advances (CharDefs), not
rendered ink — breaks are exact, cheap, and jitter-independent, so
hs-linecount always agrees with hs-write (pinned by test: a page
rendered with and without a preceding count pass is byte-identical,
despite the count consuming rand for line-start wobble). Greedy
wrap on space/tab/CR; newline forces the break; blank lines are
kept; a word wider than the column overflows rather than splits;
unknown characters (capitals aside — the script lowercases) advance
invisibly as .notdef.

**Auto-height** is the reuse story in miniature: the script runs
`hs-linecount` headlessly, sizes the page in awk, then renders.
`--height` overrides it.

Deferred: capitals (font has none — script lowercases; a proper
upper-case skeleton set is Gallery II-adjacent work), hyphenating
column-overflowing words, right-margin raggedness control, and a
`--stdin` mode if piping ever wants it. Five tests in
`tests/handwrite.rs` (25th suite).

## Stage 10 — halftone screens; stage complete (2026-07-17)

`--halftone` (`src/halftone.rs`): the optional "authentic laser
printer" render mode. A render mode, not the `setscreen` operator —
the page rasterizes normally and the finished raster is screened on
the way out, the way a mono printer's RIP screens a contone page.
Applied at window blit time and to `--png`; `--svg`/`--pdf` keep
their contone art. Composes with everything, including `--spool`
(drop files, watch them print like it's 1985) and `--dpi`.

The screen: luminance (Rec. 709) → classic euclidean dot on a
45°-rotated lattice, cell fixed at 5.66 device px (53 lpi at 300
dpi, the LaserWriter's own numbers). Black dots grow from the cell
center sized so coverage equals darkness; past 50% the pattern
inverts to white holes shrinking at the cell corners. The first cut
used the naive `r²` threshold and the in-module coverage test
caught it overshooting: mid-gray printed at 75%, because a spot
function's raw value isn't area-uniform (a circle holds only π/4 of
its bounding square). Five in-module tests pin white-stays-white,
black-stays-solid, ~50% at mid-gray, coverage monotone in darkness,
and pure-B/W output.

Deferred: the real `setscreen`/`sethalftone` operators (per-color
screens, spot-function procedures) if a found file ever needs them;
`--halftone` for SVG/PDF is deliberately out (they're vector).

With this, spool mode, and Hundred Lines below, **Stage 10 is
complete** — Gallery II remains open-ended for more pieces.

## Stage 10 — spool mode (2026-07-17)

`pscat --spool DIR`: the live window becomes the printer in the
corner of the lab. It opens on a blank page, polls the directory
every 400ms while idle, and renders each new `.ps`/`.eps` that lands
there — page by page, at the usual `--speed`, with arrow-key page
browsing between jobs. Every job runs in a **fresh interpreter** at
the session's page size and dpi, so one job's redefinitions (or
crash) can't poison the next; job errors go to stderr and printing
continues, like a printer that just moves on to the next document.

The queue policy lives in `src/spool.rs` (`Watcher`), separated from
the window so it's testable headlessly (six in-module tests):
`.ps`/`.eps` only; files present at startup are *not* printed (the
printer coming online doesn't reprint the tray); a new file must
hold the same size+mtime across two consecutive polls before
queueing, so files still being copied in aren't printed
half-written; rewriting an already-printed file queues it again.
Same-poll arrivals print in name order.

Window integration rides the existing step-driven loop: idle control
flow becomes `WaitUntil(now + 400ms)` instead of `Wait` (re-armed
every pass — a stored past instant would spin), and `about_to_wait`
does the polling and job swap. End-to-end verified by scenario: a
pre-existing file is skipped, a dropped bad job reports its
undefined error to stderr, and two successive good jobs print in
order from separate interpreters.

Deferred: the "or listen on a port" variant (add when something
actually wants to netcat jobs in); a spool-specific test of the
window half (the watcher is tested, the window loop is exercised
manually — same standing as the rest of window.rs).

## Stage 10 — Gallery II opens: Hundred Lines (2026-07-17)

`gallery/hundred_lines.ps` — the first Gallery II piece, and the
showcase demo for the Stage 12 handwriting font: a chalkboard on
which the same sentence ("i will not cache my glyphs.") is written
nine times in /HandScript. Because every glyph is generated fresh by
BuildChar, no two lines — no two letters — render alike; the ninth
line is cut off mid-word by the bell, the chalk skidding away.

Two knobs the letter demo didn't pull, both exercised here: the
jitter amplitude `J` climbs line by line (12 → 40 — the writer gets
bored), and the pen width `W` (a small adaptation to the copied
font: `setlinewidth` reads `/W` instead of a literal) picks a
chalkier stroke. Both are overridden *from outside the font* by
writing into its scratch `work` dict, which stays writable after
definefont even in gs, where the font dict itself is locked — the
same scratch-dict trick Stage 12 pinned, used in the other
direction. Verified in both interpreters: gs runs the same file and
writes the board in its own hand (its `rand` stream differs, by
design). Still is 2× supersampled like the rest; piece is seeded
(`1985 srand`), so pscat's board is reproducible run to run.

Remaining Stage 10 items (spool mode, halftone screens) still open.

## Stage 12 — dynamic handwriting (2026-07-17)

`examples/handwriting.ps`: the /HandScript Type 3 font. Letterforms
are single-stroke polyline skeletons (lowercase, digits,
punctuation) on the 1000-unit grid; `drawstroke` jitters every point
through `rand` (±16 units) and smooths the run into quadratic-style
curves (interior points become control points, segments end at
midpoints — a pen slurring through corners). BuildChar adds
per-glyph slant with a rightward bias, baseline drift, and
pen-pressure width variation, stroking with round caps/joins.

The mechanics that make it work are all existing machinery: Type 3
glyphs are deliberately uncached (Stage 6 task 7), so every `show`
re-runs BuildChar and re-rolls the pen — repeated characters never
match; `rand` is a seeded LCG, so whole pages are reproducible run
to run (`tests/handwriting.rs` pins both properties). One
portability lesson pinned by running the file in gs: font dicts go
read-only at definefont there, so BuildChar's working defs live in
a scratch dict inside the font (invalidaccess otherwise; we don't
model access control, so our run never noticed).

## Stage 11 — performance parity investigation (2026-07-17)

**The harness** (`benches/vs_gs.rs`, `cargo bench --bench vs_gs`):
identical workloads through both binaries, wall time best-of-three
plus peak RSS via `/usr/bin/time -l`, startup row for netting out.

**Headline: pscat wins almost everywhere.** M-series numbers after
this stage's fix — pscat vs gs: startup 4ms/19ms, memory 3–5×
smaller on every workload (5.6–9.4MB vs 18–44MB), saveloop 3ms/23ms,
sierpinski 3ms/26ms, specimen 4ms/35ms, postcard 5ms/28ms, defloop
now *tied* 24ms/24ms. The two remaining gaps are fib 27 (124ms vs
54ms) and fern (247ms vs 149ms).

**What profiling found** (macOS `sample` on fib 30 / fern):
- A custom `Drop for Object` (the Stage-4 iterative-teardown guard)
  taxed ~18% of fib: every popped operand — integers included —
  paid a function call. **Fixed** by moving the impl to `PsArray`
  where it belongs: non-array values get the compiler's plain drop
  glue, arrays keep the last-handle iterative teardown. fib
  134→124ms, defloop 30→24ms (tie), fern 264→247ms. Bonus:
  `obj.value` is movable now — the old E0509 gotcha is gone.
- A **name-lookup cache was built, measured, and rejected** — twice
  (global write-generation, then per-name generations with dict-
  stack invalidation). fib gained only ~5% while def-heavy code
  (fern, defloop) paid more in bookkeeping than the two integer-hash
  probes it saved. The uncached walk is already near-optimal at a
  two-dict stack. Kept: nothing; the numbers said no.
- Both remaining gaps share one root: per-element machine-loop cost
  (step dispatch ~26%, dstack walk ~17%, per-element `PsArray::get`
  clone ~11%, drop glue ~9%). Fern is interpreter-bound, *not*
  raster-bound — the tiny-skia side is already fast, and fern's
  malloc traffic is its own `3 dict` per `sethsb` call.

**Deliberately not done** (the levers left, in order of expected
payoff, all representation changes): borrow-yield or bytecode-style
procedure bodies to kill the per-element clone; gs-style per-name
binding slots; NaN-boxed objects. For an interpreter whose real
workloads (rendering) already beat gs 5–7×, they're not worth the
architectural churn today.

## Stage 9 — output targets (2026-07-16)

**Multi-page** (`tests/pages.rs`): showpage snapshots the page, does
a full initgraphics (gs-pinned: CTM/color/width reset, font kept),
and erases *lazily* — the canvas keeps the finished page until the
next mark. That single idea resolves the Stage 2 "showpage doesn't
erase" deviation without defeating watching: single-page programs
keep their picture, multi-page programs erase exactly when page N+1
starts. All paint entry points funnel through `Gfx::prepare_paint`.
copypage/initgraphics ops added; `--png` numbers multi-page output;
the window browses pages with arrow keys.

**`--dpi`** — one scale factor in the base CTM
(`Interp::with_page_scaled`); verified pixel-identical to gs -r144.

**SVG export** (`--svg`, `src/svg.rs`): a recorder mirroring the
paint pipeline at six seams in Gfx — device-space paths serialize
1:1, glyphs arrive as outlines, clips became a *chain* (ClipState
now records path links, not just the newest) rendered as nested
clipPath groups, images embed as base64 PNG with the rasterizer's
exact transform. One document per page, lazy erase mirrored.
Verified by rasterizing postcard.ps's SVG in a browser.

**PDF export** (`--pdf`, `src/pdf.rs` — the design note lives in its
module doc): same recorder pattern, PDF syntax; the design decision
(mirror vs re-execution vs retained display list) is recorded there.
Content streams reuse our y-down device coordinates behind one flip
`cm`; each element carries its own clip chain in `q…Q`; images are
Flate-compressed RGB XObjects; imagemasks are 1-bit `/ImageMask`
stencils painted in the current color — PostScript's own semantics.
The strong test: **gs rasterizes our PDFs** and they block-match our
canvas (postcard, specimen).

If a third export target appears, promote the recorder seams into a
neutral display list and make SVG/PDF serializers of it.

## Stage 8 wrap — corpus round 2, Level 2 odds and ends (2026-07-16)

**Found-file corpus round 2** (`tests/corpus.rs`): the Ghostscript
classics — tiger, golfer, escher, colorcir, doretree render
block-identical to gs; snowflak and vasarely run clean but are
rand-driven art (implementation-specific sequences), asserted to run
rather than match. Files download on first run (gitignored;
AGPL-distributed, so fetched rather than vendored) and the test
skips offline. Two comparison lessons pinned: give gs
`-dGraphicsAlphaBits=4` or AA policy swamps the signal on edge-dense
art, and colors that "look wrong" at first glance are usually that
same AA thinning, not color math (sethsbcolor matched gs exactly).
What the corpus actually surfaced: `setflat`/`currentflat` (stored
hint; tiny-skia flattens itself) and `min`/`max`/`.min`/`.max` (gs
conveniences). waterfal.ps stays out — it reads gs's QUIET
command-line define.

**Task 5** (`ops/level2.rs`): packedarray (plain arrays; packing flag
tracked but inert), usertime/realtime, languagelevel = 2, and
resource basics — defineresource/findresource/resourcestatus/
undefineresource, Font category delegating to findfont/definefont,
other categories in a per-interp registry, writes journaled so
restore rolls them back. Status/size in resourcestatus are 0 0
fictions, like vmstatus's byte counts.

**Task 6, half of it**: `--pstack-on-error` prints the operand stack
(gs post-mortem style) after errors in both REPL and headless runs.
The `--interactive` flag (window + REPL together) remains; design
note: a stdin-reader thread ships raw lines through
`winit::EventLoopProxy::send_event` into the existing event loop,
which runs each line via `run_str` between frame budgets — the
interpreter stays single-threaded and owned by the loop; the reader
thread never touches it. Deferred until someone wants it enough to
test the windowed path properly.

## Stage 8 task 4 — Indexed and Separation color spaces (2026-07-16)

`ops/color.rs` + a real `ColorSpace` enum in the graphics state
(replacing the bare component count; part of the gsave snapshot, so
spaces roll back). `setcolor` dispatches on the current space.
Indexed: string lookup tables, index-0 initial color, out-of-range
indices *clamp* (gs renders where the PLRM says rangecheck — pinned);
indexed images map samples straight to palette entries with the
[0, 2^bits−1] default Decode. Separation: the tint transform is a
real PostScript procedure run through the machine via a new generic
continuation frame (`Frame::PostOp` — the reusable "operator needs a
proc's result" pattern the HANDOFF said to copy from ShowCtx; holds
no external state, so unwinding is free; `exit` can't cross it).

Gaps (documented in `ops/color.rs`): Indexed lookup procedures and
nested bases (typecheck); CIE spaces (undefined); Separation *images*
render as 1−tint gray, an approximation noted in `image.rs`.

## Stage 8 task 2 — name interning (2026-07-16)

`src/name.rs`: a `PsName` is an interned id plus the shared text
(`Deref<Target=str>` kept the churn tiny — 25 `Value::Name` sites,
most untouched). The interner is thread-local (the interpreter is
`!Send` by design; several Interps on one thread share ids
harmlessly; the table only grows, matching PS name semantics). Dicts
key name entries by `u32` through a one-multiply hasher; name
*execution* carries only the id (`load_id`), so the hot path does no
string hashing and no Rc traffic — `last_name` is now a bare id
resolved to text only at error time. `Dict::get(&str)` probes the
interner read-only so misses don't grow the table.

Measured (`cargo bench`, M-series): fib 27 214ms→137ms, defloop
33→25ms, fern 375→258ms. gs remains ~4.5× ahead on fib; profiling
says the rest is the frame loop (per-element RefCell borrow +
Object clone), noted in `benches/perf.rs` as the next lever and
deliberately left until something feels slow.

## Stage 8 tasks 1 + 3 — save/restore and the perf yardstick (2026-07-16)

**save/restore** (`VM.md` is the design doc — read it first; every
choice is pinned against gs there). Object-granularity copy-on-write
journaling: while a save is live, the first mutation of an array's or
dict's backing store at the current save level snapshots the whole
store into an undo journal; restore replays it backwards. Zero
overhead with no save live. The long-feared object-model reach was
one new `Value::Save(Rc<SaveHandle>)` variant (savetype) plus `Clone`
on `Dict`. Graphics rollback reuses the Type 3 glyph-snapshot
mechanism (boundary state lives in the save record, not on the gsave
stack). `vmstatus` (level real, byte counts fixed fictions) and
`grestoreall` came along.

Key facts pinned before design (they shaped it):
- **Strings are exempt from restore** (PLRM §3.7.3.2, confirmed in
  gs) — no string write path needs a journal barrier.
- restore = grestoreall to the save point (unbalanced gsaves inside
  the context are discarded).
- Write barriers audited into: put/putinterval/copy/astore (arrays),
  put/def/store/undef/copy (dicts), bind, the matrix-filling
  operators, definefont (FID + FontDirectory), findfont's
  substitution install, and the interpreter's own `$error` writes.

Deviations (documented in `VM.md` and tested as deviations):
- No invalidrestore scan of the stacks for post-save composites (gs
  errors; under `Rc` the objects stay valid — render, don't error).
- restore doesn't close files opened since the save.
- `grestore` can pop through a save boundary (balanced programs never
  notice); `grestoreall` respects it.

**Perf yardstick** (`benches/perf.rs`, `cargo bench`): fib 27 /
defloop / sierpinski / fern, best-of-three. Baseline: fib 214ms vs
gs ~30ms net of startup — the interning gap is ~7×, worse than the
~3× previously recorded; Stage 8 task 2 has measurable headroom.
save/restore added no regression (209ms after).

## Stage 7 truly complete — DCTDecode (2026-07-16)

**DCTDecode** (`Decoder::Dct`, zune-jpeg 0.5). The one decoder that
buffers: JPEG needs whole-stream context (Huffman tables, progressive
scans), so the first pull reads one complete image and decodes it all
at once. The buffering is *marker-aware* (`file.rs::buffer_jpeg`):
segment lengths are honored (an EXIF thumbnail's embedded EOI is
skipped, not mistaken for the end), entropy data is scanned respecting
FF00 stuffing and RST0–7, and reading stops exactly at the EOI — so
the shared-cursor exactness contract survives even though the decode
itself isn't streaming (verified by test: bytes after EOI stay
unread). Output matches PostScript sample expectations: grayscale
JPEGs yield one byte per sample, CMYK/YCCK four, everything else
(the usual YCbCr) RGB triples.

Tradeoffs / deviations:
- CCITTFaxDecode is now the only absent decode filter.
- CMYK JPEG output is zune's CMYK — Adobe's inverted-CMYK convention
  is untested (no real file in the corpus yet).
- Fixtures `tests/data/{gray8,red4}.jpg` were generated by gs
  (jpeggray/jpeg devices, APP segments stripped); decoded values
  pinned against gs rendering the same snippets (127-gray, 254-red).

## Stage 6 complete + Stage 7 (2026-07-13)

**Stage 6 task 6 — Type 1 fonts.** `src/type1.rs` is a hint-ignoring
charstring interpreter (numbers, moves/lines/curves, hsbw/sbw,
callsubr/return, div, seac with FreeType's sidebearing reading, flex
via OtherSubrs 0–2 as literal cubics, OtherSubr 3 threaded through
pop/callsubr). Glyph dispatch in `ShowCtx::step`: registry fid ≥ 0 →
bundled TTF; `BuildGlyph`/`BuildChar` in the dict → Type 3 frame;
`CharStrings` → Type 1 (decrypted per Private/lenIV with Subrs),
rendered synchronously through the same FontMatrix∘CTM transform as
every glyph source. `tests/type1.rs` *generates a complete canonical
PFA in memory* (eexec + charstring encryption, `N RD <binary>`
delivery, Private/CharStrings/definefont sequence, zeros,
cleartomark) — and Ghostscript accepts the identical bytes, agreeing
on metrics. Supporting work that made it possible:
- **File objects** (Stage 7 task 1, `src/file.rs` + `Value::File`):
  one shared read cursor between the scanner and data reads. The
  Lexer now pulls bytes through a `FileHandle`.
- **Filters** (task 2): files layered on files, decoding *on demand*
  so a filter consumes exactly the source bytes its consumer needed —
  the property `currentfile eexec` + `closefile` + cleartomark
  depends on. ASCIIHex/ASCII85/RunLength + the eexec cipher
  (hex/binary sniffed), later Flate (flate2, byte-fed) and LZW
  (hand-rolled, EarlyChange=1).
- **Scanner delimiter fix**: a token terminated by whitespace consumes
  that one character (CRLF as a unit), per PLRM and pinned against
  gs — without it `N RD <binary>` reads the wrong byte. `token`
  remainders changed accordingly (stage5 expectations updated to
  match gs).
- **`systemdict`/`userdict` by name** — simply missing until now;
  eexec pushes systemdict for its duration (fonts assume it).
- Access ops `executeonly readonly noaccess rcheck wcheck` as
  identities (every real font executes them).

**Stage 6 task 7 — glyph cache, measured.** Faces are now parsed once
per process (OnceLock over the `'static` bundled data). Measured
(release, M-series, 110k glyphs): show 0.35s→0.33s (rasterization
dominates), stringwidth 0.04s→0.02s. At ~330k painted glyphs/sec a
bitmap cache is not worth its complexity yet — same philosophy as
name interning: revisit when something feels slow. `setcachedevice`'s
bbox remains unused until then.

**Stage 7 tasks 1–3** shipped as described above (file objects,
filter framework, Flate/LZW). DCTDecode/CCITTFax deliberately absent
(`filter` reports them undefined).

**Stage 7 task 4 — images.** `src/image.rs` + `ops/image.rs`:
`image` (Level 1 five-operand and Level 2 dict forms), `imagemask`
(polarity pinned against gs: true ≡ Decode [1 0] ≡ paint the 1s),
`colorimage` (interleaved single source; 1/3/4 components).
`Frame::Image` accumulates data like the show frame: procedure
sources run as frames above and hand back strings (empty string =
EOD; premature EOD renders what arrived, missing samples read 0);
file/string sources drain synchronously. Rasterization inverse-maps
each device pixel through CTM⁻¹ then ImageMatrix (nearest neighbor),
blending through the clip mask. Minimal `setcolorspace`/
`currentcolorspace` for the three device spaces (the dict form reads
the current space's component count; color operators set it
implicitly per the PLRM).

**Tradeoffs / deviations (this whole block):**
- `file` op is read-only (`(r)`); write modes are invalidfileaccess.
- Filter `encode` variants don't exist; parameter dicts are accepted
  and ignored.
- `colorimage` with MultipleDataSources → limitcheck (documented gap).
- Images are nearest-neighbor, no interpolation (`/Interpolate`
  ignored); 12-bit samples unsupported (rangecheck).
- Type 1: hints ignored entirely; no `/Metrics` override; CID/CFF
  (Type 2 charstrings) out of scope.
- eexec's systemdict push is popped at source exhaustion but *not*
  restored on error unwinds (program is aborting anyway).

**Closing state:** `examples/postcard.ps` (inline hex/Flate images,
colorimage, imagemask sprites) joined the golden suite — the corpus
proof for the image pipeline. Bitmap fonts (Type 3 BuildChar using
imagemask) are tested working. **Everything else about continuing
this project — orientation, gotchas, and the recommended next-work
order (DCTDecode, found-file corpus round 2, then Stage 8's
save/restore) — lives in `HANDOFF.md`.**

## Stage 6, task 5 — Type 3 fonts and kshow (2026-07-13)

**Built** (the stage's [opus] exec-stack piece; design in `FONTS.md`
Decision 6, rewritten to match):
- **`Frame::Show`**: the entire show family (`show ashow widthshow
  awidthshow kshow stringwidth charpath`) now queues a frame and lets
  the machine process one glyph per step. Frame ordering is program
  ordering, so `stringwidth` pushing its result at frame-pop, nested
  shows inside BuildChar, and glyph-by-glyph live-window rendering all
  fall out for free. Outline glyphs still paint synchronously within
  their step.
- **Type 3 glyph contexts**: `BuildGlyph` (preferred, receives the
  encoded name) or `BuildChar` (receives the code) runs inside a
  sealed context — graphics-state snapshot + gsave watermark, CTM set
  to glyph space at the pen, fresh path — restored regardless of what
  the procedure does (unbalanced gsave included). `setcachedevice`,
  `setcachedevice2`, and `setcharwidth` record the width, which
  advances the pen through FontMatrix∘CTM. Unwinding (`stop`, uncaught
  errors, program abort) seals open contexts; `exit` across a show is
  `invalidexit`.
- **Metrics execute glyph procedures**: `stringwidth`/`charpath` on
  Type 3 run BuildChar with painting suppressed (a nestable counter on
  `Gfx`) — per the PLRM, that's where the width comes from.
- **`kshow`**: proc runs between character pairs with both codes
  pushed; the pen re-reads the current point after, so procs can kern
  or even swap fonts mid-string.
- **Demos**: `examples/type3_demo.ps` (deterministic, in the golden
  suite: bit-pattern CellFont, parametric GearFont, BuildGlyph
  SprigFont dingbats) and `examples/type3_ransom.ps` (dynamic
  generation: every glyph invented at show time — random face, size,
  tilt, paper scrap — via nested show inside BuildChar; deterministic
  per run, not gs-comparable since rand differs).
- **Tests**: 15 in `tests/type3.rs` — advances and kshow displacement
  pinned against Ghostscript, context sealing, catchable BuildChar
  errors, nesting, suppressed stringwidth, charpath-on-Type-3.

**Tradeoffs / deviations:** `charpath` on Type 3 advances without
capturing outlines; `setcachedevice`'s bbox is recorded nowhere (no
glyph cache yet — task 7); color restrictions on setcachedevice glyphs
are not enforced (lenient superset). `cshow` still absent (trivial
now; with Type 0 work if ever).

**Next:** Stage 6 task 6 (Type 1 parsing) and task 7 (glyph cache).

## Stage 6, tasks 1–4 — text and base fonts (2026-07-13)

**Built** (per `ROADMAP.md` Stage 6; tasks 5–7 remain open):
- **Architecture writeup** (`FONTS.md`): font dicts are ordinary Dicts;
  `FID` (an integer index — documented type deviation) is the seam to a
  Rust-side registry of 12 bundled Liberation faces (OFL, in `fonts/`,
  `include_bytes!`'d) covering the base-13 names minus Symbol.
  Process note: the task was tagged [opus+review]; the review pass was
  done in-session (this session ran on a stronger-than-opus model —
  no stronger reviewer available) and caught one real spec error
  (scalefont/makefont must *shallow*-copy the dict, sharing the
  Encoding array).
- **Plumbing**: `findfont` (lazy materialization into a real
  `FontDirectory`; unknown names substitute the default face,
  gs-style, instead of `invalidfont` — documented), `scalefont`/
  `makefont` (shallow copy + f64 matrix composition), `setfont`
  (validates, caches FontMatrix in the graphics state — gsave/grestore
  snapshot the font for free), `currentfont`, `definefont` (FID
  assignment; Type 3 dicts accepted but unrenderable until task 5),
  `selectfont`.
- **Show engine** (`src/font.rs`): byte → live Encoding lookup →
  glyph name → glyph id (font's `post` table, then Adobe-name→Unicode→
  `cmap` fallback) → outline through glyph→FontMatrix→CTM into a
  device-space path (quads elevated to cubics), filled through the
  normal paint+clip path without touching the current path; pen
  advances by the transformed advance vector. `show ashow widthshow
  awidthshow stringwidth charpath` all drive it; rotation/shear falls
  out of the matrix pipeline.
- **Encodings** (`src/encodings.rs`): Appendix E StandardEncoding and
  ISOLatin1Encoding, installed in systemdict and (fresh copy) in each
  built-in font dict; re-encoding idiom golden-tested.
- **Tests**: 21 in `tests/fonts.rs` (plumbing, metrics vs the 600/em
  Courier expectation, ink/clip/rotation pixels, live re-encoding);
  encoding-table unit tests; `examples/specimen.ps` in the golden
  suite (text compares with wider ink bands — Liberation vs URW
  shapes differ at glyph-edge granularity; metrics agree).

**Tradeoffs / deviations:** `FID` is integertype, not fonttype; Symbol
substitutes Helvetica; unknown `findfont` substitutes rather than
erroring; `charpath`'s bool operand is ignored (one outline form);
faces are re-parsed per show call (glyph-cache task will measure);
`show` is synchronous — Type 3/`kshow` need the ShowFrame exec-stack
design sketched in `FONTS.md` Decision 6.

The gallery gained its typography piece (`ring_of_type.ps` — the
stage-goal payoff: text on eleven shrinking rings around a charpath
ampersand).

**Next:** Stage 6 task 5 (Type 3 / BuildChar, the [opus] exec-stack
piece, plus `kshow`), task 6 (Type 1 parsing), task 7 (glyph cache).

## Stage 5 — Run found PostScript (2026-07-12)

**Built** (per `ROADMAP.md` Stage 5, all twelve items):
- **Views**: arrays and strings are now `PsArray`/`PsString` — shared
  backing plus offset/length — so `getinterval`/`cvs`/`search` results
  alias their source per the PLRM. The iterative-drop protection moved
  to the backing store.
- **General dict keys**: names/strings on the original fast path;
  integers (reals with integer value unify, per spec), booleans, marks,
  and composites-by-identity in a second map preserving original key
  objects for `forall`.
- **Data ops**: `length get put getinterval putinterval copy`
  (polymorphic incl. the stack form) `forall` (new frame type, all
  three container kinds) `array string aload astore`.
- **Dict ops**: `<< >> known where store undef currentdict
  countdictstack cleardictstack maxlength`.
- **Conversions**: `cvi cvr cvn cvs cvrs cvx cvlit type xcheck`
  (string→number uses the scanner's full syntax, radix included).
- **Error recovery**: `stop`/`stopped` as an exec-stack boundary frame;
  errors inside a stopped context record errorname/command into
  `$error` and resume with `true`; `handleerror` reports and resets.
  `exit` across a stopped boundary is `invalidexit` per the PLRM.
- **Strings as programs**: executable strings run as source
  (`(...) cvx exec`); `search anchorsearch token` (token reuses the
  real scanner, procedures included).
- **Graphics**: `clip eoclip initclip clippath pathbbox` (alpha-mask
  intersection; clip is part of the gsave snapshot and leaves the
  path), `setdash currentdash`, `sethsbcolor currenthsbcolor
  setcmykcolor currentrgbcolor currentgray currentlinewidth`.
- **Matrices**: full operand set (`matrix identmatrix defaultmatrix
  currentmatrix setmatrix initmatrix concat concatmatrix invertmatrix
  transform/itransform/dtransform/idtransform`) plus matrix-form
  dispatch for `translate/scale/rotate`.
- **Scanner**: `//immediate` names (resolved at scan time, value not
  executed), ASCII85 `<~...~>` literals.
- **Misc**: `rand srand rrand` (deterministic LCG), `bitshift`.
- **Corpus**: `examples/testcard.ps` — a found-style file (shortcut
  prolog, where-probe, forall charts, cvs geometry, eoclip ring, matrix
  bookkeeping) rendering identically in pscat and Ghostscript; in the
  golden suite. 90 tests total.

**Tradeoffs / deviations:** custom procedures installed in `errordict`
are not consulted (the `stopped`+`$error` path covers found-file usage);
the operand stack is *not* restored to pre-operator state when an error
is caught (PLRM handlers see pre-error operands — revisit with the
errordict work); `maxlength` reports current length (Level 2 dicts grow
anyway); `clippath` returns the most recent clip path rather than the
true intersection outline (the mask *is* the true intersection).

**Next:** Stage 6 — text and fonts, starting with the font architecture
writeup (`ROADMAP.md` Stage 6 item 1, [opus+review]).

## Stage 4 — Robustness and polish (2026-07-11)

**Audit result:** one real crash found and fixed. Dropping a deeply
nested array (10k levels of `[`) overflowed the Rust stack — `Rc` trees
drop recursively. Fixed with an iterative teardown in `Drop for Object`
(worklist reuses the array's own element vector; only last-owner array
drops pay anything). Also: `repr` is now depth-capped (deep nesting
prints `...` instead of recursing), `--page` is capped at 8000² so a typo
can't attempt a multi-GB allocation, and the REPL reads multi-line input
(brace/string-aware continuation prompt) so procedures can actually be
typed interactively.

**New tests** (61 total, 9 suites):
- `tests/robustness.rs` — deterministic fuzz (random bytes + random
  token soup over the whole operator set, step-budgeted so `loop` bombs
  terminate), pathological nesting, degenerate graphics (singular CTM,
  1e300 coordinates, negative radius), error-then-recover.
- `tests/golden.rs` — renders all four examples with pscat *and*
  Ghostscript, compares 10×10-block downsampled images: any block inked
  in one render and blank in the other fails (geometry error), mean
  block difference must stay < 6 (observed ≤ 2.1). Skips with a notice
  if gs isn't installed.

**Performance** (release, M-series): fib(27) ≈ 830k procedure calls in
~0.20s; 5M-iteration `for`+`add` in ~0.20s (~50M executed objects/sec);
depth-8 Sierpinski (6,561 fills) renders in 57ms. Ghostscript does
fib(27) ~3× faster — its names are interned, ours hash strings per
lookup. That's the known next lever, still not worth pulling until
something actually feels slow.

**Implemented today:** scanner (radix/reals/strings/hex/procs, bytes not
UTF-8), three-stack machine (steppable, tail-call-flat, depth-limited),
stack/arith/math ops, control flow (`if`/`ifelse`/`for`/`repeat`/`loop`/
`exit`/`exec`), `def`/`dict`/`begin`/`end`/`load`/`bind`, comparisons/
boolean/bitwise, paths (`moveto`…`arc`/`arcn`), painting (`fill`/
`eofill`/`stroke`), graphics state (`gsave`/`grestore`, gray/RGB color,
line attributes), transforms (`translate`/`rotate`/`scale`), live
window + headless PNG, PLRM error names throughout, LaserWriter-style
error reports.

**Not implemented (main gaps):** array/string element ops (`get`/`put`/
`getinterval`/`forall`/`array`/`string`/`aload`/`astore`), `cvi`/`cvr`/
`cvs`/`cvx`/`cvn`, `save`/`restore`, `stopped`/`stop`/errordict,
`clip`, `setdash`, matrix operands (`concat`/`setmatrix`/`transform`),
`sethsbcolor`/CMYK, text (`show`/fonts), images, `//name`, ASCII85,
`<<`/`>>` dict literals, multi-page semantics.

**Stage 5 recommendation — "run found PostScript":** the single most
valuable next chunk is data-structure completeness plus error recovery:
`get`/`put`/`forall`/`getinterval`/`array`/`string`/`aload`, the `cv*`
conversions, `<<`/`>>`, `where`/`known`, `stopped`/`stop`, `clip`, and
matrix operands. That's what stands between pscat and real-world `.ps`
files from the internet (they lean on arrays/strings/`forall`
constantly, and on `stopped` for prolog robustness). Fonts/`show` is the
bigger prize emotionally but is a stage of its own (font formats,
encoding vectors, glyph caching) and most found art still needs the data
ops first. Suggested order: Stage 5 = data + error recovery + clip;
Stage 6 = text with a bundled font (`show`, `stringwidth`, `charpath`);
Stage 7 = images and filters, which opens EPS previews and scanned art.

## Stage 3 — Control flow and procedures (2026-07-11)

**Built:** `def`, `dict`/`begin`/`end`/`load`, `if`/`ifelse`/`exec`,
`repeat`/`loop`/`for`/`exit`, comparisons (`eq`/`ne`/`lt`/`le`/`gt`/`ge`
with PLRM semantics: cross-type numeric equality, string content
comparison, name≡string, composite identity), boolean/bitwise
(`and`/`or`/`xor`/`not`), and `bind` (recursive, redefinition-proof —
tested). Loop operators are new **frame types on the execution stack**,
not host recursion, so they inherit stepping (live rendering), the depth
limit, and `exit` is a frame unwind that stops at source boundaries
(`invalidexit`). All three fractal examples now run and render. 15 new
control tests (recursion, 200k-deep tail recursion in constant space,
runaway recursion → `execstackoverflow` not a crash) + 3 fractal
integration tests with pixel assertions. 55 tests total.

**Found & fixed — example-file bugs, verified against Ghostscript:**
`koch_snowflake.ps` converted degrees to radians before calling
`sin`/`cos` (PostScript trig is degrees) which flattened the curve, and
its triangle headings/start point didn't close; `golden_spiral.ps` passed
a stray extra `4` in its recursive calls (shifting the whole operand
protocol) and its arc geometry didn't chain. In both cases **pscat and
Ghostscript rendered the broken files identically** (good news for
interpreter fidelity), and both render the fixed files identically —
a proper snowflake and a connected golden spiral.

**Tradeoffs:** dict `capacity` ignored (Level 2 dicts grow anyway); dict
keys still name-text only; `for` control values are f64 internally with
integer presentation when all operands were integers.

**Deferred:** `forall`, `put`/`get`/array ops, `where`/`known`/`store`,
`stopped`/`stop`, `cvx`/`cvi`/`cvr`/`cvs`, `//name` immediate names,
REPL multi-line procedure input.

**Next:** Stage 4 — robustness/error-handling audit, golden-image tests
against Ghostscript, performance pass, and the what's-next writeup.

## Stage 2 — Graphics core, live window (2026-07-11)

**Built:** `src/gfx.rs` (graphics state, device-space paths, arc→Bézier
flattening, tiny-skia fill/stroke, gsave stack); the full Stage 2 operator
set (`moveto`/`lineto`/`curveto`/`arc`/`arcn` + relatives, `fill`/
`eofill`/`stroke`, `gsave`/`grestore`, colors, line attributes,
`translate`/`rotate`/`scale`, `currentpoint`, `showpage`, `erasepage`);
a public `begin_source`/`step_n` stepping API on the interpreter; a live
winit+softbuffer window that steps the machine each frame (`--speed`
knob); headless `--png` output; `--page WxH`. 13 pixel-level render tests
plus the Stage 1 suite (40 total); demo at `examples/stage2_demo.ps`,
verified by rendering to PNG and inspecting.

**Tradeoffs:** stroke width scaled by √|det CTM| instead of a true
user-space pen (exact under uniform scale/rotation, wrong for
anisotropic `scale`); `showpage` leaves the image up instead of erasing;
window blit is nearest-neighbor on HiDPI. All noted in `ARCHITECTURE.md`.

**Deferred:** `clip`/`clippath`, `setdash`, `sethsbcolor`,
matrix-operand forms (`concat`, `setmatrix`, `transform`), multi-page
semantics, REPL-attached window.

**Next:** Stage 3 — `def`, `if`/`ifelse`, `for`/`repeat`/`loop`,
`bind`, dict operators (`begin`/`end`, `dict`), comparisons — the set
the three fractal examples actually need.

## Stage 1 — Foundation (2026-07-11)

**Built:** Cargo project (`pscat`, lib + bin); byte-oriented lexer with
full PostScript token syntax (radix numbers, nested/escaped strings, hex
strings, the "failed numbers are names" rule) and in-module unit tests;
object model (`Object` = `Value` + executable flag, `Rc<RefCell>`
composites); an exec-stack machine with operand/dict/exec stacks, tail-call
frame popping, and depth limits; stack-manipulation and arithmetic/math
operators per the PLRM (promotion on overflow, degrees for trig,
ties-round-to-greater, `mod` sign rules); `=`/`==`/`stack`/`pstack`/
`print`/`quit`; a CLI with file, `-e/--eval`, and REPL modes;
LaserWriter-style error reports. 27 tests; clippy clean.

**Tradeoffs:** `i64` integers instead of the PLRM's 32-bit (documented in
`ARCHITECTURE.md`); names as `Rc<str>` with interning deferred until a
benchmark justifies it; dict keys are name-text only until arbitrary-key
dict operators exist.

**Deferred:** `def` and all control flow (Stage 3 — though the dict stack
and procedure-call machinery already work, tested via the embedding API);
`//name`, ASCII85, `<<>>` construction; `save`/`restore` (flagged as the
one feature that could reach back into the object model); REPL multi-line
input.

**Next:** paused for the architecture-writeup checkpoint per `INIT.md`.
Stage 2 (graphics + live window) starts on approval; crate leanings are in
`ARCHITECTURE.md`.
