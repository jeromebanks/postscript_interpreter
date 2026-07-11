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

(To be filled in / updated by whoever sets up the initial project
structure — module layout, where tests live, where sample `.ps` programs
used for manual testing are kept, etc. Keep this section current as the
structure solidifies so future sessions can orient quickly.)
