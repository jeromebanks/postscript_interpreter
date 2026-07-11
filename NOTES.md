# NOTES.md — stage summaries

Newest first. Per `AGENTS.md`, each stage ends with a summary here: what
was built, tradeoffs made, what's explicitly deferred.

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
