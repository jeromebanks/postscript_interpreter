# pscat

A PostScript interpreter, written in Rust, for watching PostScript programs
draw live — built for personal fun rather than as a Ghostscript
replacement, though it's meant to grow toward broad spec coverage over
time.

## Status

**Stage 2 (graphics core + live window) complete.** On top of the Stage 1
language core (tokenizer, object model, three-stack interpreter, stack and
arithmetic operators), the graphics engine now works: path construction
(`moveto`/`lineto`/`curveto`/`arc`/`arcn` and relatives, `closepath`,
`currentpoint`), painting (`fill`, `eofill`, `stroke`, `erasepage`,
`showpage`), graphics state (`gsave`/`grestore`, `setgray`/`setrgbcolor`,
line width/cap/join/miter), and coordinate transforms
(`translate`/`rotate`/`scale`). Programs run in a **live window** so you
can watch them draw, or headlessly to a PNG. `def` and control flow are
Stage 3, so recursive programs don't run yet — `examples/stage2_demo.ps`
shows what straight-line PostScript can do today. Errors use the standard
PostScript error names and never panic on program input.

See `INIT.md` for the roadmap, `ARCHITECTURE.md` for the design writeup,
and `NOTES.md` for per-stage summaries.

## Building & running

Requires a stable Rust toolchain.

```sh
cargo run -- examples/stage2_demo.ps      # watch it draw, live
cargo run -- --speed 10 file.ps           # slower (steps per frame, default 100)
cargo run -- --png out.png file.ps        # headless render to PNG
cargo run -- --page 500x500 file.ps       # canvas size (default 612x792)
cargo run                                 # interactive REPL
cargo run -- -e '3 4 add ='               # evaluate a snippet
cargo test                                # language + pixel-level render tests
```

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
