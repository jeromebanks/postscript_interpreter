# Getting started

pscat is a PostScript interpreter that draws as it runs. This guide covers
building it, installing a ready-made bundle, and the command-line flags you
will use most. If PostScript itself is new to you, skim the
[site](https://jeromebanks.github.io/postscript_interpreter/) first; every
example below is a plain `.ps` text file.

## Build and run

You need a stable Rust toolchain. From a checkout of the repo:

```sh
cargo run -- --page 500x500 examples/sierpinski.ps      # watch it draw, live
cargo run -- --png out.png examples/sierpinski.ps       # or render it headlessly to a PNG
```

The `examples/` directory has three recursive fractals (Sierpinski triangle,
Koch snowflake, golden spiral), a test card written like a found file, and a
type specimen (`specimen.ps`). They render identically in pscat and
Ghostscript.

## Command-line flags

Every flag, with the most useful ones first:

```sh
cargo run -- --page 500x500 file.ps       # canvas size (default 612x792)
cargo run -- --speed 10 file.ps           # slower (steps per frame, default 100)
cargo run -- --png out.png file.ps        # headless render to PNG
cargo run -- --svg out.svg file.ps        # export SVG (also --pdf out.pdf)
cargo run -- --dpi 300 --png out.png file.ps   # print-resolution rendering
cargo run -- --spool jobs/                # act like a lab printer: render
                                          # each .ps/.eps dropped into jobs/
cargo run -- --halftone file.ps           # classic 45° halftone dots, like a
                                          # mono laser printer (window/PNG)
cargo run -- --lint --png out.png file.ps # self-check: blank page? unbalanced
                                          # gsave? stuff left on the stack?
cargo run -- --lint-strict file.ps        # ...and exit non-zero on any finding
                                          # (a run that errored is never "clean":
                                          # lint prints [run-error] and exits 1)
cargo run -- --selftest lib/paintkit.ps   # run a library's %%SelfTest blocks
./scripts/selftest.sh                     # every library's blocks, plus strict
                                          # lint over every rendering driver
cargo run -- file.ps --sweep-seed 1:12 \  # render 12 seeds, one grid PNG
    --contact-sheet grid.png
cargo run                                 # terminal REPL (headless canvas)
cargo run -- -i                           # REPL + live window: watch what you type draw
cargo run -- -i lib/handscript.ps         # ...with a library preloaded
cargo run -- -e '3 4 add ='               # evaluate a snippet
cargo test                                # language + pixel-level render tests
```

More on the less obvious ones:

- **Multi-page output.** `showpage` ends a page. The window keeps showing the
  finished page, and `--png` numbers the pages (`out-001.png`, ...).
  `--svg` and `--pdf` export the same paint pipeline. A PDF is a real
  document: `%%Title:` and `%%For:` header comments become its title and
  author metadata, so a multi-page piece opens in any reader or e-reader with
  a proper title.
- **Seed sweeps.** `--sweep-seed` renders a file once per value instead of
  once, so exploring seeds or a parameter is one command instead of many
  hand edits. It overrides every `srand` call, so found art with a hardcoded
  `N srand` line sweeps unmodified. (A restore of a value saved with `rrand`
  puts the saved stream back instead.) `--sweep NAME=...` predefines `/NAME`
  in `userdict` for a program that reads it. `--png` writes numbered
  per-frame files; `--contact-sheet PATH` composites every frame into one
  grid PNG instead (`--grid COLSxROWS` overrides the layout). Use either or
  both. `examples/sweep_demo.ps` is the specimen. Sweeps are CLI-only; the
  `pscat-mcp` server does not expose one.
- **Gradients.** `shfill` paints axial and radial gradients (`ShadingType`
  2 and 3, with exponential and stitching functions, so multi-stop ramps
  work). Raster and window output use real gradients, SVG uses native
  `<linearGradient>`/`<radialGradient>`, and PDF uses a flat average colour
  (a known gap). [artkit](artkit.md) has helpers that build the shading
  dictionaries for you; `examples/gradients.ps` is the specimen sheet.
- **The lab printer.** `--spool DIR` turns the window into the printer in the
  corner of the lab. It idles, watches the directory, and renders each
  `.ps`/`.eps` that lands there, page by page, each job in a fresh
  interpreter.
- **Halftone.** `--halftone` screens the raster like a mono laser printer's
  RIP: classic dots on a 45° lattice, with coverage tracking darkness.

## Installing pscat without the repo

You don't need a git checkout or a Rust toolchain just to *use* pscat. Each
[GitHub Release](https://github.com/jeromebanks/postscript_interpreter/releases)
carries a ready-built archive per platform, made by
`scripts/package_release.sh` in the release workflow
(`.github/workflows/release.yml`):

| Platform | Archive |
|---|---|
| macOS, Apple silicon | `pscat-<version>-aarch64-apple-darwin.tar.gz` |
| macOS, Intel | `pscat-<version>-x86_64-apple-darwin.tar.gz` |
| Linux x86_64 (static, headless) | `pscat-<version>-x86_64-unknown-linux-musl.tar.gz` |
| Linux ARM64 (static, headless) | `pscat-<version>-aarch64-unknown-linux-musl.tar.gz` |
| Windows x86_64 | `pscat-<version>-x86_64-pc-windows-msvc.zip` |

Each holds `pscat` and `pscat-mcp`, their runtime libraries (`lib/`), the
licences (`LICENSE-MIT`, `LICENSE-APACHE`, `LICENSE-OFL`,
`THIRD_PARTY_NOTICES.md`) and the licences of the Rust libraries linked into
the binaries (`THIRD_PARTY_LICENSES.html`). `SHA256SUMS` on the release lists
every archive's checksum. The handwriting, typewriter and photo-etching tools
are subcommands of `pscat` itself (`pscat handwrite`, `pscat typewrite`,
`pscat photo-etch`), so they come with the binary.

To use one:

```sh
shasum -a 256 -c SHA256SUMS --ignore-missing     # optional: check the download
tar xzf pscat-<version>-<target>.tar.gz
export PATH="$PWD/pscat-<version>-<target>:$PATH"
pscat --png out.png some_file.ps
pscat fonts install                               # the font catalog, as font packs
```

The Linux archives are fully static (musl), so they run on any distribution,
but they are **headless**: they render to PNG, SVG and PDF and have no live
window, because a static binary cannot load the system's X11/Wayland
libraries. For the window on Linux, build from source.

`lib/` resolves automatically as long as it stays next to the `pscat` binary.
That is the point of the bundle layout. The libraries (`lib/artkit.ps` and the
rest) are also compiled into the binary, so a bare `pscat` binary, or one from
`cargo install`, runs them with nothing on disk. (`cargo install pscat
--no-default-features` builds a headless pscat without the live window; see
[pscat as a Rust library](library.md#supported-builds).) If you move the binary
away from the bundle, set `PSCAT_HOME` to wherever the bundle ended up. The
font catalog is not in the archive: `pscat fonts install` downloads it as font
packs ([Fonts](fonts.md#font-packs-the-catalog-outside-a-checkout)).

### Where pscat finds its files

This is the one place the search order is written down (`src/paths.rs`
implements it). For a relative name a program loads with `run` or `file`,
such as `(lib/artkit.ps) run`, pscat uses the first of these that exists:

1. `$PSCAT_LIB`, for `lib/...` names only: a directory that stands in for
   `lib/`, so `$PSCAT_LIB/artkit.ps` serves `lib/artkit.ps`. Use it to try
   edited libraries without rebuilding.
2. `$PSCAT_HOME`, a pscat tree holding `lib/` and `fonts/catalog/`. Its old
   name, `$PSCAT_ROOT`, still works.
3. The name relative to the current directory. Inside a checkout this is
   the checkout's own `lib/`, so edits there take effect immediately.
4. The per-user data directory: `$XDG_DATA_HOME/pscat` (by default
   `~/.local/share/pscat`) on Linux, `~/Library/Application Support/pscat`
   on macOS, `%APPDATA%\pscat` on Windows.
5. The directory holding the `pscat` binary (the release bundle).
6. The copy of `lib/` compiled into the binary.
7. Development builds only (a debug build, or `--profile ci`): the
   checkout the binary was built in. A release build never looks there,
   so `target/release/pscat` run from outside the checkout uses its
   compiled-in `lib/` and finds no font catalog unless `$PSCAT_HOME`
   points at the checkout or the font packs are installed. The repo's own
   scripts that read the font catalog (`build_site.sh`,
   `build_font_gallery.sh`) set `$PSCAT_HOME` to their checkout themselves;
   `handwrite`, `typewrite` and `photo-etch` need neither (they are `pscat`
   subcommands now, and use only the compiled-in `lib/`).

An absolute name is read as written. A plain font catalog directory
(`fonts/catalog/`, not compiled in) is looked for in steps 2, 4, 5 and
7, then the current directory, and every one found is used. After those
come the installed font packs (`fonts/packs/`, written by `pscat fonts
install`) in `$PSCAT_HOME`, the per-user data directory and the binary's
directory. See [Fonts](fonts.md#font-packs-the-catalog-outside-a-checkout).

`PSCAT_TRACE_RESOURCES=1 pscat ...` prints one line per file to stderr,
naming the step that served it (`pscat: resource lib/artkit.ps <- embedded`).

## Where next

- [Handwriting](handwriting.md): turn a string into a handwritten PNG.
- [Fonts](fonts.md): the font library and the 67-face catalog.
- [artkit](artkit.md): the generative-art toolkit.
- [All guides](README.md).
