# pscat

A PostScript interpreter, written in Rust, for watching PostScript programs
draw live — built for personal fun rather than as a Ghostscript
replacement, though it's meant to grow toward broad spec coverage over
time.

## Status

**Stage 4 (robustness & polish) complete.** Real recursive PostScript
art programs run and draw live: the language core (tokenizer,
object model, three-stack interpreter, stack/arithmetic operators, `def`,
dictionaries, `if`/`ifelse`, `for`/`repeat`/`loop`/`exit`, comparisons,
`bind`), the graphics engine (paths, `arc`/`arcn`, `fill`/`eofill`/
`stroke`, `gsave`/`grestore`, colors and line attributes,
`translate`/`rotate`/`scale`), and a **live window** so you can watch
programs draw — or headless PNG rendering, which is cross-checked against
Ghostscript on the example fractals. Errors use the standard PostScript
error names and never panic on program input (fuzz-tested); runaway
recursion is a catchable `execstackoverflow`, and tail-recursive
programs run in constant execution-stack space. The test suite includes
golden-image comparisons against Ghostscript. The REPL accepts
multi-line procedure definitions. See `NOTES.md` for the full
implemented-vs-not inventory and the Stage 5 recommendation.

Try `cargo run -- --page 500x500 examples/sierpinski.ps` and watch the
triangles appear.

See `INIT.md` for the roadmap, `ARCHITECTURE.md` for the design writeup,
and `NOTES.md` for per-stage summaries.

## Building & running

Requires a stable Rust toolchain.

```sh
cargo run -- --page 500x500 examples/sierpinski.ps      # watch it draw, live
cargo run -- --speed 10 file.ps           # slower (steps per frame, default 100)
cargo run -- --png out.png file.ps        # headless render to PNG
cargo run -- --page 500x500 file.ps       # canvas size (default 612x792)
cargo run                                 # interactive REPL
cargo run -- -e '3 4 add ='               # evaluate a snippet
cargo test                                # language + pixel-level render tests
```

The `examples/` directory has three recursive fractals (Sierpinski
triangle, Koch snowflake, golden spiral) plus a straight-line demo; all
render identically in pscat and Ghostscript.

A REPL taste:

```
PS> 40 2 add =
42
PS> mark 1 2 3
PS<4> pstack
3
2
1
-mark-
```

Sample programs for the upcoming graphics stages live in `examples/`.

## Why

I used to write raw PostScript by hand in college and send it straight to
a LaserWriter. This is a from-scratch Rust interpreter built to relive
that — watching a hand-written recursive PostScript program draw itself,
live, without depending on a decades-old C codebase to do it.
