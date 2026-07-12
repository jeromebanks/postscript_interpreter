# AGENTS.md

Operating conventions for any AI coding agent working in this repo. Project
vision, priorities, and staged milestones live in `INIT.md` — read that
first. This file covers *how* to work, not *what* to build.

## Before you start

- Read `INIT.md` in full.
- If this is a fresh session and `INIT.md`'s Stage 1 hasn't been completed
  yet (check for an architecture writeup / commit history), start there.
- If Stage 1 is done, check for a stage-summary (in commit messages, a
  `NOTES.md`, or similar — use your judgment on where past summaries were
  left) before continuing, so you're not re-deciding settled questions.

## Toolchain & commands

- Language: Rust, stable toolchain unless a specific need arises to pin
  otherwise (explain why if you do).
- `cargo build` and `cargo test` should pass before considering any stage
  or sub-step complete.
- `cargo clippy` should be clean (or warnings explicitly justified in a
  comment) before considering a stage complete.
- `cargo fmt` before committing.

## Workflow

- Follow the staged deliverables in `INIT.md` in order, unless you have a
  good reason to resequence — if so, say so explicitly rather than
  silently reordering.
- Pause after Stage 1 for the architecture writeup described in `INIT.md`
  before starting Stage 2.
- At the end of each stage: summarize what was built, what tradeoffs were
  made, and what was explicitly deferred. Commit with a message that
  reflects the stage, not just the last edit made.
- Commit at reasonable checkpoints within a stage too — don't hold an
  entire stage's worth of work in one giant commit.
- Make judgment calls on genuine coin-flip decisions and note them; don't
  stall waiting for input on things you're equipped to decide. Do flag
  decisions that are hard to reverse later.

## Code quality bar

- No `unwrap()`/`expect()` on anything derived from program input (parsed
  PostScript source, user-supplied files). Reserve those for states that
  are genuinely impossible to reach, and prefer proper error types
  (`thiserror` or similar) otherwise.
- Comments explain *why*, not *what* — skip comments that just restate the
  code; do explain non-obvious design choices (e.g. why the dict stack is
  structured a particular way).
- Tests live alongside the code they test. Add integration or golden-image
  tests where `INIT.md`'s stages call for demonstrable rendering output.
- Keep `README.md` accurate to *current* state, not aspirational state.
  Update it as capabilities land, not just at the end.

## Where things live

- `ARCHITECTURE.md` — the design writeup (object model, execution model,
  rendering-crate leanings, deliberate deferrals). Read before changing
  anything structural.
- `NOTES.md` — per-stage summaries, newest first.
- `src/lexer.rs` — tokenizer; unit tests in-module.
- `src/object.rs` — `Object`/`Value`/`Dict`/`Num`.
- `src/interp.rs` — the execution machine (operand/dict/exec stacks),
  including the `begin_source`/`step_n` API front ends drive.
- `src/gfx.rs` — graphics state, paths, tiny-skia rasterization.
- `src/window.rs` — live winit window stepping the interpreter.
- `src/ops/` — operator implementations, grouped like the PLRM's operator
  summary (`stack.rs`, `arith.rs`, `graphics.rs`, `misc.rs`; new groups
  get new modules).
- `src/main.rs` — CLI: window / `--headless` / `--png` / `-e` eval / REPL
  modes, error reporting.
- `tests/eval.rs` — end-to-end tests (PostScript source in, operand-stack
  contents out, compared via `==`-style reprs).
- `tests/render.rs` — headless pixel tests (source in, canvas pixels out).
- `examples/*.ps` — sample programs used for manual testing (the Stage 2/3
  demo targets).
- `gallery/` — generative-art PostScript programs with rendered stills in
  `gallery/renders/` and a slideshow script (`show.sh`); see
  `gallery/README.md`. Art files stay within the interpreter's current
  operator set — they double as its most demanding integration tests.
