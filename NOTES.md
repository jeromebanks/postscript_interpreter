# NOTES.md — stage summaries

Newest first. Per `AGENTS.md`, each stage ends with a summary here: what
was built, tradeoffs made, what's explicitly deferred.

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
