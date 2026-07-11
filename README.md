# pscat

A PostScript interpreter, written in Rust, for watching PostScript programs
draw live — built for personal fun rather than as a Ghostscript
replacement, though it's meant to grow toward broad spec coverage over
time.

## Status

**Stage 1 (foundation) complete.** The language core exists but graphics
do not yet: a full PostScript tokenizer (numbers including radix form,
nested/escaped strings, hex strings, procedures), the object model, and an
interpreter with the three-stack execution model running the
stack-manipulation and arithmetic/math operators (`add` through `atan`,
`dup`/`roll`/`index`/marks, etc.), plus `=`, `==`, `stack`, `pstack`,
`print`, `quit`. Procedures scan and can be invoked, but `def` and control
flow are Stage 3. Errors use the standard PostScript error names and never
panic on program input.

See `INIT.md` for the roadmap, `ARCHITECTURE.md` for the design writeup,
and `NOTES.md` for per-stage summaries.

## Building & running

Requires a stable Rust toolchain.

```sh
cargo test              # unit + end-to-end interpreter tests
cargo run               # interactive REPL (prompt shows stack depth)
cargo run -- -e '3 4 add ='   # evaluate a snippet
cargo run -- file.ps    # run a program
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
