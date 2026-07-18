---
name: pscat
description: Render, preview, and debug PostScript with the pscat interpreter — PNG/SVG/PDF out, handwritten-note generation, watch-it-draw window, printer-style spool and halftone modes. Use for any .ps/.eps file work, generating diagrams or art in PostScript, or producing human-looking handwritten notes.
---

# pscat — PostScript as a tool

`pscat` is a PostScript interpreter in this repo. Build once with
`cargo build --release`; the binary is `target/release/pscat`.
Everything below also works with `cargo run --release --` in place of
the binary.

## Render a file (the common case)

```sh
pscat --png out.png file.ps            # headless render to PNG
pscat --svg out.svg file.ps            # vector SVG (one file per page)
pscat --pdf out.pdf file.ps            # multi-page PDF
pscat --page 500x500 --png out.png f.ps  # canvas in points (default 612x792)
pscat --dpi 300 --png out.png f.ps     # print resolution (default 72)
pscat --halftone --png out.png f.ps    # screen like a mono laser printer
generate | pscat --png out.png -       # `-` reads the program from stdin
```

Multi-page documents number their pages: `out.png` → `out-001.png`,
`out-002.png`, … SVG does the same; PDF is naturally multi-page.

## Debug a program

```sh
pscat --headless file.ps               # run without rendering; errors → stderr
pscat --headless --pstack-on-error f.ps  # + gs-style operand stack post-mortem
pscat -e '3 4 add ='                   # evaluate a snippet
```

Exit code 0 = clean run; nonzero = a PostScript error, named on
stderr with the standard error names (`undefined`, `typecheck`, …).
`--png` still writes the partial canvas after an error — useful to
see how far a program got.

## Handwritten notes (string → PNG)

```sh
./scripts/handwrite.sh "meet me at the bandshell at nine" -o note.png
./scripts/handwrite.sh --paper ruled --size 30 --seed 7 "text"
./scripts/handwrite.sh --ink 0.4,0.1,0.1 --jitter 24 "shakier, redder"
```

Renders text in the /HandScript dynamic font: rand-jittered strokes,
no two letters alike, reproducible per `--seed`. Word-wraps to the
page and auto-sizes the height. **Lowercase only** — input is
lowercased, and characters outside a–z 0–9 `.,-'!?` advance
invisibly. The underlying library `lib/handscript.ps` is embeddable
(one options dict; see its header for the schema).

## Watch it draw (needs a display)

```sh
pscat file.ps                          # live window; --speed N paces it
pscat -i                               # REPL + window: type, watch it draw
pscat -i lib/handscript.ps             # ...with a library preloaded
pscat --spool DIR                      # render every .ps that lands in DIR
```

Arrow keys browse completed pages in any window mode.

## Pitfalls

- **No `showpage`, no page**: Ghostscript emits nothing for a
  program that never calls `showpage`. pscat's `--png` is forgiving
  (it writes trailing art), but add `showpage` to any file that
  should also work in gs.
- `--page WxH` is in points and sets the canvas; `--dpi` scales
  device resolution without changing coordinates. For a 2× PNG of a
  612x792 page use `--dpi 144`, not `--page 1224x1584`.
- The interpreter targets Level 2 PostScript. If a construct
  misbehaves, compare against `gs` — the repo treats gs as the
  behavioral oracle (see AGENTS.md).
- MCP alternative: `pscat-mcp` (same target dir) exposes
  `render_postscript`, `handwrite`, and `eval_postscript` as MCP
  tools over stdio, for agents wired that way. See README "For
  agents".
