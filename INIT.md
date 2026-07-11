# INIT.md — Project: A PostScript interpreter in Rust

This is the project spec and vision. Operational conventions (build/test
commands, commit hygiene, workflow) live in `AGENTS.md`. If you're an agent
picking this up fresh, read this file in full before writing any code, then
check `AGENTS.md` before starting Stage 1.

## Motivation

I used to write raw PostScript by hand in college and send it straight to a
LaserWriter. I want to relive that — but I also want a real, ambitious,
long-lived project: a PostScript interpreter in Rust that starts small and
grows toward genuinely broad spec coverage over time. Think of this less as
"a script that renders fractals" and more as "the beginning of a serious
implementation of a serious language," built by someone (you) who cares
about doing it well.

I am deliberately not going to prescribe architecture. Rust is the only
hard technology constraint. You have full discretion over object model,
rendering backend, windowing, threading model, crate choices, module
structure — all of it. I'd rather you make those calls well than have me
make them badly. Where there's a real tradeoff (e.g. threaded interpreter
vs. step-based execution for live rendering), use your judgment and tell me
what you chose and why, rather than asking me to pick.

## What I care about, in rough priority order

1. **Correctness on the PostScript language core** — the stack-based
   execution model (operand stack, dictionary stack, execution stack),
   proper operator semantics, faithful behavior on real PostScript programs.
   This is the part of the language that's genuinely elegant and worth
   getting right, not just approximating.
2. **Live rendering** — I want to watch programs draw, not just get a final
   image. A recursive fractal building up path by path, visibly, is the
   whole point of doing this instead of just using Ghostscript. How you
   achieve "live" (threaded interpreter + shared framebuffer, step-based
   interpreter driven by a render loop, something else entirely) is your
   call.
3. **Performance** — this should be fast. Rust is the whole pitch here
   versus reaching for something higher-level; don't waste that with
   needless allocation, cloning, or naive algorithms where better ones are
   easy.
4. **Robustness** — real PostScript programs (including ones I hand-write
   badly) should produce sensible errors, not panics. No `unwrap()` on
   anything derived from program input.
5. **Code quality** — this is a project I want to read, learn from, and
   keep extending for a long time. Idiomatic Rust, sensible module
   boundaries, tests at the levels that matter (tokenizer, interpreter
   semantics, maybe golden-image tests for rendering), and enough comments
   to explain *why* on anything non-obvious — not comments that just
   restate the code.

## Long-term ambition (not v1 — context for the direction to grow in)

Eventually: full Level 2 operator coverage, Type 1/Type 3 font rendering
and `show`/text layout, the standard filters (LZW, ASCII85, DCT, RunLength),
more color spaces (CMYK, Indexed, Separation), PDF output, and enough
robustness to throw real-world found PostScript/EPS files at it — the kind
of scope that would make this a legitimate alternative to reaching for
Ghostscript for personal use. I'm not expecting this in the first pass.
I'm telling you so early decisions (object model, module boundaries) can
leave room to grow rather than requiring a rewrite later.

## Staged deliverables

Each stage should be independently demonstrable — I want to see/run
something real at the end of each one, not just a status report. Treat
these as milestones, not a rigid spec; if you find a better sequencing or
want to split/merge stages, go ahead and say so.

**Stage 1 — Foundation**
Project scaffolding, your chosen object model for PostScript values, a
tokenizer/lexer with tests, and a minimal interpreter loop that can execute
programs using only the stack-manipulation and arithmetic operators (no
graphics yet). Demonstrable via a test suite plus maybe a tiny REPL or
`--eval` mode. This is where I'd like a short writeup of the architecture
you chose and why, before graphics enter the picture.

**Stage 2 — Graphics core, rendering to a live window**
Path construction and painting operators (`moveto`, `lineto`, `curveto`,
`arc`, `fill`, `stroke`, `closepath`), graphics state (`gsave`/`grestore`,
color, line width, CTM/`translate`/`rotate`/`scale`), and a live-updating
window showing the canvas as the interpreter runs. Demonstrable by feeding
it a hand-written recursive PostScript program (I'll have some fractal/
geometric art ready) and watching it draw.

**Stage 3 — Control flow and procedures**
`if`/`ifelse`, `for`/`repeat`/`loop`, user-defined procedures via `def`,
recursion, local dictionaries. This is what turns "draws shapes" into
"executes real PostScript art programs" — Sierpinski triangles, Koch
curves, anything recursive. Demonstrable the same way as Stage 2, but with
programs that actually exercise recursion and loops.

**Stage 4 — Robustness and polish pass**
Error handling audit (no panics on malformed input), a real test suite
(unit tests plus some form of rendering/golden-image tests if you think
that's worthwhile), performance pass if anything's obviously wasteful, and
a written note on what's implemented vs. not, plus your recommendation for
what Stage 5 should be, given everything you've learned building this.

Beyond Stage 4, I'd like your recommendations rather than a plan I hand
you — you'll know by then what the natural next chunk of scope is (fonts?
filters? PDF export? something I haven't thought of?).

## Working style

- After Stage 1, pause and show me the architecture writeup before
  continuing — I want a checkpoint before graphics complexity gets layered
  on top.
- At the end of each subsequent stage, a brief summary of what was built,
  what tradeoffs you made, and what's explicitly deferred is more useful to
  me than a wall of code with no narration.
- If you hit a design decision that's genuinely a coin flip with no clear
  right answer, make the call and tell me — don't stall waiting for my
  input on things you're equipped to decide.
- If you hit something that seems like it should be a quick fix but turns
  out to reveal a deeper architectural issue, flag it rather than papering
  over it — this project is supposed to last, so I'd rather know.

## First task

Propose your architecture for Stage 1 (object model, rendering/windowing
crates you're leaning toward for Stage 2 even though you won't use them
yet, overall module structure) in a short writeup, then build it.
