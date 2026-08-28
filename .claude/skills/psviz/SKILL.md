---
name: psviz
description: Create paper-ready data visualizations with lib/graph.ps and lib/dataviz.ps — function plots, surfaces, and bar/line/area/scatter/pie charts for papers and documents.
---

# psviz — paper-ready charts with pscat

Use `lib/graph.ps` (function/surface plotting) and `lib/dataviz.ps` (categorical charts) to produce attractive, embeddable figures. Both libs define into the current dict on `(lib/...) run`, draw nothing on load, and run unchanged in Ghostscript when file access is permitted (see Pitfalls — SAFER) — sibling libs to `lib/artkit.ps`, no dependency on each other.

## When to use which

- `graph.ps`: y=f(x), parametric (t -> x y), polar (theta -> r), and 3D surface z=f(x,y) under `setview`/`project3`. Choose this for mathematical functions and height fields.
- `dataviz.ps`: categorical data — arrays of numbers. `barchart`, `linechart`, `areachart`, `scatterchart`, `piechart`. Choose this for measured data.

If the user says "chart", "bar", "line", "pie", "scatter" -> `dataviz.ps`. If they say "plot", "function", "surface", "polar" -> `graph.ps`.

## Calling conventions

### graph.ps
```
(lib/graph.ps) run
% frame: data domain -> viewport
x0 y0 x1 y1 px py pw ph setframe
x gmapx -> X      y gmapy -> Y
x y gmoveto   x y glineto
% curves (append to current path; caller does newpath/stroke)
x0 x1 n {x->y} plotfn
t0 t1 n {t->x y} plotparam
th0 th1 n {theta->r} plotpolar   % degrees
nx ny tlen axes                % border + ticks, caller strokes
% 3D
az el scale cx cy setview
x y z project3 -> X Y
x0 x1 y0 y1 nx ny {x y->z} plotsurface
lx ly lz axes3
```
Angles are degrees. `setframe` is persistent (like TurtleState); call once before curves/axes. Each section uses disjoint scratch prefixes (`gp-`, `gv-`, `g3-`, `gsf-`) — if your sampling proc re-enters a driver, wrap it in `n dict begin ... end` so its `def`s don't clobber the driver.

### dataviz.ps
```
(lib/dataviz.ps) run
% frame for bar/line/area
y0 y1 px py pw ph setdvframe
y dvmapy -> Y        i n dvcatx -> X   n gap dvbarw -> w
[vals] dvbounds -> lo hi     [vals] dvsum -> total
% bar/line/area (category-center x)
[vals] gap {i v -> r g b} barchart
[vals] linechart         % caller strokes
[vals] areachart         % caller fills
n ny tlen dvaxes
% scatter (continuous domain, separate frame)
x0 y0 x1 y1 px py pw ph setscatterframe
[[x y] ...] rad {i x y -> r g b} scatterchart
% pie/donut
[vals] cx cy r ir {i v -> r g b} piechart   % ir=0 pie, >0 donut
% colors
DVColors   i dvcolor -> r g b
```
`{ pop dvcolor }` (bar/pie) and `{ pop pop dvcolor }` (scatter) are the defaults. Color proc is called *before* path building, so a stray `newpath` inside it can't corrupt geometry. Baseline for bars is `0 dvmapy` (unclamped — pick a domain that brackets zero). `dvcatx` centers categories, so line charts are inset from edges by design. `dvaxes` decorates only `DVFrame` (bar/line/area); scatter has no axes helper.

## Minimal templates

### Clean line chart (paper)
```
%!PS
(lib/dataviz.ps) run
1 1 1 setrgbcolor clippath fill
0 0 20 72 320 468 170 setdvframe
0.15 0.45 0.75 setrgbcolor 1.4 setlinewidth
newpath [4 6 5 9 7 11] linechart stroke
0.6 0.66 0.72 setrgbcolor 0.6 setlinewidth newpath 6 4 5 dvaxes stroke
showpage
```

### Clean bar chart (paper)
```
%!PS
(lib/dataviz.ps) run
0 0 24 72 320 468 170 setdvframe
newpath [8 15 12 19] 0.22 { pop dvcolor } barchart
newpath 4 4 5 dvaxes stroke
showpage
```

### Clean function plot (paper)
```
%!PS
(lib/graph.ps) run
0 -1.2 360 1.2 72 120 468 260 setframe
newpath 4 4 6 axes stroke
newpath 0 360 200 { dup sin } plotfn stroke
showpage
```

## Pitfalls

- Ghostscript SAFER (the default since 9.50) blocks `(lib/...) run` with `/invalidfileaccess`. Allow file access with `--permit-file-read` or a search path, e.g. `gs '--permit-file-read=lib/*' '--permit-file-read=examples/*' -I lib -dBATCH -dNOPAUSE -sDEVICE=png16m -g612x792 -o out.png file.ps`, or `gs -dNOSAFER -dBATCH ...` if appropriate for your environment.
- Don't use `(lib/...) run` in browser/wasm presets — the wasm build has no filesystem. Inline the needed procs or fetch the lib text and prepend at runtime (see `site/charts.html`).
- Wrap `setrgbcolor`/`dvcolor` callbacks in `gsave/newpath ... grestore` only if you also manage the path; the drivers already call the color proc before `newpath`.
- `barchart`/`linechart` share `setdvframe`; don't mix with `setscatterframe` without resetting.
- Surfaces are wireframe only — no hidden-line removal (see `graph.ps` header; `gallery/ripple_range.ps` shows the cheap per-row occlusion trick for height fields).

## Paper checklist

- White page (`1 1 1 setrgbcolor clippath fill`), Palatino or Helvetica labels, `showctr` for centered titles.
- `pscat --page 612x792 --png out.png file.ps` + `gs '--permit-file-read=lib/*' '--permit-file-read=examples/*' -I lib -dBATCH -dNOPAUSE -sDEVICE=png16m -g612x792 -o gs.png file.ps` both succeed (or `gs -dNOSAFER -dBATCH ...` — SAFER blocks `(lib/...) run` by default; see Pitfalls).
- Keep the figure self-contained with `%%BoundingBox` and a one-line `%%Title`.

See `lib/graph.ps` and `lib/dataviz.ps` headers for the full procedure list, and `examples/graph_paper.ps` / `examples/dataviz_paper.ps` for complete paper-ready pages.

