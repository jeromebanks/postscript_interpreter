# Architecture

The Stage 1 design writeup, per `INIT.md`. This documents the choices that
are expensive to reverse; things that are cheap to change later are noted
as deliberate deferrals rather than decisions.

## Object model (`src/object.rs`)

A PostScript value is an `Object`: a `Value` enum plus an `executable`
flag.

```
Object { value: Value, executable: bool }
Value  = Integer(i64) | Real(f64) | Boolean | Mark | Null
       | Name(Rc<str>) | String(Rc<RefCell<Vec<u8>>>)
       | Array(Rc<RefCell<Vec<Object>>>) | Dict(Rc<RefCell<Dict>>)
       | Operator(fn)
```

**Why a flag rather than doubled-up variants** (name vs. executable name,
array vs. procedure): in PostScript the literal/executable attribute is
orthogonal to type — `cvx`/`cvl` can flip it on *any* object, including
integers. One representation per type, with the attribute alongside, is
both smaller and truer to the language.

**Why `Rc<RefCell<..>>` for composites**: PostScript composites have
reference semantics — `dup` on an array yields a second handle to the same
storage, and mutation through one handle is visible through the other.
`Rc<RefCell>` gives exactly that. `Rc`, not `Arc`, because the interpreter
core is single-threaded by design (see "Live rendering plan" below); if
that ever changes, the type aliases are the choke point.

**Integers are `i64`**, a documented deviation from the PLRM's 32-bit
integers. Arithmetic that overflows still promotes to a real, so spec
*behavior* is preserved with a higher threshold. Nothing hand-written will
notice; a conformance-test suite someday might, and can be revisited then.

**Strings are `Vec<u8>`**, not Rust `String` — PostScript is a byte
language, and real-world files aren't reliably UTF-8. The lexer operates
on bytes throughout for the same reason.

**Names are `Rc<str>`** and dictionaries are `HashMap<Rc<str>, Object>`.
Interning names to integer symbols (making dict lookup a small-int hash or
array index) is the single most obvious performance lever left unpulled;
it's deferred until there's a benchmark showing lookup on the hot path,
which won't be before Stage 3's procedure-heavy programs exist. The PLRM
also allows non-name dict keys; the key type will need generalizing when
`put`/`get` on arbitrary keys land (noted in the code).

## Execution model (`src/interp.rs`)

Three stacks, per the language spec: operand stack (`Vec<Object>`),
dictionary stack (`Vec<Rc<RefCell<Dict>>>`, systemdict then userdict), and
an **explicit execution stack** of frames:

```
Frame = Scanner(Lexer)                  — tokens read incrementally from source
      | Proc { body, pc }               — a procedure being executed element-wise
```

The core loop pulls one object at a time from the top frame and executes
it. This "machine, not recursive evaluator" shape is the load-bearing
Stage 1 decision, made for three reasons:

1. **Live rendering (Stage 2)** needs the interpreter to be pausable
   mid-program. A step-able machine can be driven "run N steps, present a
   frame" from a render loop without threads or locks.
2. **Deep PostScript recursion must not consume Rust call stack.** Depth
   is bounded by an interpreter-owned limit, and exceeding it is a
   catchable `execstackoverflow`, not a process abort.
3. **Tail calls are free**: a procedure frame is popped the moment it
   yields its last element, so tail-recursive PostScript runs in constant
   execution-stack space.

The scanner is itself a frame, so tokenization is incremental (matching
real PostScript's file-execution semantics) rather than a parse-then-run
phase. `{...}` is collected at scan time into a procedure object and
*pushed*; executable arrays only run when a name resolves to one — the
distinction between "encountered" and "invoked" that makes deferred
execution work.

Semantics rule of thumb throughout: when the PLRM specifies behavior
(promotion rules, `roll` direction, `mod` sign, ties-round-to-greater,
degrees for trig), implement that, not the convenient approximation. The
tests cite the PLRM's own examples where they exist.

## Errors (`src/error.rs`)

`PsError` variants are named for the PLRM's standard errors
(`stackunderflow`, `typecheck`, `undefined`, ...) so behavior can track
the spec, and so the future `errordict`/`stop` machinery has the right
vocabulary from day one. Every operator returns `Result`; nothing derived
from program input panics. Malformed-input recursion (e.g. 2000 nested
`{`) is depth-capped into `limitcheck`. The CLI reports errors in
LaserWriter style: `%%[ Error: undefined; OffendingCommand: frobnicate ]%%`.

## Module map

```
src/
  lib.rs        crate root, re-exports
  main.rs       CLI: file / -e eval / REPL modes, error reporting
  error.rs      PsError (PLRM error names)
  lexer.rs      byte-oriented tokenizer; unit tests in-module
  object.rs     Object / Value / Dict / Num
  interp.rs     the machine: three stacks, frames, name resolution
  gfx.rs        graphics state, device-space paths, tiny-skia painting
  window.rs     winit event loop stepping the interpreter live
  ops/          operators, grouped like the PLRM operator summary
    stack.rs    pop exch dup copy index roll clear count marks ]
    arith.rs    add sub mul div idiv mod neg abs rounding sqrt trig exp ln log
    graphics.rs paths, painting, gsave/grestore, colors, translate/scale/rotate
    misc.rs     = == stack pstack print quit
tests/eval.rs   end-to-end: source in, operand-stack contents out
tests/render.rs headless rendering: source in, canvas pixels out
examples/*.ps   sample programs (stage2_demo.ps runs today; fractals need Stage 3)
```

## Live rendering (Stage 2 — implemented)

**Single-threaded, step-driven**, as planned. The winit event loop owns
the interpreter (`src/window.rs`); each frame runs a budget of interpreter
steps (`--speed`, default 100/frame ≈ 6000 objects/sec) and blits the
canvas via softbuffer. No locks, no channels, deterministic; pause and
slow-motion are just budget changes. The interpreter's `begin_source` +
`step_n` API is the seam between the machine and any front end — the
headless `--png` mode drives the identical machine to completion.

Crates: **winit** (windowing — the standard choice), **softbuffer** (CPU
pixel presentation), **tiny-skia** (rasterization — pure Rust and its
model matches PostScript's needs almost exactly: Bézier paths,
nonzero/even-odd fills, stroking with joins/caps/miter limits).
Considered and passed on: `minifb` (less maintained, less control),
`pixels`/wgpu (GPU pipeline is overkill for CPU 2D), `skia-safe` (huge
C++ dependency — against the spirit of the project).

Graphics semantics worth knowing (`src/gfx.rs`):

- **Paths live in device space** — points go through the CTM at
  construction time, per the PLRM, which is what lets programs transform
  the coordinate system mid-path. `currentpoint` and the relative
  operators map back through the inverse CTM.
- **Arcs** are flattened to ≤90° cubic Béziers in *user* space and the
  control points transformed, so arcs under rotation/scaling become the
  correct ellipses.
- **Stroke width approximation**: PostScript strokes with a user-space
  pen; we stroke in device space with width scaled by √|det CTM|. Exact
  for uniform scale/rotation; anisotropic `scale` won't produce
  elliptical pens. Deliberate, documented, revisit on demand.
- **`showpage` deviation**: marks the page complete and leaves the image
  visible instead of erasing for the next page — erasing would defeat
  watching. Multi-page documents are a future problem.
- The current path is part of the `gsave`/`grestore` snapshot (that's
  what makes `gsave fill grestore stroke` work), per the PLRM.

## Known-hard things being deliberately deferred

- **`save`/`restore`** — full VM snapshotting touches the object model
  (copy-on-write or generation-stamping composites). `gsave`/`grestore`
  (graphics state only) covers most real programs and comes first, in
  Stage 2. Flagged now because it's the one future feature that could
  reach back into the object model's design.
- **Name interning** — see above; do it when a benchmark exists.
- **`//name` immediate evaluation, ASCII85 strings, `<<`/`>>` dict
  construction** — the lexer recognizes and cleanly rejects the first
  two, tokenizes the third (it's just names); all await their stages.
- **REPL line-continuation** for multi-line procedure definitions —
  single-line REPL semantics until the REPL matters more.
