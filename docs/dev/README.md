# Contributor docs

Notes for people working on pscat itself, and for the coding agents that work
on it with them. For conventions start with [`AGENTS.md`](../../AGENTS.md) and
[`SDLC.md`](../../SDLC.md); both are at the repo root.

## Read in this order

1. [`HANDOFF.md`](HANDOFF.md) orients you in the code: where things live, the
   gotchas that have cost time before, and what is still open.
2. [`ARCHITECTURE.md`](ARCHITECTURE.md) explains the design: the object model,
   the execution model and the deliberate deferrals.
3. The topic docs below, as the work needs them.

## Topic docs

| Doc | What it covers |
|---|---|
| [`VM.md`](VM.md) | How `save` and `restore` work, and the rules for mutating arrays and dictionaries. |
| [`FONTS.md`](FONTS.md) | The font machinery: Type 3 programs, Type 1 parsing and the runtime catalog. |
| [`RELEASING.md`](RELEASING.md) | Interpreter releases: `scripts/release.sh`, the tag-driven pipeline, safe resume, the owner's one-time setup, the release checklist, hotfixes and yanking. |
| [`RELEASING-FONTS.md`](RELEASING-FONTS.md) | The font-pack release train: pack formats, cutting a `fonts-*` release, pinning a pscat release to it. |
| [`CAPABILITIES.md`](CAPABILITIES.md) | The machine-readable art catalog that `pscat --capabilities` prints. |
| [`PS_LIBRARY_COUPLING.md`](PS_LIBRARY_COUPLING.md) | What a PostScript-only change must touch, and how that list was shortened. |
| [`SELFTEST.md`](SELFTEST.md) | Checking a PostScript-only change without writing Rust. |
| [`GS_CHECK_INVENTORY.md`](GS_CHECK_INVENTORY.md) | Every Ghostscript cross-check test, and what each one asserts. |
| [`DOCS_PLAN.md`](DOCS_PLAN.md) | How the documentation is organized, and why. |
| [`../STABILITY.md`](../STABILITY.md) | Which surfaces are stable, experimental or internal, pre-1.0 semver and the deprecation rule. |
| [`../../SECURITY.md`](../../SECURITY.md) | The threat model: what untrusted PostScript and the MCP server can do today, and the limits that do not exist yet. Update it with any change to file, process or MCP behaviour. |

## Library design records (`design/`)

Why each art library is built the way it is, with the evidence images and
prototypes behind the decisions. Each one is a record of a design, not a
usage guide; for how to use a library, see the [guides](../guides/README.md).

| Doc | Library |
|---|---|
| [`design/DRYMEDIA.md`](design/DRYMEDIA.md) | `lib/drykit.ps` and the chalk, crayon and charcoal libraries on it (references and prototypes in [`design/drymedia/`](design/drymedia/)). |
| [`design/PAINT_LOADING.md`](design/PAINT_LOADING.md) | Double-loaded brushes and highlight colours in `lib/paintkit.ps`. |
| [`design/CANVAS_SURFACE.md`](design/CANVAS_SURFACE.md) | The shared canvas surface under the paint and dry-media libraries. |
| [`design/WATERCOLOR.md`](design/WATERCOLOR.md) | The watercolor rendering spike (prototypes in [`design/watercolor_prototypes/`](design/watercolor_prototypes/)). |
| [`design/TYPEWRITER.md`](design/TYPEWRITER.md) | `lib/typewriter.ps` (references and tools in [`design/typewriter/`](design/typewriter/)). |

## Where new docs go

- A library **design doc**: `docs/dev/design/`, with its images beside it.
- A **user guide**: [`docs/guides/`](../guides/README.md).
- A one-off writeup, such as the record of a decision: [`docs/archive/`](../archive/README.md).
- **Per-issue notes**: [`notes/`](../../notes/README.md), one file per issue.

History is in [`docs/archive/`](../archive/README.md): the original spec
(`INIT.md`), the staged roadmap, and the notes written before `notes/` existed
(`NOTES.md`, frozen).

## Previewing the website

The project site is at
**[jeromebanks.github.io/postscript_interpreter](https://jeromebanks.github.io/postscript_interpreter/)**:
architecture and extending guides, the full art/font/example gallery, a
dedicated [font gallery](https://jeromebanks.github.io/postscript_interpreter/fonts.html)
(every Type 3 face and every catalog family, one card each), and a live
playground running the wasm interpreter, all in the browser, no install.

It is published from this repo by `.github/workflows/pages.yml` (GitHub Pages,
"GitHub Actions" source) on every push to `main`. The sources are in `site/`.
To preview locally:

```sh
./scripts/build_site.sh                  # assembles _site/
python3 -m http.server -d _site          # open http://localhost:8000
```

`build_site.sh` also renders the font gallery cards
(`scripts/build_font_gallery.sh`) and the example and gallery stills fresh
each time, so the site never drifts from what the interpreter actually does.

The gallery pages are generated: `gallery/manifest.json` lists every piece and
specimen, and `scripts/gallery.py` writes `gallery.html` and one page per piece
from it (there is no hand-written gallery page). Add a piece by adding a
manifest entry; see [`gallery/README.md`](../../gallery/README.md).
