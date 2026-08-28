# Plan for #103 — paper-ready graph.ps/dataviz.ps demos, interactive page, agent skill

## Issue summary
lib/graph.ps (#13) and lib/dataviz.ps (#14) exist but are demoed only via stylized art (ripple_range.ps, field_notes.ps). Need: plain paper-ready demos, interactive page like playground.html for data viz, docs framing embed use case, and a new agent skill so agents reach for these libs directly.

## What will change
### 1. Plain demos
- `examples/graph_paper.ps` — clean 2D function plot: y = sin(x) + 0.3x, axes, tick labels, title/caption, serif font, suitable for paper. Uses `(lib/graph.ps) run`, setframe, plotfn, axes. Letter page, black on white.
- `examples/dataviz_paper.ps` — clean bar+line composite plus scatter and pie subfigures, dvaxes, DVColors, white background, labelled.
### 2. Interactive page
- `site/charts.html` — mirrors playground.html: left textarea + preset picker, right canvas. Picker: graph line/param/polar/surface, dataviz bar/line/area/scatter/pie. Uses Pscat wasm. Self-contained limitation: wasm has no filesystem, so lib source fetched via fetch('lib/...') and prepended before run.
- Update nav in playground/index/gallery, build_site.sh
### 3. Documentation
- README.md paragraph framing graph.ps/dataviz.ps as paper-ready viz, NOTES.md entry.
### 4. Agent skill
- `.claude/skills/psviz/SKILL.md` — alongside pscat/psart: when to use each lib, calling conventions, domain mapping, color callback contract, pitfalls, copy-paste templates.

## Files
New: examples/graph_paper.ps, examples/dataviz_paper.ps, site/charts.html, .claude/skills/psviz/SKILL.md
Modified: site/playground.html, site/index.html, scripts/build_site.sh, README.md

## Not doing
- Hidden-line removal, numeric labels in libs, gallery/show.sh entries, lib logic changes.

