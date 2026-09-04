# NOTES.md — stage summaries

Newest first. Per `AGENTS.md`, each stage ends with a summary here: what
was built, tradeoffs made, what's explicitly deferred.

## `lib/pagekit.ps`: give the page templates a demo on the site (issue #63, 2026-09-04)

Closes issue #63, the follow-up #61 filed after that PR's review found
issue #18's five page templates (`pgcard`/`pgletter`/`pgcertificate`/
`pginvitation`/`pgposter`) had `examples/template_*.ps` specimens and
doc coverage but no visibility on the published site.

Added a "Page templates" section to `site/gallery.html`, right after
Style packs — the closest existing precedent, since pagekit is also a
sibling library layered on artkit rather than a standalone one. One
card per template (five total), each backed by a new `render` call in
`scripts/build_site.sh` producing `assets/renders/template_*.png` from
the existing `examples/template_*.ps` files at their declared
612×792 `%%BoundingBox`. Not added to `gallery/show.sh` (these are
`examples/`, not `gallery/`, pieces) or `site/playground.html` (per
#61's guidance and the issue body itself: pagekit needs `(lib/artkit.ps)
run (lib/pagekit.ps) run`, so it isn't self-contained and isn't
playground-eligible). `tests/site.rs`'s existing
`build_site_assembles_everything` — which runs the real
`build_site.sh` and asserts every `assets/renders/*` the built
`gallery.html` references actually exists — is what verified the new
cards wire up correctly, not a new test; the gap it exists to catch (a
typo'd basename between an `<img>` tag and a `render` call) is exactly
the failure mode this change could have introduced.

`README.md` already documented pagekit fully (issue #18's own work),
so no changes there.

## `lib/etching.ps`: honor EXIF orientation (issue #56, 2026-09-04)

`et-dims`/`et-draw` now read a JPEG's APP1/TIFF Orientation tag
(0x0112) via a hand-rolled IFD walk (`et-exif-orientation`,
`et-str-u16`/`et-str-u32` for byte-order-aware reads over the APP1
payload string) folded into `et-scan`'s existing marker walk — no
second file pass. Orientations 5-8 (90°-rotated) swap the
width/height `et-dims` reports and the display-space size `et-draw`
samples against; the actual pixel remap is a single new proc,
`et-orient-map` (display (x,y) -> raw buffer (row,col), one non-
identity branch per orientation 2-8, identity default), called from
`et-tone` right before it indexes `buf` — `buf` itself, and the
row-decode loop that fills it, stay untouched, since they're keyed off
the raw SOF dimensions regardless of orientation. `et-hatch`'s own
sampling scale switched from raw width/height to the new
`dispwidth`/`dispheight` pair so it walks display space, matching what
`et-orient-map` expects as input. `et-dims` sets `dispwidth`/
`dispheight` too (not just the width/height it returns) even though it
never reads them itself — `EtchLayout` is a persistent dict, so
leaving them unset after an `et-dims`-only session would surface as an
`undefined` name to any direct call into `et-hatch`/`et-tone` (not
hidden — both live in `EtchLayout` alongside the public API), a real
gap `et-draw`'s own unconditional recompute papers over for the
documented API but not for that path; caught in review, not by the
test suite. Every EXIF read is bounds-checked
against the payload's own length before it happens (PostScript `and`
isn't short-circuit, so the checks are nested `if`s, not chained
`and`s) — malformed/truncated/non-Exif APP1 data degrades to
orientation 1 (identity) rather than a rangecheck crash, verified by
hand against five deliberately-malformed payloads (truncated, XMP-
flavored, bad byte-order marker, out-of-bounds IFD offset, absurd
entry count) all rendering as if untagged.

All eight orientation transforms were verified numerically, not just
read off the derivation: a small script builds an 80x40 grayscale JPEG
with a hand-spliced EXIF APP1 at each of the 8 orientation values,
renders each through `et-draw`, and checks which edge of the output
actually went dark — matched predictions for all eight. The first
pass at this used a full-width dark stripe and caught a real gap in
its own test design (found in review, not by the stripe itself):
orientations 5 and 7 land the same stripe on the same edge as 6 and 8
respectively, since a stripe is symmetric on the axis a 90°-rotation
swaps — so a fixture built that way can't tell a 5/6 or 7/8 mixup from
a correct implementation. The committed fixture
(`tests/data/cornerdark_exif6.jpg`) uses a corner block instead — dark
only in the raw top-left quadrant — which is asymmetric on *both*
axes; orientations 5/6/7/8 each rotate that corner to a different one
of the four display corners (5→top-left, 6→top-right, 7→bottom-right,
8→bottom-left), so `et_draw_honors_exif_orientation_rotation` pins the
specific orientation, not just "some rotation happened." Only
orientation 6 is a committed regression test (that one fixture plus
`et_dims_honors_exif_orientation_swap` for the dimension swap) — the
other seven were confidence-building for this session, not added as
fixtures, since the transform math is one shared proc
(`et-orient-map`), not seven independent implementations that could
each be wrong differently. Not done: validating the TIFF
entry's `type`/`count` fields before trusting its value (a
non-SHORT/non-count-1 Orientation entry, or a tag-274 collision on
some other field, would just clamp to a garbage-but-plausible 1-8
value or fall through to identity) — out of scope per the issue,
which asked for the tag read, not full TIFF conformance.

Independent review (Codex's cross-model pass failed to return a
response for this PR, so this fell back to a blank-context Claude
agent per the skill's fallback path) found three real gaps the initial
pass missed, all fixed:
- `et-scan`'s `len 2 sub` (used for every marker segment's payload
  read, not just APP1) wasn't guarded against a corrupted length field
  of 0 or 1, which would send a negative count into `string` and raise
  a raw rangecheck instead of a named, catchable error — the one
  malformed-input case the "verified by hand against five payloads"
  claim in the paragraph above didn't actually cover. Pre-existing on
  `main` (an `APP0` with a bad length crashes identically via
  `et-skip`), not a regression this PR introduced, but the new APP1
  path inherited it and the PR's own claim was narrower than the code.
  Fixed with a `len 2 lt { et-jpeg-bad-marker } if` guard right after
  the length read — closes the gap for every marker type, not just
  APP1 — plus a regression test
  (`et_dims_rejects_a_corrupt_segment_length`).
- The doc comment on `et-scan` claimed orientation comes from the
  "first" Exif APP1 segment encountered; the loop actually lets the
  *last* one win (it reassigns `/orientation` on every APP1 match, no
  early exit). Harmless in practice — real JPEGs carry at most one —
  fixed by correcting the comment rather than changing the behavior.
- `et-str-u16`/`et-str-u32` shared identical scratch-variable names
  (`etub`/`etui`/`etus`), breaking the file's own per-proc unique-
  prefix discipline (not a live bug — neither proc calls the other, so
  no reentrancy — but a real style deviation). Renamed to `et16*`/
  `et32*`.

## Halftone gallery piece (issue #126, 2026-09-03)

"Out of Register" (`gallery/out_of_register.ps`, 720x900) — a
three-plate risograph sunset misregistered on purpose, and the first
piece whose screening *is* halftonekit (issue #53): a teal dot plate
(sky/water gradients, 15°, offset [4 2]), an orange dot plate
(linear-cone sun halo plus horizon band, 75°, offset [-3 3]), and a
dark line plate (two tent ridges shaded by height fraction, 45°,
offset [2 -2]), with flat sun core, deliberately shifted ring,
registration crosshairs, plate color bars, and letterpress title in
solid key ink. Plan review passed provisional GO with six pins, all
adopted (page-space tone fields, one clip + explicit inflated boxes,
Frequency 10, total procs, opaque light-first paint order, all five
gallery surfaces). Rendering caught two real bugs reasoning missed: a
teal branch leaving `x` on the stack (contract error, fail-fast) and
the ridge field clamping to full ink *above* the ridgeline (black
sky) — the band is now gated on side. All five surfaces wired:
show.sh triple-array, README row, 2x still, hand-written
`site/gallery.html` card verified through a local `build_site.sh`
run. Ghostscript renders the same composition (only its Bangers
fallback differs — accepted display-font variance, same as older
pieces). Deferred: playground picker (needs self-containment, no lib
loads); actual pages deploy (local build verified only). Also noted:
`site/gallery.html` already lags the gallery (no cards for
`compositors_proof`, `fugitive_pigments`) — pre-existing, out of
scope.

## Woodcut, linocut, and engraving mark presets (issue #52, 2026-09-03)

A tenth sibling library, `lib/printkit.ps` — tag-migrated from birth
like every recent sibling here, composing `hatchkit.ps`'s `hatch`
(issue #49), `artkit.ps`'s `scatter` (issue #48), and, optionally,
`surfacekit.ps`'s `grain` (issue #51) into three printmaking presets:
`woodcut` (directional gouges, grain-following breakup), `linocut`
(bolder carved regions, simplified marks), `engraving` (fine parallel/
cross-hatched line work), over one shared options dict (`/Scale`,
`/Density`, `/Roughness`, `/Seed`, `/Color`, `/Budget`, `/Paper`,
`/Angle`).

**API deliberately breaks from the `region opts NAME` convention its
own siblings (surfacekit/stipplekit/halftonekit) established.** A
`scatter`-based preset only ever needs a `screct`/`scpath` region for
`scin`'s own approximate containment test; a printmaking preset also
needs `hatch`, which needs a real graphics-state `clip` for an *exact*
boundary — and a stored region's `/Edges` is a flattened line-segment
soup with no subpath boundaries recorded, so rebuilding a `clip`-able
path back out of it would mean re-deriving subpath breaks by
coordinate-equality matching between adjacent edges, for no benefit
over the path the caller already has. Every preset here instead takes
the *current path* directly (the same convention `hatch` itself uses)
and manages its own `clip` internally: `<path> opts woodcut -`, no
pre-`clip`, no region object, required of the caller at all. `advisor`
flagged this design call explicitly (before any code existed) as the
right one — "the `/Edges`-reconstruction alternative is exactly the
machinery hatchkit and surfacekit both declined to build."

**Sequencing inside each preset, settled by the same `advisor` plan
review**, closing three real correctness gaps before any code existed:
(1) `pathbbox` is read from the caller's real, un-flattened path
*before* anything else runs and handed to every `hatch` call as an
explicit `/BBox` — never left to `hatch`'s own `pathbbox`-of-current-
path default, since a later `/Paper true` grain pass or a chip-mark
scatter pass leaves the current path in a different (flattened, or
briefly gone) state than what the caller built, and the default would
then read the wrong shape or raise `nocurrentpoint` outright. (2) The
real, un-flattened path is `clip`-ped first, *then* `scpath` builds a
region from it — clipping the true curves first is more precise than
clipping a flattened polygon. (3) `/Budget` defaults to 20000 —
matching `hatch`'s and `scatter`'s own defaults exactly, so `printkit`
never *lowers* either primitive's own safety ceiling for a caller who
never asked for a smaller one; an early draft defaulted to 4000; the
advisor call caught that this would reject unremarkable art `hatch`
alone would happily draw.

**A second `advisor` pass, over the finished implementation, caught
five more.** (1) The `ghostscript_accepts_*` test's early-return-if-
`gs`-missing path is indistinguishable from a genuine pass in
`cargo test`'s own summary — worth flagging explicitly rather than
just trusting the green checkmark: `gs` is installed here (verified
`gs --version`), and both `examples/printkit.ps` and
`gallery/nightfall_triptych.ps` were run through it directly as an
extra check beyond the automated test. (2) The gallery piece had
redefined `/setcolor`, a real Level 2 operator `tests/color.rs`
exercises elsewhere in this codebase — renamed to `/setink` before it
became a landmine for some future library proc reaching for the real
one. (3) `woodcut`/`linocut` called `scpath` unconditionally but
`engraving` only inside `/Paper true` — an asymmetry, caught by
`advisor`, that looked like a real caller-visible inconsistency
(curves surviving with `/Paper false`, flattened with `/Paper true`).
Made unconditional across all three for uniformity — and building the
regression test for it surfaced that the concern, while worth fixing
for consistency, was never actually caller-visible either way: `path`
lives on this interpreter's own saved graphics state, so the preset's
closing `grestore` restores the caller's path exactly as it was at
entry, real curves included, regardless of what `scpath` did to it
internally. An earlier draft of this file's own header claimed the
caller's path comes back flattened; the regression test this finding
produced is what caught that claim was wrong, not just untested — the
header now describes the corrected (and simpler) contract. (4) A test
asserting `/Budget` forwarding had drifted into asserting something
false along the way: a comment claimed a smuggled-in `/Spacing 40`
narrowed `hatch`'s own candidate count, but `printkit` never reads
`/Spacing` at all — every options dict handed to `hatch`/`scatter`/
`grain` is built fresh from this file's own eight documented keys,
never layered onto the caller's, so an unrecognized key is silently
ignored, not forwarded. Fixed the test and stated the ignore-unknown-
keys behavior explicitly in the header, since the deliberate
vocabulary-shadowing note nearby made it an easy thing to assume
otherwise. (5) `--lint` was run over both new `.ps` files (clean) —
recorded here since HANDOFF.md notes `--lint` catching real operand
leaks in `tfdrawline`/`et-hatch` before, and this file's own
`prchipmark` (a `/Mark` callback with a four-operand contract) and the
gallery piece's `forall` body are exactly its beat.

**Step 8's independent review (Codex, after two transient runtime
failures despite a healthy authenticated session — a same-family
Claude fallback launched for one of those attempts also failed, to a
session rate limit, before Codex itself came back clean on retry)
found one real bug: `/Seed` was never validated by `propts`.**
`prapplyseed` derives each sub-call's own seed as `Seed + offset`
(0/1/2 for `woodcut`/`linocut`'s three sub-calls, 0/1 for
`engraving`'s two) — a non-numeric `/Seed` failed with a raw
`typecheck` from `add` deep inside `prapplyseed`, not a self-
documenting name, and a `/Seed` near `scatter`'s own documented
`+/-2147483647` bound reproduced *inconsistently between presets*:
`engraving` (max offset 1) accepted `2147483647` while `woodcut`/
`linocut` (max offset 2) rejected the same value with `scatter`'s own
error name once their own `+2` pushed it past `scatter`'s bound.
Fixed by validating `/Seed` in `propts` itself — numeric, and
`abs(n) <= 2147483645` (2147483647 minus the largest offset this file
ever adds), checked uniformly for every preset regardless of that
preset's own maximum offset, so the same seed either works or fails
the same way everywhere. Three new tests pin it: the type/range
errors, and that the documented boundary value (2147483645) succeeds
for all three presets uniformly.

**A second Codex round, on the pushed `/Seed` fix, caught a real
regression the first round's own fix (making `scpath` unconditional
for "uniformity") had introduced.** `engraving` with `/Paper false`
(its common case) never uses the `scatter`-shaped region `scpath`
builds — but `scpath` enforces its own 20000-edge ceiling regardless
of whether anything downstream needs a region that large, so a caller
with a genuinely complex path (a real one built for the regression
test: 21,000 segments), one `hatch`'s own `clip` would happily draw,
got a spurious `scpath-too-many-edges` rejection from a region this
preset was never going to use. Fixed by reverting `engraving` to
calling `scpath` only inside its own `/Paper` branch (`woodcut`/
`linocut` still call it unconditionally — their chip marks always
need the region) — justified now in a way it wasn't when the first
round's "uniformity" framing was written: the flattening consistency
concern that motivated calling it unconditionally in the first place
was never actually caller-visible either way (see the round-1 finding
above), so there was nothing real to gain from paying `scpath`'s own
budget on a call that never uses its result, only a real regression to
lose. Two more tests: a 21,000-edge path succeeds under `/Paper
false` and still correctly rejects under `/Paper true` (proving the
fix is "skip the unused call," not "never call `scpath` at all").

**A third Codex round found two more real bugs, one of them an actual
crash.** (1) `/Scale` was validated only for `num > 0` — an extreme
value (`1e9`, on a 10x10 path) reached `tiny-skia`'s rasterizer with
geometry degenerate enough to panic the interpreter *process itself*,
not raise a catchable PostScript error (verified: `assertion failed:
edges[curr_idx].last_y >= curr_y as i32`, `tiny-skia`'s own scanline
code) — a real violation of AGENTS.md's code-quality bar, and an
extreme-small value (`1e-20`) hit a raw `rangecheck` in a downstream
`cvi` first. Bounded to `[0.001, 1000]` in `propts` — chosen by
sweeping actual values against a live interpreter (clean through
`1e8`, panics by `1e9`; clean at `0.01`/`0.001`, `rangecheck`s by
`1e-20`) rather than guessed, leaving three-plus orders of magnitude
of headroom past either bound on both sides, far past any real print
size. (2) The same "unused region" bug round 2 fixed for `engraving`
also applied to `woodcut`/`linocut`: with `/Paper false` and
`/Density` low enough to round their own chip count to 0, neither
preset's marks end up touching the region either, so the same
unconditional `scpath` call could reject a genuinely complex path for
a region nothing was going to use. Fixed the same way — the chip-
count computation moved ahead of the `gsave`/`clip` block (it only
needs `/Density`, already resolved) so a `/prneedregion` flag (chip
count > 0, or `/Paper` true) can gate the `scpath` call. The two
round-2 tests generalized into one parametrized-by-preset test
covering all three, `/Density 0` standing in for engraving's
structural "no chip marks at all."

**Shared option names deliberately shadow the sibling family's own
same-named options with different semantics — validated under
`printkit`-owned error names before any derived value reaches
`hatch`/`scatter`/`grain`.** `/Scale` here is one positive number (a
size multiplier), not `scatter`'s `[lo hi]` range; `/Density` here is
a 0..1 ink-coverage fraction, not `grain`'s marks-per-square-unit;
`/Angle` here is a dominant-direction offset, not `hatch`'s literal
sweep angle. A caller reaching for a sibling-shaped value out of habit
(`/Scale [0.4 1.4]`, say) hits `printkit-scale-must-be-a-number`
rather than silently misbehaving or surfacing a *different* file's
error name for a value this file rejected. `propts`, one shared
validator (mirroring `surfacekit.ps`'s own `sfgetdef`/`sfbindnum`/
`sfcolordef`, `pr`-prefixed here — grepped clean against every
`lib/*.ps`/`lib/styles/*.ps` file's own `/pr...` names, not just
top-level `def`s, the halftonekit lesson about what a narrower grep
misses), resolves and validates every shared option in one pass; only
the `<preset>-opts-must-be-a-dict` check stays per-preset (an
`errproc` parameter into `propts`), matching the sibling convention of
a per-operator name for that one check specifically.

**Preset differentiation**, each with its own base constants over the
shared knobs: `woodcut` is one directional `hatch` pass (the grain)
at the highest wobble/dropout/trim of the three, plus a scatter of
small chip marks jittered widely around the grain angle — texture and
breakup. `linocut` is one bold, low-wobble `hatch` pass (widest
stroke width, least irregularity — deliberate, not hand-wobbly cuts)
plus a *sparse* scatter of a *few, large* chip marks — "simplified."
`engraving` is a single `hatch` call sweeping three angles 60 degrees
apart (not three separate calls — `hatch`'s own `/MaxLines` already
bounds the summed candidate count across every angle in one `/Angles`
array, so one call gives a true total `/Budget` bound and one seed
instead of three) at the finest spacing and thinnest width, near-zero
wobble/dropout — no chip marks at all.

**`/Paper true` composes `surfacekit.ps`'s `grain` under the ink,
inside the same clip** — the "interplay with surfacekit stays #52's
problem" item this repo's halftonekit entry explicitly deferred here.
Drawn first (before any ink), at low `/Strength` (0.18) and a fixed,
subtle `/Density`. Because it's scattered inside the same `clip` this
file already established from the caller's exact path, its paint is
exactly bounded too — better than `scin`'s own approximate overhang, a
bare standalone `grain` call would only get.

**Seed derivation, not a second seed-management layer.** `/Seed`
derives one sub-seed per underlying call (`Seed`, `Seed+1`, `Seed+2`,
...) via a small `prapplyseed` helper, rather than this file managing
one shared `srand`/`rrand` pair itself — `hatch`/`scatter`/`grain`
each already seed and restore their own stream per their own
documented contract, so layering a second one here would be redundant
bookkeeping with its own chance to disagree.

**Deliverables:** `examples/printkit.ps`, a four-panel specimen sheet
(one per preset, plus a `/Paper true` panel); the gallery piece
*Nightfall, Three Cuts* (`gallery/nightfall_triptych.ps`) — a moonlit
ridge over water using all three presets in one scene, each silhouette
inked solid first and then cut a second time with a *lighter* `/Color`
instead of the usual dark ink, since printkit's presets only ever add
ink and never subtract: the trick reads as a moonlit highlight cut
through the block, the actual look real relief printmaking gets from
gouging a line's worth of wood or lino away and leaving it uninked. It
departs from `printkit.ps`'s siblings' own "a primitive gets a
specimen, not a gallery card" precedent — issue #52 explicitly asked
for a gallery composition, so `gallery/README.md` states this is a
deliberate one-off, not drift, for the next sibling-file author to
read correctly.

**19 tests** (`tests/printkit.rs`): loads-draws-nothing; each preset
draws from a bare path with no pre-clip; containment on a concave path
(the same chevron shape hatchkit's own test uses); per-preset seeded
reproducibility; a distinguishability test asserting all three
presets' pixel output differs pairwise *and* that engraving's
three-angle crosshatch out-covers the two single-pass presets (a
concrete, formula-pinning claim, not just "the bytes differ");
`/Paper true` adds ink and stays clipped, for both a chip-mark preset
and `engraving` (which never calls `scpath` on its own); `gsave`/
`grestore` restores color, linewidth, and leaves the caller's own path
current (`pathbbox` still reports it); `/Budget` omitted never lowers
`hatch`'s own ceiling, an explicit small one rejects via `hatch`'s own
name, and a small one also trips `scatter`'s own name for chip marks
— proving `/Budget` really forwards to both subsystems, not just one;
the full shared-option validation error table, run against all three
presets; the sibling-shaped-`/Scale`-value regression from the design
review above; a procedure standing in for `/Color` cannot auto-execute
(the array-boxing discipline hatchkit/surfacekit both learned from
Codex review, applied here from the start); the specimen sheet's
four panels each render ink (inset-measured, clear of each panel's own
border stroke — the halftonekit/stipplekit false-positive lesson);
every `% @example:` tag runs clean; the standard `ghostscript_accepts_*`
test.

## Paper, canvas, and print-surface textures (issue #51, 2026-09-03)

A ninth sibling library, `lib/surfacekit.ps` — tag-migrated from
birth like `hatchkit.ps`/`stipplekit.ps`, `@requires: (lib/artkit.ps)
run` for the same reason `stipplekit.ps`/`paintkit.ps` do. Five
presets: `grain` (paper grain speckle), `fiber` (paper fibers),
`scuff` (scratches and scuffs), `misreg` (print/registration
imperfections), and `weave` (canvas weave).

**Four of the five are thin `scatter` wrappers, not a new placement
engine** — the same choice `stipplekit.ps` (issue #50) already made,
now extended a second time. `grain`/`fiber`/`scuff`/`misreg` take the
same `region` operand `scatter`/`stipple` do and forward every option
they don't own (`/Count`, `/Density`, `/MinSpacing`, `/Seed`, `/Tries`,
`/Budget`, `/Scale`, `/Rotate`, `/Mark`, `/Weight`) unchanged. Unlike
`stipple`, none of them repurpose `/Weight` or give `/Density` a
second meaning — this file's "how strong" knob is a new `/Strength`
option instead of a tone callback, so there's no option-conflict
surface to police the way `stipple` has one. Visual variation
(shade, and for `fiber`/`scuff` length) rides entirely on `scatter`'s
own already-random `scale` per mark, rather than each default `/Mark`
drawing its own extra random numbers — zero new placement arithmetic,
same discipline `stipple`'s own header states.

**`weave` is deliberately not scatter-based.** A basket weave is a
regular grid, not a random scatter, so it walks its own grid over the
region's `/BBox` and tests each cell's center with `scin` — the same
"a mark can overhang a curved region's true edge, clip first for an
exact one" contract every scatter-based preset here already has.
Since it has no `/Budget` to inherit, it pre-flights its own **two**
independent caps, mirroring `hatchkit.ps`'s `/MaxLines`/`/MaxSamples`
shape exactly: `/MaxThreads` bounds cell count (cheap for any region),
and `/MaxEdgeSamples` additionally bounds cell-count-times-edge-count,
checked only for a `scpath` path region — `scin`'s own cost is O(1)
against a `/Kind /rect` region but linear in edge count against a
`/Kind /path` one (the same asymmetry `scarea`'s own measurement pass
documents), so a cell-count cap alone doesn't bound the path case. A
dedicated regression test builds a ~300-edge path region sized so cell
count alone stays comfortably under `/MaxThreads` while
cells-times-edges still trips `/MaxEdgeSamples`, proving the two caps
are independently enforced rather than one silently covering for the
other.

**Naming: `scuff`, not `scratch`.** The issue's own wording is
"scratches and scuffs," but this codebase already uses "scratch"
throughout every sibling library's own docs as a term of art for
private working state (a library's own scratch prefix, scratch dict,
scratch names) — a public operator named `scratch` would collide with
that vocabulary in prose and search, not just code, so the preset is
named `scuff` instead.

**`/Color`/`/Strength`, new to this file, not inherited from any
sibling.** Every preset's whole call runs inside its own
`gsave`/`grestore` (a caller-supplied `/Mark` override runs inside it
too) since every default mark sets color — the same reason `hatch`
wraps its own drawing loop — and every default mark (`weave`'s own
grid loop included) `newpath`s before drawing, since a `scpath` region
deliberately leaves its flattened path behind and a mark that skipped
`newpath` would fill it on its very first call only (the same footgun
`stipple`'s own header documents; a dedicated regression test builds a
region from a triangle and confirms its interior stays unpainted).
`/Strength` lerps toward paper-white rather than toward transparent —
deliberately not `setalpha`, for the same gs-portability reason
`lib/paintkit.ps`'s watercolor section documents (real Ghostscript has
no PostScript-callable alpha operator at all).

`examples/surfacekit.ps` is a six-panel specimen sheet (one per preset
plus a sixth demonstrating the exact-edge clip idiom); no gallery/site
entry, matching `hatchkit.ps`/`stipplekit.ps`'s own precedent that a
primitive gets a specimen, not a gallery card.

**Six review rounds, six real bugs, all in two families.** Three
Codex passes plus three same-family Claude-agent fallbacks (Codex
intermittently failed to produce output — confirmed a healthy,
authenticated runtime via `codex:setup`, so this was transient
resource contention on a machine running many concurrent Codex
sessions, not an auth or repo issue) converged on: (1) a caller-
supplied procedure passed as the region operand, or as `/Color`/
`/Length`, could auto-execute before its type was ever checked — every
option here now stays inside a private 1-element array until confirmed
safe, the same discipline `stipple`'s own `/DotRadius` fix already
established, just missed for these; (2) a `/Weight` callback closing
over and mutating the caller's own `/Scale` array (shared, not copied,
by a shallow dict `copy`) could corrupt a later mark's read of it,
after `scatter`'s own validation of the original values had already
run; (3) `weave`'s ceiling-based grid could drop an entire boundary
row/column for a non-exact pitch/bbox ratio, or draw nothing at all
for a region narrower than half a pitch — fixed by rounding to fit and
re-deriving per-axis spacing; (4)-(6) three separate numeric options
(`misreg`'s `/Rings`, `weave`'s `/Seed`, twice) ran `cvi` before
checking whether the value was even in a convertible range, so an
extreme but finite input (`1e300`) raised a raw `rangecheck` instead
of a self-documenting name — and `sfregiondef` validated a region's
`/Kind` but not its `/BBox`, so `weave` (the one preset that unpacks
`/BBox` directly) inherited the same auto-execution hazard as (1) one
level deeper. 40 tests total, most added specifically to reproduce
each finding before fixing it.

## Reusable halftone screens and misregistration offsets (issue #53, 2026-09-03)

An eighth sibling library, `lib/halftonekit.ps` — tag-migrated from
birth (like `hatchkit.ps`/`stipplekit.ps`, issues #49/#50), so it
registers with the `% @kind:`/`@summary:`/`@example:`/`@param:`
capability catalog automatically from `cargo build`, no hand-written
`capabilities.rs` entry. One operator, `halftone`: `opts halftone -`
fills whatever region is currently *clipped* with a regular dot, line,
or cross-line screen. Zero sibling dependencies — deliberately, not
by omission: `scatter` (issue #48) is a random retry-until-accepted
placement engine and a halftone is a deterministic lattice, so routing
a fixed grid through a random placer would add noise the medium is
defined by not having, plus a `/Seed` where none is wanted. This
operator makes no random draws at all: fixed options reproduce
identically with no stream to seed or restore (a /Tone callback that
calls `rand` itself still works, under the caller's own `srand`).

**The plan review (a subagent standing in for `advisor`, which this
runtime doesn't provide) changed the design before code existed.**
Six findings, all taken: (1) the planned `ht-` scratch prefix is owned
by artkit's hyperbolic-tiling driver (`htmax`, `httile`, 15+ names) —
my own collision check had only grepped `/ht-` with the hyphen and
missed them; the file uses `hf-` (verified clean) instead. (2) /Tone
callability is a local `hfcallable` predicate (executable array, not
bare `xcheck` — `xcheck` also admits `cvx`-marked strings and
executable names, stipplekit's round-3 lesson), with a same-file
precedent comment explaining why a bare executable name is rejected
rather than `load`ed. (3) /Offset and /BBox lengths pinned before
unpacking (hatchkit's round-3 `aload` lesson), element by element for
/Offset. (4) per-mark `newpath` — `gsave`/`grestore` alone restores
the caller's path at the end but accumulates one giant path within
the call, and consecutive line segments without it would stroke as
one joined polyline. (5) clamp-then-derive order explicit, since
`sqrt` of a negative is a domain error in both interpreters.
(6) the drawing loops reuse the *stored* counts/origins, never
recomputed — plus the note that integer `for` evaluates its limit
once, immunizing loop bounds against mid-loop /Tone redefinition.
Design calls settled: `/Frequency` in cells per inch (the issue's own
"frequency" vocabulary, print's lpi); ONE budget (`/MaxCells` — one
tone call and a fixed 1-or-2 marks per cell, so cells bound
everything; hatchkit's two budgets exist only because its per-line
sample counts vary); cross-in-one-call over a shared lattice origin
*and* caller-side layering, both (misregistration needs separate
calls anyway); always-`translate`, even for [0 0], so omitted and
zero take one path.

**Two real bugs, both caught by rendering rather than reasoning.**
(1) `/halftone { exch 1 array astore ...` — a two-operand `exch`
copied from `stipple`'s two-operand shape onto a one-operand
operator: every call died with `stackunderflow` before drawing
anything. (2) After fixing that, every screen rendered *blank*: the
screen normal (`hfnx`/`hfny`) and the lattice cell counts (also
`hfnx`/`hfny`) shared names, so the counts overwrote the normal and
the whole 161×161 lattice parked at (560, 540), far outside any
clip — decoded from one debug print of the first cell's coordinates
(20 + 20·27 = 560 gives it away once you see it). The counts are now
`hfncols`/`hfnrows`; the normal keeps `hfnx`/`hfny`.

**One behavior the tests first got wrong and Ghostscript settled:**
`/Screen (dot)` — a string, not a name — selects the dot screen,
because `eq` compares strings and names by content. Verified `true`
in `gs` itself before accepting it: the library follows the
language's equality rather than narrowing it, and the header
documents that instead of the original "including a non-name"
wording. A red second plate in the layering test similarly taught
that the test harness's own ink metric counts dark pixels only —
the plate is blue now.

**API surface:** `/BBox` (default `pathbbox`), `/Screen`
/dot|/line|/cross (default /dot; cross is two passes over one
lattice origin in a single call), `/Frequency` (default 9),
`/Angle` (default 45; per-layer rotation is per-call values),
`/Tone` number or `{x y -> w}` in pre-offset user space (default 1;
zero draws no mark — load-bearing, since a zero-width stroke is a
PostScript hairline, not nothing), `/MaxRadius` (dot branch only,
default half pitch) / `/MaxWidth` (line/cross only, default full
pitch — each validated only where read, merely harmless elsewhere,
stipplekit's lesson), `/Offset` (default [0 0]), `/MaxCells`
(default 200000). Dots scale by `sqrt` (area-proportional, the
print-correct curve — documented so nobody "simplifies" it away);
rules scale linearly; line segments run one full pitch with round
caps so neighbors join seamlessly at uniform tone. `halftone` never
sets the color — like `hatch`, it inks in the current color, and the
header documents the one-call-per-plate layering recipe.

**`tests/halftonekit.rs` (22 tests):** loads-clean, clip fill plus a
concave chevron, two-interp pixel-identical reproducibility (with a
/Tone callback in the loop), three-screens-distinct ink bands plus
ordering, offset-omitted-identical and offset-shifts-marks,
budget-rejects-before-ink, tone clamp (5≡1, −1≡blank),
pathbbox-default equivalence, malformed boxes, the full validation
error table, three never-execute side-effect tests (opts proc,
executable-name /Tone, /Screen proc), a leaking-proc stack test,
zero-tone line silence, harmless-unused-size-options, a two-plate
layering test, a four-panel inset-measured specimen test (the
full-box false-positive lesson from stipplekit's round 3 applied
from the start), and the standard `ghostscript_accepts_*` test.

**Specimen:** `examples/halftone.ps` — dot ramp, 45° line screen over
a circular clip, 30° cross screen, and a misregistered two-plate
spread (teal + red-orange, second plate shifted 5 over 3 up, second
call reusing the surviving path for its default /BBox). No
gallery/site entry, matching the reusable-primitive precedent. README
gains a paragraph mirroring hatchkit's (stipplekit skipped README;
a short entry is the better call for a user-visible print
capability — flagged here so the next sibling-file author can see
the choice was deliberate, not drift).

**Deferred:** interplay with surfacekit (#51, in progress elsewhere)
stays #52's problem — this file draws marks, not surfaces, and takes
no position on paper grain. No `/DensityThreshold`-style cutoff: a
zero tone already draws nothing, and a threshold knob is one more
name for no demonstrated caller.

**Implementation review (same subagent substitute) found eight more,
seven fixed.** (1) The nested loops re-read `hfncols` as the inner
limit per row — after /Tone runs — so a hostile callback redefining
it multiplied every remaining row past the budget the header claimed
was immune. Fixed with a single flat loop (limit fixed pre-sample)
plus kind/const/shape snapshots on the operand stack under the walk,
read back through fixed-depth `index`. (2) /BBox elements were never
type-validated: a 4-long string silently reinterpreted as byte
values. Now requires an array of four numbers (new
`halftone-bbox-must-be-an-array-of-four-numbers`), read element by
element through the box. (3) The contract said "`hf-` names" while
every scratch name is `hf` with no hyphen — read literally it
constrained nothing; fixed to "`hf`". (4) `hfscreenkind`/`hftoneconst`
were re-read per cell, so a kind-flipping /Tone reached the branch
whose knob was never validated — closed by the same stack snapshots.
(5) A /Tone failing mid-loop unwinds past `grestore`: documented as
fatal (hatchkit's own convention) rather than handled — the one
explicit disposition. (6) The /BBox length error itself left the
array stacked: subsumed by the box-read rewrite, pinned with a
`stopped` stack-cleanliness test. (7) The dot-band test couldn't see
a removed `sqrt` (19.6% sits inside the old band): added a
tone-0.25 ink floor (without sqrt it renders literally zero) plus an
ink-halves-with-tone ratio band, and a cross upper bound. (8) The
/Offset edge strip (tight box + shift = un-inked strip) is inherent
to translate-based offsets — one doc sentence, no code change.
**The snapshot fix introduced its own bug, caught by the suite:**
leaked /Tone operands shift every `index` read, failing deep under
an internal name — so the consume-both-leave-one-number contract is
now *enforced* per sample (`count`-based depth check plus a numeric
check, unwound through a `mark` so even these exits are
stack-clean), and the old leak-visibility test is a named-error
test. Verifying the check's own arithmetic needed a live trace, not
reasoning: E stays on the stack through `exec`, so the balanced
condition is M − E = 1, not 2.

**Independent (Codex) review found three more P2s, all fixed.**
(1) The flat pass-major walk sampled /Tone once per *trip* — twice
per cross cell, doubling side effects and splitting stateful arms.
The walk is now cell-major (one flat loop over cells, one sample
each) with an inner pass loop whose limit re-reads the npass stack
snapshot per cell — safe, because a stack value cannot be
redefined. (2) The per-sample contract errors unwound the operand
stack but skipped `grestore`, leaving a translated CTM for a
`stopped`-recovering caller — pinned by a test that draws after a
caught failure and diffs against a fresh interp (verified to fail
against the pre-fix library). Both contract exits now run counted
pops plus `grestore`. (3) A callback returning a lone `mark` passed
the count check and shadowed `cleartomark`, stranding the snapshots
— the mark sentinel is gone entirely, replaced by entry-depth
counted pops that no forged object can shadow (also pinned by a
test, same negative-control treatment). The `cross_with_a_callback_
samples_once_per_cell` test pins (1): 144 samples on a 12×12 box,
not 288. Single-pass panels re-render pixel-identical; the cross
panel churns ~3% of its ink pixels from paint reordering alone
(pass-major to cell-major overlap order under coverage blending) —
geometry unchanged, bands still green.

**Codex round 2 found three P2s and a P3, all fixed.** (1) Residual
lattice slack pooled at the far edge (full-tone marks stopped a
pitch short) — the origins are now centered like hatchkit's round-4
fix; pinned with dots, not rules (a rule's own round caps already
reach past an edge-anchored endpoint, so only the dot variant
discriminates — the first, rule-based version of the test passed
against pre-fix code and had to be rewritten). (2) An absurd
/Frequency (`10 30 exp`) `rangecheck`ed in `cvi` before /MaxCells
was checked — the budget now runs on the real quotient first (sound:
cvi(r)+1 always exceeds r, so a past-budget quotient means a
past-budget count). (3) An over-consuming /Tone (`clear`) destroyed
the stack slots the depth check itself read — every check now reads
only `count` and dict names (the pre-call depth lives in `hfN`,
which `clear` cannot remove), and cleanup is positional pops back
to the recorded entry depth plus `grestore`, so even that unwinds
both-stacks-clean. (4, P3) The specimen's "horizontal" ramp kept `y`
(`exch pop`) instead of `x` — now `{ pop 240 div }`, verified
numerically (left 3937 vs right 12764, top 7547 vs bottom 7565).

**Codex round 3 found two P2s, both fixed — one of them reverts a
round-2 decision.** (1) The round-2 centering made the lattice phase
a function of the sweep bounds: loosening /BBox (the documented
/Offset advice) silently re-registered every mark — e.g. at
frequency 12, angle 0, `[0 0 10 10]` centered at (2,2) while
`[-1 -1 11 11]` centered at (-1,-1) — contradicting the header's own
"a loose /BBox costs a few wasted cells, never wrong output". The
phase is now absolute: cells are drawn iff their centers (integer
multiples of the pitch from the user-space origin) fall in the
projected box ranges, so the box only selects multiples and never
moves marks. The far-edge test became the phase test:
`loosening_the_bbox_neither_moves_nor_adds_marks` renders a
single-cell tone field (1 near (6,6), 0 elsewhere) under a tight and
an inflated box and demands byte-identical pixmaps plus ink at
(6,6) — verified to fail against the centered library. Documented
consequence: a tight box keeps only cells whose centers it
contains, so full tone can stop a pitch short of the far edge
(fixed screens behave exactly so — the screen is fixed, the clip
cuts it). Counts are now exact integers (floor/ceiling of the
projected ranges), so the budget check on them is exact in both
directions — no over-eager limitcheck, no missed walk; the real
pre-guard survives with +2 slack (raw - n lies in [0,2), proved in
the comment) purely to keep absurd quotients answering
`halftone-maxcells-exceeded` instead of `rangecheck`. Side effect on
old pins: the 50×50 @12/45° box visits 12×11 = 132 cells now, not
12×12 = 144 (the normal projection [-35.4, 35.4] holds eleven
multiples; the relative scheme had forced symmetry) — both count
tests re-pinned, comments updated. (2) The positional cleanup popped
back to the entry depth, which pops nothing when the callback ate
the caller's own operands too (`42 ... /Tone { clear 0.5 }` lost
the 42 and kept the 0.5) — violating the documented exact-restore
guarantee the empty-stack test couldn't see. The entry stack is now
copied into an array box without consuming it (`count copy ...
array astore`, verified identical in gs), and `hfcleanup` is
`clear`, push the saved items back, `grestore` — exact restoration
even after `clear`, pinned by a pre-existing-operand variant of the
stack-eating test (also negative-controlled). `hfdepth0` is gone;
`hfsaved` is the one dict name the error path trusts (redefining it
corrupts the recovery, never the budget — tiered in the comment).

**Codex round 4 found two P2s, both fixed — plus one self-inflicted
bug caught by probing before any test existed.** (1) `hfcallable`
admitted only `arraytype`, so under `true setpacking` Ghostscript
packed every `{ ... }` /Tone into `packedarraytype` and the call
failed — paintkit's round-5 lesson, same fix (artkit's guard lists
the same callable shapes). pscat itself never produces
packedarraytype (its `packedarray` returns plain arrays), so the new
half is gs-observable only: a `setpacking` partial-coverage test on
this side plus an inline-driver gs test running all three screens
with callback tones under packing (inlined lib, no -dNOSAFER —
paintkit's harness shape), both negative-controlled where
controllable. (2) The round-3 per-axis raw guards fired on one
axis's raw size alone, rejecting zero-cell lattices (`[1 0 2 100]`
@9/Angle 0/MaxCells 10: zero columns, 13 raw rows) that draw
nothing — each raw guard now also requires the other axis to
provably hold a cell (raw >= 2 covers a full pitch), with the exact
boxed check still deciding everything the raw guards miss; the
soundness proof is in the comment. Pinned by a zero-cell no-op test
(also negative-controlled). The self-inflicted bug: the first
packed-accepting `hfcallable` stashed the typename in `/hftn` — but
`type` answers an *executable* name, so the bare `hftn` read
executed it (`arraytype` lookup → undefined). Same auto-execution
trap the array boxes exist for; the predicate is now an inline `1
index` shape with the trap documented in its comment. Truth table
probed directly in both interpreters (proc true; number, cvx-string,
executable name false; packed proc true in gs).

**Codex round 5 found two P2s, both confirmed real and both fixed.**
(1) Snapshot forgery: a *balanced* /Tone (`{ pop pop pop 100000 1 }`
eats x, y, and the npass snapshot, net -1) passed the count-only
check and drove 100000 pass trips on a one-cell screen — the
"immune by construction" claim was wrong; snapshots are
forgery-evident, not forgery-proof. Fixed by clamping the pass
limit to [1,2] at the loop (identity on the legit 1/2): a forged
100000 draws twice, a forged -5 draws once instead of zero trips.
Enumerated the other five slots to confirm the clamp is complete —
kind/const/cols/rows only reshape or misplace their own cell's
marks (tier-2), and the cell loop's limit was evaluated once
throughout, so pass trips were the only unbounded channel. The
header tiering, the walk comment, and the pass-loop comment now say
"bounded, not prevented". Pinned by a one-cell test asserting ink
for both the -5 (discriminating: blank pre-fix) and the verbatim
100000 shape (bounded completion, clean stack) — negative control
trips on the first half. (2) The round-4 raw guards used a
full-pitch-span proxy (raw >= 2) for "the other axis holds a cell",
so a narrow-but-nonempty axis beside an astronomic count skipped
both guards and died in `cvi` with `rangecheck` (`[0 0 100 3.6e-29]`
@1e30: one row, ~1e30 columns — confirmed in both interpreters).
The proxy is now exact emptiness in pure real arithmetic
(ceil(lo) > floor(hi); floor/ceiling never rangecheck), pinned by
the reviewer's example expecting `halftone-maxcells-exceeded`
(negative-controlled: `rangecheck` pre-fix). Also softened the
"counts never negative" note: float rounding around a straddled
multiple can push a small negative — still safe (zero trips, and
the budget check only fires upward).

**Codex round 6 never ran: three consecutive "Reviewer failed to
output a response" failures (service-side, no verdict on the
code).** Five successful rounds already covered the branch, and the
round-5 delta is small (pass clamp, exact emptiness, two tests,
comment corrections), so it got a line-by-line self-review instead:
the clamp is identity on legit values (bit-identical walks, no
render churn possible — suite confirms), the emptiness test is total
real arithmetic, and all four budget shapes (absurd, narrow-empty,
narrow-nonempty, over-budget product) are pinned. Proceeding to PR
on that basis; a post-merge Codex pass can still be requested if
the service recovers.

## Density-driven stippling and point-shading primitives (issue #50, 2026-08-31)

A seventh sibling library, `lib/stipplekit.ps` — tag-migrated from
birth (like `hatchkit.ps`, issue #49), so it registers with the
`% @kind:`/`@summary:`/`@example:`/`@param:` capability catalog
(issue #39/#94) automatically from `cargo build`, no hand-written
`capabilities.rs` entry. Unlike `hatchkit.ps`, it *does* depend on a
sibling: `@requires: (lib/artkit.ps) run`, the same declaration
`paintkit.ps` already uses for the same reason.

**One operator, `stipple`, a thin convenience layer over `scatter`
(issue #48) — not a second placement engine.** The issue's own
instruction ("build on the common placement conventions ... rather
than introducing a competing options format") is taken literally:
`stipple` takes the same `region` operand `scatter` does, and forwards
`/Count`, `/MinSpacing`, `/Seed`, `/Tries`, `/Budget`, `/Scale`,
`/Rotate` straight through unchanged. Reproducibility, the exact
min-spacing hash grid, and the attempt/deposit budgets are inherited
wholesale — no new arithmetic in territory PR #119 already spent eight
Codex review rounds hardening. `scplaced` (already public via
`artkit.ps`) reports a `stipple` call's count too; no duplicate
readback was added.

**`/Density`, extended, not replaced.** A plain number forwards
verbatim as `scatter`'s own `/Density` (uniform). A callback `{x y ->
w}` is read as a *relative* tone in [0,1] — the same convention
`hatchkit.ps`'s own `/Density` tone callback already uses — paired
with a required `/MaxDensity` (peak marks per unit area at a tone of
1). Internally `stipple` does no count arithmetic of its own: it hands
`scatter` `/MaxDensity` as its own `/Density` (which is what actually
drives `scatter`'s `Count = truncate(MaxDensity * area)`) and the
caller's callback, unwrapped, as `scatter`'s own `/Weight`. Two
mechanisms `scatter` already had, recombined — zero new placement
logic.

**The advisor caught a real correctness bug in the first design before
any of this was implemented.** The original plan defined the callback
as returning absolute marks-per-area (the same units as the constant
form) and computed `Count` from `truncate(MaxDensity * area)` while
using `d / MaxDensity` as the acceptance weight, on the claim that the
realized total would then track the field's own spatial integral. It
doesn't: `scatter`'s placement loop retries up to `/Tries` (default
20) times per candidate slot until one is accepted, so with any
non-tiny mean acceptance probability the failure rate per slot is
`(1-p̄)^20 ≈ 0` — the placed count converges on `Count` itself,
essentially independent of the field's average. A concrete check
(density 0.005 on the left half of a region, 0.01 on the right,
`/MaxDensity 0.01`) would have realized ≈400 under the integral
reading but ≈800 under the actual mechanism — confirmed once
implemented (`callable_density_total_tracks_the_peak_not_the_average`
in `tests/stipplekit.rs`). The fix taken is the smaller of the two the
advisor offered: drop the units claim, document the callback as
relative tone (matching `hatch`'s own convention), and state plainly
that **the realized total tracks the peak times the area, not the
field's own integral** — reshaping *where* marks land, not *how many*
land overall. A caller who genuinely needs total-tracks-the-integral
can call `scatter` directly with `/Tries 1`.

**A second, purely mechanical bug surfaced during manual smoke
testing, not code review: the auto-execution trap.** `/spdens
spscopts /Density get def` followed by a bare `spdens spisnum` looked
correct but crashed with a stack underflow on every callable-density
call — binding a name directly to a procedure makes every *bare*
reference to that name auto-execute it (the exact hazard
`scgetdef`/`hkgetdef`/`hdensityopt`'s 1-element-array wrapping exists
to avoid, documented in both `artkit.ps` and `hatchkit.ps` but easy to
reintroduce in a new file that doesn't reuse their code). Fixed by
wrapping `/Density`'s value in a 1-element array immediately and
always reading it back via `spdensbox 0 get`, mirroring `hatchkit.ps`'s
own `hdensityopt` exactly.

**A cross-model (Codex) review of PR #122 found the same hazard still
open for `/DotRadius` and `/MaxDensity`.** The first draft left them
bare-bound (`/spdotradius spopts /DotRadius 1.5 spgetdef def`, then a
bare `spdotradius spisnum` to type-check it) on the reasoning that
`hatchkit.ps`'s own `/Spacing`/`/Wobble`/`/Dropout` — genuinely
scalar-only options — use exactly that pattern. The reasoning doesn't
transfer: those options being scalar-only is convention, not something
`hkgetdef` enforces, so a caller who hands *any* proc-typed option a
procedure by mistake hits the identical auto-execution hazard there
too (unfixed, out of scope for this PR) — this file just happened to
get the fresh review that caught it. A malformed `/DotRadius {
pop /gotexecuted true def }` would `def`-bind the procedure, then the
very reference meant to type-check it would execute it instead,
running arbitrary caller-supplied code (or crashing with a raw
`stackunderflow` from the proc's own stack use) before the intended
`stipple-dotradius-must-be-a-number` error ever had a chance to fire.
Fixed the same way as `/Density`: both are now wrapped in a 1-element
array immediately after `spgetdef`/`get` and validated through that,
only bound to a bare name (`spdotradius`/`spmaxd`) once confirmed
numeric — `tests/stipplekit.rs`'s
`a_malformed_dotradius_procedure_never_gets_executed` and its
`/MaxDensity` counterpart pin the fix directly, by giving the
malformed procedure a side effect and asserting it never ran.

**A second Codex round on the same PR found three more, related
defects, all fixed the same way.** First, `stipple`'s own *options*
operand had the identical auto-execution hazard one level up: `/spopts
exch def` then a bare `spopts type` reference, so a caller passing a
bare procedure where the whole options dict belongs would get it
executed instead of rejected with `stipple-opts-must-be-a-dict` — now
wrapped exactly like every proc-typed key inside it. Second,
`/DotRadius` was validated unconditionally even when a caller supplied
their own `/Mark` — directly contradicting this file's own documented
"unused, harmless" claim for that combination; a malformed `/DotRadius`
sitting in a shared options dict alongside a real custom `/Mark` used
to fail a call that never touches `/DotRadius` at all. Fixed by only
resolving and validating it inside the branch that actually installs
the default mark. Third, a non-callable, non-numeric `/Density` (a
bare string, say) with no `/MaxDensity` given used to report
`stipple-maxdensity-required-when-density-is-callable` — wrong and
confusing, since the value was never trying to be a callback in the
first place; the documented contract (this file's own header) says it
should surface `scatter`'s own `scatter-weight-must-be-a-procedure`
instead, since it's forwarded as `/Weight` and never re-validated.
Fixed by gating the whole `/MaxDensity`-required branch on `xcheck` —
the same first check `sccallable` itself applies before it does any
name-chain resolution, so it can never disagree with what `scatter`'s
real validation would ultimately accept, without stipplekit
reimplementing that chain-following logic itself. One implementation
slip surfaced while fixing the first of these: an initial
`spscopts /Mark known { {} } { ... } ifelse` doesn't push a genuine
no-op — the *inner* `{}` is data, not an empty procedure body, so
executing the outer one pushed a stray empty procedure onto the
operand stack on every call with a caller-supplied `/Mark`, caught
immediately by the existing test suite (a `run()` helper here, like
`tests/hatchkit.rs`'s, asserts an empty stack after every call).

**A third Codex round found the fix for round 2's third finding was
itself wrong, plus two tests with false-positive coverage.** The
`/MaxDensity`-required gate had been changed from `xcheck` to... still
`xcheck` in spirit — the actual bug is that `xcheck` and `sccallable`
are not the same predicate. `xcheck` is true for anything with its
executable bit set, including a `cvx`-marked string or an undefined
name `cvx`'d into looking executable; `sccallable` correctly rejects
both after its own `xcheck` gate, via the type/name-chain checks that
follow it. Gating on `xcheck` alone let exactly those malformed values
reach the `/MaxDensity`-required branch and report the wrong error
instead of `scatter`'s own `scatter-weight-must-be-a-procedure`. Fixed
by calling `sccallable` itself — safe here, since it's pure and
`scatter`'s own loop hasn't started yet, so there's no `sc-` scratch
state alive to collide with — which by construction can never disagree
with what `scatter`'s real `/Weight` validation will do, closing the
gap for good rather than chasing another almost-equivalent predicate.

The same round also caught two tests that would have passed under a
broken implementation. `custom_mark_overrides_the_default_dot` only
checked that ink increased after a custom `/Mark`, which is also true
if `stipple` silently ignored the custom mark and drew its own default
circles instead — fixed by giving the custom mark a side effect (its
own running count) and asserting it equals `scplaced` exactly, which
only holds if the *custom* proc is what actually ran. The specimen
render test measured each full 240×240 panel, which includes the
panel's own drawn border — several hundred pixels on its own,
comfortably past the test's 500-pixel threshold even if a panel's
`stipple` call silently placed nothing — fixed by measuring a 10px-
inset interior instead, so a real-but-zero-ink regression can no
longer hide behind the frame.

**Two conflicts only `stipple` can catch; one it deliberately doesn't
duplicate.** An explicit `/Weight` alongside a callable `/Density` is
rejected by `stipple` itself
(`stipple-weight-and-callable-density-are-mutually-exclusive`) —
`scatter` has no way to see this conflict, since `stipple` would
otherwise silently overwrite the caller's own `/Weight` rather than
erroring. `/MaxDensity` missing when `/Density` is callable is the
same shape (`stipple-maxdensity-required-when-density-is-callable`).
By contrast, `/Count` given alongside a callable `/Density` is *not*
separately checked — after `stipple`'s substitution the private
options copy ends up with both `/Count` (the caller's) and `/Density`
(the synthesized peak), which trips `scatter`'s own
`scatter-count-and-density-are-mutually-exclusive` for free. Same
reasoning for a non-callable, non-numeric `/Density` (e.g. a bare
string): forwarded straight to `scatter` as `/Weight`, which surfaces
`scatter-weight-must-be-a-procedure` — `stipple` never reimplements
`sccallable`'s own (nontrivial, name-chain-resolving) validation.

**Default `/Mark` is a filled circle, `newpath`ed first.** A region
built via `scpath` deliberately leaves the flattened region path
behind (`artkit.ps`'s own documented contract), so a mark that skipped
`newpath` would fill that leftover outline on its first invocation
only — `fill`'s own implicit `newpath` clears it for every mark after,
making this a one-shot, easy-to-miss corruption if not caught early
(caught during manual smoke testing before it reached the test suite).
Radius is `/DotRadius * scale`, so "dot-size variation" is `scatter`'s
existing `/Scale [lo hi]` range, not a new option. `/DotRadius`
(default 1.5) is simply unused, not an error, when a caller supplies
their own `/Mark` — the override path a genuine point-shading use
(crosses, ticks, glyphs) needs.

**Specimen sheet:** `examples/stippling.ps`, three 240x240 panels
mirroring `hatching.ps`'s own layout — constant density, a
callback-driven left-to-right sparse-to-dense tonal ramp, and
point-shading with a custom rotated-cross `/Mark` sized by `/Scale`.
No gallery/site entry, matching `hatchkit.ps`'s own precedent: a
reusable primitive gets an `examples/` specimen, not a gallery card.

**`sp-` scratch prefix**, distinct from `scatter`'s `sc-`/`sq-`/`si-`
and `hatchkit.ps`'s `h-` (checked for collisions against every
sibling library before picking it).

## Reusable hatching and cross-hatching primitives (issue #49, 2026-08-30)

A sixth sibling library, `lib/hatchkit.ps` — no dependency on
`artkit.ps` or any other sibling, matching `graph.ps`/`dataviz.ps`/
`etching.ps`'s precedent. One operator, `hatch`: fills whatever region
is currently *clipped* with a family of parallel line strokes. The
caller supplies the region (an ordinary `<path> clip`) and, optionally,
a tone-driving `/Density` callback; image analysis and tone extraction
stay `lib/etching.ps`'s job, per the issue's own scope cut.

**Lean on the real `clip`, don't reimplement polygon math.**
`et-hatch` already proved the technique: sweep parallel lines across a
bbox-sized area and let the graphics state's own clip cut them to
shape. `hatch` generalizes that geometry — lines are drawn well past
the region's actual boundary, so it clips to concave and
self-intersecting paths exactly as well as convex ones, with no
point-in-polygon or edge-crossing code anywhere in the file. This also
sidesteps `clippath`'s known multi-clip bug (issue #120) entirely —
`hatch` never calls `clippath`; `/BBox` defaults to `pathbbox` of the
*current path* instead (which survives `clip`, since `clip` doesn't
consume the path).

**Bounding candidate work, not just marks — twice.** `scatter`'s own
NOTES entry (issue #48) records the lesson this reuses: a deposit
budget on *marks placed* doesn't bound *work done* when each candidate
costs more than O(1) to test. Here that shows up as two independent,
fully-deterministic-from-`/BBox`/`/Spacing`/`/Angles` pre-flight
checks, both computed and enforced *before* any drawing or RNG draw:
`/MaxLines` (total candidate lines across every angle) and
`/MaxSamples` (total `/Density` callback invocations, gated only when
`/Density` is given, bounded per-line by an exact projection formula —
`|dx|*w + |dy|*h`, not the ~2x-larger bbox diagonal a cruder estimate
would use). A first draft only had `/MaxLines`; an advisor review
before implementation caught that a small `/Spacing` over a modest
bbox already drives the sample count into the tens of millions —
exactly the "bounds marks, not work" gap scatter's own history warns
about — which is what actually makes "density callbacks cannot create
unbounded output" (the issue's own acceptance criterion) hold.

**A line's own clipped-in-region span, not the raw sweep, is what
`/Trim` shortens.** An early draft trimmed a fraction of the full
bbox-diagonal sweep length, which the same advisor review flagged as
badly conditioned — a fixed fraction of an ~850-unit diagonal is
invisible against a small centered shape and total against one in a
bbox corner. Each candidate line is instead clipped analytically
against the bbox first (a small unrolled Liang-Barsky, four boundary
tests — by construction every offset this file sweeps already
intersects the box, proven by convexity of the projection onto the
sweep normal, so the general-purpose "parallel and outside" rejection
branch is a defensive backstop, never a load-bearing path), and
`/Trim`'s fraction applies to *that* real span.

**Two real implementation bugs, caught by actually rendering, not by
reasoning about the PostScript.** (1) `hkclipseg` was first called
with an initial `t` range of `[0, 1]` — mimicking a unit-length probe
— instead of a range wide enough to contain the whole bbox
intersection; Liang-Barsky can only *shrink* a given range, never grow
it, so every stroke silently clipped down to length ≤ 1 (rendered as a
diagonal chain of dots, not lines) until the initial range became
`[-diagonal, diagonal]`. (2) `/Density`'s value was stored under a
bare name (`/hdensity`) rather than wrapped in the 1-element-array
trick every other option-default helper in this codebase already uses
for exactly this reason (`scgetdef`/`pkgetdef`/`pggetdef`, and this
file's own `hkgetdef`) — a name bound directly to a *procedure*
auto-executes on every bare reference, so each `hdensity null ne` /
`hdensity xcheck` / `hsx hsy hdensity exec` was silently re-running the
caller's own density callback mid-setup instead of testing or invoking
it deliberately, corrupting the operand stack (`stackunderflow` at
`pop`, sourced from inside the *caller's* proc, not `hatch`'s own
code — a genuinely confusing symptom to trace back). Wrapping it as
`hdensityopt`, retrieved via `hdensityopt 0 get`, fixed it; the same
footgun this file's own `hkgetdef` exists to avoid, applied
inconsistently to a second name in the same file.

**API surface:** `/BBox`, `/Angle` or `/Angles` (each a full layered
pass, in order — the mechanism behind cross-hatching and multi-angle
engraving fills), `/Spacing`, `/Width` (number or `[lo hi]` range),
`/Wobble` (seeded perpendicular offset — position only, not a
mid-stroke jitter; a genuinely shaky hand-drawn stroke stays
`paintkit.ps`'s territory), `/Dropout`, `/Trim`, `/Density` +
`/DensityThreshold` (quantized into 6 fixed width buckets, one stroke
per constant-bucket run along a line — `et-hatch`'s own technique,
reused for the same reason: stroke count, not sample count, dominates
render time), `/Seed` (srand + rrand-restore, `scatter`'s convention),
`/MaxLines`, `/MaxSamples`. `hatch` brackets its own drawing pass in
`gsave`/`grestore` — an advisor review before the PR caught that
every drawing branch calls `setlinewidth` with no restore, which would
otherwise silently overwrite the caller's own line width with no way
back (`et-draw` already brackets its own two `et-hatch` passes this
way). That fix has a second effect worth stating: it also protects the
current path, so a *second*, layered `hatch` call over the same clip
can keep relying on the default `/BBox` (`pathbbox` of the current
path) — an earlier draft needed every layered call in
`examples/hatching.ps`'s cross-hatch panel to pass `/BBox` explicitly,
since `hatch`'s own strokes used to end with `stroke`'s ordinary
implicit `newpath`, leaving nothing for a second call's default to
read; that workaround is gone now that the path survives.

**Tag-migrated from the start, not added to `build.rs`'s
`LEGACY_FILES`.** `lib/paintkit.ps` is still the only *pre-existing*
file migrated to the `% @kind:`/`@summary:`/`@example:`/`@param:`
doc-comment catalog (issue #94) — migrating the rest is itchy-when-
you-get-to-it follow-up work per `HANDOFF.md`. A brand-new file has no
migration debt to defer, though: `build.rs`'s own docs frame
`LEGACY_FILES` as distinguishing "deliberately uncataloged" from
"forgotten" for files that predate the mechanism, not as a default for
new ones, so `hatchkit.ps` tags every top-level definition (`@internal`
for scratch helpers, a full block for `hatch` itself) and gets
capability-catalog registration (issue #39) for free from
`cargo build` — no hand-written `capabilities.rs` entry needed.

`tests/hatchkit.rs`: reproducibility (identical pixels, same seed and
options), a two-angle `/Angles` pass inking more than one angle alone,
dropout measurably reducing ink, a concave (chevron) clip leaving its
own bbox corners blank, `/Density` carving a hard region boundary and
clamping an out-of-range return value instead of erroring, `/BBox`
defaulting to `pathbbox` matching an explicit box pixel-for-pixel, and
both safety limits rejecting *before* any ink lands — plus the
`ghostscript_accepts_*` acceptance test every sibling library carries.
Every `run()` call also asserts an empty operand stack afterward, not
just a separate `--lint` pass — the same review that caught the
`setlinewidth` leak flagged that none of the 16 original tests would
have noticed a `/Density` proc leaking an operand (`--lint`'s own
issue-#17 history already found two such leaks elsewhere in this
codebase); `density_proc_that_leaks_an_operand_is_visible_on_the_stack`
confirms the assertion actually fires rather than just existing. The
same review also caught `/Dropout`'s roll firing unconditionally even
at `/Dropout 0` — unlike `/Wobble`/`/Trim`, which were already guarded
— silently consuming a random draw from the caller's ambient stream on
every plain `hatch` call with no `/Seed`; now guarded the same way.

**A Codex review of the PR (this issue's own #121) found three more —
real bypasses of the documented safety limits, none caught by the 20
tests above at the time.** (1) `/Trim`'s two fractions were never
validated: an out-of-range or inverted pair (`/Trim [-100 0.1]`) could
sample a *negative* trim fraction, which lengthens a line's span
instead of shortening it — sampling well past what `/MaxSamples`'s
pre-flight estimate ever accounted for, since that estimate is only
sound on the assumption Trim can shrink a span, never grow it. Now
validated up front (`hatch-trim-must-be-ordered-fractions-in-0-1`),
closing the gap regardless of `/MaxSamples`. (2) `/Angles` read the
caller's own array by reference, not a copy, and was read *twice* —
once by the pre-flight budget, once by the drawing pass — with a
`/Density` callback (caller-supplied code, called in between, mid-
drawing-pass) able to mutate a not-yet-swept angle after the budget
was already computed from its original value: a static `/Angles
[0 45]` call correctly rejects against a tight `/MaxLines`, but the
equivalent live-array version — start at `[0 0]` (budgeted low),
mutate the second entry to `45` from inside `/Density` before that
pass draws — silently swept the un-budgeted 45 anyway. Fixed by taking
a private array copy immediately after reading `/Angles`, before the
budget is computed, so nothing the caller's own code does afterward
can change what gets swept or how it was budgeted. (3) The sweep
normal was computed independently via `cos`/`sin(angle+90)` rather
than derived from the already-computed direction vector — two separate
floating-point trig evaluations of *different* input angles are not
guaranteed exactly orthogonal, and for a region thinner than
`/Spacing` at a plain axis-aligned angle, that sub-ulp slack could
place the sole candidate offset just outside the box's true
projection, so `hkclipseg` rejected it and the pass silently drew
nothing. Fixed by deriving the normal algebraically as `(-hdy, hdx)`
in both the pre-flight and drawing loops, which is exact relative to
the already-computed `(hdx, hdy)` regardless of floating-point trig
rounding. All three now have regression tests in `tests/hatchkit.rs`;
fixing (1)'s validation itself needed a second pass after a
self-introduced bug (a boolean `or` chain missing one combinator for
five terms, caught immediately by testing the expression standalone
rather than trusting it against the fix).

**A second Codex review of the updated diff found two more, both the
same underlying shape.** `hspacing`/`hstep`/`htrimlo`/`htrimhi` were
internal working state that gates a loop bound — the exact kind of
name the file's own docs claimed was protected by the `hk-` scratch
prefix — but were never actually `hk`-prefixed. A `/Density` callback
redefining the *unprefixed* `/hstep` mid-call (`/hstep 0.1 def`, well
outside the documented `[0.25, spacing/2]` range) reads back in a
later line's own sampling loop, since each line constructs its `for`
loop fresh rather than capturing the value once — one 50×50 `/BBox`,
`/MaxSamples 15` reproduction ran the callback 1005 times, not 15.
`/Trim`'s validated bounds had the identical exposure for the same
reason: the validation only runs once, so corrupting the same-named
variables it validated reintroduces the negative-span-growth bug
issue #49's *first* Codex round already closed for malformed *input*.
Fixed by renaming all four to `hkspacing`/`hkstep`/`hktrimlo`/
`hktrimhi`, which brings them under the contract the docs already
state (and were, in every other name's case, already accurate about)
— not a new mechanism, just closing a gap between what the docs
claimed and what the code actually named. Every *other* `h`-prefixed
working-state name (geometry, width, tone) only affects rendering
correctness if corrupted, not how much work gets done, so left as-is;
`hatchkit.ps`'s "Scratch prefix" section now says this explicitly
rather than the earlier, inaccurate blanket "hk- throughout" claim.
The same review's second finding — `/Wobble`, `/Dropout`, and
`/DensityThreshold` silently accepting out-of-contract values (a
negative `/DensityThreshold` making even a density-0 sample count as
ink, the exact `le`-at-threshold behavior the design notes above
specifically call out getting right for the *documented* range) —
got the same validate-up-front treatment `/Spacing`/`/Trim` already
had. Both rounds' fixes are covered by dedicated regression tests
(`tests/hatchkit.rs`) that reproduce the exact clobber/malformed-input
shape a docs-only reading wouldn't have caught.

**A third Codex review found two more real bugs — neither adversarial,
both hit by an ordinary caller — plus a third finding that's the same
naming class round 2 already closed, restated against different
names, and deliberately not fixed this time.**

The one that mattered most: PostScript's own `for` loop, used for both
the line sweep and the per-line density sampling, accumulates its step
by repeated floating-point addition — `kmin spacing kmax { ... } for`
— which does not always take the same number of trips as
`cvi((kmax-kmin)/spacing)+1`, the formula the pre-flight budget uses to
approve that same work. A 12-sample estimate saw a real 13th
`/Density` call; no callback involved, just an ordinary `/BBox`/
`/Spacing` combination landing on a case where the two computations
disagreed. Fixed by making both loops integer-indexed —
`0 1 n-1 { /i exch def kmin i spacing mul add ... } for` — so the real
trip count *equals* the pre-flight formula by construction rather than
merely agreeing with it in the common case; confirmed directly
(printing both loops' own computed line count for the same non-trivial
`/BBox`/`/Angle`/`/Spacing`, matching exactly) rather than trusted from
the reasoning alone, and the specimen sheet was re-rendered to check
the sub-ulp coordinate change (deriving each line from `kmin + i*spacing`
instead of accumulated addition) didn't visibly shift anything.

The second: `hkfrnd` can return exactly `1.0`, which a bare
`hkfrnd hdropout lt` turns into "never dropped" even at a
documented-certain `/Dropout` of 1 — `lib/artkit.ps`'s `scodds` already
names and guards against this exact trap for the same reason; the
dropout roll now mirrors its pattern (`>= 1` and `<= 0` both skip the
roll entirely, matching `/Wobble`/`/Trim`'s existing convention of not
consuming a random draw for a degenerate range). A third, unrelated bug
in the same round: `/BBox` accepted any array `aload pop` could unpack,
silently reading an oversized array's *last* four elements as
coordinates and leaving the rest sitting on the operand stack —
violating `hatch`'s own `opts hatch -` contract. Now validated to be
exactly four elements.

The finding *not* acted on: `hbx0`/`hbx1`/etc. (the bbox bounds) and
`hangles` (the angles array binding) aren't `hk`-prefixed either, the
same shape as round 2's `hspacing`/`hstep` finding. This class doesn't
converge by renaming — `clobbering_hstep_from_density_...`
(`tests/hatchkit.rs`) already proves a callback redefining the
*already-`hk`-prefixed* `hkstep` directly still bypasses the cap, since
PostScript has no mechanism that would stop it regardless of which
name is targeted. `lib/artkit.ps`'s `scatter` (issue #48, cross-model
reviewed in its own right) ships with the identical exposure and
documents it as a plain contract: `/Mark`/`/Weight` "must not touch
sc-, sq-, or si- names." `hatchkit.ps`'s own "Scratch prefix" section
already states the equivalent contract over every `h`-prefixed name,
with the `hk`-prefixed subset called out as the part that also gates a
safety limit — this finding doesn't change that, it's the same
documented risk restated against names the round-2 rename didn't
happen to cover. Not a new exposure this PR introduced.

**A fourth Codex review found two more floating-point robustness
bugs, both non-adversarial — an ordinary `/BBox`/`/Angle`/`/Spacing`
combination, no callback involved — and both fixed.** (1) The
pre-flight budget computed its corner projections *raw* (uncentered),
while the drawing loop centered them (subtracting the bbox center's
own projection) before round 4 — mathematically the same difference,
but raw and centered subtraction round differently in floating point
for a large-magnitude `/BBox` far from the origin, so the two loops'
own line counts could actually disagree: one repro passed `/MaxLines
1`/`/MaxSamples 1` at pre-flight while the drawing loop computed two
candidates and called `/Density` twice — the exact "two computations
expected to agree" trap round 3's integer-loop fix closed for
`for`-loop trip counts, recurring one level up in the corner-
projection math that feeds those loops. Fixed by using the identical
centered-projection formula in both loops, closing the gap by
construction rather than by argument (this is also what round 3's own
sanity check — "run one config through both and check they agree" —
was checking for, and the config it happened to use didn't surface
this one; round 4's repro used far-from-origin coordinates
specifically). (2) A region thinner than `/Spacing` places its sole
candidate line at its own swept range's boundary (`hkmin`), tangent to
the bbox; at a near-axis-aligned angle, floating-point roundoff could
make `hkclipseg` reject that exact tangent intersection, silently
drawing nothing for a region that geometrically should get one line —
a plain 0-degree hatch over the same region drew fine, only a
near-0.0004-degree tilt triggered it. Fixed by centering the whole
candidate distribution within `[hkmin, hkmax]` (splitting the leftover
slack — the span rarely divides evenly by `/Spacing` — across both
ends instead of anchoring flush at `hkmin`), which also fixes the
general case, not just the single-candidate one: every candidate now
sits a little inside the box rather than the first one always
grazing its edge. Both fixes are covered by regression tests
(`tests/hatchkit.rs`) using the review's own repro parameters; the
specimen sheet was re-rendered and re-eyeballed after each (the
"layered cross-hatch" panel's grid shifts by up to half a spacing unit
at its seams from the centering change — cosmetic, not a defect).

Four review rounds, nine fixed findings, one explicit disposition.
Round 5 was not run: rounds 3 and 4 both surfaced genuine,
non-adversarial floating-point edge cases worth fixing, but the
returns are visibly narrowing (round 4's two findings needed
far-from-origin coordinates and a ten-thousandth-of-a-degree tilt to
surface), and the remaining exposure class (documented-contract
scratch-name collisions) doesn't converge by further review — see the
round-3 disposition above. `hatchkit.ps`'s own geometry now computes
every safety-critical count and coordinate exactly once per shape
(centered projections, integer-indexed loops, centered candidate
placement) rather than through two paths expected to agree, which is
the actual property that closes this whole class of finding, not
another round of chasing individual repros.
Deliberately cut, and recorded rather than silently skipped: no
gallery piece or site/playground entry — the issue's own acceptance
criteria ask for a "specimen page," not a gallery piece, and
`examples/hatching.ps` (three panels: flat shading, a `/Density`-driven
tonal band that reads as a curved, lit sphere from perfectly straight
strokes, and layered cross-hatching) covers that.

## Deterministic scatter and distribution primitives for artkit (issue #48, 2026-08-29)

The area-shaped counterpart to `alongpath`/`walkpath`: place a
caller-supplied mark many times *across a region* instead of stringing
it along a curve. Five public names in a new `lib/artkit.ps` section —
`screct`/`scpath` build a region, `scin`/`scarea` interrogate one, and
`scatter` places marks in it.

**Regions are objects, not arguments.** `screct` takes a rectangle;
`scpath` captures the current path — flattened and implicitly closed
exactly as `fill` sees it, with the path left behind flattened
(`alongpath`'s own contract), so `<shape> scpath ... stroke` can draw
the region's outline afterwards. `clippath scpath` covers the issue's
"clipping to an arbitrary current path" reading with no separate
mechanism. Containment is a real crossing test over the captured
edges, not a bounding-box approximation and not rasterizer clipping:
candidates outside the shape are *rejected* rather than drawn and
clipped, so `/Density` resolves against the shape's own area and the
deposit budget isn't spent on invisible marks. `scin` answers under
the nonzero winding rule by default (matching `fill`); a region's
`/Rule` can be set to `/evenodd` (matching `eofill`), which is a
genuine choice about what "inside" means for a donut rather than a
detail to hard-code.

**Placement.** `/Count` or `/Density` (mutually exclusive — checked by
key *presence*, since a `known`-less check would see the `/Count`
default and reject every `/Density` call), a `/Weight` procedure for
non-uniform distributions, `/Scale` and `/Rotate` ranges handed to the
mark, `/MinSpacing`, `/Seed`, `/Tries`, `/Budget`. The mark is called
`x y scale angle`, and the count actually placed lands in the global
`scplaced` rather than on the operand stack — a returned count is a
`--lint` operand-leak trap waiting for the first caller who forgets to
`pop` it.

**Minimum spacing is exact, and cheap.** Dart-throwing against every
placed mark is O(n²); instead each accepted mark goes into a sparse
hash grid held in an ordinary PostScript dict, with cells of
`MinSpacing/1.5` so a cell's diagonal (0.943·MinSpacing) can hold at
most one mark and no per-cell capacity case arises. A candidate checks
the 5×5 cell neighborhood, which provably covers the whole
MinSpacing disc. Memory tracks the number of marks placed, not the
region's size.

**`/Seed` restores the stream it borrowed.** `rrand`/`srand`
round-trips exactly in this interpreter *and* in Ghostscript (pinned
by hand in both before the option was written), so a seeded scatter is
reproducible regardless of what drew before it and doesn't perturb
what draws after it — a bare `srand` would have made `/Seed` a hidden
global side effect. Under `--sweep-seed` the sweep overrides the
restore too, which is what a sweep is for.

**Three scratch prefixes, deliberately.** `sc-` (scatter's loop), `sq-`
(region capture), `si-` (containment). The natural way to write a
non-uniform scatter is a `/Weight` proc that calls `scin` — which runs
*inside* scatter's own placement loop, so a shared prefix would have
corrupted the loop's bounds or options partway through. Caught in plan
review before any code existed; `scatter_weight_may_itself_call_scin`
pins it. A `/Mark` or `/Weight` that calls `scatter` again is the
`gasket`/`carpet` nesting case: the library stays unwrapped, the
caller wraps.

**Bounds.** Every option is range- and type-checked before a single
mark is drawn (paintkit's precedent: fail on the blank page, not
halfway through one), a resolved count over `/Budget` is rejected,
total work is bounded by `Count × Tries` with both capped, and
`scpath` refuses a path past 20000 flattened edges — `scin` is linear
in the edge count, so an unbounded path would make every candidate
arbitrarily expensive.

**Two defects a cross-model (Codex) review of PR #119 caught, both
real.** First, `scarea` originally used the cheap formula — the
absolute value of the summed signed shoelace terms — which is *not*
the area `scin` accepts, in three separable ways: two disjoint
contours wound oppositely cancel to zero (and then trip `/Density`'s
own positive-area guard on a perfectly good region), nested
same-winding contours report outer+inner rather than the solid outer
one, and nothing about the formula responds to `/Rule` at all even
though the even-odd reading of a donut is a genuinely different area.
Replaced with scanline integration under the region's own rule —
asking the same containment question `scin` does, a row at a time —
which gets every case right by construction at the cost of exactness
on shapes whose vertices don't line up with slab boundaries (a
fraction of a percent; a rectangle region stays exact, and a stored
`/Area` short-circuits the measurement for a caller who needs one).
Second, an explicitly closed subpath was closed *twice* — `pathforall`
reports the `closepath`, and `scpath` also closes whatever is left
open at the end — appending a zero-length duplicate edge per closed
subpath: geometrically inert (it can't cross a scanline) but it
inflated `/Edges` and would have tripped the 20000-edge ceiling one
edge early. Fixing it surfaced a third case worth handling: a `lineto`
*after* a `closepath` legitimately starts a new subpath at the
closepath's own point, so the capture reopens rather than silently
dropping it.

**A second review round found the first fix's own two holes.** The
replacement measurement sampled a fixed 400 evenly spaced scanlines,
which can step straight over a component thinner than one step — a
1x1000 sliver beside a disjoint 1000x1 one reported half the region,
and `/Density` would have underplaced it by half. And its per-scanline
insertion sort is quadratic in the crossings, so a zigzag whose every
edge spans the bounding box could run for minutes at the 20000-edge
ceiling. Both are fixed by the same change of footing: slabs are now
bounded by the *edges' own vertex heights* rather than by a fixed
count, which no component can fall between and which is additionally
*exact* (covered width is linear in y between consecutive vertex
heights, so midpoint times height integrates it exactly — a
self-intersecting path is off only by a sliver at each crossing
height); and the measurement counts its own work against a budget
(`sqabudget`), raising `scarea-region-too-complex-to-measure` rather
than grinding. The ceiling admits roughly 1400 flattened edges — a
seven-letter word set at 72pt and captured with `charpath` is about
330, and the gallery piece's ridge is 304 — and a region past it can
still carry its own `/Area` or be scattered by `/Count`, which needs
no area at all.

**Round three found three more, all real.** (1) "Exact between vertex
heights" holds only while nothing *crosses* between them: a bow tie —
`(0,0) (100,100) (0,100) (100,0)` — has vertex heights of only 0 and
100, and the single slab's midpoint lands exactly on the crossing at
y=50 where the covered width is zero, so a region of 5000 measured as
0 and `/Density` would have rejected it as empty. Slabs are now
integrated *adaptively*: sample the midpoint and both quarter points,
and if the midpoint isn't the average of the quarters, something bends
in this slab — halve it and try again, depth-first through an explicit
stack (`gasket`/`carpet`'s precedent). Linear slabs pass immediately
and cost three scanlines instead of one; a bend costs one subdivision
per crossing height and then converges, so the bow tie comes out at
exactly 5000 under both rules. (2) The deposit budget bounds *marks*,
which is not the same as bounding *work* — one candidate against a
path region costs a pass over its edges, so a perfectly legal `/Count
200000 /Tries 100` over a 20000-edge region is twenty million
candidates at twenty thousand edge tests apiece, and no deposit budget
touches it. Containment work is now metered as it is spent, against
`scworkmax`. (3) `scplaced` was a plain `def`, so a caller who wrapped
the call in the ordinary `N dict begin ... end` — exactly what a
`grid` or `truchet` stamp does — got the count written into their own
scratch dict and thrown away with it. It now reads out of a
`ScatterState` dict (`TurtleState`'s precedent), so it survives any
dict scoping while the spelling at the call site is unchanged.

**Round four replaced the round-three fix's own criterion.** The
adaptive subdivision test — subdivide when a slab's midpoint width
misses the average of its quarter widths — is a *sample* of
linearity, and a region can be built whose three samples line up
across a genuine crossing: a six-vertex even-odd polygon read that way
measured 4000 against an exact 43025/14 ≈ 3073.21, so `/Density` would
have overplaced it by a third. Sampling was replaced with finding: the
slab boundaries now include every height at which two edges actually
cross, computed by testing each edge pair, alongside the vertex
heights. Inside a slab where no edge begins, ends, or crosses another,
the span structure is fixed and every span's width is linear, so a
plain midpoint is exact — for *any* polygon, self-intersecting ones
included, with no adaptive machinery at all. It is also cheaper on the
ordinary crossing-free paths that make up nearly every real region:
one scanline per slab instead of three, against a one-time pass over
the edge pairs. The same round also caught that a scatter nested
inside a `/Mark` reset the shared published count (an outer `/Count 3`
finished reporting 2 and numbered its marks 1, 3, 3); the running
total is now a local republished on each placement, so a nested call
gets its own counter and can't renumber its caller's marks.

**Round five, two more real ones and a language-level limit.** A bare
`moveto` draws nothing — `fill` skips such a subpath entirely — but
the capture was stretching the region's bbox around it, so one stray
`1e6 1e6 moveto` appended to a 100x100 square made scatter sample a
million-unit box and place none of the marks asked for; bounds now
come from edges, and a zero-length closing edge isn't emitted at all
(which also subsumes round two's double-close more directly). And many
edge pairs can cross at the *same* height, each claiming another slot
in the boundary array: nine stacked copies of one bow tie are 36 edges
with hundreds of pairwise crossings at two heights, and the
measurement rejected that trivial region as too complex — coincident
boundaries are now merged. The third finding, that `/Seed`'s restore
is skipped when the call errors out under a caller's `stopped`, is
real and documented rather than fixed: PostScript has no finally, and
buying the restore back means swallowing the error and re-raising it
as a bare `stop`, losing the self-documenting name that makes these
errors worth reading — the same shape of leak `gsave` has when an
error skips its `grestore`. The header names the caller's own
two-line workaround.

**Round six sharpened two of round five's fixes and found a third
hole.** A subpath of one line segment is retraced by its own implicit
close, so `fill` paints nothing for it — but it *has* edges, and
edge-derived bounds included it, which is round five's bug with a
harder input. A subpath now reaches the region's bbox only if it can
enclose something: fewer than three edges cannot (two edges is always
an out-and-back retrace), and neither can a subpath box with no width
or no height — both provable exclusions rather than heuristics.
Merging coincident slab boundaries with an epsilon scaled to the
*region's* bbox can also exceed a whole component's height when the
components' scales differ wildly (a 1x1e8 sliver beside a 1e10x0.01
one), so the merge test is relative to the boundaries' own magnitude
instead. And `xcheck` alone turned out to be too weak a guard for the
callbacks: `3 cvx` is executable, and invoking it merely pushes 3, so
an accepted `/Mark 3 cvx` would have reported placements while leaking
five operands per mark — a callback must now be a procedure or an
executable name for one.

**Round seven closed the same bbox hole's last shape, plus two
smaller ones.** A remote *diagonal* run of three collinear points has
three edges and a non-degenerate box, so neither earlier exclusion
caught it, yet it encloses nothing; the test is now collinearity,
which catches it and still keeps a bow tie (signed area zero, filled
area real). `nametype` alone also said nothing about what an
executable callback name is *bound* to — `/M3 3 def` then `/Mark /M3
cvx` passed validation and leaked five operands per placement, and an
undefined name failed only mid-placement after the seed and the random
stream had moved — so a name is resolved and its target checked. And
`/Budget` was compared against the raw resolved value rather than the
truncation actually placed, rejecting a density of 1.5 against a
budget of 1 for a call that places exactly one mark.

**Round eight caught a real cross-interpreter break.** Ghostscript
packs literal procedures under `true setpacking` — `{ ... }` is
`packedarraytype` there where this interpreter leaves it
`arraytype` — so round seven's callback guard, which insisted on
`arraytype`, would have rejected an ordinary `/Mark { ... }` in gs and
nowhere else. `sccallable` now accepts a procedure, a packed
procedure, or an operator, and resolves a chain of executable names
with a depth cap so a name bound to itself is refused rather than
chased (paintkit had already hit the packing difference; its own
`packedarraytype` checks are the precedent). The gs driver in
`tests/artkit.rs` now runs a packed-procedure scatter directly, since
that break can only appear in the interpreter that packs. The round
also found the collinearity tolerance too coarse: scaled to the
subpath's own extent, it called a genuinely triangular 100-by-1e-11
sliver collinear, emptied its bbox and piled every mark on the origin.
It is now a few ulps of the cross product's own terms, forgiving only
the rounding error in computing it — and the test for that runs under
an anisotropic CTM, since at identity `flattenpath`'s own coordinate
quantization makes such a sliver genuinely flat before scpath ever
sees it.

**Round nine completed rounds seven and eight.** Those excluded an
unfillable subpath from the *bounds* but left its segments in
`/Edges`, so the measurement still integrated them. An out-and-back
pair should cancel — both crossings land on the same x at every
height — but only exactly, and at a coordinate like 1e9 the two
computed crossings differ in their last bits, leaving a hair of width
across an enormous span: a 10x10 square plus `0 0 moveto 1e9 1e9
lineto` measured 159.6 instead of 100. A rejected subpath's edges are
now rolled back out of the buffer too, which makes `/Edges` mean
exactly what it should — the segments that can affect the fill — so
`scin`, the edge ceiling, and `scarea` all agree about what the region
is.

**Round ten: two tolerance constants tightened, and a boundary
found.** Both findings named tolerances looser than the rounding error
they exist to forgive — the collinearity test at 1e-14 and the
boundary merge at 5e-13, against a double's own 2.2e-16 — so both were
tightened to 1e-15, about four ulps. Neither reported failure actually
reproduces in this interpreter, though, and the reason is worth
recording: `flattenpath` quantizes coordinates, and at the magnitudes
those cases use it quantizes the very deviation being tested away
before any PostScript library can see the path. A triangle
`(0,0) (1e8,1e8) (2e8, 2e8+1e-6)` arrives with its apex at exactly
`(2e8, 2e8)` — measured, printed straight out of `pathforall` — so it
*is* collinear by the time `scpath` runs, and `fill` paints nothing
for it either. That is the floor on what any of these region
predicates can distinguish, and it sits well above the arithmetic;
the tightened constants are correct on their own merits rather than
because they fix an observable bug.

**One finding dispositioned rather than fixed.** The same round noted
that `clippath scpath` doesn't capture the true clip when several
clips are nested — correctly, but the cause is this interpreter's
`clippath`, which returns the most recently established clip path
instead of the intersection of all of them (Ghostscript returns the
intersection; both measured). That's a pre-existing pscat divergence
from the PLRM, not something this issue introduced, and fixing it
means path intersection in the renderer — filed separately (#120).
The scatter docs now scope the idiom honestly instead of implying more
than `clippath` delivers.

**Deliberately not built:** true Poisson-disk (Bridson) sampling —
dart-throwing with a spacing grid is the placement primitive this
issue asked for, not a sampler with a guaranteed fill quality; density
*fields* as first-class objects, since `/Weight` plus `noise2` already
composes into one (issue #19 was explicitly not a blocker); and any
particle simulation. `alongpath` is untouched.

**gs, and a second divergence that isn't `rand`'s doing.** The section
runs unchanged in Ghostscript, but *placements* don't match: gs's
`rand` is a different generator, so a seeded scatter is reproducible
within each interpreter, not across the two. The gs driver in
`tests/artkit.rs` therefore checks the count contract rather than
pixel parity. Counts and areas *do* agree exactly — except for curved
regions, and that one is `flattenpath`, not `rand`: its tolerance is a
fixed fraction of a **device** pixel (HANDOFF's documented deviation
from `setflat`), so the chord count follows the CTM. The same ridge
measured 304 chords at 1x and 372 at 2x in this interpreter, and 232
in gs — so a curved region's `/Area`, and any count `/Density`
resolves from it, differ both across scales and across interpreters.
The consequence worth stating plainly, because it isn't obvious: a
boundary that moves by sub-chord amounts changes *which candidates get
rejected*, which shifts the whole random-draw sequence after it. A
seeded scatter over a curved region is reproducible at a given scale,
not across scales. A straight-edged region has no such dependence —
exact and identical at every scale in both interpreters — and
`scpath_chord_resolution_follows_the_ctm_for_curves_only` pins the
relationship both ways. Found by cross-model plan review at the
implementation-review stage, before the PR, rather than empirically
after.

**Demos.** `examples/scatter.ps` is a six-panel specimen (fixed count,
one density over two region sizes, a `noise2` weight field, minimum
spacing, a star as the region, one seed reproduced by two separate
calls). `gallery/firefly_census.ps` is a night meadow in which every
mark on the page is scattered and none placed by hand: a star field
weighted by a Milky Way band times coherent noise, hill stipple
scattered into the silhouette's own `scpath` outline, min-spaced grass
whose height and tone come from each blade's own y, and fireflies
drawn as two passes over one seed so every core lands inside its own
halo.

## Watercolor: setalpha/setblendmode + paintkit's pkwash/pkpaper (issue #47, 2026-08-28)

Implements the architecture issue #46's spike recorded in
`docs/WATERCOLOR.md` — Approach B, a small renderer-level alpha
extension — and the artist-facing medium on top of it.

**Renderer.** `GraphicsState` gains `alpha: f32` (promoted from the
spike's `pub(crate)` prototype) and `blend: BlendMode`, a two-variant
pscat enum (`Normal`/`Multiply`). Four new operators: `setalpha`/
`currentalpha` and `setblendmode`/`currentblendmode`. They are
**pscat extensions, not PLRM operators** — the spike established
directly that gs 10.x has no PostScript-callable alpha operator at all.
Both ride `gsave`/`grestore` for free and reset with `initgraphics`;
alpha clamps like the color operators, an unknown blend name is a
`rangecheck` rather than a silent fall-back.

Reach: `fill`, `stroke`, shown text (`fill_path_direct`), and `shfill`
(whose gradient needs `Shader::apply_opacity` — `paint()`'s color alpha
can't reach a shader). **Not** `image`/`imagemask`, which blit samples
straight into the pixmap; that gap is documented at the field, at the
operator, in README, and in the tests rather than quietly left.

Export, both required before merge per the spike's own scope cut:
`--svg` emits `fill-opacity`/`stroke-opacity` on the painted element and
`style="mix-blend-mode:multiply"` on the *outermost clip wrapper* — a
non-`none` `clip-path` establishes a stacking context, so a blend
declared inside the group composites against transparent black and
renders plain source-over. Confirmed in Chrome rather than reasoned
about: the same clipped-Multiply scene gives pscat's own rgb(51,92,46)
with the group placement and the unblended rgb(51,102,230) with the
element placement. `pkwash` paints its bloom and grain inside a `clip`,
so this is the default path, not an edge case. `--pdf` carries an `ExtGState`
registry deduped by content with a per-page reference list (the same
shape the image XObject machinery already had), inline in each page's
`/Resources`. Both emit *nothing* at the defaults, so a program that
never touches the operators exports byte-identical SVG and PDF —
asserted directly, not assumed.

**The gs-verification question the spike left open for this issue** is
answered by `tests/pdf.rs`: PDF-path verification substitutes for the
`ghostscript_accepts_*` pattern for alpha-bearing content, and is
strictly stronger — it rasterizes our PDF *with gs* and block-compares
against our own canvas, so it asserts gs's own transparency lands on the
same pixels tiny-skia did, not merely that gs doesn't error. Both plain
alpha and `/Multiply` pass inside the existing tolerance.

**Library.** `lib/paintkit.ps` gains `pkwash` (the wash: /Alpha,
/Layers, /Wet, /Bloom + /BloomWidth, /Grain, /Blend, /Pitch, /Seed) and
`pkpaper` (the ground: /Tone, /Grain, /Alpha, /Depth, /Fiber, /Blend,
/Seed). The wet boundary is a pink-spectrum harmonic ladder at *integer*
multiples of each subpath's normalized progress — seamless on a closed
path — displaced along the local normal, plus a small per-layer
translation that leaves the crescents of single-layer coverage a glazed
wash actually shows. Edge pooling is a stroke of that boundary clipped
to the wash, so only its inward half survives. All of it vector; no
diffusion solver, no raster pass.

Two deliberate departures from the sibling presets, both recorded at the
section header: randomness comes from the section's own Schrage-
decomposed LCG rather than `rand`, so (a) `/Seed` reproduces one wash
without moving the caller's stream *and* without the `--sweep-seed`
breakage an `rrand`/`srand` save-restore would cause (issue #21's
override intercepts every `srand`, so the "restore" would reset the
caller's stream on every wash), and (b) every intermediate product stays
inside 32-bit integer range, so the texture is identical under
Ghostscript's 32-bit ints.

**The Ghostscript fallback**, and what it is not. Two names, not one:
`pwhasalpha` (internal, immutable) is the load-time probe of
`systemdict /setalpha known`, and `pkalphaok` is the documented dial
set from it. They only diverge when someone sets the dial false by hand
to preview the fallback in pscat — and that divergence is load-bearing,
because everything that neutralizes ambient compositing keys off the
*probe*: gs has no `setalpha` for an ambient value to leak out of, so a
preview that inherited one wouldn't be a preview of gs. Without alpha,
each mark is painted in its flattened-over-white equivalent
(`1-(1-c)*a`), accumulated across layers so the build-up survives, and a
one-line diagnostic prints the first time it engages. A `gs file.ps` run therefore renders a legible,
opaque version of any watercolor program instead of erroring. What it
provably cannot do is let anything underneath show through — a wash over
`pkpaper`'s ground, or two overlapping washes, goes flat. That is
asserted as a test in its own right so the limitation stays documented
rather than discovered. This substitutes for the spike's nominated
portable fallback (Approach A's nested `clip` technique), which cannot
degrade automatically: it needs 2ᴺ hand-ordered region fills and a blend
color guessed per pair.

`/Blend /Multiply` answers the wash-order question the spike explicitly
handed to this issue: source-over is order-dependent (honest watercolor,
and documented as such), and Multiply is the commutative alternative.
Both are shown side by side, in both orders, in the specimen.

Surfaces: `examples/paintkit_wash_demo.ps` (a six-row specimen ramping
each control) and `gallery/first_rain.ps`, a river valley in layered
washes — wired into all three of `gallery/show.sh`'s parallel arrays,
`gallery/README.md`, and `site/gallery.html`, with its committed
2× supersampled still. `pkwash`/`pkpaper`/`pkalphaok` register with the
#39/#94 capability catalog automatically from their `% @...` tags.

Deliberately out of scope: separate fill/stroke alpha (PDF's `ca`/`CA`
split — one `setalpha` drives both), soft masks, blend modes beyond
Normal/Multiply, any raster post-pass (the spike's rejected Approach C),
a diffusion solver, and alpha on `image`/`imagemask`. `pkpaper` is a
watercolor ground, not a general surface library — issue #51's
`surfacekit` is the right home for paper/canvas/print surfaces in
general and should supersede it rather than duplicate it.

One incidental finding, filed here rather than fixed: `build.rs`'s
tag scanner models the depth-0 stack with a two-slot window, so
`/pkalphaok systemdict /setalpha known def` reads `/setalpha` as the
name being defined. Same family as issue #104's open parser gaps; the
definition is written probe-first (`... /pkalphaok exch def`) to stay
inside what the scanner understands, with a comment saying why.

## Mochi in Denim Blue — pkoil gallery portrait (issue #100, 2026-08-27)

Closes the gallery deliverable deferred from issue #45 with a
reference-based oil portrait rather than another parameter specimen.
`gallery/mochi_denim_blue.ps` builds a golden Pomeranian from broad,
overlapping `pkoil` planes, then uses narrower ridges for coat direction,
`pkdry` for broken fan-brush contour edges, and crisp conventional fills
only at the eyes and nose where likeness needs focus. The denim-blue
ground echoes the source photograph but turns its folds into cross-woven
canvas strokes. Seed 27 fixes every paint deposition.

The exact 3000×4000 source photograph is checked in at
`gallery/references/mochi.jpg`; `gallery/README.md` and the Pages gallery
compare it explicitly with the interpretation, naming both the preserved
identity anchors and the deliberate simplifications. The 1440×1800
supersampled still is committed at `gallery/renders/mochi_denim_blue.png`,
registered with all three `gallery/show.sh` arrays, and copied to Pages by
the existing render wildcard. `scripts/build_site.sh` now also publishes
gallery reference assets under `assets/references/`; the portrait remains
out of the wasm playground because `paintkit.ps` is a filesystem-loaded
dependency, not a self-contained program.

Deliberately deferred: no attempt to trace the photograph, synthesize
individual hairs, or claim physical oil/pigment simulation. This is a
stylized vector impasto study whose recognizability comes from selected
proportions, markings, and focal detail.

## Doc-comment-driven capabilities catalog (issue #94, 2026-08-25)

Closes issue #94, the first of #92's `docs/PS_LIBRARY_COUPLING.md`
("Touchpoint 1") follow-ups: replaces `src/capabilities.rs`'s
hand-maintained `Entry` rows with ones generated at build time from
new `% @...` doc-comment tags in `lib/*.ps` source, for files that
opt in.

Tag prefix is `% @tag:` (e.g. `% @summary:`), not `%%` -- this repo's
existing DSC comments already feed PDF `/Info` (`%%Title:`/`%%For:`)
and Ghostscript's own DSC parser reads `%%`, so a new `%%`-prefixed tag
risked colliding with both; `%!` was also ruled out as too close to
the `%!PS-Adobe-3.0` interpreter shebang. `% @tag:` can't collide with
either. Six tags: `@kind`/`@summary`/`@example`/`@param` (0+) per
top-level definition, `@internal` as the alternative to that set for a
deliberately private helper, and `@requires` once per file for the
prerequisite `run` chain (replacing `capabilities.rs`'s hardcoded
`load_sequence` match arm per source file).

`build.rs` (new) reads `lib/*.ps` from disk at build time -- a normal
build-script filesystem read, not a wasm-runtime one -- and
code-generates fully-resolved Rust (`GeneratedEntry` rows, one
`*_INTERNAL` const per migrated file) into
`$OUT_DIR/capabilities_generated.rs`, `include!`'d into
`capabilities.rs`. No `include_str!` needed: by the time the wasm
target compiles, the generated file is already literal string data,
same as any other `static`. Verified directly with `cargo build --lib
--target wasm32-unknown-unknown` (0 errors, pre-existing unrelated
`font.rs` warnings only).

A file is "migrated" iff it contains at least one `% @...` tag
anywhere; every `lib/*.ps` file `build.rs` discovers (`lib/*.ps` plus
one level into `lib/styles/`; `lib/fonts/` stays a separate,
live-enumerated mechanism) must be either migrated -- then every
top-level definition needs `@internal` or a full tag set, enforced
strictly -- or listed in `build.rs`'s own `LEGACY_FILES` allowlist,
with the two states cross-checked against each other. A file in
neither bucket, including a brand-new untouched one, fails `cargo
build` immediately -- closes the "new sibling file silently
uncataloged" gap `docs/PS_LIBRARY_COUPLING.md` calls mandatory, since
nothing today catches a wholly-new `lib/*.ps` file that isn't one of
the six kinds `tests/capabilities.rs` already has a dedicated test
for. Also enforced at build time: an unrecognized `@word` (a typo)
fails the build rather than silently dropping data, and `@kind: Font`
is explicitly rejected (fonts are enumerated live from
`font::catalog_entries`, never tag-driven).

That's the *file*-level guarantee -- a migrated or newly-tagged file
gets noticed. Per-*name* coverage within a migrated file still rests
on `build.rs`'s tokenizer being right, with no independent ground
truth of its own; closed at the test level instead, generically, by
`tests/capabilities.rs`'s new `every_migrated_file_names_match_the_
catalog_exactly` (below) rather than a new hand-written per-file test
each time a file migrates.

Migration scope this issue: `lib/paintkit.ps` only (8 public
procedures, 19 internal helpers, all `CapabilityKind::Procedure` --
matches the coupling doc's own worked example, `pkoil`). Every other
`lib/*.ps` file stays on the hand-written `ENTRIES`/`*_INTERNAL` path,
unchanged -- explicitly allowed by the issue's acceptance criteria
("a defined migration subset, if staged"). `Palette` entries are
deferred for a structural reason, not deferred effort: they're
`Palettes /name [...] put` dict-literal mutations, not `/name ... def`
bindings, so they need a different discovery mechanism in `build.rs`
than the depth-tracked `def`-scanner this issue built (which does
already handle `Template`/`Dial` generically, alongside `Procedure`,
for whenever `lib/pagekit.ps`/a style pack migrates next).

Parameters come from a new structured `@param: /Name text (default D)`
tag, not by parsing the existing free-text `/Key description (default
D)` legend blocks the coupling doc's own touchpoint-1 finding
describes as "wraps across lines, contains cross-references and
caveats mid-sentence" -- a deliberate deviation from the doc's literal
suggestion ("keep parsing `/Key ... (default D)`"), lower-risk since
it avoids a prose parser and the doc's own required-tag list doesn't
mandate reusing that exact convention. The old itemized legend blocks
in `lib/paintkit.ps` were trimmed (not just added to) to avoid two
sources of truth for the same defaults, keeping the genuinely
non-derivable behavioral caveats (validation strictness, degenerate
-case fallbacks) as flowing prose pointing at the `@param` tags for the
actual values -- matching the coupling doc's own "replacing, not
adding to" framing for what a migrated file's line count should do.

Added two tests to `tests/capabilities.rs`, replacing the old
per-file `paintkit_names_match_the_catalog_exactly` (deleted -- fully
subsumed, see below): `every_migrated_file_names_match_the_catalog_
exactly` runs the same forward/reverse `Interp`-vs-catalog name
cross-check generically over `capabilities::migrated_files()` (a new
`build.rs`-generated list of `(source, @requires chain, internal
names)`, one entry per migrated file), so migrating the *next*
`lib/*.ps` file gets this protection automatically -- no new
hand-written per-file test function to remember, closing the
name-level half of the "new file forgotten" gap the same way
`build.rs`'s own `LEGACY_FILES` check closes the file-level half.
`generated_paintkit_entries_have_the_right_fields` checks specific
field *values* directly (kind/load/example/param defaults, including
a `/Density` case with a parenthesized aside before its trailing
`(default ...)`, to pin that `parse_param`'s `rfind` picks the right
one) -- the generic name check alone wouldn't catch a parser bug that
gets a value wrong while still producing the right name set.

Codex review on PR #97 caught eight real defects across two rounds,
all fixed before merge. Round 1: `@kind: Type3Face` was accepted but
unreachable (a Type 3 face binds with `/Name Dict definefont pop`, not
`/name ... def`, so a tag above one is never seen) -- now explicitly
rejected, same as `@kind: Font`, until `definefont` discovery is
added; `@summary:`/`@example:`/`@param:` with an empty value (after
trim) satisfied the required-tag check while producing an unusable
entry -- now rejected; a stale hand-written `ENTRIES` row left behind
after a file migrates would silently duplicate a catalog row past
every `BTreeSet`-based cross-check -- closed by a new
`catalog_has_no_duplicate_entries` test.

Round 2, all in `find_top_level_defs` (the tag-block/def-name
tokenizer) and its consumers, needed a genuine redesign, not another
patch -- two rounds of "make the heuristic pick the right literal"
each broke on a different real line before landing on the actual fix:
the original "last name literal before `def` wins" mis-cataloged
`/spmetal /brass def` (a Dial bound to another name literal, e.g.
`lib/styles/steampunk.ps`) as `brass`; the round-1 fix, "first name
wins, ignore later ones," then mis-cataloged the *next* definition
when an unrelated bare-token statement intervened -- that same file's
`Palettes /brass [...] put` immediately before `/spmetal /brass def`
left a stale `/brass` the "first name" rule never released, so it
named that definition `brass` too. The actual fix treats `def` as
popping two objects off a virtual depth-0 stack (`key value def`) and
tracks a 2-slot sliding window over *every* depth-0 token -- opaque
ones (`Palettes`, `put`, numbers, closed bracket groups) included, not
just `/name` literals -- so unrelated statements correctly flush stale
candidates instead of leaving them to leak into the next `def`.
Verified against both real `lib/styles/steampunk.ps` lines together,
in sequence, with a standalone tokenizer probe (plus `bind def`, which
this file's own doc comments already claimed to support but nothing
exercised -- confirmed working too). Same round: `@kind: Palette` was
also accepted but unreachable, same shape as the `Type3Face` gap
(`Palettes /name [...] put` is a dict mutation, not a `def`) -- now
rejected the same way; the duplicate-row test's `(name, kind, source)`
key let a stale row under a *different* kind than its generated
replacement slip through as "distinct" -- narrowed to `(name, source)`,
since one PS name in one source can't legitimately have two kinds; and
`parse_param` validated its whole `@param` value was non-empty but not
the two halves *after* splitting off `(default ...)` -- `/Width
(default 6)` (empty description) and `/Width text (default )` (empty
default) both produced silently-accepted malformed rows -- now
rejected.

**Round 3 caught the sharpest gap of the three: a tag block could be
silently dropped in its entirety, with no error at all.** Every check
up to this point validated tags *once they'd been attached* to a
discovered `/name ... def` binding -- but `collect_tag_block` only
ever looks upward from a definition `find_top_level_defs` actually
found. A tag block sitting above a binding shape that scanner doesn't
discover (the exact `% @kind: Palette` above `Palettes /foo [...]
put` case round 2's `@kind: Palette` rejection was meant to catch) is
never reached by that walk at all -- `parse_kind`'s rejection panic
never fires, because nothing ever calls it for that block. The tag
text just sits there, inert, and the entry it documents never gets
generated. Fixed by tracking every non-`@requires` tag line found
anywhere in a migrated file against the set actually consumed by some
discovered binding's tag block, and failing the build on any leftover
-- reattaches the existing kind-rejection panics to *every* misplaced
or undiscoverable tag, not just the ones lucky enough to sit above a
`def`. Verified against the exact scenario Codex described: an
`% @kind: Palette` block placed above a real `Palettes /faux [...]
put` line in an already-migrated file now fails the build with a
clear "not attached to any discovered binding" message, restoring
cleanly once removed.

Verified empirically, not just asserted: a probe file with one
untagged `/name { } def` and no `LEGACY_FILES` entry fails `cargo
build` with a clear message; dropping `@example` from `pkoil` fails
the build the same way; a typo'd `@exemple` tag fails the build;
`@kind: Font` fails the build. All four restore to a clean build
immediately after.

Deferred, not solved here: migrating the rest of `lib/*.ps`
(`artkit.ps`/`pagekit.ps`/the four style packs/`handscript.ps`/
`hangul.ps`) -- the mechanism is generic enough to handle
`Template`/`Dial` today, `Palette` needs new discovery logic first.
The other two `docs/PS_LIBRARY_COUPLING.md` follow-ups (PS-native
self-check convention, CI diff-shape detection) are untouched,
independent of this one per the issue's own "Dependencies" section.

## Reducing PS-library-only coupling to Rust/CI/Ghostscript (issue #92, 2026-08-25)

Closes issue #92: a time-boxed architecture spike into the three
Rust/CI/Ghostscript coupling points a new `lib/*.ps` primitive touches
today, recorded as a decision document, `docs/PS_LIBRARY_COUPLING.md`.
Same shape as #46's spike: recommends architectures for three
follow-up implementation issues (a doc-comment-driven capabilities
catalog, a PS-native `%%SelfTest`/`--selftest` verification path, and
CI diff-shape detection), implements none of them.

The most consequential finding reverses the spike's own first-pass
assumption: `ci.yml` has no explicit Ghostscript install step, which
first looked like proof `gs`-dependent checks silently skip on CI.
Checking real CI logs (`gh run view --log` on PR #86) showed the
opposite -- macOS GHA runner images ship `gs` preinstalled, and every
`ghostscript_accepts_*`/`golden`/`corpus` test genuinely runs on every
PR today. Caught before it was written down as a headline claim, not
after.

Classified all 18 real defects found across all seven rounds of Codex
review on PR #76 (issue #41, `pkribbon` -- read in full via `gh pr
view 76 --comments`, not just round 1) against what would actually
catch each one: 10 of 18 need no new interpreter work at all
(PostScript's existing `stopped`/`errordict`, or `--lint`'s existing
blank-page heuristic), verified directly by running a `stopped`-wrapped
malformed-input call against real `lib/paintkit.ps` on a locally built
release binary and confirming it discriminates (catches the bad call,
doesn't fire on a well-formed one); a further one closes by
construction once the capabilities-catalog follow-up lands. Five need
a new one-time pixel-sample operator, scoped as a follow-up's "Phase
B." The remaining two stay uncovered by anything proposed -- one
(`pathforall`'s missing implicit moveto after `closepath`) a genuine
interpreter bug only Rust/gs-parity testing caught, direct evidence
for keeping `tests/golden.rs`/`tests/corpus.rs` exactly as-is per the
issue's own acceptance criteria; the other (a demo missing `showpage`)
a real but low-severity gap whose cheap automated fix turned out to
false-positive on two already-committed demos, caught in a second
round of cross-model review on this very PR and withdrawn rather than
shipped.

Worked example: `pkoil` (issue #45, PR #86)'s real diff was 467 lines,
335 PS / 132 Rust (82 in `src/capabilities.rs`, 50 in
`tests/paintkit.rs`) against a ≈106s CI job dominated by `cargo test`
(≈58s) more than `clippy`+`fmt` (≈7s combined, measured from the same
run's per-step timings) -- the 132 Rust lines are what the three
follow-ups target; the PS side's core logic doesn't shrink, but isn't
perfectly flat either, since the projected mechanism adds new
`%%Summary:`/`%%Requires:`/`%%Example:`/`%%SelfTest` content (a real,
unsized-here cost the write-up initially left out, caught in review --
see `docs/PS_LIBRARY_COUPLING.md`'s worked-example section for the
detail rather than trusting this summary, which has itself needed
correcting more than once).

## Watercolor rendering architecture spike (issue #46, 2026-08-25)

Closes issue #46: a time-boxed architecture spike comparing three ways
to get watercolor-like transparency, pooling, and bloom out of pscat,
recorded as a decision document, `docs/WATERCOLOR.md`, with matched
rendered samples of the same three-circle Venn scene
(`docs/watercolor_prototypes/common_gesture.ps`) under each approach.

Recommends Approach B -- a small renderer-level alpha extension -- as
the primary mechanism for #47, with Approach A's technique (nested
`clip` intersection for exact overlap-region recoloring, no boolean-
geometry library needed) kept available as a portable fallback for
small hand-composed scenes, and Approach C (an external raster post-
pass) explicitly not built as a standalone pipeline. The write-up
covers all six of the issue's "questions to settle" and sketches the
public contract #47 should build against (an alpha field on
`GraphicsState`, SVG `fill-opacity`/PDF `ExtGState ca`/`CA` export,
at least `/Multiply` as a blend mode) -- posted to #47 as a comment
per this issue's own acceptance criterion.

Two findings only showed up by actually running the prototypes, not
by reasoning about the architecture in the abstract: gs 10.07.1 has no
PostScript-callable alpha operator at all (`.setfillconstantalpha`,
`.setopacityalpha`, `.setstrokeconstantalpha`, checked directly and
recorded in `docs/watercolor_prototypes/gs_alpha_check.ps`) -- a
watercolor medium built on Approach B would be the first paintkit-
adjacent feature that doesn't render under plain `gs file.ps`, unlike
every paintkit preset so far (#41-#44's `ghostscript_accepts_*`
tests). And a raster post-pass (Approach C) over an *already-opaque*
render cannot recover pigment-pooling -- there was never any
transparency information in the flattened PNG for a filter to work
with -- which pushes a real Option-C implementation toward exporting
separate alpha-bearing layers for external compositing, a materially
bigger undertaking than "pipe the render through a blur."

Approach B's prototype (`gfx::tests::watercolor_prototype_b_alpha_sample`
in `src/gfx.rs`) deliberately does not add a public operator: #47's
own acceptance criteria assign the public contract to #47, and a
PNG-only `setalpha` merged now would make `--svg`/`--pdf` silently
diverge the moment a program used it (the same bug class NOTES.md
already records fixing for stroke/PDF under issue #8). Instead the new
`alpha: f32` field on `GraphicsState` is `pub(crate)` -- reachable
from nowhere in the PostScript language, snapshotted by `gsave`/
`grestore` for free like every other paint attribute since it lives on
the state struct they already clone -- and exercised only by a single
`#[ignore]`d unit test that drives `Gfx` directly and writes
`docs/watercolor_prototypes/approach_b_alpha_ext.png` on demand
(`cargo test --release watercolor_prototype_b_alpha_sample --
--ignored`). `cargo test` skips it by default; `cargo build`/`clippy`/
`fmt` cover it like any other code in the crate.

Deliberately out of scope, per the issue's own scope note: no fluid/
diffusion PDE simulation, no watercolor library or preset (#47's
actual implementation work), no site/gallery wiring, no SVG/PDF alpha
export (sketched as a #47 requirement, not built here).

## A spray-paint deposition brush (issue #44, 2026-08-24)

Closes issue #44, the fourth of the painterly-brush series (#42-#53):
`lib/paintkit.ps`'s `pkspray`, seeded opaque particles deposited around
each sampled centerline stop under a radial falloff, with an optional
overspray mist past the nozzle edge and optional trigger-dwell bursts
pooling particles at each subpath's ends. The one preset in the file
*not* built on `pkribbon` -- spray is discrete particle deposition, not
an offset band -- but the same walkpath-driven shape as everything else
here (issue #40's sampler, #41's opts conventions, #43's safety-limit
doctrine).

Emission uses a **cumulative accumulator**, not a fixed count per stop:
each stop adds `Density * sp / (2*Nozzle)` to a running total, deposits
the integer part, and keeps the fraction. Total particles therefore
track arc length -- about `Density` per nozzle-diameter of travel --
regardless of `/Pitch`, the same portability contract pkdry's
per-Width rate scaling gives its Markov rates (and pinned by a
dedicated pitch-independence test, which a buggy fixed-per-stop
implementation would fail while passing everything else). The
accumulator resets at each subpath's first stop, so subpaths stay
independent marks; the guaranteed-final-stop duplicate (sp==0
coincidence case) then naturally deposits nothing extra at a closed
subpath's seam.

Radial falloff avoids `pow`/`exp` entirely: the particle's radius is
`Nozzle * (minimum of m fresh frnd draws)`, `m = 1 + truncate(2*
Falloff)`. Min-of-m-uniforms has radial density ∝ (1 - r/Nozzle)^(m-1)
-- three discrete levels, deliberately not interpolated (the jump at
Falloff 0.5 is visible but predictable, and keeping m integral keeps
the per-particle draw count, and therefore the deposit budget, honest).
One wording subtlety the plan review caught: m=1 is uniform *in
radius*, not a uniform disc (areal density still rises mildly toward
the center) -- the header says so.

Two bugs the test suite caught before any review did, both in the
burst/dab plumbing:
- The end-burst test failed with *identical* ink counts for burst and
  no-burst: `pzendburst pzdensity 2 mul truncate` computes
  `Density*2` and leaves `EndBurst` stranded on the stack, so
  `pzburstcluster` read the burst strength as the base radius, the
  nozzle-scaled radius as a y coordinate, drew the cluster in the
  wrong place, and leaked an operand. The dab block only worked
  because `Density*2` involves no second factor. Fixed by multiplying
  through: `pzendburst pzdensity mul 2 mul truncate`.
- The demo's first draft sprayed the star stencil with a single thin
  line that only grazed the star's bottom tip -- visually invisible
  despite "working." Caught by looking at the rendered page, not by
  any test (the same lesson as #43's dab fix: render the actual demo
  and look at it). Several passes cover the shape's extent now, same
  as the charpath stencil row.

The deposit-budget estimate includes the degenerate-dab count
explicitly (a review finding on the plan, before implementation): a
bare moveto reports sp=0, so its `truncate(Density*2)`-particle dab is
invisible to the accumulated-emission term, and `/Density` is
uncapped -- without the explicit term, a page of movetos with huge
`/Density` slips arbitrarily many deposits past the limit. There's a
regression test for exactly that bypass. The check itself runs inside
the counting callback (every stop adds ≥1 spare, so a pathological
fine `/Pitch` rejects within ~budget-many callbacks), same placement
argument as pkdry's. Overspray's escape roll is clamped at both ends
(`pzroll`, mirroring `pbroll`): `frnd` can return exactly 1.0, which
would make `/Overspray 1` never escape with a bare `frnd rate lt`.

Scratch prefix `pz-` (`pzpdot`/`pzburstcluster`/`pzroll`),
deliberately distinct from pk-/pn-/pb- per the file-header doctrine.
All particles of a call batch into one path and fill once; per-particle
draw order (overspray roll, radial position, angle, size) is pinned
and documented, since changing `/Falloff` changes the draw count and
therefore reshuffles the whole random stream. Cataloged in
`src/capabilities.rs` (`pkspray` + the three `pz-` internals in
`PAINTKIT_INTERNAL`); tested against real Ghostscript both through a
synthetic driver and through `examples/paintkit_spray_demo.ps` itself,
same pattern as #42/#43.

Deliberately out of scope, matching the issue's own scope note: no
fluid dynamics, raster convolution, or aerosol physics; no per-particle
color variation (color is not a key, same doctrine as every preset
here); no closed-subpath ring special-casing (discrete dots don't
care about closure); no site/gallery wiring (the demo surface for
library presets is `examples/*_demo.ps` + tests, matching #42/#43).

## A dry-bristle brush with deterministic broken coverage (issue #43, 2026-08-22)

Closes issue #43, the third of the painterly-brush series (#42-#53)
built on #41's `pkribbon`: `lib/paintkit.ps`'s `pkdry`, a bounded
family of thin offset bristles scattered across the centerline, each
broken into ink/no-ink runs by a seeded two-state Markov chain --
`/Load` is the resume-contact rate, `/Dropout` the lose-contact rate.
Ranges from a mostly loaded stroke (high Load, low Dropout) to visibly
broken dry-brush texture (low Load, high Dropout) with no raster work
at all: every contiguous "on" run becomes its own small `pkribbon`
call, so caps, taper, and a single isolated fleck (`pkribbon`'s own
degenerate-single-point dot fallback) all come for free rather than
needing their own geometry.

Built as a scatter-and-delegate wrapper over `pkribbon`, the same
shape as `pknib`: one shared `walkpath` pass collects the centerline's
raw stops once (reused across every bristle), then for each bristle a
per-bristle offset/width/color is drawn once and a helper
(`pbdashrun`) walks that bristle's samples per subpath, running the
Markov chain and flushing each on-run through `pkribbon`. Deliberately
its own scratch prefix (`pb-`), distinct from `pkribbon`'s own `pk-`
and `pknib`'s `pn-` -- `pkribbon`'s body freely redefines every
`pk*`-prefixed name on each call, so a `pkdry`-owned name sharing that
prefix would be silently clobbered by its own nested `pkribbon` calls.
Verified this is safe to rely on before writing any of it: pscat's own
`gsave`/`grestore` (`src/gfx.rs`) clone/restore the *entire*
`GraphicsState`, including the current path -- a deliberate deviation
from the PLRM (where `gsave` does not save the path) that `pkribbon`'s
own header already claims ("the caller's current path survives") and
that this design leans on too.

`/Load` and `/Dropout` are documented as a rate per one `/Width` of
travel along the path, not per raw sample, scaled down to a per-sample
transition probability by `(Pitch/Width)` (clamped to 0..1) -- without
that scaling the same numbers would read as a different dryness at a
fine vs. coarse `/Pitch`, undercutting "the same options render the
same dryness" as a portable contract. The very first sample of each
subpath is the one exception: it rolls initial contact with the raw,
unscaled `/Load`, not the Pitch-scaled rate, since "does this bristle
start loaded" is a one-time boundary condition, not a rate along the
path -- and it's the *only* roll a degenerate single-point subpath
ever gets. Missed on the first pass: with the scaled rate, a
single-point stroke (see below) at `/Load 1 /Dropout 0` still left
most bristles unmarked, since the scaled rate is typically well under
1 even at `/Load` 1. Caught by rendering the actual degenerate-point
demo case and looking at it, not by a test -- the existing test suite
at that point only exercised multi-sample strokes.

A degenerate single-point subpath (a bare `moveto`) has no direction
of travel to offset perpendicular to (`walkpath` reports a synthetic
`ang=0`, the same case `pknib`'s own `pnpressure` guards against) --
fanning bristles perpendicular to that arbitrary angle drew a straight
vertical line of dots in an early version. Fixed by scattering each
bristle isotropically (both x and y independently jittered) around the
point instead, so a lone point reads as a pressed-down dab cluster.
`tests/paintkit.rs`'s `dry_degenerate_single_point_scatters_isotropically_not_in_a_line`
asserts the ink's bounding box is a genuine 2D cluster, not a
collapsed line.

Two safety limits, matching the issue's "bristle or deposit count"
wording as two separate things: `/Bristles` is hard-capped at 1..100
(not 200 -- see Performance below), and independently, `Bristles *`
(raw stop count) is checked against a fixed budget (150000) right
after the cheap counting `walkpath` pass, before the second pass or
any drawing -- catches a long path combined with a fine custom
`/Pitch` even when `/Bristles` alone is within range.

Performance was measured, not guessed, and changed the design: the
dominant cost is the per-sample Markov loop itself (interpreted
PostScript, O(Bristles * PathLength/Pitch) iterations), not the
raster fills layered on top -- confirmed by timing the *same* sample
count under both a near-maximal-alternation `/Load 0.5 /Dropout 0.5`
and a typical `/Load 0.6 /Dropout 0.4`, which came out within a few
seconds of each other despite very different actual dash-flush counts.
That ruled out a worst-case-dash-count budget and pointed at
`Bristles * raw-stop-count` as the right metric. Measured on a
~700-unit curved stroke (Width 24): the original `/Bristles` cap of
200 at the *default* `/Pitch` took ~15s -- too slow to be a reasonable
artistic-range cap -- so the cap was lowered to 100 (~8s at the same
settings) and the deposit budget raised from an initial 20000 (which
falsely rejected that same ordinary 200-bristle call) to 150000. The
defaults (18 bristles) render the same stroke in ~1.5s.

`examples/paintkit_dry_demo.ps`: row 1 is the acceptance criterion
directly -- the same path at loaded/medium-dry/very-dry `/Load`+
`/Dropout` pairs; row 2 varies `/Bristles`, `/Spread`, and
`/WidthJitter`; row 3 shows `/ColorJitter`'s small per-bristle color
variation (0 vs. a subtle amount) and a pressed-down single-point dab;
row 4 is a flourish combining all of it. `pkdry` is cataloged in
`src/capabilities.rs` alongside `pkribbon`/`pknib`; its one top-level
helper proc (`pbdashrun`) is listed in `PAINTKIT_INTERNAL`. Tested
against real Ghostscript as well as pscat, including running
`examples/paintkit_dry_demo.ps` itself through `gs` directly (not just
a synthetic driver string), same pattern as `pknib`'s own gs test.

Deliberately out of scope, matching the issue's own scope note: no wet
diffusion, alpha compositing, or external image filters -- every mark
is an opaque vector fill, same doctrine as `pkribbon`. Also deliberate:
unlike `pkribbon`, `pkdry` does not special-case a truly closed
subpath into a two-loop ring; every subpath is walked once as a linear
sequence of raw stops for dash purposes, and the Markov chain does not
wrap across a closed subpath's own seam. Dashes are discrete marks,
not a continuous offset band, so there's no ring geometry to get right
here, and not wrapping keeps the seam no more special-cased than any
other sample.

## An angled-nib calligraphy brush preset (issue #42, 2026-08-16)

Closes issue #42, the second of the painterly-brush series (#42-#53)
built on #41's `pkribbon`: `lib/paintkit.ps`'s `pknib`, a chisel/
broad-edge calligraphic-nib preset that derives mark width from the
angle between the path's local direction of travel and a fixed
`/Angle` -- widest where travel runs perpendicular to the nib,
narrowing toward a `/MinWidth` floor (default 0.08, before /Pressure
and the tapers multiply through) where travel runs parallel to it. A
real flat-nib pen's own physical model: `width ~ |sin(travel - Angle)|`.

Built as a thin wrapper over `pkribbon` rather than a new geometry
engine: `pkribbon`'s own `/Pressure` hook only sees normalized progress
`t`, not travel angle, so `pknib` does its own preliminary `walkpath`
pass (at the same pitch `pkribbon` would use) to sample `(t, travel
angle)` once per stop, then installs a synthesized `/Pressure` closure
on a shallow copy of the caller's options dict -- nearest-sample angle
lookup (deliberately no interpolation; a real chisel nib's width
genuinely jumps at a sharp corner, which the demo's zigzag and cusp
rows both exercise) folded together with the caller's own `/Pressure`
-- before delegating straight through. `/Width`, `/Pitch`,
`/StartTaper`, `/EndTaper`, `/StartCap`, `/EndCap`, and `/Jitter`
forward to `pkribbon` unchanged, so all of its existing machinery (caps,
degenerate-point/empty-path fallbacks, seeded jitter) comes along for
free.

Requires the current path be exactly one *open* subpath -- a nib
stroke has a start and an end by definition, the same as lifting a
real pen; a closed loop or more than one subpath errors rather than
guessing which subpath is "the" stroke. Callers draw multiple strokes
by calling `pknib` once per stroke.

Known, deliberate approximation (documented in `pknib`'s own header,
not just here): edges still offset perpendicular to *travel*
(`pkribbon`'s own `pkedge`), not sheared along the fixed nib axis, and
caps cut perpendicular to travel rather than at the nib angle -- a real
broad nib's edges and end-cuts follow the nib axis regardless of travel
direction. That distinction shows up at corners and cut ends, not along
a stroke's body; the issue's acceptance criterion asks for the width
response specifically ("derive mark width from the relationship between
path direction and a configurable nib angle"), which this delivers
exactly. The sheared-edge/angled-cut refinement is a real next step for
a later revision.

`examples/paintkit_nib_demo.ps`: the same arch rendered at three nib
angles (0/45/90 -- 0 and 90 are the unambiguous discriminators, since
`|sin|` is otherwise symmetric), a zigzag (corners) and a sharp cusp
(direction reversal), a broad-edge lettering "H", and a flourish
combining taper, a bell pressure profile, and jitter. `pknib` is
cataloged in `src/capabilities.rs` alongside `pkribbon`; its two
top-level helper procs (`pnangleat`, `pnpressure`) are listed in
`PAINTKIT_INTERNAL`. Tested against real Ghostscript as well as pscat
-- including running `examples/paintkit_nib_demo.ps` itself through
`gs` directly (not just a synthetic driver string), since it's the
only path that also exercises `pal`/`findfont`/`show` and the demo's
own local helpers.

Known tradeoff, not yet a problem: `pnangleat`'s nearest-sample lookup
linear-scans all n travel-angle samples, and `pkribbon` calls it
roughly 2n times per stroke while filling -- O(n^2) overall. Invisible
at the scale every current caller uses (the demo's largest stroke
samples well under 200 points), so not optimized preemptively. An
O(1) alternative (`round(t*(n-1))` index math) only holds because
`walkpath`'s pitch-spaced stops are near-uniform in t; if a future
caller pushes a stroke long/dense enough for this to matter, that
shortcut needs its own verification, not a drive-by swap.

## Pressure-sensitive ribbon strokes, a new paintkit library (issue #41, 2026-08-15)

Closes issue #41, the first of the painterly-brush series (#42-#53)
built on #40's `walkpath`: `lib/paintkit.ps`'s `pkribbon`, a single
dict-driven entry point that treats the current path as a centerline
and fills a variable-width ribbon along it. Options: `/Width`,
`/Pitch`, a `{t -> mult}` `/Pressure` proc (three presets ship --
`pkflat` constant, `pktaper` linear, `pkbell` non-linear via `sin` --
covering the acceptance criteria's three required profiles),
`/StartTaper`/`/EndTaper`, `/StartCap`/`/EndCap`
(`/round`/`/flat`/`/pointed`), and `/Jitter`. Color is deliberately not
a key -- fills with whatever the caller already set, same as any other
artkit shape helper. Multiple subpaths each become an independent
ribbon; a closed subpath (walkpath's start and guaranteed-end
coincide) is filled as two concentric closed loops with no caps, not
capped-and-notched; a degenerate single point, or a closed loop too
short to leave two distinct offset points, falls back to a filled dot
rather than erroring; an empty source path is a no-op.

Two real bugs surfaced only through actually running the code, not
through review alone:

1. `pkgetdef` (the dict-with-default reader, one per sparse-options
   library following `pagekit.ps`'s `pggetdef`) initially bound the
   default value to a name and returned it via a bare `{ pkgdef }`
   reference. That's fine for the plain-value defaults every other
   dict-driven library here has ever used -- but `/Pressure`'s default
   is itself a procedure (`pkflat`), and a name bound to a procedure
   auto-executes on bare reference, same as calling any other proc by
   name. So resolving an *absent* `/Pressure` key silently *called*
   the default with no argument instead of returning it, underflowing
   inside `pkflat`'s own `pop`. Fixed by wrapping the default in a
   1-element array and reading it back with `get` -- a data fetch,
   which never auto-executes regardless of what it holds.

2. The stop-collection loop first tried the obvious one-liner,
   `[ pitch { 6 array astore } walkpath ]` -- leave each stop's six
   values on the operand stack for the enclosing `[ ... ]` to collect,
   since `walkpath`'s own state lives entirely in named scratch vars,
   not the stack, so nothing *should* care what's sitting underneath.
   That's true within a single segment, but wrong across a subpath
   boundary: `walkpath`'s `moveto` handler calls `wkend` (emitting the
   previous subpath's guaranteed end stop through the proc) *before*
   consuming the x/y `pathforall` already pushed it for the *new*
   subpath's start -- so a proc that leaves an array on the stack
   instead of consuming it steals that x/y out from under the very
   code that was about to read it, corrupting `wkx`/`wky` and
   cascading into a `typecheck` a few operators later. Confirmed
   against plain `walkpath` directly, no paintkit involved -- a latent
   trap in the walkpath contract itself, not something specific to
   this library, worth flagging for anyone else writing a `walkpath`
   proc. Fixed with the two-pass shape `wkmeasure`/`wkseg` already
   established: count first, then fill a preallocated array, so the
   proc's net stack effect is always zero.

(A suspected corner-notch limitation from unmitered per-vertex
offsetting on a closed polygon turned out, on closer look, to be a
symptom of the closed-loop winding bug below, not an independent
issue -- gone once that was fixed, confirmed at ribbon widths several
times the polygon's own size. `fill` vs `eofill`'s self-intersection
tradeoff on a tight Bezier at a generous width is the real, still-true
limitation, documented in the header.)

`examples/paintkit_demo.ps` demonstrates all three centerline shapes,
independent start/end taper, all three pressure profiles, all three
cap styles, and seeded jitter. Like `walkpath_demo.ps`, it can't join
`tests/golden.rs`: both load `(lib/artkit.ps) run` at runtime, which
trips Ghostscript's `SAFER` sandboxing on the file read -- confirmed
that's the *only* blocker, not an unsupported operator, by running the
demo again with `gs -dNOSAFER` (renders clean). `tests/paintkit.rs`'s
own `ghostscript_accepts_paintkit` covers gs compatibility under the
default sandboxed mode instead, the same way `tests/pagekit.rs` does
-- artkit's and paintkit's *source* embedded directly into a combined
driver file, no runtime `run` calls.

Two gaps an `advisor` pass over the initial implementation caught,
both fixed before opening the PR: `/StartTaper`/`/EndTaper` had zero
coverage anywhere (header, demo, tests, gs driver) despite being one
of the issue's explicit bullets; and the degenerate-single-point dot
fallback silently drew *nothing* under a nonzero `/StartTaper` (its
radius went through the same `pkhalfwat` a real ribbon segment uses,
which multiplies in the taper ramp -- meaningless for a lone point,
and zero at t=0 for any nonzero `/StartTaper`). Fixed by giving the
dot its own width computation (`Width*Pressure` only, matching what
the header already promised) and adding both a taper demo row and
taper-specific tests, including one that forces the degenerate-radius
path (`/StartTaper 1` collapsing a `/round` cap to a point) rather than
leaving it exercised only by coincidence.

Cross-model review (Codex, round 1 on PR #76) found four more real
defects, none caught locally:

- **Closed loops filled solid, no hole.** Both `pkloop` calls (right
  and left offset of the same closed centerline) traversed forward,
  giving them the same winding sign under nonzero fill -- confirmed
  visually beforehand (a closed square rendered as a solid filled
  square) and misattributed at the time to the ring simply being too
  thick to show a hole. Fixed by traversing one side in reverse index
  order (`pkloop` gained a `reverse` argument); a real annulus needs
  opposite winding between its two offset loops, not just opposite
  sides.
- **`/StartTaper`+`/EndTaper` past 1 jumped discontinuously.**
  `pktaperf`'s mutually exclusive branch (ramp from the start *or* the
  end, based on which side of the midpoint `t` fell on) only applied
  one ramp once the two regions overlapped, jumping sharply at the
  branch boundary instead of blending. Fixed by computing both ramps
  unconditionally (each already defaults to 1.0 outside its own
  region) and taking the minimum.
- **Closed-path detection used a scale-fragile epsilon.** The same
  category of bug already fixed once in `walkpath` itself (round 2 of
  #40's review): a fixed `0.0001` user-space distance decided whether
  a subpath's start/end coincided, which breaks under a large CTM
  where a user-space-tiny gap is visually enormous. Switched to exact
  equality -- correct because walkpath's own closepath handler feeds
  the closing segment the *stored* start coordinates verbatim, never
  recomputed through curve-flattening math, so a genuinely closed
  subpath's guaranteed-end stop always lands bit-for-bit on its start.
- **Cap-degeneracy check had the same scale-fragility.** The
  round-to-pointed threshold (half-width <= 0.001) was also a fixed
  user-space epsilon, silently collapsing a real, visibly-wide-once-
  scaled cap (e.g. `/Width 0.0015` under a 10000x CTM) to a point.
  Verified directly that a zero-radius `arc` is safe in both pscat and
  Ghostscript, so there was nothing to guard against by treating a
  merely-small-in-user-space width as zero -- narrowed the check to a
  truly zero (or negative) half-width.

All four came with regression tests (`closed_polygon_leaves_a_hole_*`,
`overlapping_start_and_end_taper_ramps_stay_continuous`,
`small_width_cap_survives_a_large_scale_*`), two of them specifically
constructed under a large `scale` to pin the CTM-fragility class of bug
rather than just the 1x-scale symptom. Full quality gate re-run clean
(664 tests) before pushing the fix and re-running Codex review.

Round 2 found four more, all fixed:

- **Malformed `/Pressure`/`/StartCap`/`/EndCap` weren't validated.** A
  non-procedure `/Pressure` (e.g. `1`) was never auto-executed by
  `pkhalfwat`'s `t pkpressure` call -- it just got pushed, leaking
  extra operands into every downstream computation instead of raising
  a clean error. An unrecognized cap value silently fell through to
  `/flat`. Both are validated now (`xcheck` for the procedure check,
  an explicit three-way `eq`/`or` chain for caps), each with its own
  self-documenting guard name and regression tests. Writing the
  `/Pressure` guard itself hit the *exact* auto-execute trap its own
  fix is protecting against: the first attempt, `pkpressure xcheck`,
  bare-referenced `pkpressure` -- which auto-runs it, since it's
  already bound to a procedure in the common case -- calling the
  default pressure proc with nothing on the stack before `xcheck` ever
  saw it, `stackunderflow`. Fixed with `/pkpressure load xcheck`:
  `load` fetches a bound value without executing it, regardless of
  type. Two run-ins with the same PostScript gotcha in one library is
  worth over-explaining in the comment for whoever touches this next.
- **`pkribbon` was undiscoverable via the capability catalog.**
  `CAPABILITIES.md`/`pscat --capabilities`/MCP's
  `describe_art_capabilities` are the documented source of truth for
  what an agent can find installed (issue #39) -- paintkit shipped
  absent from all three. Registered `pkribbon` (its eight options-dict
  keys as real `Param`s, same treatment templates and
  `hs-write`/`hg-write` get) and the three `/Pressure` presets
  (`pkflat`/`pktaper`/`pkbell`) in `src/capabilities.rs`'s `ENTRIES`,
  added `PAINTKIT_INTERNAL` for the remaining scratch helpers, and a
  `paintkit_names_match_the_catalog_exactly` test mirroring
  `pagekit_names_match_the_catalog_exactly` -- the reverse-coverage
  check that fails if a future public name in `lib/paintkit.ps` ships
  uncataloged.
- **The demo lacked `showpage`.** Reaching EOF without it emits no
  page under a standard PostScript consumer (Ghostscript, a real
  printer) -- masked locally because `pscat --png` snapshots the final
  canvas regardless. Added; re-verified under both pscat and
  `gs -dNOSAFER`.

Full quality gate re-run clean (665 tests) before pushing and
re-running Codex review a second time.

Round 3 found two more, both fixed:

- **A short pointed stroke rendered nothing.** `walkpath` emits only
  start+end stops for a subpath shorter than `/Pitch` (no interior).
  With both ends pointed, `pkopenrun`'s forward and reverse edge loops
  were *both* empty -- the polygon it built was the raw start-tip to
  end-tip line, zero area under `fill`. A generously wide, genuinely
  short stroke with `/StartCap /pointed /EndCap /pointed` (or a full
  taper collapsing both ends) silently produced a blank page instead
  of the lens shape a longer version of the same stroke renders fine.
  Fixed by detecting exactly that condition (both ends pointed, no
  interior stops) and synthesizing one interior sample at the run's
  midpoint -- chord direction (exact regardless of the underlying
  curve shape, since there's nothing else to look at over such a short
  span) and averaged progress for pressure/taper -- so it builds a
  proper 4-point lens instead.
- **`xcheck` alone doesn't prove something is callable.** The
  round-2 `/Pressure` guard used `xcheck`, which only tests the
  executable attribute -- an executable non-procedure like `2 cvx`
  passes it, but `pkht pkpressure` still just pushes the number instead
  of running anything, the identical corruption the guard exists to
  catch. Added a `type` check (`arraytype` for a user proc,
  `operatortype` for a bound built-in) alongside `xcheck`.

Both came with regression tests. Full quality gate re-run clean (666
tests) before pushing and re-running Codex review a third time. (The
second `--wait` review invocation was itself killed with no output and
no job ever registered with `codex-companion`'s tracker -- confirmed
via `status --all --json` showing nothing running, nothing finished,
nothing recent, not even a dead PID to `cancel`. Treated as transient
rather than genuine runtime unavailability, since the identical command
had already succeeded twice in a row on this same PR; a bare retry
registered and completed normally.)

Round 4 found three more real defects, subtler than any prior round --
all three about walkpath's own edge cases (the exact-multiple-of-pitch
coincidence, the too-short-to-sample case, coordinate-only closure
inference) that hadn't come up until Codex specifically went looking
for them:

- **A stroke exactly one pitch long could still render blank.** Round
  3's fix only handled a subpath *shorter* than `/Pitch` (walkpath's
  guaranteed 2-stop minimum). When a subpath's length is an exact
  multiple of `/Pitch`, walkpath's regular stepping already lands
  exactly on the endpoint, and the guaranteed-end stop then duplicates
  it (`sp == 0`) -- three stops total, not two, so round 3's "index
  distinct from start" check for whether a real interior sample exists
  passed even though the "interior" stop is really just an echo of the
  endpoint at the same position. Combined with a pressure profile
  that's genuinely zero right at the edge (`pkbell` at `t=1`), the only
  candidate sample available also carried zero width. Fixed by
  coalescing the trailing `sp==0` duplicate *before* deciding whether a
  real bulge is possible, not after.
- **A closed subpath shorter than `/Pitch` rendered blank instead of a
  dot.** The mirror image of the above for closed paths: walkpath
  returns only start and a guaranteed end at the same coordinates, but
  the end's `sp` is the subpath's own positive length, not 0 -- the
  duplicate-coalescing check alone doesn't catch it, since there's no
  duplicate to coalesce here, just two stops that both happen to be the
  same physical point. `pkloop` built two literally coincident-point
  "loops," zero area, instead of the documented dot fallback. Needed a
  check for whether a genuinely distinct sample exists, not just
  whether the dedup left two array indices.
- **Coordinate coincidence can't distinguish real closure from an open
  path that merely returns to its own start.** The round-1 exact-
  equality fix correctly stopped comparing coordinates under a
  tolerance, but never questioned whether comparing coordinates *at
  all* was the right test -- an open subpath ending with an explicit
  `lineto` back to its exact starting point (no `closepath`) gives
  bit-for-bit identical endpoint coordinates to a real closed one,
  silently dropping its requested caps and building a ring instead.
  Fixed properly rather than patched further: `pkscanclosed`, a new
  helper mirroring `wkmeasure`'s own two-pass shape, does a `pathforall`
  pass that tracks whether each subpath's close proc actually fired --
  authoritative, since `pathforall` only calls it for a real
  `closepath` segment -- and `pkribbon`'s dispatch now threads that
  per-subpath flag into `pkbuildrun` instead of it inferring closure
  itself.

Implementing the second fix's first attempt introduced its own bug,
caught only by looking at the actual render rather than trusting the
logic: "does a real interior sample exist for a closed run" was
written as a *position* comparison between the run's last stop and its
first -- but a closed subpath's guaranteed-end stop always returns to
the start's coordinates *by definition*, whether the loop has real
content or not, so that comparison is comparing a closed loop's end to
itself and is always true regardless of what's actually being asked.
Every closed ribbon in the whole demo collapsed to a single dot at the
seam instead of following its perimeter -- confirmed by re-rendering
the same closed-square fixture from round 1's own fix and seeing a dot
where a ring used to be. None of the existing hole-in-the-middle tests
caught it (a dot at the seam still leaves the center unpainted and the
one edge pixel they happened to sample still inked), which is exactly
why the fix this time samples all four sides of the perimeter, not
one. Corrected to an index-based check (is there a real *interior*
array index between start and end, not whether the endpoint coincides
with itself) -- the thing round 3's short-stroke fix already needed to
get right for open paths, misapplied here to a case where it doesn't
mean the same thing.

All fixes came with regression tests, including one for the
self-caught bug. Full quality gate re-run clean (670 tests) before
pushing and re-running Codex review a fourth time.

Round 5 found two more, one of them P1:

- **A packed procedure literal failed `/Pressure` validation --
  including `pkribbon`'s own default.** Under a Level 2 interpreter
  with packing enabled (`true setpacking`), a plain procedure literal
  like `{ pkflat }` can have type `packedarraytype` instead of
  `arraytype` -- confirmed directly against real Ghostscript (pscat
  itself doesn't actually pack, so `type` stays `arraytype` there
  regardless of `setpacking`). The round-3 type-check only accepted
  `arraytype`/`operatortype`, so `pkribbon` would fail on its own
  documented default the moment a caller's environment had packing on.
  Added `packedarraytype` to the accepted set, and pushed
  `true setpacking` to the front of `ghostscript_accepts_paintkit`'s
  driver so the whole test actually exercises the branch under real
  gs, not just calls `pkribbon` under packing's (irrelevant, for
  pscat) default-off state.
- **`/StartTaper`/`/EndTaper` outside their documented 0..1 range
  weren't validated.** Doesn't crash -- a negative value just silently
  disables that ramp, above 1 keeps the whole stroke short of full
  width -- but it's a real contract violation, same category as every
  other documented constraint here, so validated the same way.

Both have regression tests. Full quality gate re-run clean (671 tests)
before pushing and re-running Codex review a fifth time.

Round 6 found two more, one of them a real bug in `pscat` itself, not
paintkit:

- **`atan` on a zero-length chord.** The round-3 short-pointed-stroke
  fix synthesizes a midpoint between an open run's two stops, using
  their chord direction -- but an open subpath that returns to its own
  exact starting coordinates (an unclosed full-circle `arc`, an
  explicit `lineto` back to the start) and is also shorter than
  `/Pitch` has that chord collapse to zero length, and `atan` on
  `(0,0)` is undefined in both pscat and Ghostscript (confirmed
  directly against both). No chord direction exists to synthesize from
  in that case; falls back to a dot instead, same as any other
  genuinely degenerate short run.
- **`pathforall` didn't insert the PLRM's implicit moveto after
  `closepath`.** Not a paintkit bug at all -- a real, previously
  undiscovered gap in `pscat`'s own `pathforall` (`src/ops/
  graphics.rs`), exposed by `walkpath`'s reliance on `pathforall`
  correctly reporting subpath boundaries. Per the PLRM, a `lineto`/
  `curveto` immediately following `closepath` with no intervening
  `moveto` behaves as though a `moveto` to the current point
  (`closepath`'s own return-to-start point) had been inserted first --
  Ghostscript honors this, `pscat` didn't, merging a closed subpath
  with whatever open drawing followed it into one run instead of two.
  Confirmed the divergence directly: the same path reports 1 `moveto`
  under `pscat`, 2 under real `gs`. Fixed in `pathforall`'s own element-
  building loop (not the underlying path/segment model, which every
  other consumer -- rendering, SVG/PDF export -- walks unchanged and
  doesn't need this for), tracking the last `moveto` point and
  synthesizing one right before a `Line`/`Curve` that immediately
  follows a `Close`. gs-pinned test added to `tests/pathforall.rs`,
  the existing home for this operator's tests, not folded into
  paintkit's own suite -- this is core interpreter behavior any future
  `pathforall` consumer benefits from, not something specific to
  ribbons.

Both have regression tests; the `pathforall` fix's test lives with the
operator's own suite rather than paintkit's, and the full test suite
(not just paintkit's) was re-run to confirm the core-interpreter change
didn't regress anything else that walks paths -- golden-image
comparison against real gs included, since that's exactly the kind of
divergence it would have caught. Quality gate re-run clean (673 tests,
clippy clean, fmt clean) before pushing and re-running Codex review a
sixth time.

Round 7 found one more: **every plain value option (`/Width`,
`/Pitch`, `/StartTaper`, `/EndTaper`, `/Jitter`, `/StartCap`,
`/EndCap`) had the same exposure `/Pressure` was fixed for back in
round 2, just not yet applied to fields that are documented as plain
values rather than callbacks.** Binding one of these straight to its
own name -- `pkgetdef`'s normal result -- makes every later *bare*
reference to that name auto-execute it if the caller happened to
supply an executable array, e.g. `<< /Width { } >>` (a zero-push
procedure): `pkwidth 0 le` would then run against whatever was on the
stack *before* that reference instead of the intended width, silently
corrupting downstream computation rather than erroring. Confirmed the
zero-push case is exactly this real, not hypothetical (a non-zero-push
procedure like `{ 10 }` happens to net the same effect as the number
it produces, which is why a smaller example wouldn't have caught it).
Every affected field now gets the same `load`+`xcheck` guard
`/Pressure` already had, checked immediately after binding, before any
other bare reference to that name -- error names follow the same
self-documenting `pkribbon-<field>-must-not-be-a-procedure` pattern.
Regression tests added for all seven fields. Quality gate re-run clean
(673 tests) before pushing and re-running Codex review a seventh time.

## A reusable centerline path sampler for procedural brushes (issue #40, 2026-08-15)

Closes issue #40, the foundation for the painterly-brush series
(#41-#53): `walkpath` in `lib/artkit.ps`, a richer sibling to the
existing `alongpath`. Same even-arc-length pitch stepping, but each
call also carries normalized progress `t` through the current subpath
(0 at its start, 1 at its end), the arc-length spacing `sp` since the
previous stop, and an `atend` bitmask flagging the first/last stop of
a subpath. `walkpath` additionally *guarantees* one call at the
literal end of each subpath even when that doesn't fall on a pitch
multiple — `alongpath`'s stepping alone can never promise that, and a
pressure ribbon needs an exact endpoint to place tapers/caps.

Implementation: one `flattenpath`, then two `pathforall` passes over
the same flattened path (confirmed empirically that a flattened path
survives repeated traversal, same as `alongpath`'s existing doc
comment implies) — the first (`wkmeasure`) mark-collects each
subpath's total arc length into an array so `t` can be computed
without a second, dynamic length pass; the second (`wkseg`/`wkend`)
walks and stamps, reusing `alintern`/`apseg`'s proven segment-walking
approach.

Deliberately *not* delegating `alongpath` to `walkpath` (the issue
allowed either): `alintern`'s pitch argument is itself a *procedure*,
re-evaluated before every stamp — `pathtext` depends on this to use
each glyph's own advance as the next pitch, which `walkpath`'s scalar
pitch can't express. `alongpath` stays untouched; its own tests are
unaffected. First implementation attempt (per `advisor`, plan-review
round) computed the start stamp's tangent as a hardcoded 0 in the
moveto handler, before any segment had been seen — a ribbon can't
orient a start cap off that. Fixed by letting the first stamp fall out
of the normal segment-walking loop (bit 0 of `atend` marks it),
instead of special-casing it separately.

`examples/walkpath_demo.ps` demonstrates all three path shapes the
acceptance criteria call out (line, Bézier curve, closed polygon).
Ghostscript compatibility is verified by extending the existing
`ghostscript_accepts_artkit` test's driver (mark-based `[ ...
pathforall ... ]` array collection needed confirming under gs, not
just pscat) rather than gs-running the demo file directly — the demo,
like every other example that does `(lib/artkit.ps) run`, hits gs's
default `SAFER` sandboxing on that file read, the same reason
`paragraph_layout.ps` and friends are absent from `tests/golden.rs`'s
list.

Cross-model review (Codex, round 1 on PR #75) caught two real defects
the local quality gate couldn't: a zero or negative `pitch` left
`wkt2` never advancing past `wkseglen`, hanging the interpreter
instead of erroring — fixed with the file's existing malformed-input
idiom, a guarded call to a self-documenting undefined name
(`walkpath-pitch-must-be-positive`, same pattern as `et-spacing-must-
be-positive` in `lib/etching.ps`). And the header's claim that a
subpath too short for one pitch step gets a single `atend=3` call was
simply wrong for any subpath with nonzero length (only a true
single-point subpath does) — the actual behavior (a distinct start
and guaranteed-end call) is the more useful contract for a brush to
build on, so the fix was correcting the documentation, not the code.

Round 2 caught a third, more interesting one: `wkseg`'s "is this
segment real" guard used an *absolute* `0.0001` (user-space) epsilon
rather than strict positivity. `pathforall` reports pre-CTM
coordinates, so a subpath drawn tiny in user space and blown up with
`scale` at render time — a real, visible mark — could have every one
of its segments individually under that threshold, misclassifying the
whole thing as a degenerate single point. `atan`/division only
actually fail at *exact* zero (confirmed against `src/ops/arith.rs`'s
`atan`, which only raises `undefinedresult` when both args are
`0.0`), so the fix is `wkseglen 0 gt` instead of the arbitrary
threshold — which also incidentally closes a smaller gap flagged (but
judged negligible) during the original implementation review: with
the absolute epsilon, `wkmeasure`'s unconditional length sum and
`wkseg`'s conditional arc accumulation could drift apart by the sum of
any skipped near-threshold segments; strict positivity keeps them in
exact sync.

Closes issue #39: an autonomous artist agent needs a dependable way to
discover which fonts/palettes/templates/procedures a given `pscat`
build actually has, rather than trusting names remembered from prose
documentation — which drifts. Confirmed while scoping this issue:
`psart`'s `SKILL.md` had already fallen behind artkit's
paragraph-flow, hyperbolic-geometry, noise/flow, and gradient
sections (issues #16/#10/#19/#20), several of which shipped after the
skill's "toolkit" tour was last touched.

- `src/capabilities.rs`: `--capabilities` (CLI) and
  `describe_art_capabilities` (`pscat-mcp`) both serialize one JSON
  payload — 273 entries as of this build: 147 fonts (105 builtin/
  catalog-stem, plus 40+ implicit `-Regular`-stripped aliases two
  cross-model review rounds found missing — see below), 87 procedures
  (57 from `lib/artkit.ps`, 26 across the four style packs, and 4 more
  from `lib/handscript.ps`/`lib/hangul.ps`), 22 palettes, 9 Type 3
  program faces, 5 page templates, 3 dials — plus `pscat_version` and
  `catalog_signature` fields so a caching agent can treat either
  changing as the re-fetch signal.
- Fonts are the one section built *dynamically*: `font.rs` gained
  `catalog_entries()`/`FontOrigin` (Builtin/Catalog/Alias), and
  `available_fonts()` (the existing `--fonts` output) now derives from
  it instead of duplicating the directory scan — so the catalog's font
  section and `--fonts`/`findfont` resolution can't independently
  disagree about what's installed, which is exactly the failure mode
  the issue is about.
- Palettes/templates/procedures have no PostScript docstring
  convention to parse, so their metadata is hand-maintained in
  `capabilities.rs`'s `ENTRIES` table — but `tests/capabilities.rs`
  loads each `.ps` source into a real `Interp` and checks the name set
  in *both* directions: every cataloged name still resolves where
  claimed (`Palettes`/`userdict`/`FontDirectory`, via
  `{ pop } forall` collecting every dict key onto the operand stack in
  one call — no printing/parsing needed), and every name a source file
  actually defines is either cataloged or on one of two explicit
  internal-helper allowlists (`ARTKIT_INTERNAL`, `PAGEKIT_INTERNAL` —
  scratch names like `apseg`/`tfdrawline`, and the `Palettes`/
  `TurtleState` dicts themselves, not part of the public API). The
  reverse direction is the one a naively forward-only test would miss:
  nothing stops a future style pack from registering a new palette or
  procedure and forgetting to catalog it: with this test, that fails
  CI instead of silently missing from `--capabilities`.
- Procedures deliberately get no structured `parameters` — PostScript
  stack arguments are positional, not named/defaulted, so forcing them
  into the same `Param` shape templates use (whose content dicts
  *do* have real optional keys with defaults) would mean inventing
  names the source doesn't have. A procedure's calling convention
  lives in `example` instead: the stack-effect comment already written
  at its definition site.
- Scope cut, stated rather than silent: `graph.ps`/`dataviz.ps`/
  `etching.ps` are not cataloged — the issue's "What" section names
  fonts/Type-3/palettes/style-packs/templates/*artkit* procedures
  specifically, and those three are independent sibling libraries by
  design (graph.ps and dataviz.ps share nothing with artkit on
  purpose, per their own NOTES.md entries). A reasonable follow-up,
  not an oversight.
- A cross-model (Codex) review on PR #74 caught two real gaps before
  merge, both fixed on the branch:
  1. `--capabilities`/`--fonts` advertised every catalog-font alias
     whenever `fonts/catalog/` existed, even when that specific
     alias's target file was missing from an incomplete install —
     `findfont` would substitute Helvetica for it while the catalog
     claimed it resolved. `font::catalog_entries()` now filters
     `ALIASES` against the same stem/`-Regular` fallback lookup
     `catalog_fid` itself uses, so only aliases that would actually
     load are listed (no behavior change for this repo's own complete
     catalog install).
  2. The Type 3 face list stopped at `lib/fonts/`'s seven files and
     missed `lib/handscript.ps`'s `/HandScript` (the face behind the
     `handwrite` tool) entirely — the review's exact finding — plus,
     found while fixing it the same way, `lib/hangul.ps`'s
     `/HangulScript` (issue #6's Unicode-mode jamo-composition face),
     a second instance of the identical gap the review didn't happen
     to name. Both are now cataloged, along with their `hs-write`/
     `hs-linecount`/`hg-write`/`hg-linecount` options-dict procedures
     (real `parameters`, not `example`-only, since an options dict is
     genuinely `Param`-shaped the way a template's content dict is).
     The Type 3 reverse check, which had been a hardcoded `len() == 7`
     plus a forward-only `FontDirectory` lookup, is now a real
     `lib/fonts/*.ps` directory scan (plus the two named historical
     outliers) compared against the catalog's Type3Face sources —
     closing the same forward/reverse gap the rest of the catalog
     already guarded against.
  A `load` field was also added to every entry (the exact `run`
  sequence needed before `example` works — a template or style-pack
  procedure errors `undefined: Palettes` without `lib/artkit.ps`
  loaded first, invisible from `source` alone), derived from
  `(kind, source)` in one `load_sequence` function rather than
  hand-written per entry.
- A third review round on the same PR caught three more real gaps,
  all in the dynamic font section, all fixed:
  1. A catalog stem was listed whenever a file matched the
     `.ttf`/`.otf`/`.ttc` extension filter, without confirming the
     file actually reads and parses — a corrupt or unreadable file in
     an otherwise-present catalog directory got advertised as
     installed while `findfont` would silently substitute Helvetica
     for it. `catalog_entries()` now calls a new `parses_as_font`
     (read + `Face::parse`) before including a stem.
  2. `catalog_fid`'s own resolution logic tries a bare requested name
     *and* a `-Regular` fallback against catalog files for *any* name,
     not just ones in the curated `ALIASES` table — so a file named
     exactly `<Name>-Regular.ttf` makes the bare `<Name>` resolve too,
     entirely independent of `ALIASES`. This repo's own catalog has 37
     such reachable names (confirmed by testing one, `/Bangers
     findfont`, directly) that `--capabilities`/`--fonts` had never
     listed at all. `catalog_entries()` now synthesizes an implicit
     alias for every stem ending `-Regular`, guarded against
     colliding with an existing name.
  3. `pscat_version` alone under-signals drift for a filesystem-backed
     section: the font catalog can change (a different `PSCAT_ROOT`,
     an install updated in place) without the binary version changing
     at all. `payload_json()` now also emits `catalog_signature`, a
     hash over every entry's (name, kind, availability) — either
     field changing is the re-fetch signal.
  `FontEntry.alias_target` changed from `Option<&'static str>` to
  `Option<String>` to carry the second fix's call-time-discovered
  target without leaking memory per `catalog_entries()` call (the
  first draft of the fix did leak; caught before commit by checking
  it against the codebase's own leak discipline — `'static` leaks here
  are supposed to be bounded per unique font file, process-lifetime,
  not per catalog listing).
- A fourth review round caught four more real gaps, all fixed:
  1. The implicit `-Regular`-alias fix from round three used an
     exact-case `strip_suffix("-Regular")`, but the bundled TeX Gyre
     files are named e.g. `texgyreadventor-regular.otf` — lowercase.
     `catalog_fid` itself resolves case-insensitively
     (`eq_ignore_ascii_case`), so `/texgyreadventor findfont` already
     worked; the catalog just didn't know it. Fixed by lowercasing a
     copy and using `strip_suffix` on that (boundary-safe — manual
     byte-index slicing on the original string risked a panic on a
     hypothetical non-ASCII stem, caught while writing the fix, not by
     the review).
  2. `lattice`'s cataloged calling convention, `x0 y0 v1 v2 n1 n2
     ... lattice`, was copied faithfully from `lib/artkit.ps`'s own
     top-of-file API index — which itself compresses two 2D vectors
     into `v1 v2`. The proc actually pops four separate numbers
     (`v1x v1y v2x v2y`); an agent following the catalog literally
     would come up two operands short. Corrected to match the
     definition, not the header's shorthand.
  3. `hex`'s description said "flat-top"; `lib/artkit.ps`'s own
     comment at its definition says "pointy-top" (hex starts its walk
     at 90 degrees). Corrected.
  4. `spmetal`/`sfworld`/`tnink` (the three style packs' dial
     variables) were cataloged as `kind: procedure`, despite not being
     callable — invoking one just pushes its current value. Split into
     a new `CapabilityKind::Dial`; `tests/capabilities.rs`'s style-pack
     reverse check now unions `Procedure` and `Dial` names for its
     expected set, since both still land in `userdict`.
- A fifth review round caught four more real gaps, all fixed:
  1. `noise2`'s example omitted the `noiseinit` call its own
     permutation table read depends on — run from a fresh interpreter,
     `(lib/artkit.ps) run 0 0 noise2` errors `undefined: Perm`. Added
     to the example.
  2. `ldraw`'s example likewise omitted `thome` — `fd`'s first
     `lineto` has no current point without it, `nocurrentpoint`.
  3. `HangulScript`/`hg-write`/`hg-linecount` described `Text` as
     "UTF-8 Korean text (may mix in ASCII)" — true of what the source
     *accepts*, but misleading about what *renders*: `lib/hangul.ps`'s
     own header says non-Hangul codepoints (ASCII, spaces,
     punctuation) get a half-width advance and draw nothing. An agent
     mixing English into `Text` expecting it to show would get
     invisible gaps instead. Corrected in all three entries.
  4. A catalog stem whose own name exactly matches a builtin's
     `ps_name` or an `ALIASES` key is permanently unreachable under
     that name — `resolve()` checks builtins, then `ALIASES`-key
     remapping, before ever trying a catalog stem directly, so e.g. a
     hypothetical `Helvetica.ttf` in a custom `PSCAT_ROOT` catalog
     would never actually be selected. `catalog_entries()` now filters
     such shadowed stems out of the directly-listed Catalog names
     (inert for this repo's own catalog — no such collisions exist in
     it today, confirmed by an unchanged font count after the fix; the
     precedent for the *fully* general filtering rule, though, is
     already the same `seen`-set mechanism the implicit `-Regular`
     alias derivation added in round three uses).
- A sixth review round found one more real gap (fixed) and two that
  are deliberate, documented scope cuts (not fixed — accepted, per
  `font.rs::catalog_entries`'s own doc comment):
  1. **Fixed.** The implicit `-Regular` alias derivation (round three)
     guarded against a short name colliding with a curated `ALIASES`
     entry only via the `seen` set — which only catches the collision
     if that curated entry actually got *added* (i.e. its target
     exists). But `catalog_fid` checks `ALIASES` unconditionally
     before ever trying a bare stem, regardless of whether the
     `ALIASES` target resolves — so in an install missing a curated
     target but happening to also hold a same-named `-Regular` file
     under a *different* underlying name, the implicit alias would
     claim a resolution `findfont` would never actually reach (it'd
     substitute Helvetica via the `ALIASES` redirect instead). Fixed
     by excluding any short name that's an `ALIASES` key outright, not
     just guarding by `seen`.
  2. **Not fixed, scope cut.** Two catalog files differing only by
     case (`foo.ttf` and `Foo-Regular.ttf` in the same family
     directory) would make this catalog and `catalog_fid`'s fully
     case-insensitive stem match disagree about which file a name
     resolves to. No shipped catalog family does this; a general fix
     would mean making every name comparison in `catalog_entries()`
     case-insensitive-aware, disproportionate to a scenario no real
     catalog produces.
  3. **Not fixed, scope cut.** `catalog_fid` aborts font resolution
     entirely the instant it hits an unreadable family subdirectory
     (`.ok()?`); this scan just skips it and keeps listing everything
     else (`.into_iter().flatten()`). A real discrepancy, but one that
     predates this catalog — the original `available_fonts()`'s own
     directory scan already had it, before `capabilities.rs` existed.
     Reconciling it means changing `catalog_fid`'s own error handling
     (font resolution proper), out of scope for a capabilities-catalog
     issue; a dedicated follow-up if it ever matters in practice.
- `CAPABILITIES.md` documents the payload shape and the
  register-a-new-capability workflow; `.claude/skills/psart/SKILL.md`
  now points at `--capabilities` as the source of truth over its own
  prose.

## Add `/issue-summary` dashboard skill (issue #36, 2026-08-15)

Closes issue #36: seeing "what's been worked on, what's active, what's
done" meant scrolling raw `gh issue list`/`gh pr list` output or
re-deriving it ad hoc — `work-issue` already does exactly that
derivation internally every run for its own picking/resuming logic
(`.claude/skills/work-issue/SKILL.md` step 1).

- `scripts/issue_summary.sh`: a `gh`/`jq`-only script (no model
  reasoning) that groups open issues into in-progress/in-review/open
  by label, matches each to an open PR via the same `Closes #N`/`Fixes
  #N`/`Resolves #N` body-text convention `work-issue` writes into every
  PR it opens, surfaces that PR's CI/review status, and lists the N
  most-recently-updated closed issues (`--closed N`, default 10).
- `.claude/skills/issue-summary/SKILL.md`: a thin wrapper per the
  issue's hard requirement — its only job is "run the script, print
  its output," not reasoning about the data on every invocation.
- Left for later (per the issue's "left to the implementer" list):
  richer CI/review surfacing beyond a one-word status, and any
  time-window (vs. fixed-count) framing for "recently closed."

## Fix mid-show font-switch Unicode segmentation (issue #31, 2026-08-15)

Closes issue #31: `ShowCtx` decided `unicode_mode` once, from the font
in effect when a show began, and pre-decoded the entire string into a
`Vec<u32>` under that single decision — even though the font itself is
legitimately re-read per glyph (a `kshow` proc, or a nested `show`
inside `BuildChar`, can switch fonts mid-string). A kshow proc switching
*into* a Unicode-mode font from a byte-mode one would find that
codepoint's UTF-8 bytes already split apart, unrecoverable as one glyph.

- `ShowCtx` now holds the raw `Vec<u8>` and a byte cursor instead of a
  pre-decoded code vector; each glyph step decodes one code from the
  *live* font's perspective and advances the cursor by however many
  bytes that consumed — full architectural rewrite the issue called
  for, not a patch on top of the old per-glyph resolution-function
  recheck (which could route to the right function but never undo an
  already-wrong segmentation).
- No mid-string switch ⇒ byte-identical to the old eager decode — the
  byte-mode regression gate and `tests/catalog.rs` pass unchanged.
- The fix is deliberately asymmetric: switching *out* of Unicode mode
  mid-codepoint now correctly yields one glyph per leftover raw byte
  (a byte-mode font can't know 3 bytes were meant to be one codepoint),
  where two `tests/type3.rs` tests had previously encoded the old bug's
  incidental truncated-scalar behavior as their expected values. Full
  writeup in `FONTS.md`'s new addendum.

## Gate merges on a perf/memory regression check (issue #25, 2026-08-15)

Closes issue #25: `benches/perf.rs` and `benches/vs_gs.rs` existed as
regression tripwires nobody pulled — `cargo bench` wasn't part of CI,
and memory wasn't measured at all. A new `benches/regression.rs` plus
a `perf-regression` job in `.github/workflows/ci.yml` close that gap.

- **Same-job A/B, not a stored baseline.** The job checks out both the
  PR's HEAD and its merge base (`github.event.pull_request.base.sha`)
  into `head/`/`base/`, builds a release `pscat` binary from each, then
  runs `benches/regression.rs` — compiled from `head/` only — pointed
  at both binaries by path. Comparing on the *same runner in the same
  job* cancels out GitHub-hosted macOS runner-to-runner hardware/
  thermal noise, which the issue flagged as a real risk, rather than
  inheriting it the way a baseline stored from a separate prior run
  would. `benches/regression.rs` only exists on HEAD, so this also
  sidesteps a bootstrap problem: running two different bench-harness
  versions against each other would be an apples-to-oranges
  comparison, and a `main` checkout that predates this PR doesn't have
  the harness at all. One harness, two binaries.
- Workload `.ps` files (`examples/sierpinski.ps`, `gallery/fern.ps`)
  always come from the HEAD checkout for both sides (the bench's cwd
  is `head/`) — a PR that only edits a gallery file can't misreport as
  an interpreter regression.
- Reuses the same peak-RSS measurement this repo already proved out in
  `vs_gs.rs`: subprocess launch under `/usr/bin/time -l` (macOS). No
  new tool (`valgrind`/massif/heaptrack/dhat) — those are Linux-first
  and this repo's CI is macOS-hosted.
- Four workloads (fib 27, defloop 200k, sierpinski, fern — mirroring
  `perf.rs`'s set), 5 interleaved runs each (A/B/A/B..., alternating
  which side goes first per run to avoid loop-drift landing on one
  side), best-of-5 wall time, median-of-5 peak RSS.
- Two named thresholds (`WARN_PCT` = 20%, `FAIL_PCT` = 50%) at the top
  of `regression.rs`. Unvalidated against the actual GitHub-hosted
  runner at merge time — this PR's own run (touches no `src/`, so its
  expected delta is ~0) is the first real noise-floor sample; retune
  if that run's deltas sit close to `WARN_PCT`. Locally (M-series), an
  identical-binary smoke test showed -2.1%..+3.1% on wall time, well
  inside the current band.
- **Missing RSS is a hard failure, not a silent `-`.** If
  `/usr/bin/time -l` doesn't yield a reading for a *majority* of a
  workload/side's samples, the bench panics rather than reporting "no
  regression" — an `-` here would have meant the memory half of the
  check silently stopped checking anything. (A minority miss — one
  dropped sample out of five — is tolerated as ordinary measurement
  jitter and just shrinks the median's sample size.)
- **Gates on signed delta, never `.abs()`.** A large positive delta
  (slower/bigger) can fail the job; a large *negative* delta
  (surprisingly faster/smaller) is flagged in the report — a workload
  that quietly stopped doing real work would also look faster, so it's
  worth a glance — but is worded as a surprise, not a "regression," and
  never fails the job on its own. An `.abs()`-based first draft would
  have failed CI on a genuine speedup; caught by `advisor` before this
  PR was opened, verified with a head/base-swapped smoke test (exit 0,
  ⚠️ rows, no ❌) alongside the reverse (exit 1) case.
- **A workload under `MIN_MS_FOR_FAIL` (15ms) can't fail on time.**
  `sierpinski` measures ~5ms end-to-end against a ~4ms process-startup
  baseline (`vs_gs.rs`/Stage 11) — almost the whole row is launch
  jitter, not interpretation, and was the noisiest row in every local
  smoke test. Below the floor its time metric can still warn but never
  contributes to a job failure; `fern`/`fib`/`defloop` all clear it
  comfortably. This floor is *absolute* but proxies a *relative*
  property (startup-dominated), which only holds as long as the
  runner's actual launch overhead stays well under 15ms — flagged as a
  watch item on issue #68 rather than redesigned now, since there's no
  GitHub-runner data yet to size it against. When a row is
  floor-suppressed despite a delta past `FAIL_PCT`, the report says so
  explicitly (a line below the table) — the closing summary says "no
  workload **failed the gate**," not "no workload regressed," so it
  can never contradict a visibly large delta sitting right above it.
- Delivery mirrors issue #24's `ci_test_summary.sh` pattern, with one
  change from how #24 shipped: both this job and the `test` job now
  post via a new `scripts/gh_comment_upsert.sh` (find-by-marker,
  then PATCH or POST) instead of `gh pr comment --edit-last`.
  `--edit-last` scopes to the authenticated user's *most recent*
  comment with no content awareness — with two jobs both posting as
  github-actions[bot], each one's `--edit-last` would grab whichever
  comment the *other* job had posted most recently and overwrite it,
  so the PR's one bot comment would ping-pong between test results and
  perf results on every push. Caught by `advisor`, not by testing
  (both jobs work fine individually; the collision only shows up with
  both present on the same PR). Fixing it required touching the `test`
  job's comment steps too — a one-sided fix would just move which job
  wins the clobber.
- **Deliberately not blocking merge.** `perf-regression` is *not* added
  to `SDLC.md`'s `required_status_checks` — that frontmatter is
  `sdlcify`-owned branch-protection config (shared GitHub state), not
  something to hand-edit mid-issue without confirming the policy
  change separately. The job still exits non-zero (visible red ❌) on
  a `>=FAIL_PCT` regression, so it's a strong signal even though
  `agent-full` merge policy can currently proceed past it. Follow-up:
  issue #68 tracks the decision to promote it to required once real
  GitHub-runner threshold data exists.
- **This PR's own `perf-regression` run was the noise-floor validation
  it committed to.** Both binaries built from identical `src/`, so
  every delta is pure runner noise: -0.5%..+1.3% across all eight
  rows on GitHub's actual macOS-hosted runner — well inside `WARN_PCT`
  (20%). No retuning needed; `sierpinski`'s base time was 9ms on that
  runner (vs. ~5ms locally), still comfortably under the 15ms floor.
  Confirmed the comment-upsert fix at the same time: exactly two
  `github-actions[bot]` comments total on the PR and the issue (one
  `ci-test-summary`, one `perf-regression`), not four.
- **Codex review on the first pushed diff caught three more real
  bugs**, all in code the local smoke tests structurally couldn't
  reach: (1) a plain `cargo bench` (no args) now runs every
  `[[bench]]` target including `regression.rs`, which panicked on
  missing `--head`/`--base` — broke `perf.rs`'s documented bare-
  `cargo bench` dev workflow; fixed by having `regression.rs` print a
  one-line skip notice and exit 0 when *both* flags are absent
  (exactly one present without the other is still a real usage
  error). (2) A feature PR that both adds a PostScript operator and
  updates `gallery/fern.ps`/`examples/sierpinski.ps` to use it (normal
  per `AGENTS.md` — gallery art tracks the interpreter's current
  operator set) would have the *base* binary exit non-zero on that
  workload and crash the whole check with a panic, hard-failing a
  perfectly normal PR. Fixed: a base-side failure is now reported as
  "incomparable" (a distinct table row + explanatory note) rather than
  fatal; a *HEAD*-side failure still panics, since that means this PR
  broke the interpreter itself, which should never quietly pass. (3)
  The RSS-majority check in `finish()` accepted an exact tie (2 of 4
  samples) as satisfying "majority" — off-by-one in the comparison
  operator (`<` where `<=` was needed); fixed and re-verified against
  the arithmetic by hand.
- **A second Codex round on the pushed fix caught two more**: (1)
  `gh_comment_upsert.sh`'s marker lookup matched *any* comment
  starting with the marker text regardless of author — since PR
  comments are public, a human comment that happened to start with
  the same HTML-comment prefix would be matched and silently
  overwritten on the next CI run. Fixed by also requiring
  `.user.login == "github-actions[bot]"` in the lookup. (2) The final
  "failed the gate" message unconditionally claimed the failing
  workload "cleared the 15ms floor" — true for a time-metric failure,
  but the floor doesn't apply to RSS at all, so an RSS-only failure
  printed a claim that made no sense next to it. Fixed by naming the
  specific failing `(workload, metric)` pairs in the message instead
  of a generic floor-referencing sentence.
- `perf.rs`/`vs_gs.rs` untouched — they keep serving their existing
  purposes (dev-loop tripwire, gs comparison); `regression.rs` is
  purpose-built for the CI A/B and doesn't reuse their code (each is a
  handful of lines; not worth a shared module for two call sites, and
  the file-provenance requirement above means it *can't* just call
  `vs_gs.rs`'s `measure()`, which hardcodes `CARGO_BIN_EXE_pscat`).

## Surface CI test results on the PR (issue #24, 2026-08-14)

Closes issue #24: `cargo test`'s real pass/fail counts and failure
output are now posted directly on the PR and echoed to the issue it
closes, not just visible via a green/red `test` status check that
requires a trip into Actions to corroborate.

- `.github/workflows/ci.yml`'s test step now tees output to a log,
  runs under `continue-on-error: true`, and a new
  `scripts/ci_test_summary.sh` turns that log into a markdown summary
  (pass/fail/ignored counts, failing test names, panic output capped
  at 4000 chars) written to `$GITHUB_STEP_SUMMARY` on every run and
  posted via `gh pr comment --edit-last --create-if-none` on
  `pull_request` runs — edited in place on re-push rather than
  appended, so a PR doesn't accumulate one comment per commit.
- The same summary is echoed to whichever issue the PR's `Closes
  #N`/`Fixes #N`/`Resolves #N` line references, matching `work-issue`'s
  PR template — read via `PR_BODY` env var rather than templated
  directly into the `run:` script, since PR body text is
  attacker-controlled on a public repo and interpolating it into a
  shell command is a known Actions injection vector.
- A separate final step re-fails the job if the (continue-on-error'd)
  test step actually failed, so suppressing its immediate failure to
  let the summary/comment steps run doesn't weaken the `test` required
  check `SDLC.md` keys merge eligibility on.
- The two comment-posting steps run under `continue-on-error: true` —
  a fork PR's read-only `GITHUB_TOKEN` (a GitHub Actions security
  default, not something this repo works around) or a transient GitHub
  API error must not flip the `test` check red on a run where the
  tests themselves passed; the job summary already carries the same
  content as a fallback for that case.
- Confirmed via `cli/cli`'s source that `--edit-last` scopes to the
  *current authenticated user's* comments only, so `--create-if-none`
  correctly creates on the first CI run even though the issue/PR
  already has human comments from other authors (`work-issue`'s
  "Opened <PR URL>" note, etc.) — it isn't fooled into thinking a
  comment already exists.
- Cross-model (Codex) review caught a real bug: GitHub Actions' *implicit*
  default shell (no `shell:` on a step) is `bash -e` **without**
  `pipefail`, so `cargo test 2>&1 | tee test-output.log` reported
  `tee`'s exit code (always 0), not cargo test's — `steps.test.outcome`
  stayed `success` on a failing test run, silently defeating both the
  required `test` check and the "Fail job if tests failed" step.
  Confirmed empirically (`bash -e -c 'false | tee ...; echo $?'` → `0`
  vs. `bash -eo pipefail -c '...'` → nonzero) before fixing by setting
  `defaults: run: shell: bash` on the job, which gets `-eo pipefail`
  from Actions' *explicit*-bash default. Also added a
  `concurrency: {group: ci-<workflow>-<ref>, cancel-in-progress: true}`
  per Codex's second finding — without it, an old run outliving a newer
  one for the same ref/PR (superseded push, manual rerun) could finish
  last and overwrite the PR/issue comment with stale results via
  `--edit-last`.
- A second Codex round on that fix caught one more real gap: when a
  *later* push fails fmt/clippy/build (so the test step is skipped
  entirely), the summary/comment steps used to no-op too — leaving the
  *previous* commit's green "passed" comment sitting on the PR looking
  current. Fixed: those steps now run whenever the job wasn't
  cancelled (`ci_test_summary.sh` handles `skipped` as a distinct case,
  before it ever touches the log file that in that case doesn't
  exist) and post "tests did not run this time" instead of leaving
  stale data in place.
- Deliberately not fixed: that same round flagged that a *manually
  rerun* old job can still race a newer completed run and overwrite
  the comment, since `concurrency`'s cancellation is arrival-order,
  not commit-recency, and only cancels a still-*running* job, not one
  that already finished. A correct fix means comparing the run's
  commit SHA against the PR's live head before writing the comment,
  which needs an extra API call and meaningfully complicates the
  workflow for a failure mode that requires someone deliberately
  clicking "re-run jobs" on an old run after newer commits landed — not
  a path this repo's actual (agent-driven, `agent-full`-policy) flow
  exercises. Noted here rather than fixed now; revisit if a real rerun
  ever actually produces a stale comment in practice.
- Deferred: clippy/build/fmt results aren't surfaced the same way —
  the issue scoped this to test evidence specifically, and those three
  already fail the job outright with output in the existing status
  check, so there's less of a "trust me, it passed" gap to close there.

## Sweep / contact-sheet rendering (issue #21, 2026-08-14)

Closes issue #21: render a file once per seed or parameter value in a
single invocation, so exploring a design space is one command instead
of an agent hand-editing the source and re-running N times. Two
independent, mutually-exclusive sweep axes, one per invocation:

- `--sweep-seed SPEC` overrides every `srand` call *transparently* —
  `Interp::set_seed_override` (`src/interp.rs`) and a check in
  `ops/arith.rs`'s `srand` make it ignore its operand and reseed with
  the override instead, still popping the operand per the PLRM. This
  was the deliberate design call: every generative-art script in this
  repo already hardcodes its own `N srand` line (the reproducible-art
  doctrine), so making the sweep *transparent* means it works on the
  entire existing example/gallery corpus unmodified, rather than
  requiring an opt-in convention. Checked before committing to this:
  none of `examples/`/`gallery/`'s top-level pieces call `srand` more
  than once per render (`lib/artkit.ps`'s multiple hits are all in
  comments; `lib/handscript.ps`'s two real calls are two independent
  entry points, not a single render deliberately re-seeding mid-run,
  so collapsing both to one override value is fine).
- `--sweep NAME=SPEC` predefines `/NAME` in userdict before each
  frame, for a source that opts in by reading it (`/NAME where { pop
  NAME } { default } ifelse`). Implemented as a second `run_source`
  call on the same `Interp` before the real one (the same pattern
  `repl()` already uses for line-by-line input) rather than textually
  prepending `/NAME <v> def\n` to the source — a plan-review advisor
  pass caught that the prepend would shift every line by one and
  silently corrupt `error_report`'s `Line: N` attribution (issue #17).

`SPEC` is `A:B` (inclusive range, step 1), `A:B:STEP`, or a comma
list, capped at 64 values (same spirit as `--page`/`--dpi`'s existing
clamps). Output: `--png PATH` writes numbered per-frame files (reusing
the existing multi-page `numbered_path` convention); `--contact-sheet
PATH` composites every frame into one grid PNG via a new
`src/contact_sheet.rs` (`--grid COLSxROWS` overrides the default
square-ish layout) — either or both, at least one required. The
composite is capped at the same 8000px-per-side ceiling `--page`
already enforces, erroring instead of attempting a multi-gigabyte
allocation for a large sweep at high `--dpi`.

Two failure modes an advisor pass specifically flagged and this
implementation now handles: a source that never calls `srand` at all
sweeps to N identical frames with no explanation otherwise — `--sweep
-seed` now warns on stderr (`seed_override_fired`, checked across all
frames) when the override never actually fired; and a per-frame
PostScript error no longer aborts the whole sweep — later frames still
render, the failed frame's partial canvas is still written (this
CLI's existing partial-render-on-error philosophy), and the process
exits nonzero only if the caller should know something failed.

Scoped deliberately: no animation/GIF output (a contact sheet already
satisfies "compare results side by side" without a new dependency; the
issue left format open); no multi-axis (seed × param) cartesian sweep;
no `--svg`/`--pdf`/`--lint`/`--interactive`/`--spool`/`-e` combined
with a sweep (errors out clearly, same style as `--spool`'s existing
mutual-exclusion checks); no `pscat-mcp` sweep tool (CLI-only for
now — the MCP server shells out to the CLI per tool call, so a sweep
tool would need to either shell out N times itself or grow direct
library access; not built without a concrete agent workflow asking for
it). `examples/sweep_demo.ps` is the specimen — an N-petaled rosette
that demonstrates both mechanisms (a hardcoded `srand` for seed
sweeping, a `/Petals where` lookup for parameter sweeping).

A cross-model (Codex) review at the PR stage, which ran the binary
empirically rather than only reading the diff, caught six real bugs in
the first draft, all fixed before merge:

- The first draft accumulated every rendered frame in one `Vec<Pixmap>`
  before writing anything — at `--dpi 300` even the default page is
  ~34MB/frame, so a 64-frame sweep held over 2GB in memory before the
  first byte hit disk, and an oversized `--contact-sheet` was rejected
  only *after* every frame had already rendered. Fixed by streaming:
  `--png` now saves each frame as it renders; `src/contact_sheet.rs`
  gained `new_sheet`/`blit_cell` primitives so the sheet is allocated
  (and its size validated) *before* the loop starts, and each frame
  blits straight in and is dropped, never accumulating a `Vec` at all.
- A range spec like `--sweep X=0:1000000000` eagerly `collect()`ed the
  whole (billion-element) `Vec<f64>` before the `MAX_SWEEP` cap check
  ran afterward — a short string could still attempt a multi-gigabyte
  allocation. Fixed by checking the computed count against the cap
  *before* generating the range, in both `parse_sweep_spec` and the
  new seed-specific parser below.
- `--sweep-seed` parsed every value through `f64` on the way to `i64`,
  which loses integer distinctness above 2^53 (`9007199254740992` and
  `...993` collapsed to the same seed) and silently saturates an
  out-of-range value on the `as i64` cast. Fixed with a dedicated
  `parse_seed_spec` that stays in native `i64`/`i128` arithmetic
  throughout, never touching `f64`.
- `format_sweep_value`'s fixed 9-decimal rounding (added to hide
  binary float drift from *range* generation, e.g. a `0.1` step
  landing on `0.7000000000000001`) was applied uniformly, so a
  literal list value like `--sweep X=0.0000000001` silently became
  `/X 0 def`. Fixed by only rounding range-*computed* values; a
  literal typed on the command line now prints via Rust's exact
  shortest-round-trip `Display`, whatever its precision.
- `--contact-sheet`/`--grid` with neither `--sweep-seed` nor `--sweep`
  passed every validation check and then silently did nothing (no
  file written) — nothing in the non-sweep code path ever consumed
  those two options. Fixed with an explicit rejection.
- A sweep frame's `--pstack-on-error` was silently dropped: the
  per-frame error branch printed `error_report` but never called
  `print_pstack`, even though the flag is honored for an ordinary
  (non-sweep) headless run. Fixed to match.

`--grid`'s own validation was already tightened once during
plan-then-implementation review to bound both axes to `1..=MAX_SWEEP`
(a `--grid 70000x70000` typo could otherwise overflow `u32` in the
`cols * rows` multiply — a debug-build panic) before the Codex pass
even started; that fix predates and is independent of the six above.

A second Codex review round, on the fixed diff, again ran the binary
rather than only reading the patch and found five more real defects:

- The frame count was still precomputed via `((b-a)/step) as usize +
  1` (or the `i128` seed equivalent) — `--sweep X=0:inf:1` or a seed
  range spanning the full `i64` domain converts a non-finite or
  maximally-wide quotient to `usize::MAX`, then panics on `+ 1`
  overflowing before the `MAX_SWEEP` check ever runs.
- The normal (non-sweep) code path's `Interp` — with its own canvas,
  up to 256MB at max `--page` — was still constructed *before* `main`
  branched to `run_sweep`, so it stayed alive (unused) for the sweep's
  entire run: on top of the frame streaming round one's fixes already
  added, an unrelated third canvas sat in memory the whole time.
- `--sweep X=9007199254740993` silently became `/X 9007199254740992
  def` — the *generic* parameter path had the same `f64`-precision
  bug `--sweep-seed` was fixed for in round one, just not yet applied
  there.
- The fixed `1e-9` tolerance added to hide *legitimate* float drift
  (e.g. `0:0.9:0.3` needing a nudge to include its true last value)
  also let a value cross a *genuine* upper bound: `--sweep
  X=0:0.9999999999:1` generated `X=1`, past the declared B.
- `--contact-sheet`/`--grid` with no sweep axis at all was already
  fixed in round one, but a sweep active with `--png`/`--grid` and no
  `--contact-sheet` slipped through the same class of gap — `--grid`
  validated cleanly and `run_sweep` never read it.

Given the same root cause kept resurfacing, the fix wasn't another
patch: `--sweep NAME=`'s value type became `SweepValue::Decimal(i128
numerator, u32 scale)` — every plain decimal literal ("5", "-3.25",
"9007199254740993", "0.0000000001") is parsed and generated with pure
integer arithmetic (`parse_decimal_exact`/`format_decimal`), so a list
value can't lose precision and a range can't drift *or* overshoot its
bound: a range now generates values by iterating with a bounds check
*inside* the loop (never precomputing a count that could itself
overflow) and comparing exactly against the declared upper bound (no
tolerance to get wrong). `SweepValue::Float(f64)` is now
only a fallback for a literal that isn't a plain decimal (scientific
notation), explicitly rejecting non-finite values. `checked_mul`/
`checked_add` guard the one theoretical remaining overflow — rescaling
two operands at very different decimal precisions to a common scale —
so even that edge case errors instead of panicking. `--sweep-seed`
already avoided `f64` from round one; its own count-overflow bug got
the same iterate-with-inline-bounds-check fix.

A third review round, on that rewrite, found the one class of value
`parse_decimal_exact` still didn't read exactly: scientific notation
("1e-20") still fell to the `Float`/`f64` fallback, which a fixed
12-decimal-rounding then truncated to `0` on the way out — the exact
same precision bug round two had just fixed for plain decimals and
huge integers, one input form further out. `--sweep
X=0e0:9.999999999e-1:1e0` (scientific notation for the round-two
overshoot example) reproduced that bug too, for the same reason.
Rather than special-case scientific notation's formatting or its range
tolerance yet again, `parse_decimal_exact` itself now reads a
mantissa+exponent literal into the same exact `(numerator, scale)`
representation as a plain decimal (`1e-20` → `(1, 20)`, i.e. exactly
`1 / 10^20`) — folding the entire scientific-notation case into the
already-exact Decimal path instead of leaving it in the lossy
fallback. With that, the `Float` fallback's range tolerance (still
there to hide legitimate `f64` drift for a value past
`parse_decimal_exact`'s own 30-digit-shift precision cap) was also
simplified to a direct bound comparison — the tolerance's original
job was entirely about plain-decimal literals, which no longer reach
that code path at all.

A fourth review round found four smaller, genuinely edge-case defects,
all fixed rather than dismissed as unrealistic given the previous three
rounds' hit rate on this exact area: an exponent near `i32::MIN`
(`1.0e-2147483648`) underflowed the `exp - frac_len` subtraction in
`parse_decimal_exact` (`checked_sub` now); a value past that function's
own precision cap where `step` was smaller than an ULP at that
magnitude (`1e31:1e31`) never advanced in the `f64` fallback loop, so
it kept pushing the same value until wrongly erroring past `MAX_SWEEP`
instead of yielding the one correct frame (the loop now detects no
forward progress and stops); the comma-list `f64` fallback accepted
"inf"/"nan"/an overflowing literal like "1e400" (which parses to
infinity, not a parse error) instead of rejecting them the way the
range path already did; and `contact_sheet::compose` — a public
library function, not just `main.rs`'s own already-validated call
site — didn't check `cols*rows >= pages.len()` before blitting,
letting an undersized grid index past the sheet's own cell count and
panic.

A fifth round found the deepest issue of the five rounds: **each swept
frame's `Interp` leaks its `userdict`, not just the small systemdict
node HANDOFF.md's "one leak per Interp, process-lifetime object" gotcha
already documents.** systemdict holds a strong `Rc` to itself *and* to
`userdict` (`Interp::with_page_scaled`); with plain `Rc` and no cycle
collector, dropping an `Interp` can never free either. For the usual
one-`Interp`-per-process pattern that's genuinely inert — one bounded
leak, and the OS reclaims everything at exit anyway — but the sweep
loop is the first caller in this codebase to construct many `Interp`s
within a single process run, so "doesn't matter" stopped holding: a
program that stores meaningful data in `userdict` (a lookup table, a
large string) would leak a full copy of it, on top of the canvas, per
frame. Confirmed empirically before trusting the claim or dismissing
it — the review itself ran the binary rather than only reading the
diff, so the fix held to the same bar: a temporary diagnostic test
using `Rc::downgrade`/`Weak::upgrade` around a real `Interp` drop
showed `userdict` genuinely still alive afterward (a first RSS-based
memory measurement had misleadingly suggested otherwise, most likely
masked by zero-filled pages never actually being touched). The fix,
`Interp::break_permanent_dict_cycle` (`src/interp.rs`) plus a small
`Dict::clear` (`src/object.rs`), empties systemdict right before a
sweep frame's `Interp` is dropped — safe because nothing runs
PostScript on it again — breaking both the self-reference and the
`userdict` reference in one step; `run_sweep` calls it once per frame.
The diagnostic test became the permanent regression: assert `userdict`
frees with the fix, not just that memory stayed low.

Two smaller findings rounded out the fifth pass: `contact_sheet::
new_sheet`'s own `u32` arithmetic could overflow for a caller passing
large enough `cols`/`cell_w`/`gap` — `main.rs`'s own call site happens
to stay under that, but it's a public function — now checked `u64`
arithmetic, not just wider (two near-`u32::MAX` terms summed can still
overflow `u64`, so `checked_mul`/`checked_add` closes it for real); and
the `f64` fallback range's `a + i as f64 * step` form could overflow
its intermediate product to infinity for an opposite-sign range near
`f64`'s own limits (`-1e308:1e308:1e308`), silently dropping the
inclusive upper-bound endpoint even though every real value along the
way was finite — fixed by accumulating incrementally (`v += step`)
instead of recomputing from an index each time.

A sixth pass was considered but not run: by round five the findings had
shifted from bugs in this issue's own diff to a pre-existing property
of the interpreter (`Interp::with_page_scaled`, documented in
`HANDOFF.md` well before issue #21) and to inputs no real caller would
send (`--grid 70000x70000`, `1e400` seeds) — a signal to stop reviewing
and instead act on what round five had already surfaced, rather than
asking a reviewer that has run out of diff to review for one more
round. Two things from round five's own findings still needed
follow-up, both applied without a further review round:

`break_permanent_dict_cycle` originally took `&mut self` with a doc
comment saying "safe to call only once you're done with this
`Interp`" — a real but purely-documented guarantee on a `pub fn` in a
library crate. Changed it to take `self` by value instead: calling it
now consumes the `Interp`, so a caller that mistakenly tried to keep
using it afterward gets a compile error (use of moved value) rather
than a runtime `undefined` once systemdict's operators are gone. This
is a better fix than a stronger doc comment or a debug assertion
because the compiler enforces it unconditionally, not just in debug
builds or for a reader who read the comment.

Re-reading the fix with "who else constructs many `Interp`s in one
process run" in mind (the exact question round five's finding raised)
turned up a second, live instance of the same leak that hadn't been
touched: `--spool`'s watch loop (`window.rs::poll_spool`) constructs a
fresh `Interp` per job and assigns it into a struct field
(`self.interp = interp`), which drops the *previous* job's `Interp` in
place — and unlike the sweep loop's bounded frame count, `--spool`
runs indefinitely, so this leaked one systemdict/userdict cycle per
completed job for the life of the process. Fixed with `mem::replace`
to pull the outgoing job's `Interp` out of the field before the new
one overwrites it, then `break_permanent_dict_cycle()` on the
returned value. `HANDOFF.md`'s gotcha note now names both call sites
so a future third one doesn't get missed the same way.

## Axial/radial gradient (shading) fill support (issue #20, 2026-08-14)

Closes issue #20. The interpreter had no `shfill`/shading machinery at
all before this (a grep of `src/ops/graphics.rs` turned up nothing) —
the issue flagged real uncertainty about whether this was an
artkit-layer or interpreter-layer feature, and it turned out to be
both: `shfill` (PostScript Level 3's real shading-fill operator —
confirmed against gs 10.07.1, `/shfill where` finds it, `/sh where`
doesn't) landed at the interpreter level (`src/shading.rs`,
`src/ops/shading.rs`, `Gfx::shfill` in `src/gfx.rs`), with a
convenience layer on top in `lib/artkit.ps`'s new "gradients" section
(`gradfn`/`axialsh`/`radialsh`/`gradfill`). A first draft named the
operator `sh` instead — that's PDF's content-stream operator of the
same shape (a shading painted directly, no pattern colorspace needed),
but it takes a shading *resource name*, not the dictionary directly,
and doesn't exist as a PostScript-language operator at all. Every
hand-test during development called it `sh` and nothing caught the
mistake; a Codex review at the PR stage (checked against gs directly,
not just asserted) did.

Scoped deliberately, not just to the easy subset: `ShadingType` 2
(axial) and 3 (radial), `ColorSpace` `/DeviceGray`/`/DeviceRGB`/
`/DeviceCMYK`, and `FunctionType` 2 (exponential interpolation) and 3
(stitching — what makes multi-stop gradients possible, and what real
design tools like Illustrator actually emit for anything beyond a
plain two-color ramp). The 2/3-only cut on functions isn't arbitrary:
those two types are pure arithmetic over dict contents, so evaluating
them never needs to run a PostScript procedure through the machine —
unlike `FunctionType` 0 (sampled) or 4 (PostScript calculator), which
would need the same `Frame::PostOp` continuation pattern `Separation`
colorspace uses for its tint transform (`ops/color.rs`). Staying
inside 2/3 keeps the whole feature synchronous, with no interpreter
reentrancy questions to answer. `FunctionType` 0/4, an array of
one-in-one-out functions in place of a single N-out function, and
Indexed/Separation as a shading's `ColorSpace` are all documented
gaps, same style as the codebase's existing Indexed/Separation
deviations in `ops/color.rs`.

`Gfx::shfill` paints via the same masked-full-page-rect mechanism
`fill`/`clip` already use, so `gsave <path> clip shfill grestore` (the
standard idiom, and what `gradfill` wraps) bounds it exactly like any
other paint operator. The one design decision that isn't obvious from
reading tiny-skia's docs alone: Coords/radii pass through in *user*
space, with the current CTM handed to the gradient shader as its own
`transform` parameter, rather than pre-mapping the two endpoints via
`user_to_device`. Verified empirically before committing to it (a
render test under `45 rotate`-equivalent rotation+anisotropic-scale,
`sh_axial_gradient_respects_rotation_and_anisotropic_scale` in
`tests/render.rs`) rather than trusting the read of tiny-skia's
internals alone — an `advisor` review flagged this as the plan's one
load-bearing assumption worth confirming before building the rest on
top of it, and it came back correct on the first try. The alternative
(pre-transform points, scale the radius by `ctm_scale()` like `stroke`
already does for width) would have gotten the axis direction right but
the perpendicular banding wrong under anisotropic scale.

`/Extend` is parsed and shape-validated but always behaves as
`[true true]` (`SpreadMode::Pad`) — documented deviation, same style
as "filter params accepted and ignored" elsewhere in this codebase.
The realistic idiom already bounds the painted region with an
explicit clip, so implementing `false` (transparent beyond the axis)
would only matter for an edge case that idiom doesn't hit; getting it
exactly right would need a genuinely separate mechanism (a geometric
band test, not a spread-mode trick — tiny-skia's stop positions can't
go negative, so there's no way to fake "transparent beyond bounds"
with stop alpha alone without a discontinuity exactly at the true
boundary).

A shading's color ramp is pre-sampled into a fixed stop list rather
than evaluated per-pixel, handed straight to tiny-skia's own
stop-to-stop linear interpolation. Two rounds of `advisor` review
shaped this:

The plan review raised two panic risks in the original sketch
(`stitch_index` reachable with an empty `Functions`/`Bounds` array on
malformed input, and non-finite color components from `N ≤ 0` or
non-monotonic `Bounds` silently painting black via an `unwrap_or`),
fixed with upfront shape/finiteness validation in
`parse_function`/`build_stops` before any of that reached tiny-skia.

The implementation review (after the code was written and all tests
were green) caught a real, silent-wrong-render bug the plan review
couldn't have: `build_stops` was evaluating the function at t-values
sampled from the *function's* own `/Domain`, while normalizing those
same samples' gradient position against the *shading's* `/Domain` —
correct only when the two domains happen to coincide, which every
existing test used (the default `[0 1]`) without exercising the
general case. A shading's `/Domain` maps geometric position onto the
function's input; the function's own `/Domain` only clips that input
once it arrives — different axes, silently conflated. Fixed by
sampling gradient *positions* directly and mapping them into t-space
via the shading's domain (`PsFunction::sample_positions`), with each
interior stitching-leg boundary (`interior_bound_positions`) and each
point where the shading's swept t-range crosses the function's own
domain edge (`clamp_corner_positions` — `eval` clamps there, so the
color goes flat beyond it, and folding in the corner itself is enough
to render that flat region exactly) folded in as exact stops. That
same review pointed out the corollary: since tiny-skia/SVG both
interpolate linearly, a function that's exactly piecewise-linear
(every leg's `N == 1`, which is what `gradfn` always emits) needs
*only* those exact-boundary stops to render correctly — no dense
sampling at all — cutting a four-stop `axialsh` gradient's SVG output
from dozens of `<stop>` elements to four. A third, smaller pass closed
a panic risk the fix itself introduced (non-finite `Domain`/`Bounds`
values, reachable from a real literal like `1e400` since the lexer's
`f64` parse doesn't reject those, reaching an `.expect()` in the sort
comparator) with finiteness validation at the same parse boundary as
everything else, plus a total-order comparator as defense in depth.

Export: SVG gets real `<linearGradient>`/`<radialGradient>` support
(`SvgRecorder::shfill`), using the same `gradientUnits="userSpaceOnUse"` +
`gradientTransform` trick as the raster path, and SVG2's `fx`/`fy`/
`fr` for the two-circle radial case — emitted only when `r0 > 0`, so
the common burst-from-a-point case (`r0 = 0`) stays plain SVG 1.1
markup that renders identically everywhere, and only the rarer
two-circle case depends on `fr` (uneven renderer support, but
degrades to an approximate, not broken, render when ignored). PDF
export approximates a shading as a flat fill in the ramp's average
color (`PdfRecorder::fill`, reused as-is) — real PDF shading
dictionaries need pattern-colorspace machinery this exporter doesn't
have, and building that was out of scope; a `tests/pdf.rs` regression
test (`shfill_appears_as_an_average_color_fill_in_pdf_only_export`, same
pattern as the existing `strokes_appear_in_pdf_only_export` guard from
issue #8/#23) exists specifically to keep this an intentional
approximation rather than a silent content-drop.

`examples/gradients.ps` is the specimen sheet (raw `shfill` for a two-stop
axial ramp, `axialsh` for a four-stop ramp, `radialsh` for both a
point-burst and a two-circle gradient) — clean under `--lint`.

The same Codex review that caught the operator name also checked the
implementation against gs directly rather than just asserting from the
spec, and found four more real defects, all pinned by fixes and a
render.rs regression test each:

- A reversed *function* `/Domain` (e.g. `[1 0]`) reaches `eval`'s
  `x.clamp(domain.0, domain.1)`, and `f64::clamp` panics if `min >
  max` — reachable with an empty `Bounds` array, which the
  bounds-monotonicity check alone doesn't rule out. Confirmed against
  gs that a *function's* own Domain must be non-decreasing (rangecheck
  otherwise) — but a *shading*'s top-level `/Domain` is different: gs
  accepts that one reversed, as the documented way to flip a
  gradient's direction, and this file's own domain-mapping arithmetic
  already handles it correctly either way. `parse_function` now
  rejects only the former; `eval`'s Stitching arm also switched to the
  same min/max-clamped pattern the Exponential arm already used, so
  the panic can't resurface even if a future caller builds a
  `PsFunction` some other way.
- `FunctionType` 2's `C0`/`C1` were required (confirmed against gs
  they default to `[0.0]`/`[1.0]` when omitted) while `N` silently
  defaulted to `1.0` (confirmed against gs it's required — omitting it
  is a rangecheck). Exactly backwards: rejected valid dicts, accepted
  invalid ones.
- A genuinely discontinuous stitching function (e.g. a constant-red
  leg followed by a constant-blue leg — a real, spec-legal shading,
  just not one `gradfn` ever emits, so nothing exercised it) rendered
  wrong: `stitch_index`'s `x < b` test means a sample taken exactly at
  a bound always resolves to the *right* leg, so the single stop
  `interior_bound_positions` placed there smeared the *entire
  preceding leg's segment* into a false gradient instead of a hard
  edge. Fixed by placing two stops straddling each bound by a small
  t-space epsilon instead of one exactly on it — small enough that a
  *continuous* bound (`gradfn`'s own output) still reads as one exact
  stop in practice.
- The optional `/Range` key (clips each evaluated output component,
  independent of `/Domain`, which clips the input) wasn't parsed at
  all. Added, but deliberately only at the top-level Function a
  shading dict names directly, not recursively per stitching leg —
  real-world Range usage is overwhelmingly on a simple function used
  directly, and supporting it per-leg would have meant threading a new
  field through every `PsFunction` variant (and every test fixture
  constructing one directly) for a case nothing here actually needs;
  documented gap, same style as the other scope cuts in this file.

Also SVG-specific: the two-circle radial branch only emitted `fx`/`fy`
(the *focal point*) when `r0 > 0`, conflating them with `fr` (the
*focal radius*, the actually-uncommon SVG2 feature) — a burst-from-a-
point gradient (`r0 = 0`) whose start point wasn't also the outer
circle's center silently recentered on the wrong point. `fx`/`fy` are
plain SVG 1.1 and now always emitted (a no-op when they do coincide
with the center); only `fr` stays gated on `r0 > 0`.

A *second* Codex review, run on the fixed diff before merging (per
`work-issue`'s policy: re-review the pushed fix, don't just trust the
first pass caught everything), found five more real defects the first
round's fixes had either introduced or left unguarded — all against
gs again, not just asserted:

- `parse_function`'s recursion (for a Type 3's `/Functions` array) had
  no depth limit: a few thousand levels of acyclic nesting, or a
  self-referential dict built via `put` after construction, overflows
  the Rust stack instead of raising a catchable error. Capped at
  `MAX_FUNCTION_DEPTH = 32`, past which it's a `limitcheck`.
- The `/Range` support added in round one had the exact same
  reversed-pair panic risk the round-one fix had *just* closed for
  `/Domain`, freshly introduced: `build_stops` feeds an unvalidated
  `(lo, hi)` pair straight into `f64::clamp`, which panics if `lo >
  hi`. Confirmed against gs that `/Range [1 0]` is a rangecheck; now
  validated at parse time.
- `/Domain` on a *function* dict was silently defaulted to `[0 1]`
  when absent (via the same `get_domain` helper the *shading*'s own
  optional top-level Domain uses) — confirmed against gs that it's
  required on a function (rangecheck if missing), unlike the
  shading's.
- The piecewise-linear "exact 2-stop" fast path from round one didn't
  account for what happens *after* `eval`: `/Range` clamping can
  introduce a knee partway through an otherwise-linear ramp, and
  DeviceCMYK's `(1-c)(1-k)`-shaped conversion isn't linear in general
  even when every component ramps linearly in `t` (C0=[0,0,0,0] to
  C1=[1,0,0,1] makes red `(1-t)²`, not `1-t`) — so the fast path was
  silently rendering the wrong curve for CMYK content and anything
  with a Range. Gated on `range.is_none() &&
  matches!(cs, Gray | Rgb)` now; DeviceCMYK and Range always take the
  dense-sampling path.
- The fast path also didn't account for *nested* stitching: `is_piecewise_linear`'s
  recursion checked a nested Stitching leg's own children's `N`
  values, classifying a function as exact even when a nested leg had
  its own hard color-stop bound — which `interior_bound_positions`
  can't see (it only reads `self`'s own top-level `bounds`), so that
  bound's discontinuity was silently smeared into a continuous ramp
  the same way the round-one single-level bug did, just one level
  down. Restricted to exactly one level: a leg that's itself a
  `Stitching`, not a plain `Exponential`, disqualifies the whole
  function from the fast path — imprecise for that (rare, `gradfn`
  never produces it) case, but no longer silently wrong.

A *third* Codex review, on the round-two fix, found five more —
against gs again, plus one against `tiny-skia`'s actual source after
this file's own comment turned out to have been wrong about it:

- `/BBox` (an optional shading-dict key that further clips the painted
  region, in the same user space `Coords` uses) was parsed nowhere and
  ignored entirely — confirmed against gs it's enforced (pixels
  outside it stay untouched even though `Coords`/`Extend` would
  otherwise reach them). `Shading` now carries it; `Gfx::shfill` builds
  its paint path from the transformed BBox corners instead of the
  whole page when present — a change that, because the raster/SVG/PDF
  export all already shared that one `path` value, needed no changes
  to the export code itself.
- `CsKind::Cmyk::to_rgb` computed `(1-c)(1-k)` etc. straight from
  whatever components `eval` produced, without clamping them to
  `[0,1]` first — confirmed against gs's own `setcmykcolor` (`-1 0 0
  0.5 setcmykcolor` reads back `0.5 0.5 0.5`, i.e. C clamped to 0
  *before* the product, not the naive `(1-(-1))*(1-0.5) = 1.0`
  clamping only the result). Out-of-range components are reachable
  from an ordinary Type 2 function — C0/C1 aren't themselves range-
  checked — so this wasn't a synthetic edge case.
- `/Extend` with the wrong array length returned the same `typecheck`
  as a right-length array with non-boolean elements; confirmed against
  gs a wrong length is `rangecheck`. Split into two checks.
- Coincident axial endpoints (`Coords [x y x y]`) are a no-op in gs,
  even with `Extend [true true]` — but tiny-skia 0.12's
  `LinearGradient::new` in Pad mode does *not* return `None` for this,
  despite what `Gfx::shfill`'s own comment claimed: re-reading the
  actual source (prompted by this review, not the earlier read its
  doc comment rested on) shows the degenerate-length branch returns
  `Some(SolidColor(last stop))` for Pad specifically, so the whole
  clip was silently filling with C1 instead of staying untouched.
  Checked explicitly before construction now, rather than trusting
  the return value.
- A *shrinking* radial (`r0 > r1`, PostScript-legal — confirmed
  against gs) broke the SVG export: SVG requires `fr <= r`, and the
  code always mapped PostScript's start circle to `fx/fy/fr` and its
  end circle to `cx/cy/r`, regardless of which was bigger. Now swaps
  which circle is "outer" vs "focal" when `r0 > r1`, and reverses the
  stop offsets to compensate (SVG's offset 0 is always the focal
  circle).

A *fourth* Codex review, on the round-three fix, found four more:

- `/BBox` support (round three) only reached the raster and PDF paint
  paths, which already shared one `path` value — the SVG call site
  still always drew a full-page `<rect>`, ignoring `/BBox` entirely.
  `SvgRecorder::shfill` now takes that same `path`'s SVG data and
  paints a `<path>` instead of an unconditional whole-canvas `<rect>`,
  so `/BBox` (or lack of one) reaches SVG export the same way it
  already reached the other two.
- The PDF average-color fallback was an unweighted mean over the stop
  list, not weighted by how much of the `[0,1]` axis each stop
  actually represents — `build_stops` packs extra stops at stitching
  bounds and domain-clamp corners, so a ramp that's red through
  position 0.99 and blue only for the last 1% has just a handful of
  stops clustered near that boundary, and an unweighted mean rendered
  it roughly 50/50 purple instead of ~99% red. Switched to trapezoidal
  integration weighted by each consecutive pair's position gap.
- The axial coincident-endpoint check (round three) used a fixed
  `1e-6` epsilon in *user* space — scale-blind: under a large CTM
  (`1e8 1e8 scale`), two Coords 1e-7 apart (under the threshold, so
  treated as coincident and skipped) still span 10 device units, a
  real, visible near-hard transition. Switched to exact equality,
  which is scale-invariant by construction and precisely matches the
  one case actually confirmed against gs (literal coincidence);
  anything short of exact equality renders as an ordinary, if sharp,
  gradient, same as gs would.
- SVG's two-circle radial model only renders *faithfully* when the
  focal circle is entirely inside the outer one
  (`distance(centers) + focal_r <= outer_r`); an off-center
  `ShadingType` 3 that fails that (real, valid PostScript — no such
  constraint exists there) isn't detected or worked around, so its SVG
  export can visibly diverge from the raster. Pointed out because this
  file's *own* two-circle test and, it turned out, one hand-picked
  test geometry (`examples/gradients.ps`'s own two-circle panel was
  actually fine, checked by hand after the fact) were themselves
  uncontained — replaced with contained geometry; documented as a gap
  rather than building a rasterized-fallback embedding for SVG (the
  `image` operator's PNG-embedding machinery could do it, but that's a
  bigger lift than this pass warranted).

A *fifth* Codex review, on the round-four fix, found one real bug and
raised one suspected gap that turned out, on direct testing against
gs, not to be one:

- **Real:** a stitching `Bounds` entry coinciding with the function's
  own `/Domain` edge (`/Domain [0 1] /Bounds [1]`, giving the *last*
  leg zero width — accepted by gs) rendered as a full-width smooth
  gradient instead of the near-solid color gs actually shows (a bound
  exactly at the domain's *own* start is a no-op by construction —
  `eval` always resolves it to the leg *after* the zero-width one
  already — but a bound at the domain's *end* needs the same
  discontinuity-preserving straddle treatment the interior case
  already had, and `interior_bound_positions`' strict `>`/`<` filter
  excluded exactly this edge case). Relaxed to `>=`/`<=`; verified
  directly against gs before and after (solid red across the whole
  visible axis both times, matching within a hairline).
- **Not a bug, on inspection:** the review flagged coincident-center,
  equal-*positive*-radius radials (`/Coords [50 50 20 50 50 20]`) as
  another case where tiny-skia returns `None` and nothing paints. That
  claim traced back to this file's own comment, which still said so
  after the axial-specific version of the same claim was corrected two
  rounds ago — the radial mention was never re-checked. Rendering the
  exact case directly (both against gs and against this branch's own
  build) shows it already matches gs pixel-for-pixel: tiny-skia's
  degenerate fallback for a coincident-center, positive-equal-radius
  pair returns `Some` (a solid disk of the start color, nothing
  painted beyond it), not `None` — the same fallback shape already
  confirmed for the *shrinking-but-distinct* case two rounds back, just
  not re-verified for this exact-equal-radius variant until now. Fixed
  the comment; left the (already-correct) behavior alone. Worth
  recording precisely because it's a case where taking a Codex finding
  at face value, without reproducing it first, would have meant
  "fixing" code that wasn't broken.

A *sixth* Codex review, on the round-five fix, found five more, all
confirmed against gs before fixing:

- tiny-skia's own internal degenerate-length threshold (~3e-5,
  `DEGENERATE_THRESHOLD`) still applies to the *raw* user-space points
  `Gfx::shfill` hands it, even after round four's fix to this file's
  *own* coincident-check — a tiny-but-nonzero raw axis (`/Coords
  [0 0 1e-7 0]` under `1e8 1e8 scale`, a real 10-device-pixel
  gradient) still got silently flattened to solid C1 by tiny-skia
  itself before ever reaching that check. Fixed by rescaling the
  geometry and compensating via `ctm.pre_scale` before construction —
  an exact reparametrization (`ctm' = ctm.pre_scale(1/k, 1/k)` exactly
  undoes multiplying the points by `k`), not an approximation, so it
  can't regress the anisotropic-scale/rotation correctness the
  raw-points-plus-CTM-transform design was built around. The round-4
  test for this exact axis had been asserting the *wrong* expectation
  the whole time — it queried the exact device-space axis-start pixel
  and expected the far/extended color, which only "passed" because the
  whole gradient had gone flat; rewritten with the actual device-space
  geometry worked out by hand.
- The domain-span "treat as zero" checks (three call sites) used
  `<= 1e-12` instead of exact zero — the same class of unjustified
  epsilon round three's `/Range`/`/Domain` reversed-pair fixes had
  already replaced with exact comparisons elsewhere in this file, just
  not here yet. `/Domain [0 1e-13]` (confirmed valid and non-flat
  against gs) collapsed to one constant color. Switched to `== 0.0`;
  the straddle epsilon in `interior_bound_positions` also had an
  absolute `.max(1.0)` floor that turned out *larger than that whole
  tiny domain*, collapsing both straddle points onto the same domain
  edge — removed, so the epsilon now scales purely proportionally to
  whatever the function's own domain span actually is.
- A negative `/N` was accepted (confirmed against gs it's a
  rangecheck) and could reach `x.powf(n)` at `x=0` as `+inf`, which the
  finite-component check downstream would only catch for stops that
  happen to sample exactly `x=0`. Rejected at parse time instead.
- `ShadingType`/`ColorSpace`/`Function` absent from a shading dict
  returned `rangecheck` (via the shared `get` helper every other key
  in this file also uses) where gs reports `undefined`; `Coords` and
  most function-dict keys are genuinely `rangecheck` on gs's own
  behavior and stay that way. Split into a separate `get_required`
  helper for just those three keys, rather than changing `get` itself.
- SVG's `fmt_f32` truncates to 3 decimals — fine for device-pixel path
  data, wrong for gradient-*local* coordinates and stop offsets, which
  a `gradientTransform` can multiply back up by an arbitrary factor (a
  shading authored in tiny raw units the same way the raster-side fix
  above targets, or this file's own straddle-epsilon stop-offset
  pairs, which sit far below 3-decimal precision and were silently
  merging back into a single offset in SVG output specifically, even
  though the raster and PDF paths render the discontinuity correctly).
  Added `fmt_precise` (plain `{v}` Display, which already gives the
  shortest round-trip-exact string) for exactly the values inside
  `SvgRecorder::shfill` that need it, leaving `fmt_f32` for the
  genuinely device-pixel-scale values elsewhere in this file.

## Noise and flow-field procedures for artkit (issue #19, 2026-08-14)

Closes issue #19. Added a "noise / flow fields" section to
`lib/artkit.ps`: `noiseinit`/`noise2` (classic 2D gradient/Perlin
noise — a 256-entry Fisher-Yates-shuffled permutation table, 8 fixed
unit/diagonal gradient directions selected by `hash & 7`, quintic
fade, bilinear interpolation), `curl2` (turns *any* `{x y -> n}`
scalar-field proc into a flow by central-differencing its
perpendicular gradient and normalizing to a unit vector), and `advect`
(traces one particle through a `{x y -> dx dy}` field proc as a
sequence of `lineto`s). Three composable primitives, not a
`noise2`-specific convenience wrapper — matches the file's existing
pattern (`grid`/`hexgrid`/`gasket` all take caller procs rather than
hardcoding what they paint).

Scoping went through a real revision, not just a first-draft
adjustment: `curl2`/`advect` originally wrapped their own bodies in a
private `N dict begin ... end` on every call, reasoning that they take
a caller-supplied field proc (so the gasket/carpet/hexgrid nested-
composition gotcha applies) and run orders of magnitude less often
than `noise2` (~10^5 samples/piece), so the dict-alloc cost seemed
cheap insurance. A cross-model (Codex) review at the PR stage found
the real cost of that convenience: the private dict is current for
every field-proc call, not just nested ones, so an entirely ordinary
(non-nesting) field proc that tries to hold plain `def`-based state
across calls — the same thing every other artkit callback can do —
gets silently discarded the moment the dict closes; confirmed
empirically (a `/calls calls 1 add def` counter read back as `0` after
3 `advect` calls). Switched `curl2`/`advect` to plain global scratch
(`c2*`/`ad*`), matching `gasket`/`carpet`'s own precedent exactly: the
library doesn't protect itself from a caller who nests, the caller
wraps *their own* inner call in a private dict if they need to nest
(documented in the section header, same wording gasket's header
already uses). Four regression tests now cover both directions per
primitive: `curl2_uses_plain_globals_so_an_ordinary_field_proc_can_
hold_state` / `advect_uses_plain_globals_so_an_ordinary_field_proc_
can_hold_state` pin the common case that motivated the change, and
`curl2_nested_in_its_own_field_proc_needs_the_inner_call_wrapped_in_a_
dict` / `advect_nested_in_its_own_field_proc_needs_the_inner_call_
wrapped_in_a_dict` pin both halves of the nesting caveat (unwrapped
measurably corrupts the outer call's result; wrapped restores it
exactly), mirroring `gasket_nested_in_its_own_leaf_needs_the_inner_
call_wrapped_in_a_dict`'s own two-part structure. Rendered output
(`examples/noise.ps`, `gallery/lodestone.ps`) is byte-identical before
and after the change, confirming it's purely a scoping fix, not a
math change.

The same review also caught an overclaim in `curl2`'s original
docstring: "divergence-free by construction, so particles neither pool
nor source" is true of the *unnormalized* perpendicular gradient (an
exact vector-calculus identity) but not of the *unit vector* `curl2`
actually returns — normalizing by a position-dependent magnitude does
not preserve the identity in general, measured directly (not just
argued) at around -0.27 divergence for a field with a strongly varying
gradient (`x*y`) at one test point. Normalizing anyway is the standard
curl-noise tradeoff (a raw curl vector's magnitude swings with local
field steepness, unusable for `advect`'s fixed-stepsize walk); the
docstring and a new pinned test
(`curl2_output_is_not_exactly_divergence_free_after_normalization`)
now say so accurately instead of overclaiming an exact guarantee the
returned value doesn't have.

A second Codex review round (after pushing the scoping/docstring fixes
above) found a third real bug: `curl2`'s flatness guard used an
arbitrary absolute cutoff (`c2len 1e-9 gt`) that didn't actually match
what its own docstring already promised ("0 0 if the gradient is
*exactly* flat") -- a field with a real but tiny uniform gradient
(`(x+y)*1e-10`) finite-differences to a magnitude under `1e-9` and got
misclassified as flat, so `advect` would stop immediately on a field
that never actually goes flat. A genuinely flat/constant field's
finite difference subtracts two identical evaluations to exactly
`0.0` (not just something small), so the fix is `c2len 0 gt` --
matches the docstring exactly now instead of approximating it, and
`curl2_normalizes_a_low_amplitude_but_genuinely_nonzero_gradient` pins
Codex's own repro case (`2 3 1 { add 1e-10 mul } curl2`) against the
field's analytically-known curl direction.

Two more real bugs caught during development, both empirically, before
either reached a permanent test:

- **`-1 255 and` vs. `mod` for negative lattice coordinates.**
  `noise2` needs to wrap negative lattice indices into the 256-entry
  permutation table; `mod` truncates toward zero and returns negative
  remainders for negative dividends (would index `Perm` out of range
  for `x<0` or `y<0`), while `and 255` on this interpreter's two's-
  complement ints gives the correct floor-mod-256 result — checked
  directly against the interpreter (`-1 255 and` is `255`) before
  committing to the design, then pinned by
  `noise2_is_exactly_zero_at_every_integer_lattice_point` (checked at
  negative and mixed-sign lattice points specifically) and
  `noise2_is_continuous_across_lattice_boundaries_positive_and_negative`.
- **A field proc that doesn't consume its `x y` arguments silently
  leaks stack values instead of erroring.** Building the nesting-
  regression test for `advect`, an early draft's field proc pushed
  `1 0` without first popping the `x y` `advect` hands it — the bug
  was invisible when only checking `currentpoint` afterward (a
  graphics-state query, unaffected by operand-stack garbage), and
  only surfaced once the test asserted the exact stack length. Now
  `advect_nested_in_its_own_field_proc_needs_the_inner_call_wrapped_in_a_dict`
  asserts `got.len() == 2` explicitly, and `--lint` (issue #17) was run over
  every touched `examples/`/`gallery/` file — it caught the same
  class of bug directly in `examples/noise.ps`'s first draft, where
  the demo's `advect` panel was accidentally passed the scalar
  `noise2`-wrapper proc `curl2` itself expects, instead of a proper
  2-value flow-vector proc.

New demo: `examples/noise.ps`, a three-panel specimen sheet (`noise2`
as a tinted grid, `curl2` as a grid of direction arrows, `advect` as
fifty traced particles — the same shared field threading all three).
New gallery piece: `gallery/lodestone.ps`, "Lodestone" — a naturalist's
demonstration plate of 1,400 `advect`-traced iron filings around a
jittered rock, following a `curl2` field built from a hand-composed
potential (coherent `noise2` texture plus a term proportional to
distance from the stone). Curl is the perpendicular gradient, so a
purely radial potential curls into concentric tangential flow — any
radial field's gradient points straight at/away from the center;
rotate that 90 degrees and it runs in loops around it instead — and
the noise term breaks the perfect circles into the ragged, organic
loops real filings make. No new artkit.ps API needed for that
composition; it's exactly the kind of caller-side proc composition
`curl2`'s generic signature was designed for.

Deliberately not built: fbm/multi-octave noise (not one of the three
things the issue named, and single-octave noise already demonstrates
"coherent noise field"), simplex noise (its main advantage over
gradient/Perlin — avoiding directional artifacts at higher dimension —
doesn't matter at 2D), and a `curl2`-of-`noise2` convenience wrapper
baked into the library (`curl2`'s docstring shows the one-line wrapper
verbatim instead — `/flow { 0.02 mul exch 0.02 mul exch noise2 } def`
— keeping the library to 3 orthogonal primitives rather than
multiplying entry points).

Checked against gs directly, not just this interpreter, per HANDOFF's
"gs is the oracle" convention — both constructs the design leans on
are gs-accepted: `-1 255 and` (255, matching this interpreter) and
`1e-9`-style exponent-notation reals (`curl2`'s flatness threshold).
`ghostscript_accepts_artkit`'s shared driver string (one exercise per
library section) now also calls `noiseinit`/`noise2`/`curl2`/`advect`,
matching how every prior section landed in that same test.

Also brought `README.md`'s "Making art" prose and `site/gallery.html`
up to date with this section/piece — both had already fallen behind
by one prior gallery piece (issue #16's "The Compositor's Proof" is in
`gallery/README.md`'s table but not in either of those two), a
pre-existing gap this issue didn't create and doesn't fix (issue #63
tracks the same class of gap for issue #18's page templates); flagged
here rather than silently left for the next person to rediscover.

## Parameterized page templates for artkit (issue #18, 2026-08-08)

Closes issue #18. The issue asked for reusable page/document templates
(card, letter, certificate, poster, and similar) that an agent can
fill with a title/body/signature without making layout decisions from
scratch, pairing with the paragraph-flow work (issue #16) and PDF
document output (issue #8) already shipped.

**A fourth sibling library, `lib/pagekit.ps`** -- but unlike
`graph.ps`/`dataviz.ps`/`etching.ps`, which are deliberately
independent of `artkit.ps`, this one depends on it (the same
relationship `lib/styles/*.ps` has), leaning on `tfblock` (paragraph
flow), `showctr`, `rrect`, and `Palettes`/`pal`. Five templates --
`pgcard`, `pgletter`, `pgcertificate`, `pginvitation`, `pgposter` --
each `x y w h dict pgNAME  leftover -`: bottom-left-plus-size like the
rest of artkit's layout procs, a content dict of optional keys (a
sparse or empty dict still renders a complete, structurally sound
template), and returning `tfblock`'s leftover-text contract so a
caller learns if its copy overflowed the space. `examples/template_
{card,letter,certificate,invitation,poster}.ps` is one specimen per
template; `tests/pagekit.rs` covers load-cleanliness, ink coverage,
the empty-dict case, the leftover contract, and a Ghostscript
compatibility pass.

**Two bugs `advisor` caught in plan review, before any template code
existed, that would have made every fitted title/name wrong.** First:
artkit's `fitfont` scales *unconditionally* -- give it a two-letter
awardee name and a certificate-width target and it would blow the name
up to fill the whole page, exactly backwards for a name/headline whose
length varies with content. Fixed by adding `pgzfitmax`, a shrink-only
variant that clamps the scale ratio at 1.0.
Second: the plan's first draft picked `/parchment` and `/carnival` as
role-indexed (0=ink..4=background) defaults for the certificate and
invitation, on the assumption that artkit's eight mood palettes run
dark to light like the plan's other two defaults (`/dusk`, `/stone`)
happen to. Checking the actual values: `/parchment` runs *light* to
dark (backwards), and `/carnival` is a hue wheel with no lightness
order at all -- artkit's doc comment promises only "five colors," not
an ordering. `lib/styles/*.ps` gets away with role-by-index because
each pack defines its own palettes to that convention; pagekit follows
the same pattern instead of relying on artkit's incidental ordering --
two new palettes, `/vellum` (formal cream) and `/marigold` (festive
warm), each built and regression-tested (luminance strictly increasing
index 0 to 4) to actually satisfy it.

**One real implementation bug the render-and-look step caught, that a
passing test suite didn't.** `pgletter`'s body-flow call carried a
stray leftover operand copy-pasted from `pgcertificate`'s version
(which legitimately needs an extra x-inset term `pgletter` doesn't),
misaligning `tfblock`'s x/y/w/h arguments by one slot. `--lint`'s
`stack-leak` check caught it immediately on the first rendered
example -- confirming issue #17's lint mode earning its keep exactly
as intended, and the reason every example here gets rendered and
lint-checked, not just loaded.

**Deliberately not doing:** no new Rust (pure PostScript, same
precedent as `etching.ps`); no hyphenation or optimal line-breaking
(inherits `tfflow`'s documented greedy wrap as-is); no image/logo
embedding (text-only, matching the issue's title/body/signature
framing); no automatic multi-page pagination -- overflow surfaces via
the leftover return, and a caller re-invokes on a fresh page itself,
same contract `tfblock`/`tfcols` already establish.

## Self-check/lint mode for agent-driven rendering (issue #17, 2026-08-07)

Closes issue #17. The issue asked for a diagnostic/lint pass that
catches common agent-driven-rendering mistakes an agent could easily
miss just by eyeballing the PNG — a blank page, an unbalanced
`gsave`/`grestore`, stack or dict leaks — plus better source-line
attribution on errors, with which checks, how they're surfaced, and
how strict they are all left to the implementer.

**Four checks, all built on state the interpreter already tracks.**
`src/lint.rs` is a new, small module: `blank-page` (every completed
page, plus the trailing canvas if nothing emitted it yet, checked
against the untouched-white background — not "uniform color," so a
deliberate solid fill isn't a false positive), `gsave-imbalance`
(`Gfx`'s save-stack depth nonzero at program end), `stack-leak`
(operand stack nonempty, showing up to five items), and `dict-leak`
(dict stack deeper than the systemdict/userdict baseline). Findings
are advisory text, never fatal — `--lint` doesn't touch the exit code,
and `pscat-mcp`'s `render_postscript` only appends a `Lint:` block when
there's something to say, staying silent on a clean render rather than
adding a "nothing's wrong" line to every single call.

**`blank-page`/`stack-leak` are gated behind whether a render was
actually requested.** First draft ran every check unconditionally,
which meant `pscat-mcp`'s `eval_postscript` — the calculator/debug
tool, whose entire idiom is "leave the answer on the stack and print
it" — would have reported a false-positive blank page and stack leak
on every single call (`3 4 add` "leaking" its own result). Caught by
`advisor` review before implementation started. Fixed by threading a
`render_checks: bool` through `lint::check` (on for `--png`/`--svg`/
`--pdf`, off otherwise) and, at the MCP layer, simply not wiring
`--lint` into `eval_postscript` at all in v1 — its checks don't fit
that tool's normal usage pattern. `render_checks`'s exact gate moved
twice more under review: round 2 caught it keying off the output-format
flags instead of `-e`, which meant a plain `pscat --lint file.ps` (no
`--png`) silently skipped both checks; round 7 caught the opposite gap
in the fix for that — gating purely on `eval.is_none()` meant `-e`
*paired with* an explicit `--png`/`--svg`/`--pdf` (a real render
request, e.g. `pscat --lint --png out.png -e 'showpage'`) also skipped
them and reported a blank artifact as clean. The gate is now `eval.is_
none() || png.is_some() || svg.is_some() || pdf.is_some()`: skip only
when it's a bare calculator-style `-e` snippet with no output asked
for at all.

**Source-line attribution (`error_report`'s new `; Line: N`) is scoped
to the top-level program only**, not `run`-loaded library files, eexec
streams, or executable strings (`Lexer` gained an `is_main` flag, true
only for `Lexer::main_program`) — an artkit script does `(lib/
artkit.ps) run`, and reporting one of *its* line numbers as if it were
a line of the submitted program would be actively misleading, not
just imprecise. Also caught by `advisor`: the first draft read the
scanner's live position at error time, which is off by one for the
common case of a token immediately followed by a newline (a token
terminated by whitespace consumes that delimiter as part of scanning
it — see `lexer.rs`'s `eat_token_delimiter`) — fixed by capturing the
line at the *start* of each token (`Lexer::next_token` now does a
`skip_insignificant` pass first, then records `token_start_line`
before dispatching). Line attribution is best-effort even within
scope: it's the line of the last token scanned directly from real
program source, sticky across procedure calls, since objects aren't
tagged with a source position — an error deep inside a previously
defined procedure is attributed to the call site that most recently
touched real source text, not the procedure's original definition
site. Documented as a deliberate deviation in `HANDOFF.md`.

**Found two real, previously undetected bugs on its first real-world
run — the strongest evidence the feature works.** Unit tests only
exercise 1–3 token toy programs; running `--lint` over every file in
`examples/` and `gallery/` (a corpus scan `advisor` asked for before
calling this done, since that's the population `render_postscript`
will actually see) found two genuine operand-stack leaks in shipped
libraries, neither ever caught by eyeballing rendered output because
neither affects a single pixel: (1) `lib/artkit.ps`'s `tfdrawline`
`/justify` branch calls `search` to find each word's trailing space;
`search`'s "not found" return is `string false` — the searched string
comes back unchanged, still sitting under the bool `ifelse` just
consumed — and the last-word branch fell through without popping it,
leaking one string per justified line (found via
`examples/paragraph_layout.ps`, 3 leaked words; also present in
`gallery/compositors_proof.ps`, 7). (2) `lib/etching.ps`'s `et-hatch`
computed an x-in-range and a y-in-range boolean, each already reduced
with its own `and`, but never combined the two results with a further
`and` before the `if` that used only the top one — stranding the
x-in-range boolean on the stack every sample point, 71,556 of them
over `examples/etching_demo.ps`'s hatch pass. Both fixes are one line
(a missing `pop`, a missing `and`); regression tests assert
`operand_stack().is_empty()` after the exact call that leaked, and
`tests/cli.rs`'s `lint_is_clean_on_real_example_and_gallery_pieces`
runs `--lint` against all three affected files so the corpus check
that found them isn't a one-off scan lost after this session.

**Cross-model review (Codex) found a real pre-existing bug in the
execution machine, not the lint feature itself — `dict-leak`'s
`dict_stack_len()` was just the first thing to ever look.**
`begin_eexec` pushes `systemdict` onto the dict stack for the
duration of the encrypted stream (Type 1 fonts run their decrypted
body with `systemdict` implicitly current), popped again once the
stream is spent. Three separate places drop that scanner frame —
`unwind_all` (an unhandled error or `quit`), `do_stop` (`stop`, or an
error caught by an enclosing `stopped`), and `Action::PopScannerAndDict`
(the stream simply running out of bytes, by far the most common case)
— and all three had grown their own copy of "pop whatever's on top of
the dict stack," assuming it must be eexec's injected copy. It isn't,
in general: PostScript running inside the encrypted stream is free to
manage the dict stack itself, and a real Type 1 font's `currentdict
end ... Private begin` does exactly that. Round 4 fixed `unwind_all`;
round 5 found `do_stop` had the identical gap (worse there, since
execution continues afterward — a leftover phantom `systemdict` push
silently redirects the next `def`); round 6 found that even the
`unwind_all`/`do_stop` fix was wrong in one case, an unconditional pop
that would remove a program's *own* dict if it had already `end`-ed
eexec's copy itself before stopping. The eventual fix: one shared
`cleanup_unwound_frame` used by all three sites, popping the injected
dict only when the dict stack's top is, by pointer identity
(`Rc::ptr_eq`), the *exact* object `begin_eexec` pushed — never by
position. `Action::PopScannerAndDict` (the normal-completion path) was
the last of the three to still have its own positional pop, caught in
review after round 6's PR comment before merge, not by Codex — same
fix, made to delegate to the shared helper rather than duplicate it a
fourth time. Regression tests cover all three unwind paths plus the
program-owned-dict-survives case: `an_error_inside_eexec_does_not_
leave_a_phantom_dict_stack_entry`, `a_caught_stop_inside_eexec_does_
not_leave_a_phantom_dict_stack_entry`, `a_program_owned_dict_left_
open_by_eexec_survives_the_cleanup`, `eexec_completing_normally_pops_
its_injected_dict_by_identity_not_position` (all in `src/interp.rs`).
`HANDOFF.md`'s deviations list had a line describing this exact gap
removed once it was actually closed.

**Known remaining limitation, intentionally not chased further**: a
program that `end`s eexec's injected `systemdict` copy and then
*re-enters* it (rather than leaving a dict of its own on top) before
the stream unwinds would confuse the identity check the same way the
old positional pop did — the check only distinguishes "eexec's exact
copy" from "something else," not "eexec's copy, buried under
further pushes." No known real Type 1 font does this, and chasing
further edge cases in this one code path stopped being productive
after three review rounds of diminishing-severity findings in the
same spot.

**Deferred**: "ink drawn outside `%%BoundingBox`" from the issue's
suggestion list — the interpreter has no DSC-bbox-vs-page-size
infrastructure today (confirmed: no `%%BoundingBox` parsing anywhere
in the tree), and the page canvas already clips all ink to itself, so
this would only ever matter for an EPS-style declared bounding box
smaller than the canvas — a distinct, larger feature, not a quick
addition to this one.

## Paragraph/flowing-text layout for artkit (issue #16, 2026-08-07)

Closes issue #16. The issue asked for reusable paragraph/flowing-text
layout procedures in `lib/artkit.ps` — wrapping, justification,
columns, margins/leading, and setting a block of copy into an
arbitrary region, not just a straight or curved baseline — with wrap
algorithm, justification method, and the shape/column API explicitly
left to the implementer.

**Five procedures, one shared primitive.** `tflinebreak` (internal)
peels one greedy-wrapped line off a string under the current font,
forcing a break on an embedded newline and reporting whether this is a
paragraph's last line (so callers know not to stretch-justify it).
`tfwrap` builds on it for the fixed-width, no-drawing case
(measurement/line-counting). The real design decision was `tfflow`:
rather than take a fixed box, it takes a `boundsproc` — `{y -> x0 x1}`,
called once per line — so a region's available width can vary with
height instead of being locked to a rectangle. `tfblock` (a plain box)
and `tfcols` (columns, feeding one call's leftover into the next) are
both just `tfflow` with a constant-width boundsproc built from their
own args; a caller who needs a trapezoid, an arch, or a circle (the
gallery piece below) writes their own boundsproc and calls `tfflow`
directly. `tfdrawline` does the per-line alignment (`/left /right
/center /justify`, unknown names falling back to `/left` — this file's
usual latitude-not-error posture); `/justify` skips stretching
whenever the caller says this is the paragraph's last line, or the
line is a single word with nothing to stretch between.

**Four real bugs, caught across three different passes, none of them
by the first draft of tests.** (1) `advisor` review of the plan caught,
on inspection alone, that `tflinebreak`'s end-of-string branch
unconditionally took the whole remainder as one line without checking
whether it actually fit the width — so the last word of every
non-terminal paragraph silently overflowed instead of wrapping to its
own line. Fixed by measuring the remainder before taking it, falling
back to the last known-good break otherwise. (2) Rendering
`examples/paragraph_layout.ps` (not a targeted test — just looking at
the output) turned up a second, worse bug: `/justify`'s word-advance
added `stringwidth(word) + tdextra` but never added the line's own
*natural* space width, so every inter-word gap was short by a full
space — with several gaps on one line the shortfall compounded into
visibly overlapping words ("flowsabrush:wordbyword..."). The original
ink-bbox regression test for justify didn't catch this, because
crushed-together text still reaches close to the right margin, same as
properly-spaced text — a new test was added that calls `tfdrawline`
directly with `lastline` forced false and asserts the *specific*
x-position the arithmetic predicts, confirmed to fail against the
pre-fix code (rightmost ink at 141 instead of ~170) before being
folded into the suite.

A follow-up cross-model (Codex) review of the implementation, run per
`SDLC.md`'s independent-review step, found two more, both genuine and
both fixed before merge: (3) `tfwordgaps` counted *every* literal space
in a line, including a trailing one `tflinebreak` can leave behind at a
double-space wrap point (confirmed directly: `(aa bb  cc dd)` at width
45 wraps to `["aa bb ", "cc dd"]`, trailing space intact) — so
`/justify` spent stretch on that invisible trailing gap instead of the
real ones, leaving the actual last word short of the margin. Fixed by
trimming trailing spaces off the line at the top of `tfdrawline`,
before either the width measurement or any alignment branch runs (this
also quietly fixes the same trailing-space nudging `/right` and
`/center` slightly off their true position, not just `/justify`). (4)
The nesting gotcha documented for `tfblock`/`tfcols` below turned out
to be incomplete: `tfflow` itself is just as vulnerable, since it's
built from the same kind of plain globals, and a boundsproc that calls
`tfflow` again unwrapped doesn't just draw the wrong thing — it
silently discards real, unflowed text from the *outer* call (confirmed
directly: an outer call with too little room to fit its whole
paragraph reports an empty leftover instead of the real remainder, the
instant the inner call's own state overwrites the outer's mid-loop).
Same fix as the tiling section's hexgrid+hex gotcha: wrap the inner
call in its own dict; confirmed this restores the outer call's correct
leftover.

A second Codex round, on the pushed fixes, found a fifth issue outside
the library itself: the gallery piece's medallion motto overflowed its
`tfflow` region at the bounds/leading/font it was set with, and the
`pop` after the call silently discarded the non-empty leftover — the
committed render ended mid-sentence ("...A LINE FINDS", dropping "THE
ROOM A CALLERS OWN RULE ALLOWS IT"). Fixed by shortening the motto to
one that fits (confirmed empty leftover), not by enlarging the
medallion or shrinking the font, which would have changed the piece's
proportions. A third Codex round, on that fix, flagged that `tab`
isn't recognized as a wrap separator — correct as read, but not a
defect: this interpreter's own `show`/`stringwidth` give tab no
special treatment either, so treating it as ordinary word content
(not a break point) keeps the wrap logic agreeing with what actually
renders, rather than disagreeing with it by inventing meaning for a
character nothing else here understands. Dispositioned as intentional
scope, not implemented; the section header now says explicitly that
space and newline are the only recognized separators rather than
leaving "whitespace" ambiguous.

**Deliberate scope cuts** (documented in the section header, same
posture as `pathtext`'s plain-stringwidth advance): no hyphenation — an
oversized single word gets its own line rather than being split; greedy
wrap, not Knuth-Plass optimal-fit; whitespace runs around a wrap point
(other than a trailing space at the very end of an emitted line, fixed
above) are preserved verbatim rather than collapsed; vertical fit is
judged by baseline, not glyph box, so a block's last line's descenders
can fall a little past its bottom edge; `tfflow`/`tfblock`/`tfcols`
share plain globals with any boundsproc they're handed (same
convention as the tiling section's `tk-`/`tg-` prefixes), so a
boundsproc must not itself call `tfflow`, `tfblock`, or `tfcols`
without wrapping that inner call in its own dict — documented and
tested (mirroring gasket/carpet's own nesting regression tests) rather
than solved by redesigning `tfflow` off plain globals, consistent with
how every other driver in this file handles its own analogous gotcha.

`examples/paragraph_layout.ps` is a four-quadrant specimen sheet
(`tfblock` left vs. `/justify`, `tfcols`, and `tfflow` with a circular
boundsproc). The gallery piece, **The Compositor's Proof**
(`gallery/compositors_proof.ps`), sets a motto inside a round medallion
via `tfflow` and a hand-written circle boundsproc, a justified body
paragraph via `tfblock`, and a two-column colophon via `tfcols` sized
so the copy genuinely spills from the first column into the second
(not just technically able to).

## A photo-to-line-etching/sketch utility (issue #15, 2026-08-07)

Closes issue #15. The issue asked for "a utility to take a photo and
produce a line-etching/sketch rendering of it in PostScript," citing
`src/image.rs`/`src/ops/image.rs` (image reading) and `src/halftone.rs`
(tone-based dot patterns) as existing foundation, with edge detection
vs. hatching, Rust vs. PostScript, and output format all left open.

**The one decision that shaped everything else**: whether a PS program
can get at a JPEG's decoded pixel samples at all, since `image` just
rasterizes straight to canvas without exposing them back to the
operand stack. Checked empirically before writing any design doc line
two: `/DCTDecode` is a general filter in this interpreter (`src/file.rs`'s
`Decoder::Dct`), not special-cased to the `image` operator, so
`(photo.jpg) (r) file /DCTDecode filter N string readstring` hands back
raw decoded bytes directly. That meant the whole feature could be a
third sibling PostScript library — `lib/etching.ps`, no dependency on
`artkit.ps`/`graph.ps`/`dataviz.ps` — with **zero new Rust code**,
which is a very different shape than the first draft plan (a
`halftone.rs`-sibling Rust raster filter) that got flagged and dropped
in `advisor` review before implementation started.

Two entry points:

- **`et-dims`**: given a photo path, walks the JPEG's marker segments
  (SOI, then each marker's own length field to jump to the next one —
  not naive byte-scanning, which JPEG payload bytes make unsafe, since
  arbitrary entropy-coded bytes can coincidentally look like a marker)
  until it finds a SOFn frame header, then reads its precision/height/
  width/component-count fields straight out of the file. No decode.
  Cheap enough to run as a measure pre-pass, the same shape as
  `lib/handscript.ps`'s `hs-linecount`.
- **`et-draw`**: opens the same path through `/DCTDecode`, reads the
  full sample buffer in one `readstring` (the Rust side already loops
  internally to fill it — confirmed by testing a 120,000-byte string
  allocation directly rather than assuming it), then hatches: a
  primary pass of parallel lines at `/Angle` degrees whose stroke width
  is quantized into a few darkness-driven buckets, plus a perpendicular
  crosshatch pass gated to `/Threshold2` and above — the actual
  historical line-engraving technique newspapers and books used before
  halftone screens, not edge/contour detection, which the issue's
  framing (`src/halftone.rs` cited as precedent, not an edge-detection
  library) already pointed toward.

**The run-length-bucketing decision** (flagged in review before
writing the hatcher): the first design marched every sample point and
would have stroked one tiny segment per sample — thousands of
individual `stroke` calls, each a full tiny-skia rasterization. Instead
each hatch line tracks its current darkness bucket and only emits a
`stroke` when the bucket changes (or the line ends), so a uniform-tone
run of the image costs one stroke regardless of how many samples it
spans. An 800x600 render (a real gallery-scale page) finishes in about
2.6s; a 200x150 photo at native size is under 0.15s.

**A real bug the coverage tests caught, not eyeballing**: the first
version's `sy` sample-row mapping used the device `y` coordinate
directly. PostScript's `y` runs bottom-up from the page origin; the
decoded JPEG sample buffer's rows run top-down (row 0 is the top of
the photo, straight out of the decoder) — so every photo rendered
upside down. `darker_regions_get_denser_hatching` (comparing a real
photo's darker sky band against its lighter ground band) happened to
still pass with the bug in place, purely by luck of which regions got
sampled; it was a dedicated synthetic fixture (`tests/data/topdark.jpg`,
built specifically to have an unambiguous dark-top/light-bottom
signature) and a direct visual render that actually exposed it. Fixed
with a `ph y sub` flip in the sample lookup; `tests/etching.rs`'s
`photo_orientation_is_not_flipped` pins it down so it can't silently
regress, and the two coverage-based tests got re-derived against the
corrected (and independently visually verified) output rather than
left passing for the wrong reason.

**Scope, stated rather than discovered later**: JPEG input only — this
interpreter has no PNG decode path in PostScript, which isn't a gap,
since real PostScript doesn't have one either (`/DCTDecode` is the
only raster filter here, same as a real RIP). Grayscale or RGB (1 or 3
components); CMYK/YCCK JPEGs are rejected by name (`et-unsupported-ncomp`,
the same "raise on an undefined executable name" idiom `et-dims` uses
for a missing SOF marker or a truncated file) rather than silently
misreading the channel layout.

`scripts/photo_etch.sh` wraps it end to end — a headless `et-dims`
pre-pass sizes the output page to the photo's real aspect ratio, then
the real render — the same two-pass shape as `scripts/handwrite.sh`,
down to reusing its `BIN` bundle/dev-checkout resolution fallback
verbatim. `examples/etching_demo.ps` is the specimen sheet, rendering
`examples/etching_source.jpg` (a synthetic still life generated for
this demo — three shaded spheres, not a real photograph, to avoid any
rights question). `tests/etching.rs` covers `et-dims` against known
fixtures (including the pre-existing `tests/data/gray8.jpg`/`red4.jpg`)
and two malformed-input paths (non-JPEG data, a file truncated
mid-header), plus the three `et-draw` coverage/orientation checks
above.

**Deferred**: progressive JPEGs decode fine through the existing
`/DCTDecode` path (zune-jpeg doesn't care), and `et-dims` recognizes
any SOFn marker rather than only baseline's C0 for exactly that
reason, but this wasn't tested against an actual progressive fixture.
No SVG/PDF output size benchmarking beyond a single 200x150 spot check
(37KB PDF, 77KB SVG) — a much larger photo at fine `/Spacing` could
produce a large vector file, since every stroke run is a separate path
element; not a correctness problem, just an unexplored tuning axis.

## A data-visualization chart library for artkit (issue #14, 2026-08-06)

Closes issue #14. The issue asked for "a comprehensive library of
reusable data-visualization procedures — bar charts, line/area
charts, pie charts, scatter plots" — breadth of chart types was the
ask, with data format and styling left to the implementer. Went with
a third sibling, `lib/dataviz.ps`, no dependency on `artkit.ps` or
`graph.ps` either way, matching issue #13's precedent for the same
"clearly-scoped sibling library" language.

Six sections, mirroring the `graph.ps`/`artkit.ps` header/section/
prefix conventions:

- **frame**: `setdvframe` (value domain + device viewport) backs
  `dvmapy` (continuous value -> device y) and `dvcatx` (category `i`
  of `n` -> a centered device x) — deliberately category-*centers*,
  not edge-to-edge, so a bar chart and a line chart sharing the same
  category count sit precisely under/over each other (the gallery
  piece's bar+line combo is the reason). `dvbarw`, `dvbounds`
  (raw min/max), and `dvsum` round out the section.
- **bar/line/area**: `barchart` fills each bar directly rather than
  building a path for the caller to finish, since every bar can carry
  its own color via a per-element callback (`{i v -> r g b}`, the
  same shape as artkit's `grid`); baseline is `0 dvmapy`, not the
  viewport floor, so negative values draw below the line correctly.
  `linechart`/`areachart` follow `graph.ps`'s "build path, caller
  strokes/fills" convention instead, since there's exactly one series
  and no per-point coloring need.
- **scatter**: its own persistent frame, `setscatterframe` — the same
  8-arg shape as `graph.ps`'s `setframe`, deliberately, since it's a
  genuinely continuous 2D domain unlike the categorical bar/line
  frame.
- **pie/donut**: one proc, `piechart`, with an inner-radius argument
  rather than two near-duplicate procs — `ir=0` draws plain wedges
  (center point + outer arc), `ir>0` draws annular sectors (outer arc
  out, straight step in, inner arc back). Wedges start at 12 o'clock
  and sweep clockwise, the usual chart-library convention (same
  clockwise-for-legibility call artkit's `ctext` already made about
  circular text).
- **axes**: `dvaxes` decorates the categorical frame — border, x-ticks
  at category centers (matching `dvcatx`, not edges the way
  `graph.ps`'s `axes` places them — the one place this deliberately
  differs), y-ticks at even value divisions. Scoped to bar/line/area
  only; scatter's continuous frame has no axes helper (a documented
  scope cut — a caller wanting a border there draws four lines by
  hand).
- **color**: an 8-entry default qualitative cycle, `dvcolor`, so
  `{ pop dvcolor }` is a one-line default color callback for any
  chart type.

Every color callback (bar, pie, scatter) is called *before* that
element's path is built, not between path-building and `fill` — a
proc that mishandles the path (a stray `newpath`, an unbalanced
`gsave`) can't silently drop or corrupt the element that way. Same
hazard artkit's `alongpath` documents for its own stamp proc; repeated
here since every chart driver in this file has it.

An `advisor` review of the plan (before any code existed) flagged the
exact bug class that bit `graph.ps` twice: an ink-count test passes
identically whether a donut is a real ring or filled solid to the
center, and whether a pie sweeps clockwise or counterclockwise — same
total ink either way. `tests/dataviz.rs` has two tests aimed
specifically at that: `piechart_with_inner_radius_leaves_the_center_
empty` samples the actual center pixel (a donut filled solid would
pass any ink check but fail this one), and `piechart_sweeps_clockwise_
from_twelve_oclock` samples two quadrant points and checks which
wedge's color landed where, since a reversed sweep direction paints
the same total area in the wrong place. Both bugs would have been
completely invisible to the ink-coverage-only testing style
`graph.ps`'s `plotsurface`/`surfacerow`/`surfacecol` tests use.

One real implementation bug, caught while hand-tracing the stack
during authoring, before any test was written: `dvbarw`'s first draft
divided the viewport width by the gap fraction instead of the
category count (`pw gap div` instead of `pw n div`) — a stack-ordering
mistake from trying to write it as a point-free one-liner like
`dvmapy`/`dvcatx`. Rewritten
with named scratch variables instead, same as every other multi-step
helper in this file; only `dvmapy`/`dvcatx` stay point-free, since
they're genuinely single expressions.

Also found while building the gallery piece: this interpreter's
`fill`/`stroke` consume the current path (implicit `newpath`, a
documented deviation from the PLRM — see `HANDOFF.md`'s "Deliberate
deviations"), so `pathbbox` can't inspect geometry after a driver
that paints internally (`barchart`/`piechart`/`scatterchart`) the way
`graph.rs`'s tests inspect `plotfn`/`axes` paths that are never
filled. `tests/dataviz.rs`'s bar-geometry tests (including the
negative-value case) sample pixels instead.

`examples/dataviz.ps` is a six-panel specimen sheet, one per chart
type. The gallery piece, Field Notes, is a naturalist's field-journal
page: a bar chart (weekly marsh-bird sightings) and a line chart (a
temperature trend, dashed) sharing one category axis, plus a
species-mix donut with a hand-drawn legend — lettered throughout in
the Stage 12 `/HandScript` dynamic font. One piece, not several, same
as issue #13's Ripple Range: the comprehensive breadth lives in the
specimen sheet, the gallery piece is where the composition/craft
lives (`HandScript`'s glyph set doesn't include parentheses or colons,
a font-limitation caught only by rendering the piece and looking —
worth a note for whoever reaches for it next).

Cross-model (Codex) review of the PR found three more edge-case bugs,
all of the same shape — a driver that's a clean no-op on an empty
array still had *some* piece of code executing an unconditional
division when the array is nonempty but degenerate:
`areachart`'s baseline-closing lines ran even when the sampled loop
above them was empty, calling `dvcatx` with `n=0`; `piechart` divided
each wedge's sweep by `dptotal` without checking a nonempty series
summing to zero (`[0 0]`, every category filtered away) wasn't
dividing by zero; `dvaxes`'s y-tick loop (`0 1 davny for`) still runs
once when `davny=0` (0 <= 0), and that one iteration divides by
`davny`. All three got the same fix — wrap the division-bearing code
in a `> 0` guard so a degenerate-but-nonempty input draws nothing,
matching how every other empty-array case in this file already
behaves — plus a regression test each. A fourth Codex finding
(`barchart`/`areachart`'s zero baseline can fall outside a value
domain that doesn't bracket zero, e.g. `5 10 setdvframe`, and paint
past the viewport) was deliberately *not* fixed: that's the same
unclamped-baseline property every mainstream bar-chart library has,
and a domain excluding zero for a bar/area chart is itself the
well-known "truncated axis" anti-pattern — clamping would silently
change bar height's meaning from "the value" to "distance from the
domain edge," a worse outcome than documenting the expectation, which
`lib/dataviz.ps`'s `barchart` header now does.

A third review round found one more, and this one turned out to be a
real visible defect, not just a documentable domain quirk like the
baseline case: `piechart` divided each wedge's sweep by the *raw*
sum, so a negative entry (e.g. `[5 3 -2 4]`) shrank the total enough
that other, perfectly ordinary positive wedges' shares exceeded 360
degrees. `arcn` doesn't error on that — it wraps past a full turn and
silently paints over earlier wedges. Rendering `[5 3 -2 4]` before the
fix showed only 2 of the 4 wedges; the other two were completely
overpainted. Fixed by clamping both the running total and each
wedge's own share to `>= 0` — a negative entry becomes a zero-width
no-op (same "degenerate input draws nothing" treatment as the other
three fixes) while every remaining wedge's sweep stays bounded within
one turn, since the sum of non-negative clamped values can never
exceed the clamped total by construction. The color callback still
receives the caller's original, unclamped value, only the geometry is
clamped. Confirmed by re-rendering the reproduction case: all three
positive wedges visible again, correctly proportioned (150/90/120
degrees for values 5/3/4), and the two chart pieces already
built (`examples/dataviz.ps`, the gallery's Field Notes) render
byte-identical before and after, since neither ever passes a negative
value.

## 2D/3D function-graphing procedures for artkit (issue #13, 2026-08-03)

Closes issue #13. The issue asked for reusable plotting/projection
primitives — 2D curves and 3D surfaces — "added to `lib/artkit.ps` or a
clearly-scoped sibling library," leaving the projection method,
coordinate systems, and demo equations to the implementer. Went with a
sibling: `lib/graph.ps`, no dependency on artkit either way, since
sampling/projection math is a genuinely separate concern from artkit's
random/color/turtle/L-system/brush/tiling/hyperbolic/fractal toolkit
and didn't need any of it.

Four sections, mirroring artkit's own header/section/prefix
conventions:

- **frame**: `setframe` maps a data-space domain onto a device-space
  viewport (persistent `GraphFrame` state, same shape as artkit's
  `TurtleState`); `gmapx`/`gmapy` expose the mapping directly for
  callers who want it (tick labels, custom annotations); `gmoveto`/
  `glineto` build path points from data coordinates.
- **2D curves**: `plotfn` (y=f(x)), `plotparam` (parametric), and
  `plotpolar` (polar, theta in degrees — PostScript's native `cos`/
  `sin` and artkit's turtle heading are both degrees, so this stays
  consistent rather than picking radians for "more mathematical"
  reasons) all sample n+1 points and append to the current path, same
  "caller strokes/fills" contract as artkit's shape procs. `axes`
  draws a bordered, ticked frame — tick marks only, no numeric labels;
  a caller wanting labels has `gmapx`/`gmapy` to place exact `show`
  calls itself, the same latitude the issue grants implementers
  generally.
- **3D view**: `setview` (azimuth/elevation camera, degrees) +
  `project3` (rotate about Z by az, then tilt by el — a negated
  X-rotation, chosen so positive z renders up the page instead of down
  — then drop depth orthographically and scale/translate onto the
  page) — the "some form of projection" the issue asked for, without
  needing a full matrix/quaternion library for one camera.
- **3D surfaces**: `surfacerow`/`surfacecol` each draw one polyline
  through `project3`; `plotsurface` walks a full grid, rows then
  columns, into a wireframe mesh. Deliberately no hidden-surface
  removal — that needs polygon depth-sorting, a separate project, the
  same call artkit's tiling section already made about Penrose tiling.

Two real bugs, both caught before merge rather than by review:

- **Prefix collision across the composition chain.** First draft used
  one scratch prefix for the whole file. `plotsurface` calls
  `surfacerow`/`surfacecol`, which call `project3` — exactly the
  composition depth artkit's own `tg-` gotcha (hexgrid/trigrid calling
  hex/tri unwrapped, issue #9) warns about, and a first advisor pass
  caught it before any code existed: `project3`'s scratch would
  silently overwrite `plotsurface`'s outer-loop position state the
  moment a caller's sampling proc itself called back into the library.
  Fixed with four disjoint prefixes (`gp` 2D drivers/axes, `gv` the
  view/project3, `g3` surfacerow/surfacecol/axes3, `gsf` plotsurface
  specifically, since it's the one proc that calls into the `g3`
  group) instead of one shared prefix, plus the same wrap-the-inner-
  call-in-its-own-dict guidance issue #9 already documented, extended
  to cover this file.
- **Forwarding a captured proc by bare name auto-executes it.** Found
  empirically, not by review: `plotsurface`'s first draft passed its
  captured sampling proc onward to `surfacerow`/`surfacecol` as
  `gsfnx gsffn surfacerow` — but a bare reference to a name bound to
  an executable array *invokes* it on the spot in PostScript, rather
  than pushing it as an operand, the instant it's encountered outside
  a fresh `{ }`. `surfacerow` never saw a proc argument at all; it saw
  a stack one item short and failed with `stackunderflow`. This is
  exactly artkit's own `alongpath` pattern (`{ aponepitch } exch
  alintern`, not bare `aponepitch`) — under-applied here rather than
  a new kind of bug, fixed by wrapping the forward as `{ gsffn }`, and
  now has a dedicated regression test (`plotsurface_forwards_the_
  sampling_proc_without_invoking_it_early` in `tests/graph.rs`) that
  counts calls rather than just checking ink, since a silent zero-call
  failure wouldn't show up as "less ink," it'd show up as no path at
  all or a crash — the test pins the actual call count instead
  (2×(nx+1)×(ny+1): rows and columns each resample every vertex on
  their own independent pass, no z caching between them).

A third, more serious bug surfaced at the step-6 advisor review of the
finished diff, not by any test above: `project3`'s screen-y formula
subtracted the z term (`gvy1 cos(el) - gvz sin(el)`), so height
rendered *downward* on screen — a peak at higher z landed lower on the
page. Every pinned `project3`/`axes3` test used either z=0 or el=0,
where that term's sign can't matter (multiplied by zero either way),
so nothing caught it. Confirmed empirically before touching anything:
`0 0 1 0 0 setview` (identity) placed `(0,0,8)` correctly at devy 240
above the origin's 180 once flipped to `+`, versus 119.8 *below* it
before. Fixed by flipping the sign to `add`; a new test
(`project3_renders_positive_z_upward_at_nonzero_elevation`) isolates
the z term exactly by setting el=90 (sin=1, cos=0), so `(0,0,z)` must
land at precisely `(0,z)` — the gap the other tests left. The fix
mattered beyond direction alone: Ripple Range's back-to-front
occlusion claim turned out to depend on it in a way that was masked by
two errors partially canceling — the original row sweep (`0 1 ny2`)
actually drew near rows first and far rows last (backwards for
painter's algorithm), but combined with the inverted z sign, the
result still *looked* like a plausible ripple field. Verified with a
dedicated test scene (an isolated tall spike on an otherwise flat
field, rendered both sweep directions): the buggy order visibly
chopped the spike's peak off under later-drawn "farther" rows; the
corrected order (`ny2 -1 0`, drawn genuinely far-to-near) rendered it
as a clean, fully unoccluded silhouette. Both `lib/graph.ps`'s
`project3` and the gallery piece's own inlined copy needed the same
one-character sign fix — the second copy is exactly the risk the
self-containment doctrine accepts (a fix to the library doesn't
propagate to pieces that inlined it before the fix landed) in exchange
for pieces staying runnable standalone.

Also from that review: `axes` was defining its per-tick loop counters
(`gpi`/`gppx`/`gpj`/`gppy`) *inside* `GraphFrame begin ... end` — since
`def` always targets the innermost open dict, every call was leaking
loop scratch into the frame's own 8-slot dict instead of userdict,
the exact `tg-`-style composition trap this file's own header warns
about, just against a data dict instead of another driver. Didn't fail
today (both pscat and gs auto-grow dicts past their declared size),
but it violated the library's own stated rule in a way a future
reviewer would flag. Fixed by pulling `px`/`py`/`pw`/`ph` into local
variables and closing `GraphFrame`'s dict before looping, rather than
threading the whole tick computation through it.

New demo: `examples/graphing.ps`, a four-quadrant specimen sheet
(plotfn: a two-harmonic wave; plotparam: a 3:2 Lissajous figure;
plotpolar: a five-petaled rose; plotsurface: a radial ripple under
`setview`) — inlines a one-line `showctr` rather than adding an
artkit dependency just for text centering. New gallery piece:
`gallery/ripple_range.ps`, "Ripple Range" — two decaying ripple
sources summed into one height field, swept row by row far-to-near
under `project3`. Per-row rendering is the one case where cheap
painter's-algorithm occlusion *does* work without a general hidden-
surface solver: fill each row from its ridge line down to a shallow
margin below itself, in strict back-to-front order, and each nearer
row's opaque fill correctly hides whatever farther terrain would
otherwise show through underneath it — legitimate specifically because
a height field swept along one axis has no self-overlap ambiguity, the
condition plotsurface's own header names as the general case that
needs real depth-sorting instead. Inlines `project3`/`setview` from
`lib/graph.ps` and `mix3` from `lib/artkit.ps` per the gallery's
self-containment doctrine (see `hortus.ps`/`woven_labyrinth.ps`).

`tests/graph.rs`: arithmetic pinned wherever the trig lands on clean
values (multiples of 90 degrees, matching cos/sin's exact float
results) for frame mapping, all three 2D drivers, `axes`'s tick-
extended bbox, and `project3`'s camera; ink-coverage checks for
`surfacerow`/`surfacecol`/`plotsurface` the same way artkit's `ldraw`
test works, since a mesh doesn't reduce to one clean number the way a
single sampled point does; a Ghostscript-compatibility test combining
all of it in one driver, mirroring `tests/artkit.rs`'s
`ghostscript_accepts_artkit`.

## Fractal / self-similar-geometry procedures for artkit (issue #11, 2026-08-03)

Closes issue #11. `examples/koch_snowflake.ps` and `examples/sierpinski.ps`
already showed the pictures were achievable, but both had the
recursion/geometry hand-written inline with nothing reusable a new piece
could reach for. L-systems (Stage 19) already cover recursive branching
(trees, plants), so this section is deliberately the other two
well-known fractal genres the issue itself named: edge replacement
(Koch-curve-style) and area subdivision (Sierpinski-style).

Added `edgefractal` (a generalized Koch-style edge-replacement curve —
takes any `turns` array of cumulative turn-deltas plus a separate
`scale` divisor, draws via `rlineto` like `koch`/`fd`) and `edgepoly`
(walks it around a closed `[verts]` polygon, generalizing the one-off
`snowflake`). Two verified presets in `FractalGens` (`/koch`, the
classic bump; `/quadkoch`, the Minkowski-sausage/quadratic-Koch
8-segment generator), retrieved via a new `fgen` convenience (mirrors
`pal`/`palpick`'s exch-and-get pattern).

Also added `gasket` (the Sierpinski triangle, generalizing
`examples/sierpinski.ps`) and `carpet` (its square counterpart — a
distinct algorithm, not a reparameterization). Both are proc-driven
like every other driver in the tiling section (`grid`/`lattice`/
`hexgrid`/`trigrid`/`truchet`), not self-painting.

Two real bugs caught during development, both by direct arithmetic
verification rather than by reading the algebra:

- **`edgefractal`'s length divisor was wrong on the first draft** — it
  divided segment length by the turn array's own length (4 for koch, 8
  for quadkoch) instead of tracking `scale` — the number of
  base-edge-lengths the generator spans end to end — separately. Koch
  has 4 segments but spans only 3 (the bump's two slanted sides fold
  back across each other); get this wrong and the curve silently draws
  at half or three-quarters the requested length instead of erroring.
  Caught by an advisor review of the plan, which asked for a direct
  check rather than trusting recall: walking each preset's turn array
  as unit-length turtle steps confirms both presets close exactly (net
  turn a multiple of 360, net displacement `(scale, 0)`) — now a
  permanent regression test
  (`fractal_gens_presets_have_zero_net_turn_and_the_documented_scale`),
  not just a one-off script.
- **`gasket`/`carpet`'s first draft was a recursive PostScript proc**,
  each level wrapped in its own `dict begin/end` (mirroring `koch`'s
  own scoping, needed for the same reason — see `edgefractal`'s header).
  But unlike `koch`, these also invoke a *caller-supplied* proc at the
  leaves, and a recursive call happens before its own `end`, so every
  ancestor level's dict is still open at the moment a leaf runs —
  confirmed empirically that a stamp as simple as `{ /n n 1 add def }`
  (the exact idiom `truchet`'s own existing test relies on) silently
  rebinds `/n` into a throwaway ancestor frame instead of the caller's
  own dict, and the count is lost the instant that frame closes. Fixed
  by driving the walk with an explicit stack array instead of PostScript
  recursion (same reason `httile` doesn't recurse either), so the
  caller's proc always runs with no extra frame open, exactly like
  every other driver in the file.

New demo: `examples/fractals.ps`, a four-quadrant specimen (`edgepoly`
+koch, `edgepoly`+quadkoch, `gasket`, `carpet`) in the same style as
`examples/tiling.ps`. New gallery piece: `gallery/recursive_peaks.ps`,
"Recursive Peaks" — a night mountain range where each peak is a
`gasket`-subdivided triangle shaded by altitude (a low-poly look that
falls directly out of the subdivision itself, cubic falloff so only
facets near the apex go snow-white), a `carpet`-driven sparse
starfield (most cells skipped, the rest stamp a dot), and floating
`edgepoly` koch/quadkoch snowflakes.

## Hyperbolic geometry in the Poincare disk (issue #10, 2026-08-02)

Closes issue #10, the companion to issue #9's Euclidean tiling library
("non-Euclidean visualizations tend to need their own coordinate/
projection handling ... distinct from ordinary planar drawing, which is
why it's split out as its own issue"). Added a new `lib/artkit.ps`
section: `hpoint`/`hpolar` (logical-unit-disk <-> device mapping, and
hyperbolic-radius/angle placement), `horthocircle` (the circle
orthogonal to the unit circle through two points — a hyperbolic
geodesic's support circle, or the degenerate diameter case), `hreflect`
(circle inversion / mirror reflection across a geodesic), `hgeo`/`hpoly`
(single geodesics and closed geodesic polygons built from them), and
`httile` (a breadth-first-reflection generator for regular {p,q}
hyperbolic tessellations). New demo `examples/hyperbolic.ps` (three
panels: `httile`, `hpolar`, raw `hgeo`); new gallery piece
`gallery/infinite_descent.ps`, a {7,3} tessellation (232 tiles, four
reflection generations) colored in rings by BFS generation.

Two real bugs, both caught by validating the algorithm in a standalone
Python prototype before trusting the PostScript translation (matplotlib
render compared by eye against the textbook {7,3}/{6,4} Poincare-disk
tilings) rather than debugging the geometry live in PostScript:

- **The circumradius convention was backwards on the first pass.** The
  fundamental polygon's hyperbolic circumradius comes from splitting it
  into right triangles with angles pi/p at the center and pi/q at the
  vertex; the correct relation is `cosh(rh) = cot(pi/p)*cot(pi/q)`, not
  `cosh(rh) = cos(pi/p)/sin(pi/q)` — a different formula entirely (a
  ratio, not a product of cotangents), tried first and wrong ({7,3} came
  out R~0.14, visibly too small once rendered — a correct {7,3} occupies
  roughly a third of the disk's radius, not a seventh). Caught by an
  advisor review of the plan *before* any PostScript was written: it
  flagged the convention as unverified and asked for a direct check, not
  just re-reading the algebra. The direct check — build the fundamental
  polygon at a candidate R and measure the *Euclidean* tangent angle
  between adjacent edges at a shared vertex, equal to the hyperbolic
  angle there since the Poincare disk is conformal — reproduces 360/q
  exactly for the corrected formula across five different {p,q} pairs,
  and is now also a permanent Rust regression test
  (`fundamental_polygon_edges_meet_at_the_expected_interior_angle`), not
  just a one-off script.
- **The BFS dedup tolerance stopped the tiling from growing with depth**
  once the geometry itself was right. Tolerance was first scaled to a
  candidate tile's distance from the disk's origin, which stays close to
  1 near the rim even as the tiles themselves keep shrinking there in
  Euclidean terms — an increasingly loose tolerance relative to actual
  tile size, so it started rejecting genuinely new neighbors as false
  duplicates, silently capping {6,4} at 37 tiles regardless of requested
  depth. Fixed by scaling to the candidate's own max edge length instead
  (Mobius reflections are conformal, hence local similarities, so edge
  length shrinks toward the rim exactly as fast as the tile's whole
  footprint does) — {6,4} now reaches 1,711 tiles at depth 5, matching
  the Python prototype exactly.

One more edge case worth its own line: near-but-not-exactly-collinear
input to `horthocircle` (three points almost, not quite, lined up
through the origin) is mathematically valid but can produce an
arbitrarily large circle radius as the true diameter limit is
approached — large enough that `arc` chokes (`gs` raised `limitcheck` on
one, found deep in a depth-4 BFS). No fixed threshold on the
determinant that flags the degenerate/diameter case is scale-invariant
here, so the radius itself is capped instead (>50 falls back to the
diameter case) — visually identical to the true arc at that radius
regardless of device scale.

Two more real bugs, caught by cross-model review (Codex) after the
Python-prototype-validated version above was already up and passing its
own tests — both invisible to the tests that existed at that point,
which is exactly the kind of gap independent review is for:

- `httile` seeded its dedup-visited list with the fundamental polygon's
  *vertex 0* (`htfund 0 get`/`htfund 1 get`) instead of its centroid —
  which happens to be the origin by construction, but the code never
  said so; it read two array slots that looked like they'd give the
  center and didn't. Every `{p,q}` tiling reflects at least one gen-1
  neighbor back across its shared edge into an exact copy of the
  fundamental tile by depth 2 (an unavoidable involution, not an edge
  case), and that copy's true centroid — the real origin — was never
  actually in the visited list, so it passed the dedup check as "new"
  and got enqueued as a bogus generation-2 duplicate sitting exactly on
  top of generation 0. Silent in a render (the duplicate just repaints
  the center in whatever color that generation gets) but not silent in
  the tile count: this fully explains a ~1-tile discrepancy against the
  Python prototype that earlier testing (see the arithmetic-pin test's
  original comment, since corrected) wrongly wrote off as floating-point
  noise near the dedup threshold — it was this bug, deterministically,
  every time. Fixed by seeding `htvisited` with `0 0` outright rather
  than reading it off `htfund`; tile counts now match the Python
  prototype exactly at every `{p,q,depth}` checked (previously off by
  one). The pinned regression test's count changed from 30 to 29
  accordingly, and the gallery piece's tile count from "233" to "232" in
  every place it's mentioned.
- Both `examples/hyperbolic.ps` and `gallery/infinite_descent.ps`'s
  `httile` paint callbacks did `fill` then `stroke` on the same
  driver-built path, expecting the outline stroke to trace the same
  shape just filled. `Gfx::fill` in this interpreter does an implicit
  `newpath` after painting regardless of whether anything was actually
  filled (`src/gfx.rs`: "Painting consumes the path... filled or not")
  — a deliberate, documented deviation, but one this code didn't
  account for, so every tile's outline stroke was silently a no-op (no
  error, just no ink) in both files. Fixed with the idiom this
  interpreter's own gsave/grestore semantics is built to support —
  `gsave fill grestore stroke`, since (per the note above about
  gsave/grestore restoring the path) that fill's implicit newpath is
  itself undone by the grestore. `examples/tiling.ps`'s hexgrid/trigrid
  stamps sidestep this differently (rebuilding the shape fresh before
  stroking rather than relying on the path surviving fill) because they
  have direct access to their own shape-building procs (`hex`/`tri`);
  `httile`'s client callbacks don't (the driver, not the client, is the
  one holding the curved-edge geometry), so gsave/grestore is the
  fix that actually fits httile's contract.

A second round of cross-model review, after pushing the two fixes above,
found three more — this time in the geometry itself rather than around
it, and worth taking seriously precisely because they came *after* the
Python-prototype validation and the existing test suite both said the
math was right:

- **The single real correctness bug of the three.** The `hor 50 gt`
  radius cap added to `horthocircle` (the near-collinear/`limitcheck`
  fix above) approximated *any* large-radius support circle as a
  diameter — a safe simplification for *drawing* (an arc that big and a
  straight line are visually identical at any device scale this file
  uses), but wrong for `hreflect`'s transformation math: a diameter's
  reflection is a mirror across a line through the *origin*, and that
  formula only agrees with true circle inversion when the two points are
  actually collinear with the origin. A chord that merely has a large
  orthogonal-circle radius without being collinear with the origin —
  confirmed with `p1=(0.010195, 0.2)`, `p2=(0.010195, -0.2)`, radius
  ~51 — got mirrored across the wrong line entirely: reflecting `p1`
  moved it to `(-0.010195, 0.2)` instead of fixing it, breaking the one
  invariant every geodesic reflection must have (a geodesic fixes its
  own defining points). This is also the actual explanation for a
  ~1-tile discrepancy against the Python prototype noted in the first
  round of this issue's work and initially misdiagnosed as the
  dedup-seed bug alone — both bugs were real and both contributed.
  Fixed by keeping `horthocircle` mathematically exact (it always
  returns the true circle now, however large) and moving the >50
  drawing-only approximation into `hgeo` and `hpoly` specifically, each
  keeping their own local copy of the "isline" flag rather than
  mutating what `horthocircle` reports. New regression test:
  `hreflect_stays_exact_near_the_old_radius_cap_boundary`, using the
  exact point pair above.
- `httile` built each tile's path with `hpoly` alone, which only
  *appends* — so a caller who hadn't just called `newpath` (or any
  leftover path state) would have that ink dragged into the first
  tile's fill/stroke, contrary to the documented "proc sees one closed
  p-gon" contract. Fixed with an explicit `newpath` before each tile's
  `hpoly` call inside `httile` itself, so the driver's contract no
  longer depends on caller discipline. Every shipped call site already
  happened to newpath first (or had `fill`'s own implicit-newpath from
  a prior paint op clear things anyway), so this didn't change any
  rendered output — it closes a latent trap for the next caller, not a
  bug that had actually fired here. New regression test:
  `httile_does_not_leak_a_callers_pre_existing_path_into_the_first_tile`.
- `hpolar` computed `tanh(hrad/2)` as `(e^hrad - 1)/(e^hrad + 1)`, which
  overflows to `NaN NaN` once `e^hrad` exceeds `f64` range — a few
  hundred is already enough, e.g. `710 0 hpolar`. Not reachable by
  anything shipped here (every real call uses `hrad` under 4), but a
  landmine for the next caller who wants a point genuinely far out
  toward the rim. Rewritten as `(1 - e^-hrad)/(1 + e^-hrad)`, which
  underflows harmlessly to `0` instead of overflowing, giving the
  correct limit of `1`. New regression test:
  `hpolar_stays_finite_for_a_large_hyperbolic_radius`.

A third round, after pushing the fixes above, found one more in the same
family as the radius-cap bug — same failure mode, different threshold.
`horthocircle`'s collinearity check compared the raw circumcircle
determinant (of p1, p2, and p2's unit-circle inversion) against an
absolute tolerance. That determinant's magnitude scales with how far
apart p1 and p2 happen to be, not with how collinear they are with the
origin — two points a hair's width apart (exactly what `httile`'s own
edges become a few reflection generations toward the rim) produce a
tiny determinant regardless of the angle between them, so the absolute
threshold false-positived as "collinear" for pairs that plainly weren't
— confirmed with `(0.99, 0)` and `(0.98999999995, 0.0000099)`, distance
apart ~1e-5 but with a real nonzero angle from the origin, previously
returning `isline=true` and reflecting `(0.99, 0)` to roughly
`(-0.99, -0.00001)` instead of fixing it. The same failure mode as the
radius-cap bug (wrong branch of `hreflect` entirely, not an imprecision
in the right one), and easy to miss for the same reason: neither the
Python prototype (which never had this threshold in the first place —
it tests collinearity the same corrected way described next) nor the
existing test suite exercised a pair this close together non-collinearly
until this round went looking for one. Fixed by testing collinearity
angularly instead of by the determinant's raw size: sin(angle between
p1 and p2 as seen from the origin) = (x1\*y2 - y1\*x2)/(|p1||p2|), a
ratio with no dependence on how far apart p1 and p2 are, only on the
angle between them — compared in squared form to skip two square roots.
New regression test: `horthocircle_collinearity_test_is_scale_invariant`.
Tile counts and every render were re-checked against this fix too
(unchanged at every `{p,q,depth}` re-verified, including the 232-tile
gallery piece and `{6,4}` depth 5's 1,711) — this specific configuration
just hadn't come up yet in anything rendered so far, not evidence it
never will.

Checked against `gs` throughout, not just at the end: both the
Ghostscript acceptance driver (`tests/artkit.rs`) and every render done
while building this (`examples/hyperbolic.ps`, `gallery/
infinite_descent.ps`) were run under both `pscat` and `gs` and compared
by eye, and looked structurally identical there — but "by eye" is
exactly what missed what round eight's cross-model review later found
by actually counting tiles: `gs` computes PostScript reals in 32-bit
float (`1 3 div` prints `0.333333343` under `gs`, `0.3333333333333333`
here), a full order of magnitude less precise than the f64 arithmetic
this file's degeneracy thresholds are tuned against, and that
compounds through `httile`'s recursive reflections into a genuinely
different tile count at *every* depth checked — not just the extreme
`{10,10}` case, but the shipped `{7,3}` depth 4 (232 here, 233 under
`gs` after round eight's fix, 323 before it) and even `{7,3}` depth 2
(29 here, 30 under `gs`). This is the same category of difference as
the already-documented `flattenpath` tolerance gap (fixed quarter-pixel
device-space tolerance here vs. `gs`'s `setflat`-driven one — chord
counts differ, shapes agree) but with a larger, harder-to-eyeball
effect: a handful of tiles out of a couple hundred don't jump out
visually the way a coarser arc facet does. The tile counts pinned in
`tests/artkit.rs` (29, 232, 1,711, 8,201) are this interpreter's own —
exact only here, not under `gs`. What *is* still verified under `gs`:
it accepts and runs the library without error or crash (including the
former `{10,10}` `undefinedresult`), produces a valid, in-bounds
tiling, and its `{7,3}` depth-4 tile count stays within a sanity band
(`ghostscript_accepts_artkit`) rather than asserted exact, since no
single degeneracy threshold can be exact for two interpreters an order
of magnitude apart in float precision — see round eight below.

A fourth round, after pushing the third round's fix, found a real crash;
fixing it properly took a fifth round to get right, since the first
attempt was itself a band-aid in the same failure family as rounds two
and three.

**Round four.** Even past the angular collinearity test from round
three, `hod` — the determinant of the three-point circumcircle
`horthocircle` built by inverting p2 in the unit circle — can round to
exactly `0.0` from catastrophic cancellation in its sum-of-products
arithmetic, a property of that specific formula at extreme `{p,q}`, not
of the angle between p1 and p2. Confirmed under `gs`, which raised
`undefinedresult` dividing by it four generations into a `{10,10}`
tiling. The first fix added a second, much tighter (`1e-9`) absolute
guard on `hod` itself, routing the offending edge through the isline
fallback — and, on the same instrumented `{10,10}` case, every one of
43,470 pairs the new guard caught had `sin(angle) <= ~6.4e-5`, genuinely
near-collinear, which looked at the time like a clean discrimination
between "real cancellation" and "real separation."

**Round five** found that framing itself was the mistake: cross-model
review pointed out that `hod abs 1e-9 lt` was still an approximation
layered on top of an ill-conditioned formula, not a fix to the
conditioning — and produced a second, independent counterexample in the
*other* early-exit this proc had, unrelated to `hod`: any point within
0.001 of the origin (`hod1`/`hod2 < 0.000001`) was treated as
"effectively at the origin" and short-circuited to isline=true,
regardless of whether its partner was anywhere near collinear with it.
`0.0005 0 0 0.5 horthocircle` reported isline=true and reflecting
`(0.0005, 0)` across its own geodesic moved it to roughly
`(-0.0005, -1e-6)` instead of fixing it — the same defining-point
invariant violation as the round-two and round-three bugs, just in a
third input regime.

Both symptoms trace to the same root cause: the old formula used one
(cancellation-prone, non-scale-invariant) determinant as its sole
divisor and a *different* pair of ad hoc magnitude checks to decide when
that divisor was unsafe, instead of using one well-conditioned quantity
for both jobs. `horthocircle` is rewritten around a numerically stable
closed form instead of patching threshold after threshold: a circle
centered at `(cx,cy)` is orthogonal to the unit circle exactly when
`cx^2+cy^2 = r^2+1`; combined with `|c-p_i|^2 = r^2` for each of p1, p2,
the `r^2` term cancels and each point contributes one linear equation,
`c.p_i = (|p_i|^2+1)/2`. Two points give a 2x2 linear system for `c`,
solved by Cramer's rule with determinant `D = x1*y2 - y1*x2` — which is
exactly the round-three angular test's own cross product (`sin(angle)`
between p1 and p2 as seen from the origin, scaled by `|p1||p2|`), so `D`
is small precisely when p1, p2, and the origin are close to collinear
(including either point being at/near the origin, where `D` vanishes
trivially) and nowhere else. One test, `D` near zero, now both decides
degeneracy and is the only divisor in the non-degenerate branch — no
separate near-origin cutoff, no separate cancellation guard. (One
follow-on fix along the way: the degeneracy test needs `le`, not `lt` —
when a point is exactly at the origin, `D` is exactly `0.0` too, and a
strict `<` lets `0.0 <= 0.0`'s equal case fall through to divide by a
literal zero. Caught the same way as the original P1: `gs`'s arithmetic
rounds *very* slightly differently from this interpreter's own for the
same nominal formula, so a pair that lands exactly on `D = 0.0` under
`gs` can be a hair off zero under `pscat` and vice versa — this class of
bug is only reliably caught by testing under both.) A second, unrelated
latent bug surfaced by the same `{10,10}` case and fixed alongside: the
isline branch's own direction formula (`(p2-p1)/|p2-p1|`) divides by
zero if two supposedly-adjacent polygon vertices have numerically
coincided, which can happen this deep in the BFS; it now falls back to
an arbitrary unit vector for that (already-degenerate) edge instead of
propagating a NaN.

Reverified every discriminating case from rounds two through five
against the rewrite: `hreflect_stays_exact_near_the_old_radius_cap_boundary`
(radius ~51) and `horthocircle_collinearity_test_is_scale_invariant`
(scale-invariant collinearity) both pass unchanged, and a new
`horthocircle_does_not_special_case_points_merely_near_the_origin` pins
the round-five near-origin counterexample.

**Round six** found two more real bugs in what round five shipped, plus
a real test-suite cost problem — proof that "the math is now provably
right" is a stronger claim than "every counterexample so far passes,"
and worth staying skeptical of even after a from-scratch rewrite:

- The `{10,10}` depth-4 tile count the round-five rewrite computed,
  8,191, was itself still wrong — by exactly 10, not by coincidence.
  The `{p,q}` tile-adjacency graph's girth (shortest cycle) is `q`: `q`
  tiles meet at every vertex, and each adjacent pair in that fan shares
  an edge, closing a `q`-cycle in the adjacency graph with no shorter
  cycle possible. For `{10,10}`, girth 10 means no two of a depth-<=4
  BFS's root-to-tile walks (combined length <=8 < 10) can reach the same
  tile without closing a cycle shorter than the girth allows — so the
  *true* count at this depth is just the plain reflection-tree growth,
  1 + 10 + 90 + 810 + 7,290 = 8,201 tiles, and the dedup step should
  find zero genuine duplicates this shallow. It wasn't finding zero: the
  edge-length-scaled dedup tolerance (`httol`, `0.3` times a candidate's
  own max edge length) was loose enough to merge ten tiles that were
  never actually duplicates. A sweep on the *rewritten* geometry (a
  cleaner search than the same sweep run against the old, cancellation-
  noisy formula in round four, which is likely why it looked
  unreproducible then) found `0.3`/`0.25` give `8,191`/`8,201` — the
  cliff is between them — and `0.2`/`0.05` both give `8,201` with no
  runtime cost change (~31s either way; `0.02` and tighter start timing
  out, the same O(n^2)-blowup-toward-`htmax` risk found in round four).
  Retuned to `0.2`: recovers `8,201` here, leaves the shipped range
  (`{7,3}` depth 4's 232, `{6,4}` depth 5's 1,711, `{7,3}` depth 2's 29)
  and the gallery render (byte-identical) unchanged. The round-four/five
  NOTES entries above that called `8,201` an overestimate from "not
  accounting for what dedup exists to do" had the framing backwards —
  dedup exists to catch *coincidental* re-visits from different walks,
  and girth 10 rules those out entirely at this depth; the discrepancy
  was the tolerance being too loose, not the growth-series estimate
  being too naive.
- The isline branch's diameter direction, `(p2-p1)/|p2-p1|` (the chord),
  is only correct when p1/p2 are *exactly* collinear with the origin,
  where it agrees with the true radial direction. For a near-collinear
  pair let in by this branch's angular tolerance, if p1 and p2 have
  similar magnitude, the chord can be dominated by their tiny angular
  offset and point nearly tangentially instead — confirmed with
  p1=(0.99, 0), p2 a hair's-width away in angle at nearly the same
  radius: reflecting p1 across its own (isline-classified) geodesic
  moved it to roughly `(-0.99, -0.001)` instead of fixing it, the same
  defining-point violation as every other bug in this family. Fixed by
  taking the direction from whichever of p1/p2 is farther from the
  origin instead of their difference — better-conditioned, and exact
  whenever the pair really is collinear (both points' own directions and
  the chord's then agree). New regression test:
  `horthocircle_isline_fallback_uses_the_radial_not_chord_direction`.
- The `{10,10}` depth-4 regression test itself was an unbudgeted cost:
  fine in release (~31s, tolerable for one test) but the plain `cargo
  test` this repo's quality gate actually runs builds debug, where the
  same O(n^2) dedup scan over ~8,200 tiles didn't finish in 90+ seconds.
  Marked `#[ignore]` with an explanation and a pointer to run it
  explicitly (`cargo test -- --ignored`) rather than shrinking it to a
  smaller `{p,q,depth}` that wouldn't exercise the same girth-driven
  "dedup should find nothing" property, since the whole point of this
  particular test is pinning that behavior at the extreme case that
  found it. The specific bugs it originally caught (the crash, the
  near-origin misclassification, the chord-direction error) each have
  their own cheap, direct `horthocircle`-level regression test now, so
  losing this one test from the default run doesn't lose coverage of
  any individual defect — only of the exact combinatorial tile count at
  this one extreme configuration.

`{10,10}` depth 4 stays well under `htmax` (20,000) even at the tighter
`0.2` tolerance, so none of this is a silent-truncation artifact either.
The `gs`-compat driver in `tests/artkit.rs` runs a `{10,10}` depth-4
`httile` directly — the exact configuration that produced the original
crash — so the crash fix is covered under `gs` itself on every default
test run, even with the full tile-count regression test marked ignored.

**Round seven** found one more real bug in the collinearity test itself,
plus a P2 dispositioned as out of scope rather than fixed:

- **Fixed.** The collinearity test compared `sin(angle between p1 and p2
  as seen from the origin)` against a threshold relative to `|p1||p2|`
  — scale-invariant with respect to how far p1/p2 are from the origin
  (fixing round three's bug), but that turns out not to be the right
  criterion at all: `sin(angle)` is also small whenever p1 and p2 are
  simply *close together*, independent of whether the geodesic through
  them is anywhere near a diameter. Confirmed with two real vertices from
  a `{10,10}` depth-4 BFS, `(0.9371500703, 0.3489261063)` and
  `(0.9371497472, 0.3489269739)` — about `9e-7` apart near the disk
  boundary — where the old test reported `isline=true`, but the *true*
  orthogonal circle there has radius about `9.17e-6`: a small, perfectly
  well-conditioned circle, not remotely diameter-like. Treating it as a
  diameter discarded correct, easily-computable geometry for a wrong
  approximation. The first fix attempted — drop the tolerance to bare `D
  = 0` exactly, reasoning that the linear solve is stable even at tiny
  `D` so there's no geometric need to approximate anything — broke
  ordinary, non-extreme cases (`{7,3}` depth 2's pinned count moved from
  29 to 35, with ink landing outside the disk in the boundary-containment
  test), because `D`'s own subtraction (`x1*y2 - y1*x2`) accumulates
  rounding error through the chain of prior reflections composing its
  inputs, so two values that are *mathematically* equal (true
  collinearity) essentially never subtract to bit-exact `0.0` in
  practice. The actual fix compares `D` against its own subtraction's
  noise floor — `eps * (|x1*y2| + |y1*x2|)`, the standard robust-predicate
  formulation, with `eps = 1e-12` — which is small only when there's
  genuine floating-point cancellation in computing `D` itself, not merely
  when p1/p2 happen to be nearby. This also meant retuning the round-six
  regression test: its original pair no longer takes the isline branch at
  all under the tighter threshold (correctly — it isn't actually
  degenerate), so that test now uses a purpose-built pair (`(0.7, 0.3)`
  perturbed by a `~7e-13` *purely tangential* nudge) tuned to still
  exercise the isline branch's radial-vs-chord fix. New regression test:
  `horthocircle_does_not_treat_close_together_points_as_collinear`.
  Reverified: every previous discriminating case, all four pinned tile
  counts (29, 232, 1,711, 8,201), the gallery render (byte-identical),
  and the `gs` isolate case all still pass with the retuned threshold.
- **Out of scope, not fixed.** The review also flagged that `hgeo`/
  `hpoly`'s drawing-only `hor 50 gt` cutoff (approximating a large
  orthogonal circle as a straight chord for rendering, added in the
  original round-one `limitcheck` fix, well before this issue's
  cross-model review rounds began) is a fixed logical-coordinate
  threshold rather than one scaled to the actual device-space rendering
  error (sagitta) at the frame size in use — true, and a reasonable
  rendering-fidelity refinement, but it's pre-existing behavior this
  branch didn't introduce or change, not a defect in anything rounds
  four through seven touched. Left as a possible follow-up, not chased
  here.

**Round eight** found one genuine bug and two real gaps, none of them
another adjacent counterexample in the `{10,10}`-only family the
previous rounds had been narrowing in on:

- **Improved, not eliminated — see the `gs` precision paragraph
  above.** The round-seven `eps=1e-12` threshold, tuned against this
  interpreter's f64 arithmetic, was far tighter than `gs`'s own 32-bit
  float precision needs: under `gs`, pairs whose `D` should trip the
  degeneracy test instead fall through to a division `gs`'s own
  arithmetic can't reliably compute, producing corrupted circle centers
  — confirmed on the *shipped* `{7,3}` depth-4 gallery piece, which
  rendered 323 tiles under `gs` instead of 232. Retuned to `eps=1e-6` —
  the largest value that still keeps every pinned f64 tile count exact
  and doesn't misclassify either the round-six or round-seven
  discriminator pairs (whose own ratios are 9.67e-13 and 1.42e-6
  respectively, so the admissible window for a single constant is
  genuinely that narrow: about one order of magnitude, sandwiched
  between `gs`'s ~1.2e-7 float32 epsilon and the round-seven pair's own
  ratio). This took `gs`'s `{7,3}` depth-4 count from 323 to 233 — a
  large improvement — but not to the exact 232 this interpreter
  computes: even `{7,3}` depth 2 (three reflection generations, about
  as shallow as this algorithm gets) diverges by one tile under `gs`
  (29 vs 30). That ruled out chasing exact parity any further: if a
  three-generation tiling doesn't match, no single-constant threshold
  fix inside `horthocircle` is going to make a four- or five-generation
  one match either — the gap is `gs`'s float32 arithmetic itself, not a
  remaining defect in this formula. `ghostscript_accepts_artkit`
  (`tests/artkit.rs`) now checks the `{7,3}` depth-4 tile count against
  a `[200, 260]` sanity band under `gs` (catching a crash, a collapse,
  or another round-eight-style inflation) instead of asserting an exact
  count that `gs` cannot be relied on to hit; exact counts stay asserted
  under this interpreter only.
- **Fixed.** The new gallery piece wasn't linked from the published
  site's gallery page (`site/gallery.html` — separate from
  `gallery/README.md`, which was already updated when the piece
  shipped; the site page is hand-authored and copies renders at build
  time via `scripts/build_site.sh`, so adding the file wasn't enough).
  Added the card.
- **Fixed.** The root `README.md`'s "Making art" section, which
  AGENTS.md requires kept current as capabilities land, still stopped
  at the Euclidean tiling section and never mentioned the hyperbolic
  toolkit added by this issue. Added a paragraph naming `hpoint`/
  `hpolar`/`horthocircle`/`hgeo`/`hpoly`/`hreflect`/`httile` and
  pointing at the gallery piece.

Deliberate omissions: only the Poincare disk model (the issue left model
choice open; upper-half-plane wasn't needed for anything built here) and
only reflection-generated regular {p,q} tilings (no general Mobius
transformation group, no irregular/non-edge-to-edge tilings) — sufficient
for the "distinct coordinate/projection handling" the issue asked for
without building machinery nothing here uses yet.

## A procedural jittered-stroke Hangul face (issue #6, 2026-08-02)

Closes issue #6, set aside earlier (see issue #9's entry below) as "a
real design project in its own right, not a quick pass." It is one:
11,172 precomposed Hangul syllables can't be hand-authored the way
`lib/handscript.ps` hand-authored ~40 Latin letters, so the only
workable approach — per the issue body — is compositional: author the
jamo (letter) components, then assemble a syllable block from them at
show time following Unicode's own decomposition arithmetic.

**The blocker this needed first.** A Hangul syllable is up to U+D7A3 —
three UTF-8 bytes — but PostScript's Type 3 `BuildChar` is hard-wired
to a byte-per-glyph model (`src/font.rs`'s `ShowCtx`/`begin_type3_glyph`
always narrowed the character code to `u8` before handing it to
BuildChar). The existing `CatalogEncoding::Unicode` mechanism (Stage
22/23's Noto Sans KR etc.) doesn't help here — it decodes UTF-8 for
*bundled TrueType* faces via `cmap`, with no bearing on Type 3 dicts at
all. Fixed with a narrow, opt-in extension in the same spirit: a Type 3
font dict with `/UnicodeBuildChar true` gets `show` decoding its
argument as UTF-8 and passing the full codepoint to `BuildChar` as an
`Object::int`, instead of narrowing to a byte
(`font::is_unicode_type3`, checked in `ops/font.rs`'s `begin_show`
alongside the existing `is_unicode_font`). `PendingGlyph`'s `byte: u8`
widened to `code: u32` to carry it through glyph-context teardown;
`BuildGlyph`'s name-driven path (which still needs a byte to index
`Encoding`) errors `rangecheck` if a Unicode-mode dict's codepoint
doesn't fit one — no face in this repo hits that, since `lib/hangul.ps`
only defines `BuildChar`. Ordinary byte-oriented Type 3 fonts are
untouched: their codes are already ≤255, so widening the field and
passing the whole `code` through instead of a pre-narrowed `byte` is a
no-op for them, confirmed by a regression test asserting an unflagged
Type 3 dict still gets raw, non-UTF-8-decoded bytes from a multi-byte
string. Documented in FONTS.md's new "Unicode-mode Type 3 BuildChar"
addendum, parallel to the Stage 22/23 addendum for the TrueType case.
This is a pscat-only deviation with no Ghostscript equivalent — unlike
`lib/handscript.ps`, `lib/hangul.ps` does not run under gs.

**The face itself** (`lib/hangul.ps`, `/HangulScript`). Unicode's
Hangul Syllables block is arithmetic: `si = codepoint - 0xAC00`,
`L = si/588`, `V = si%588/28`, `T = si%28` picks out the initial
consonant (choseong, 19), medial vowel (jungseong, 21), and final
consonant (jongseong, 0=none plus 27) by index. Hangul's featural
design means those decompose further:

- 19 choseong + 28 jongseong reduce to 14 atomic consonant shapes
  (ㄱㄴㄷㄹㅁㅂㅅㅇㅈㅊㅋㅌㅍㅎ) plus a side-by-side compositor
  (`drawshapes`) for the doubled consonants (ㄲㄸㅃㅆㅉ) and the
  eleven two-consonant jongseong clusters (ㄳㄵㄶㄺㄻㄼㄽㄾㄿㅀㅄ) —
  no need to hand-author 33 more shapes independently.
- All 21 jungseong reduce to a vertical bar with a left/right tick
  (`drawVvowel`), a horizontal bar with an up/down tick (`drawHvowel`),
  or — for the 7 diphthongs (ㅘㅙㅚㅝㅞㅟㅢ) — a real composition of
  one of each (e.g. ㅘ is literally ㅗ + ㅏ, not an approximation).
- The strokes reuse `lib/handscript.ps`'s jitter/curve-fit renderer
  verbatim, with one addition: corners. Handscript's curve-fit (built
  for cursive Latin, where every corner is meant to round off) turned
  ㄱ's right angle into a single soft curve when its top-horizontal and
  diagonal were one 3-point stroke; splitting each corner into its own
  2-point stroke keeps Hangul's sharp corners sharp while still
  wobbling. The circle (ㅇ) and the hood on ㅎ stay multi-point
  smoothed strokes — they're supposed to be curved.
- Six standard syllable-block layout classes (vowel type
  vertical/horizontal/diphthong, crossed with final present/absent)
  place the choseong/jungseong/jongseong cells within the glyph's
  1000-unit em square; `placepoints` scales/translates each atomic
  shape's stroke data (authored once, in its own 0..1000 design
  square) into whichever cell it lands in.

Scope, stated up front rather than discovered later: modern Hangul
only (no archaic jamo); non-Hangul codepoints in the same string get a
half-width advance and draw nothing (`/HangulScript` is a dedicated
Hangul face, the same posture handscript.ps takes for Latin, not a
mixed-script one).

**The wrap engine** (`hg-write`/`hg-linecount`) mirrors
`lib/handscript.ps`'s `HSLayout` shape (a fixed scratch buffer built up
with `putinterval`, one `lineproc` call per completed line) but can't
reuse its byte-indexed width lookup — Hangul syllables are 3 UTF-8
bytes, not 1. `wordadv` decodes UTF-8 fully rather than just counting
codepoints, because the two advance classes BuildChar draws by (0.9em
for a Hangul syllable, 0.45em otherwise) depend on the actual codepoint
value; this has to match BuildChar's own choice exactly; or
`hg-linecount` and `hg-write` would disagree about where lines break,
same invariant handscript.ps's `advof`/`wordadv` keep for Latin
(verified by a test that runs a pre-pass `hg-linecount` before
`hg-write` and diffs the rendered pages).

Two real bugs surfaced by rendering and looking, not by reasoning
about the PostScript in the abstract:

- `drawshapes`'s two-shape (side-by-side) branch pushed `x0 x1 y0 y1`
  onto the stack instead of the `x0 y0 x1 y1` `drawshape` expects —
  degenerately anisotropic scaling on every doubled consonant and
  two-consonant jongseong cluster, rendering as near-invisible
  slivers rather than erroring (a silent-corruption class of bug, not
  a stack-underflow one, so nothing short of looking at the output
  would have caught it).
- `wordadv`'s inner UTF-8-decode loop used `/i` as its own byte index —
  the same name `layout`'s outer word-scanning loop uses for its own
  index, and both land in the same dict (neither proc opens its own
  scratch dict). Calling `wordadv` mid-scan silently clobbered the
  outer scan's position, making `layout` re-process the same handful
  of bytes forever — confirmed by instrumenting both loops with
  `print` and watching the scan index reset instead of advance. Fixed
  by prefixing `wordadv`'s locals (`wi`/`wn`/`wb`/`wcp`/`wstep`); see
  the comment left on `wordadv` for the general gotcha (any name
  `def`'d in a dict-less proc is visible to, and clobberable by, its
  caller's own locals in the same dict).

`examples/hangul_handscript.ps` is the specimen (ruled paper, the same
`hg-write` API, a short note using a few well-known Korean phrases
including the standard Hangul font-testing pangram, "다람쥐 헌
쳇바퀴에 타고파"). `tests/hangul.rs` covers the decomposition
arithmetic at both ends of the Hangul block, ink from a syllable in
each of the six layout classes, the no-two-glyphs-match and
same-seed-same-page invariants, non-Hangul codepoints not erroring,
and the linecount/write agreement above; `tests/type3.rs` gained two
tests for the underlying `/UnicodeBuildChar` mechanism itself (a
flagged dict receives the full codepoint; an unflagged one still gets
raw bytes).

## Cross-model Codex review + a DSC-scanner metadata bug fix (issue #26, 2026-08-01)

Closes issue #26, a postmortem/findings issue from the first
`work-issue` run that dispatched independent PR review to Codex
instead of a same-model blank-context `Agent`. Two things landed here:

- `work-issue` step 8 now runs the review through Codex
  (`codex-companion.mjs`) when the runtime is available, with a
  same-family `Agent` fallback; findings post to both the PR and issue
  unconditionally (including clean passes), and policy is "fix or give
  an explicit stated reason" per finding rather than a severity-tag
  auto-gate. Full rationale and the reliability findings from building
  it (Codex's CLI exiting silently mid-review with no error signal —
  upstream, not fixable from this repo; the real `review --json` output
  shape vs. the documented schema) are in issue #26's body.
- `pdf::scan_document_info` (added for issue #8) stopped only at the
  first non-comment line, not at `%%EndComments` — DSC's actual header
  boundary. A comment-only prologue, or an embedded document's own
  `%%Title:`/`%%For:`, appearing after `%%EndComments` but before any
  executable line could silently override the outer document's
  metadata. Fixed to stop at whichever comes first.

Deferred to its own issue (#29): a race in `work-issue`'s crashed-run
resume logic (step 1 can misclassify a still-live in-progress claim as
abandoned in the window between labeling and PR-open; step 3 also
recomputes the branch name from a fresh slug on resume instead of
reading the worktree's actual checked-out branch) — needs a real
lease/heartbeat signal, not a quick fix alongside the above.

## A tiling/tessellation library for artkit (issue #9, 2026-08-01)

Closes issue #9. Run via `work-issue` after issue #6 (a procedural
Hangul font) was set aside at the user's call — a real design project
in its own right, not a quick pass — in favor of this more tractable
backlog item.

`lib/artkit.ps`'s `grid` (Stage 19) already covered square tiling, so
the actual gap was the other regular tessellations plus something with
real generative-art teeth. Added six procedures: `lattice` (the
general primitive — walks an n1×n2 grid of points at `x0,y0 + i·v1 +
j·v2` for any basis pair, oblique included, calling a proc at each —
`hexgrid`/`trigrid` are both built on it); `hex` and `tri`, shape
primitives in the same spirit as `ngon`/`star`/`rrect`; `hexgrid`
(pointy-top hexagons, odd rows staggered) and `trigrid` (alternating
up/down triangles — cols upward triangles span the width, each row
packs `cols*2`) for the other two regular tilings; and `truchet`,
which walks a plain square grid like `grid` but wraps each cell in
`gsave`/translate-to-center/`4 chance 90 mul rotate`/`grestore` before
calling the stamp proc — so a single motif, randomly quarter-turned,
reads as a continuous flowing pattern even though the grid underneath
is perfectly regular. True Penrose-style aperiodic tiling is a
deliberate omission (documented in the section's header comment): it
needs substitution/matching-rule machinery well beyond a lattice walk,
and `truchet` already delivers the "looks aperiodic" effect generative
art actually wants.

Geometry was verified two ways before trusting it: render-and-look
(caught a `trigrid` row-count miscalculation and a `lattice` basis
that ran a demo panel off the page — both fixed by recomputing the fit
rather than fudging the numbers) and a `tests/artkit.rs` coverage
test that fills a hexgrid/trigrid tiling on a canvas sized to exactly
match the tiled region (so page-edge clipping does the job a crop
would, and spillover ink can't hide off-canvas and inflate the count)
and checks it against a solid-fill baseline — a real gap or overlap
bug would show up as materially less ink, not just a few percent from
legitimate edge rounding. Also pinned `lattice`'s exact point sequence
arithmetically (an oblique basis, checked point by point) and
`truchet`'s rotation spread by reading `cos(rotation)` back via
`currentmatrix` from inside the stamp proc across many cells —
proving the four quarter-turns actually vary, not just that `truchet`
runs the right number of times.

Caught by an advisor review of the diff, not by any of the above: a
stamp proc calling `hex`/`tri` from inside `hexgrid`/`trigrid` — the
obvious, advertised way to use them, and exactly what the new demo and
gallery piece both do — clobbers the outer driver's own position state
if the inner call isn't wrapped in its own dict, since both share the
`tg-` scratch prefix and PostScript `def` always targets whichever
dict is innermost. Confirmed with a test showing 9 hex centers
corrupting into a diverging drift starting at the 3rd cell, then
confirmed the fix (wrap just the inner call: `3 dict begin cx cy r
hex end`) restores the exact expected centers — kept as a permanent
regression test. Checked whether this was new: it isn't — `grid`
calling `ngon` un-wrapped (`tk-` prefix) has the identical bug,
already latent in the Stage 19 original. Documented the gotcha and its
fix in the tiling section's header rather than redesigning either
family's scratch-naming, which would be a much larger, unrelated
change; the demo/gallery code already wraps correctly (they don't use
a persistent per-cell counter, so the naive whole-stamp `dict
begin/end` they use is safe for them specifically — a counter defined
inside that same wrapper would itself be silently discarded at `end`,
a second trap documented alongside the first).

New demo: `examples/tiling.ps`, a four-quadrant specimen (hexgrid+hex,
trigrid+tri, truchet, and an oblique `lattice`). New gallery piece:
`gallery/woven_labyrinth.ps`, "Woven Labyrinth" — Sébastien Truchet's
actual 1704 tile (a square split corner-to-corner into two contrasting
triangles, not the later quarter-circle-arc variant) at 256 random
quarter-turns, framed and titled; inlines `chance`/`truchet` per the
gallery's self-containment doctrine (see `hortus.ps`). Renders
identically in structure under `pscat` and `gs` (colors differ between
runs of the two — expected, since nothing pins `rand`'s stream
bit-for-bit across implementations, only that both accept the file and
tile without gaps).

## PDF document metadata + a stroke-recording bug fix (issue #8, 2026-08-01)

Closes issue #8. First run of `work-issue`'s new independent-review/
merge steps, via the `/loop /work-issue` unattended loop.

The issue's title suggested a broad new capability ("generate PDF
output and support distribution to Kindle"), but multi-page PDF export
already existed in full since Stage 9 (`src/pdf.rs`, a real `/Pages`
tree, gs-round-trip tested). The actual gap, same pattern as issue
#12: pscat's PDFs carried no `/Info` document metadata (Title/Author/
Producer) — the thing that makes a reader's library (Kindle, Books,
any PDF viewer) show an authored title instead of a bare filename.
Verified against gs (this project's semantics oracle) before building
anything: `gs -sDEVICE=pdfwrite` on a file with `%%Title:`/`%%For:`
DSC header comments embeds them as document metadata (`dc:title`/
`dc:creator` XMP entries) — so pscat doing the same is matching an
established convention, not inventing one. Deliberately partial
parity, not full: gs also reads `%%Creator:` into a separate PDF
`/Creator` field (the authoring *application*, distinct from
`/Author`); pscat doesn't emit `/Creator` at all here — `/Producer`
stays fixed at `pscat` regardless of DSC comments, which already
identifies the generating tool.

Added `pdf::scan_document_info` (scans DSC header comments, stops at
the first non-comment line since DSC requires headers to precede
program content), `PdfRecorder::set_info`/an `/Info` object in
`finish()` (appended *after* every page/image object specifically so
it can't shift the existing positional object-numbering math — kids
indices and `image_base` are computed against the pre-`/Info` object
count), a `pdf_string` encoder (ASCII → escaped literal, non-ASCII →
UTF-16BE hex string with BOM, since a Hangul/Japanese title is
plausible given this project's font work and PDF text strings need
one or the other), and `Gfx::set_pdf_info` wired into `main.rs`'s
`--pdf` path.

For "distribution to Kindle": Kindle's Send-to-Kindle already accepts
PDF directly (email attachment or USB) — deliberately did not add
email-sending automation (would need external credentials, not asked
for) or a new export format (EPUB/MOBI — the issue left format open,
and PDF is what already exists and is broadly e-reader-compatible).
Also deliberately not doing: PDF outline/bookmarks/table-of-contents
for long documents (real value, distinct scope — worth its own
issue) and font embedding (pre-existing, documented deviation across
the whole PDF/SVG pipeline).

**Found and fixed a real, unrelated pre-existing bug while building
the demo example**: `Gfx::stroke`'s PDF recording lived inside `if
self.svg.is_some()`, so any `--pdf`-only export (no `--svg` — the
common case) silently dropped every stroked line. `fill` and
`fill_path_direct` never had this bug; both correctly check
`self.pdf.is_some()` independent of SVG. Caught by literally rendering
`examples/pdf_document.ps` and looking — a divider line appeared in
pscat's own PNG output but vanished from the PDF. No existing test
exercised `stroke` in a PDF-only export, so this had presumably been
silently wrong for a while. New regression test:
`tests/pdf.rs::strokes_appear_in_pdf_only_export`.

Also caught by rendering, a bug in the new example itself, not the
library: `arc` without a preceding `newpath` draws an implicit
connecting line from the current point (left over from the prior
`moveto`) to the arc's start — a stray diagonal line to a filled
circle on the demo's second page. Fixed with an explicit `newpath`.

New example: `examples/pdf_document.ps` — a two-page piece with
`%%Title:`/`%%For:` header comments, demonstrating the metadata
feature and that multi-page structure survives `--pdf`.

## Circular/curved text in artkit (issue #12, 2026-07-26)

Closes issue #12. First run of `.claude/skills/work-issue/SKILL.md`,
the new issue → branch → worktree → (advisor-reviewed plan) →
implement → (advisor-reviewed diff) → PR automation.

`lib/artkit.ps` already had `pathtext` (Stage 19) — glyph-by-glyph
text along *any* current path — which turned out to already cover the
"curved baselines more broadly" half of the issue. The actual gap was
narrower than the issue's title suggested: a circle-specific
convenience on top of `pathtext`, so a caller doesn't have to
hand-build the arc path or derive the centering-angle math themselves
(exactly what `gallery/ring_of_type.ps`, Stage 6 and thus predating
`pathtext`, still does inline).

Added two procedures, both thin wrappers around `pathtext` (no
glyph-placement logic duplicated): `ctext` (`cx cy r ang (str)`) sweeps
a circular arc clockwise from an explicit start angle — clockwise
because that's the direction that keeps glyphs upright and
left-to-right along the *top* of the circle, the usual seal/coin
composition; the bottom half reads upside down, documented as a real
property of circular type rather than hidden. `ctextctr` (`cx cy r
(str)`) is the common-case convenience: centers the string at the
circle's top (90°) by measuring `stringwidth` instead of taking an
angle. Both pad the swept arc ~5% past the string's measured width —
`pathtext` silently drops trailing glyphs if the path runs out before
the string does (no error, just missing ink), so the padding is a
deliberate margin against float/flattening error, not a fudge factor
found by trial and error.

Caught by testing, not by inspection: an early test paired a 12pt-scale
circle wrongly with a 24pt font and, separately, used one ink-count
threshold for both a 2-glyph small-radius case and a much longer
large-radius case — both looked like implementation bugs at first
glance (render-and-look confirmed the geometry itself was correct) and
turned out to be miscalibrated test expectations instead. Also added a
differential test (`tests/artkit.rs`) that compares ink with and
without a string's final character specifically, since the
"trailing-glyph-silently-dropped" failure mode is invisible to a
whole-canvas ink-count threshold — the bug this issue's arc-padding
exists to prevent wouldn't have failed the naive version of that test.

New demo: `examples/circular_text.ps` (not `gallery/` — gallery pieces
inline their own toolkit copy per the self-containment doctrine, which
would defeat the point of demonstrating a *reusable* procedure;
`examples/style_*.ps` already establishes the `(lib/artkit.ps) run`
pattern this follows). Renders identically under `pscat` and `gs`
(with `-dNOSAFER` — `gs`'s default sandbox blocks the `run`-a-file
pattern every `examples/style_*.ps` file already uses; a pre-existing
gap in gs test coverage for `examples/`, not something new here, and
out of scope for this issue to fix repo-wide).

Also: the header's reserved-scratch-prefix list (`ap-, ld-, ls-, tk-`)
was missing `pt-`, already in use by `pathtext`'s internals since
Stage 19. Added it alongside the new `ct-` prefix while touching that
line anyway.

## Stage 23 — gallery/site catch-up, `.ttc`, a second Korean face (2026-07-24)

Closes issues #3 and #5, both filed as Stage 22 follow-ups. First work
to go through the issue → feature branch → PR SDLC adopted alongside
Stage 22.

**#3 (gallery/site catch-up):** `scripts/build_font_gallery.sh`'s
`TABLE` and `examples/font_catalog.ps`'s page-2 specimen grid gained
rows for NotoSansKR/JP/Thai (and the new NanumBrushScript, below). The
grid's two-column layout turned out to have exactly 2 free slots, not
zero as first assessed — adding all 4 new faces (39 total) needed the
row pitch tightened from 37 to 33pt to fit 20 rows/column; verified by
actually rendering it; a first attempt at 35pt (18 free slots) still
overflowed by one entry, caught by rendering and spotting the overlap
before it shipped. The grid's overflow arm always resets to the second
column's position, so a 41st entry would silently overprint the 20th
rather than erroring or wrapping to a third column — a latent trap an
advisor review caught; rather than build real 3-column logic for a
specimen file, left a comment at the pitch line spelling out the
40-cell ceiling so the next addition doesn't hit it blind. `site/fonts.html`
gained a new "International scripts" section and the intro paragraph's
file count was corrected to the live count (`find fonts/catalog -type
f` value) rather than hand-computed. `.ttc` is now a recognized catalog
extension in both of `src/font.rs`'s directory scans — `load_catalog_face`
always parses face index 0 (no naming convention exists for picking a
different face out of a collection). Verified, not just claimed: built
a synthetic two-face `.ttc` with `fonttools` (not checked in) and
confirmed face 0's outlines render correctly; found along the way that
`face.names()` came back empty for the synthetic collection's sub-face,
so a real bundled `.ttc`'s `FontName` should be spot-checked rather
than assumed (falls back to the file stem, which is harmless but worth
knowing). GPOS/shaping and full Type 0/CID support were re-confirmed as
correctly out of scope (already documented, not
actionable follow-ups) — not every item in a deferred-work issue needs
code; some just need re-confirming the doc already covers it.

**#5 (second Korean face):** Nanum Brush Script (OFL, `google/fonts`,
`ofl/nanumbrushscript`) — a Korean face with a handwritten brush look,
next to Noto Sans KR's plain sans. Reused the exact
`CatalogEncoding::Unicode` mechanism from Stage 22 with one line added
to `load_catalog_face`'s match; no interpreter changes. Static font (no
`fvar` table), so the variable-font weight-pinning from Stage 22
doesn't apply here — confirmed via `fonttools`, not assumed, same
verification discipline as the Noto Sans KR/JP Thin discovery.
Deferred to its own issue (#6, not attempted here): a *procedural*
jittered-stroke Hangul face in the `lib/handscript.ps` style, which
needs jamo-composition (11,172 syllables can't be hand-authored one at
a time) — a real design project, not a quick follow-up.

343 tests (up from 341), clippy clean, `cargo fmt --all -- --check`
clean. `tests/site.rs` (which runs the full `scripts/build_site.sh`,
wasm build included) passed, confirming the gallery/site changes
integrate end to end, not just render standalone.

## Stage 22 — Korean, Japanese, Thai fonts (2026-07-24)

Not on the original roadmap; came from a direct ask ("can we print out
in Korean?"). The blocker wasn't fonts, it was architecture: `show`'s
whole pipeline is byte-indexed (a `u8` per character code, a 256-entry
`Encoding` array per font), and Hangul (11,172 precomposed syllables)
and Japanese kanji (thousands) blow past that ceiling by two orders of
magnitude — Thai's ~90 codepoints would have fit, but building a
one-off byte-packed encoding for Thai alone while leaving Korean/
Japanese impossible seemed like the wrong shape.

Landed a scoped, documented deviation instead of the full Type 0/CID
machinery real conformance would need (still out of scope — see
FONTS.md's Deferred list): a new `CatalogEncoding::Unicode` variant
marks three catalog faces (Noto Sans KR/JP/Thai, SIL OFL,
`google/fonts`) as Unicode-mode. `ShowCtx` decodes the shown string as
UTF-8 into Unicode scalars instead of raw bytes when the active font
is Unicode-mode (decided once per show, keyed on `FID` so it survives
`scalefont`/`makefont`), and each scalar maps straight to a glyph via
the face's `cmap` — no Encoding array, no glyph-name resolution.
Everything downstream of glyph-id resolution (the paint/measure math)
is shared with the byte-mode path via a new `paint_resolved_glyph`
helper; byte-mode fonts are untouched bit-for-bit (340 tests all still
pass, no line of the existing 331 needed to change). `kshow`'s proc
now sees real Unicode scalars for these fonts — strictly additive for
existing text; `widthshow`'s `char` operand deliberately stayed capped
at a byte (its one real use, widening space to justify a line, never
needs more).

Found mid-implementation: Noto Sans KR/JP ship only as variable fonts
whose *default* named instance is Thin (wght 100), not Regular —
confirmed via the `fvar` table. Without pinning, the catalog would
have silently rendered (and reported, via the name table's PS name)
the wrong weight. `load_catalog_face` now pins `wght` to 400 for any
variable-font catalog entry — a general policy, not special-cased to
today's three files — and Unicode-mode faces get their `FontName` from
the file stem rather than the name table (which stays "…-Thin"
regardless of the pinned instance; it's a static string).

Tradeoffs, documented in FONTS.md: no GPOS/mark positioning (Thai
combining marks stack on bare advance, not shaped attachment); the
`/Encoding` array on these three dicts is now vestigial (the classic
re-encoding idiom has no effect on them); `.ttc` still isn't a
recognized catalog extension. Adds ~20MB to `fonts/catalog/` and the
packaged release tarball (Korean/Japanese need full Hangul/kanji
coverage; there's no well-licensed smaller option) — a deliberate
tradeoff, confirmed with the requester before fetching the fonts.

New: `tests/catalog.rs`'s Unicode-mode section (resolution, ink
coverage, stringwidth scaling, ASCII+Hangul mixing, a kshow test
pinning scalar-not-byte codes), `examples/international_text.ps`.
Deferred: updating the public gallery/site (`scripts/build_font_gallery.sh`,
`site/fonts.html`) and `examples/font_catalog.ps`'s page-2 grid for
these three faces — the grid's two-column layout is already sized to
exactly fill the page, so appending rows needs a layout change, not
just three more lines; left for whoever picks up the gallery next.

## Stage 21 — standalone installability (2026-07-24)

Not on the original roadmap (all 20 stages were done); came up when
a teammate wanted pscat runnable without a git checkout. Two prior,
independently-written path-resolution chains existed —
`font.rs::catalog_root()` (four candidates, `CARGO_MANIFEST_DIR` and
CWD included) and `pscat_mcp.rs::repo_root()` (two candidates, no
CWD) — divergent by accident, not design (confirmed against NOTES.md
history: nothing documents why Stage 14's MCP server didn't reuse
Stage 15/18's font-catalog logic). Consolidated into `src/paths.rs`,
used by both plus a new call site: the `run`/`file` operators
(`src/ops/file.rs`), which previously had *no* fallback at all —
`(lib/artkit.ps) run` only worked with CWD already at the repo root.

The one real design wrinkle: `run`/`file` can't use the font
catalog's candidate order (heuristics before CWD) without risking a
bundle/dev-checkout heuristic silently shadowing a file the caller
meant relative to CWD (e.g. `(helper.ps) run` from a subdirectory).
Nothing in the existing suite exercised the PS-level `run` operator
with a relative path, so this was a real coverage gap, not just an
absence of failures — `tests/files.rs` now has both a CWD-resolution
regression test and unit tests in `src/paths.rs` pinning the two
different orders (`program_file`: CWD before heuristics;
`catalog_dir`: heuristics before CWD, unchanged from before this
module existed).

Found and fixed a genuine bug of my own mid-implementation:
`pscat_mcp.rs::repo_root()`'s rewire initially took `.parent()` of
the *resolved file path* (`root/scripts/handwrite.sh`) once, landing
one directory too shallow (`root/scripts` instead of `root`) —
`tests/mcp.rs`'s `handwrite_returns_a_note` caught it immediately.
Fixed by walking back up exactly as many path components as the
target has, rather than a hardcoded double `.parent()`.

`scripts/package_release.sh` (new) builds `pscat`/`pscat-mcp` and
bundles them with `lib/`, `fonts/catalog/` (wholesale — license
`.txt` files travel with the fonts, not filtered out), and a
bundle-aware `scripts/handwrite.sh` (same file works unmodified in
either layout — it already anchored to its own location via
`dirname "$0"`, only `BIN` needed a bundle-first fallback chain).
Binaries sit at the bundle root so exe-sibling resolution needs zero
configuration: unzip, add to `PATH`, done. `examples/`/`gallery/`
excluded — confirmed nothing at runtime depends on them.
`.github/workflows/release.yml` runs the packaging script and
publishes the tarball to GitHub Releases on a version tag push.

Deferred: multi-platform builds (macOS-only for now, matching what's
actually been tested); a `brew` tap (a plain tarball was enough for
the immediate need).

## Stage 20 — the style packs (2026-07-19)

Four motif libraries in `lib/styles/`, layered on artkit (loaded
first; each pack registers three palettes into artkit's `Palettes`
dict, so `pal`/`palpick` just work). All pure PostScript: nothing
drawn on load, deterministic under srand, gs-clean. The split
follows artkit's grammar — *path builders* append to the current
path for the caller to paint, *painted stamps* are self-contained
gsave/grestore units, and per-pack dials (`/spmetal`, `/sfworld`,
`/tnink`) steer the stamps the way the Type 3 faces' scratch dicts
do. Scratch prefixes sp-/py-/sf-/tn- extend artkit's reserved list.

- **steampunk.ps** — `gear` (tooth ring plus an `arcn`-wound bore so
  nonzero fill leaves the hole), `rivet`, `pipe` (body/sheen/flanges
  from one segment), `gauge`, `plateframe` (rivet counts derived
  from pitch; corners always hit). /brass /verdigris /boiler.
- **psychedelic.ps** — `rays`, `blob` (sine-breathing circle; the
  concentric-nesting workhorse), `spiral`, `wavy`, `kaleido` (n
  rotated replays about a center — the proc runs with py- names
  live, documented), `rainbow` (sethsbcolor hue wheel). /acid
  /blacklight /sherbet.
- **scifi.ps** — `glowstroke` (halo/mid/core via artkit `shade`),
  `starfield`, `planet` (bands clipped to the disc, terminator as an
  offset dark disc), `planetring` (ellipse built under a temporary
  matrix, restored with `setmatrix` so the stroke pen stays round —
  gsave/grestore would discard the path with the CTM), `hudcorners`,
  `reticle`, `hexfield` (ngon honeycomb, shared edges land twice),
  `gridfloor` (rays to a vanishing point, horizontals bunching
  quadratically). /void /hologram /synthwave.
- **toon.ps** — the adult-animation cel look: `celfill` (flat fill,
  fat round-joined ink stroke — the foundation), `burst` (jittered
  star), `bubble` (ink-silhouette-then-inset-white two-pass, so
  bubble and tail merge with no seam), `speedlines`, `dotfill`
  (halftone clipped to the current path; the path survives the
  gsave), `eye`, `dripbox` (bottom edge melting into arcn-tipped
  drips). /saturday /latenight /pastelpop.

Specimen posters in `examples/style_*.ps` (620x800, seeded, each
exercising the whole pack plus catalog type: Rye/SpecialElite,
Monoton, Orbitron/VT323, Bangers/ComicNeue). `tests/styles.rs` pins
load-cleanliness, palette registration, per-motif ink, the gear
bore, dotfill's path survival, and gs acceptance of all four packs.
The psart skill and README grew style-pack sections.

Deferred: no pattern-fill textures (would want Level 2 pattern
dictionaries), no half-behind planet rings (painted stylized, fully
in front), halftone dots are square-grid rather than 45-degree
screen-angle.

## Stage 19 — the art toolkit (2026-07-19)

pscat as an instrument: everything an agent needs to *make* art, not
just render it.

**The operators.** `pathforall` joined the loop family as
`Frame::PathForall`: the current path is snapshotted at operator
time (the PLRM leaves mid-enumeration mutation undefined; a snapshot
makes the charpath-then-rebuild idiom safe), each element pushes its
user-space coordinates (inverse CTM) and runs the matching proc via
the existing `Action::ExecWith`, and `exit` leaves it like any loop.
`flattenpath` subdivides curves by de Casteljau to a fixed
quarter-pixel chord tolerance — `setflat` isn't modeled, so chord
*counts* differ from gs while shapes agree (documented deviation;
tests pin shape). Both pinned against gs byte-for-byte on user-space
reporting, dispatch, and exit semantics.

**The library** (`lib/artkit.ps`, self-contained, nothing drawn on
load, gs-clean, deterministic under srand): seeded random
(chance/jit/frnd/oneof), color mixing (mix3/shade) and eight
five-color mood palettes, a heading-and-pen turtle with a pose stack
(`tl`/`tr`, because `lt` shadows the comparison operator — cost one
debugging session and now a header warning), an L-system rewriter
capped at 60k chars (PS strings max 65535) with `ldraw` driving the
turtle, `alongpath` (stamp x/y/angle at even arc-length along any
path — the pathforall payoff; charpath text walks like anything
else), `pathtext` (type set glyph-by-glyph along a path, each glyph
at its own advance, rotated to the tangent), shapes
(ngon/star/rrect — rrect by four arcs; no arcto here), showctr /
fitfont, and a grid driver. `tests/artkit.rs` pins turtle geometry
and L-system growth by arithmetic, brushes by stamp counts,
rendering by ink, the file by gs.

**The skill** — `.claude/skills/psart/SKILL.md`: the
render-look-refine loop, the toolkit reference, type-as-material
(catalog + Type 3 dials + charpath), composition habits, a starter
sketch.

**The piece** — `gallery/hortus.ps`, *Hortus Machinalis*: a
herbarium plate, three L-system specimens (shrub / fern / swaying
weed, each grammar labeled with its rule, angle, and depth), dried
blossoms stamped along each plant's own path by `alongpath`, foxed
parchment, double-rule frame, letterspaced Palatino letterpress
(ashow). Inlines its toolkit subset per the gallery self-containment
doctrine; renders identically in gs.

Deferred: `setflat`/flatness modeling; `pathbbox`-style access to
the flattened path from Rust; text-on-path kerning refinements
(pathtext advances by plain stringwidth).

## Stage 18 — the font catalog (2026-07-18)

Typography for every occasion, in two movements.

**The outline catalog** (`fonts/catalog/`, 58 files, ~10 MB): TeX
Gyre (GUST Font License) completes the LaserWriter 35 — Pagella,
Bonum, Schola, Adventor, Chorus, Heros Cn behind the classic
Palatino/Bookman/NewCenturySchlbk/AvantGarde/ZapfChancery/
Helvetica-Narrow names; URW StandardSymbolsPS and D050000L (AGPL
with font exception, the Ghostscript faces) finally give `/Symbol`
and `/ZapfDingbats` real glyphs, closing the oldest documented
deviation; and 35 OFL/Apache single-style families from the Google
Fonts collection cover the genres — garalde to didone, geometric to
condensed, slab, mono, copperplate to marker, fraktur, western,
horror, arcade, terminal, typewriter, sci-fi, stencil, comic.
Licenses ride per-family; `fonts/catalog/README.md` is the manifest.

Design: catalog faces are **runtime-loaded, never compiled in** —
the binary and the wasm stay lean (fs access is target-gated off
wasm, which keeps its 12 builtins). `findfont` resolution order:
builtins → alias table (standard-35 names + a few shorthands) →
file-stem scan of `fonts/catalog/*/` (case-insensitive, with a
`-Regular` family fallback so `/Bangers` means Bangers-Regular) →
Ghostscript-style substitution, unchanged. Loaded faces are leaked
`Box::leak` bytes + `Face` — bounded, process-lifetime, the
systemdict-cycle doctrine — registered behind the existing FID seam
(FIDs ≥ 12 index the catalog). The TeX Gyre/URW faces are
CFF-flavored OTFs; ttf-parser outlines them through the same
`outline_glyph` path as the builtins' glyf (the variable-font
Google files render their default instances — Montserrat's default
turned out to be Thin, so Poppins took its slot). Symbol/Dingbats
encodings were dumped from gs (`SymbolEncoding`/`DingbatsEncoding`)
into `src/encodings.rs`. `pscat --fonts` lists builtins, catalog
stems, and aliases. `tests/catalog.rs`: resolution, encodings,
gs-pinned metrics (Palatino width within 0.03% of gs's URW cut),
CFF ink. Catalog root discovery: `$PSCAT_ROOT` → exe-relative →
build-time checkout → cwd.

**Three new Type 3 faces** (`lib/fonts/`, the Stage 15 skeleton and
doctrine): `/Circuitry` — PCB copper runs (solder-mask channel,
copper, specular seam) with through-hole pads at stroke terminals
and vias at 240-unit pitch; rand-free. `/Stitchwork` — cross-stitch
X's walked at 92-unit pitch but pinned to the ±45° aida grid the
way counted stitchwork actually sits, three sewing passes
(underthread/floss/sheen), seeded-jitter hand. `/Confetti` —
paper slips and dots in a six-color party palette scattered about
the stroke, dense enough to read. All three verified in gs;
`examples/font_library2.ps` is the second folio poster
(`lib/fonts/specimen2.png`), `examples/font_catalog.ps` renders the
two catalog sheets (`fonts/catalog/specimen-{1,2}.png`; page one is
gs-compatible — standard names only). Site gallery carries all
three new renders.

Deferred: full weight/italic sets for the display families (one
tasteful style each, by design); TeX Gyre Termes/Heros/Cursor
(Liberation already owns those names); embedding catalog faces in
the wasm build; a `/Symbol` fallback when the catalog is absent
(wasm and moved binaries still substitute Helvetica there).

## Stage 17 — the website (2026-07-18)

A GitHub Pages site, hand-authored per the no-framework habit:
seven pages in `site/` (landing, playground, gallery, architecture,
extending, javascript, agents) over one shared stylesheet.
`scripts/build_site.sh` assembles `_site/`: copies the pages,
builds and stages the wasm + JS library, stages the playground's
example sources (self-contained files only — no filesystem in
wasm), copies the pre-rendered gallery stills, and renders every
example page with pscat itself at its canonical size (DSC
BoundingBox where declared). The playground grew an example picker
that reads the BoundingBox to size the page.

`.github/workflows/pages.yml` publishes on push to main (rust +
wasm target → build_site.sh → upload-pages-artifact →
deploy-pages). One repo-settings step remains manual: Settings →
Pages → source = "GitHub Actions". `tests/site.rs` runs the same
build and cross-checks the site against itself — every example the
picker offers must be staged, every render the gallery shows must
exist. Verified live in a browser (Playwright): landing, gallery
(all 17 renders), and the playground drawing Cathedral Rose from
the picker.

## Stage 16 — pscat in the browser (2026-07-18)

The interpreter core cross-compiles to wasm32-unknown-unknown with
three small moves: winit/softbuffer became target-gated deps (with
`window.rs`/`spool.rs` cfg'd out — no window, no filesystem in a
tab), the two wall clocks moved behind `src/clock.rs` (Instant and
SystemTime *panic* on bare wasm32; there they read 0, the one new
deviation), and the crate grew a `cdylib` type. Everything else —
tiny-skia, flate2's Rust backend, ttf-parser, zune-jpeg, the
include_bytes fonts — was already portable. The module is ~5.5 MB,
almost all of it the four bundled Liberation faces.

No wasm-bindgen, in keeping with the no-framework habit: the export
surface (`src/wasm.rs`) is a hand-rolled C ABI over byte buffers —
alloc/dealloc, `ps_begin`, `ps_step(n)` (the same
`begin_source`/`step_n` budget the winit window uses, so a page can
*watch the program draw*), `ps_pixels`, `ps_error`, `ps_run`. One
interpreter per instance in a thread_local; errors unwind the
machine but keep page and stacks, REPL-style.

`web/pscat.js` (dependency-free ES module) wraps it: `run`,
`begin`/`step`, `paintTo(canvas)` via putImageData, `error`.
`web/index.html` is the playground — editor, speed slider, rAF
loop: the watch-it-draw window in a browser tab, with the golden
spiral of rectstrokes as the default program. `tests/wasm.rs`
builds the module and drives it under node *through the real JS
library* (render + pixel count + error path + session reuse),
skipping gracefully when the target or node is absent.

## Stage 15 — the font library (2026-07-18)

Four original display faces in `lib/fonts/`, each a self-contained
Type 3 file on the handscript.ps doctrine (defines the font, draws
nothing, runs unchanged in gs, embeddable wholesale): **/Neon**
(five widening stroke passes, dim halo to overdriven core — glow as
pure overdraw, no alpha), **/Marquee** (dark sign channel, bulbs
walked along the raw polylines at fixed arc-length pitch, halo
disks + glint, rand burnout), **/Constellation** (stars of
rand-drawn magnitude at the skeleton anchors, hairline asterism
chords, field-star scatter), **/Lapidary** (four chisel passes —
shadow, face, incision, arris highlight; square caps, mitered
joins, deliberately rand-free). One shared capital skeleton set
(A–Z = a–z, digits, `.,-'!?`), duplicated per file. Specimen poster
`examples/font_library.ps`; render `lib/fonts/specimen.png`;
`tests/fontlib.rs` pins load/ink, case mapping, seeded
reproducibility, all four bands of the specimen, and gs acceptance.

Two craft findings worth keeping:
- **Doubled skeleton points pin sharp corners.** The midpoint-
  quadratic smoothing rounds every interior vertex; duplicating a
  vertex collapses the midpoints onto it, so the curve passes
  through with a true corner. A/M/N/V/W/Z etc. read as capitals only
  after this fix — and it costs nothing in the dot/star renderers,
  which skip zero-length segments.
- **rand's low bits correlate across successive draws.** `rand 100
  mod` for the 6% bulb burnout killed whole letter-length runs of
  adjacent bulbs (letter-shaped "ghost" patches of bare channel,
  reproduced identically in gs, whose LCG shares the trait). Drawing
  from the high bits (`rand 8192 idiv`) scattered the failures into
  believable ones. Both rand-driven faces use a `chance` helper now.

## Stage 14 — pscat for agents (2026-07-18)

Two integration surfaces, because agents differ. Docs-readers:
`.claude/skills/pscat/SKILL.md` (Claude Code loads it as a skill;
it's plain markdown for everyone else — Codex reaches it via the
AGENTS.md pointer). Tool-callers: `pscat-mcp`, an MCP stdio server
(`src/bin/pscat_mcp.rs`) with three tools — `render_postscript`
(PNG images inline / SVG text / PDF to out_path; partial renders
returned *with* the error text), `handwrite` (the handwrite.sh
options), `eval_postscript` (prints back, or error name + operand
post-mortem).

The one design decision worth recording: the server **shells out to
the pscat CLI** instead of linking the interpreter. PostScript
programs write to stdout (`=`, `print`, `pstack`) and stdout *is*
the JSON-RPC channel — in-process, one `=` would corrupt the
protocol. Subprocessing also keeps tools in lock-step with the CLI
and adds crash isolation. serde_json came in for the binary (first
new dependency since zune-jpeg); hand-rolled base64 (same as
svg.rs). CLI grew `pscat -` (program from stdin) with the arg parser
special-casing a bare `-`; `tests/cli.rs` pins the CLI contract,
`tests/mcp.rs` drives the real server binary over stdio.

## `--interactive` — the windowed REPL (2026-07-18)

Stage 8 task 6's deferred sliver, built exactly as its design note
planned: `pscat -i` (or `--interactive`) is the terminal REPL and
the live window at once — type PostScript at the prompt, watch it
draw. A stdin reader thread ships raw lines into the event loop
through `winit::EventLoopProxy` user events and never touches
interpreter state; the loop owns the interpreter (the
ARCHITECTURE.md threading rule) and runs each complete chunk through
the existing `begin_source`/`step_n` frame budget, so a pasted
fractal draws live at `--speed`, not in one frozen gulp.

Mechanics: line accumulation (complete chunk vs `...>` continuation)
moved from `main.rs` into `src/repl.rs` (`LineBuffer` +
`source_is_complete`), shared by both REPLs and unit-tested. Chunks
queue while one runs; errors print via `error_report` and the
session continues (REPL semantics — `step_n` already clears the
estack and keeps operand stack + canvas). A file argument runs first
as a prelude (load `lib/handscript.ps`, then explore by hand). EOF
sets a drain flag rather than exiting: everything queued still runs,
so `printf '...' | pscat -i` works end to end. `quit`, Ctrl-D, or
closing the window ends the session. All window modes now share one
`EventLoop<UserEvent>`; only interactive sends events.

Found immediately by using it: `rectfill` was still undefined — fixed
the same day, below.

## rectfill / rectstroke / rectclip (2026-07-18)

The Level 2 rectangle conveniences, prompted by `--interactive`'s
very first session. gs pins (all in `tests/stage5_gfx.rs`): rectfill
and rectstroke paint inside an implicit gsave — current path, point,
and graphics state survive; rectclip intersects the clip and leaves
the current path *empty*; negative width/height are corner-defined;
the flat-array form takes 4n numbers (empty = no-op for fill/stroke,
clips-everything-away for rectclip; other lengths typecheck); and
for rectstroke a 6-element array on top is *always* the matrix
operand, never a rect list — it concats after the path is built, so
it shapes the pen, not the rectangles. Not supported: the PLRM's
encoded-number-string form (a string typechecks, same as gs gives a
plain string). Known deviation, inherited not new: gs strokes a
`[3 0 0 1 0 0]`-matrix pen anisotropically; our √|det CTM| width
approximation draws it uniform (standing gap in ROADMAP.md).

## Stage 13 — handwrite: string → PNG (2026-07-17)

`./scripts/handwrite.sh "any text"` → a PNG of the text written in
/HandScript, word-wrapped like a person filling a page, page height
auto-sized to the text. Appearance options are CLI flags (size,
width/height, margin, leading, jitter, pen, ink RGB, plain/ruled
paper, seed, dpi, halftone, output path).

**Where the logic lives** — `lib/handscript.ps`, deliberately: the
font (the Stage 12 definition with the gallery's /W pen-width
adaptation) plus a dict-driven layout API in a `HSLayout` dict.
Public entries: `opts hs-write` (draws; caller shows the page) and
`opts hs-linecount` (measures, drawing nothing). One options dict,
schema documented in the file header, every key defaulted except
/Text. The library draws nothing on load and runs unchanged in gs
(pinned by test), so it can be embedded wholesale in other
applications; the bash script is only argument parsing, string
escaping (lowercase + `\()` escapes), and two pscat invocations.

**The wrap engine** measures with skeleton advances (CharDefs), not
rendered ink — breaks are exact, cheap, and jitter-independent, so
hs-linecount always agrees with hs-write (pinned by test: a page
rendered with and without a preceding count pass is byte-identical,
despite the count consuming rand for line-start wobble). Greedy
wrap on space/tab/CR; newline forces the break; blank lines are
kept; a word wider than the column overflows rather than splits;
unknown characters (capitals aside — the script lowercases) advance
invisibly as .notdef.

**Auto-height** is the reuse story in miniature: the script runs
`hs-linecount` headlessly, sizes the page in awk, then renders.
`--height` overrides it.

Deferred: capitals (font has none — script lowercases; a proper
upper-case skeleton set is Gallery II-adjacent work), hyphenating
column-overflowing words, right-margin raggedness control, and a
`--stdin` mode if piping ever wants it. Five tests in
`tests/handwrite.rs` (25th suite).

## Stage 10 — halftone screens; stage complete (2026-07-17)

`--halftone` (`src/halftone.rs`): the optional "authentic laser
printer" render mode. A render mode, not the `setscreen` operator —
the page rasterizes normally and the finished raster is screened on
the way out, the way a mono printer's RIP screens a contone page.
Applied at window blit time and to `--png`; `--svg`/`--pdf` keep
their contone art. Composes with everything, including `--spool`
(drop files, watch them print like it's 1985) and `--dpi`.

The screen: luminance (Rec. 709) → classic euclidean dot on a
45°-rotated lattice, cell fixed at 5.66 device px (53 lpi at 300
dpi, the LaserWriter's own numbers). Black dots grow from the cell
center sized so coverage equals darkness; past 50% the pattern
inverts to white holes shrinking at the cell corners. The first cut
used the naive `r²` threshold and the in-module coverage test
caught it overshooting: mid-gray printed at 75%, because a spot
function's raw value isn't area-uniform (a circle holds only π/4 of
its bounding square). Five in-module tests pin white-stays-white,
black-stays-solid, ~50% at mid-gray, coverage monotone in darkness,
and pure-B/W output.

Deferred: the real `setscreen`/`sethalftone` operators (per-color
screens, spot-function procedures) if a found file ever needs them;
`--halftone` for SVG/PDF is deliberately out (they're vector).

With this, spool mode, and Hundred Lines below, **Stage 10 is
complete** — Gallery II remains open-ended for more pieces.

## Stage 10 — spool mode (2026-07-17)

`pscat --spool DIR`: the live window becomes the printer in the
corner of the lab. It opens on a blank page, polls the directory
every 400ms while idle, and renders each new `.ps`/`.eps` that lands
there — page by page, at the usual `--speed`, with arrow-key page
browsing between jobs. Every job runs in a **fresh interpreter** at
the session's page size and dpi, so one job's redefinitions (or
crash) can't poison the next; job errors go to stderr and printing
continues, like a printer that just moves on to the next document.

The queue policy lives in `src/spool.rs` (`Watcher`), separated from
the window so it's testable headlessly (six in-module tests):
`.ps`/`.eps` only; files present at startup are *not* printed (the
printer coming online doesn't reprint the tray); a new file must
hold the same size+mtime across two consecutive polls before
queueing, so files still being copied in aren't printed
half-written; rewriting an already-printed file queues it again.
Same-poll arrivals print in name order.

Window integration rides the existing step-driven loop: idle control
flow becomes `WaitUntil(now + 400ms)` instead of `Wait` (re-armed
every pass — a stored past instant would spin), and `about_to_wait`
does the polling and job swap. End-to-end verified by scenario: a
pre-existing file is skipped, a dropped bad job reports its
undefined error to stderr, and two successive good jobs print in
order from separate interpreters.

Deferred: the "or listen on a port" variant (add when something
actually wants to netcat jobs in); a spool-specific test of the
window half (the watcher is tested, the window loop is exercised
manually — same standing as the rest of window.rs).

## Stage 10 — Gallery II opens: Hundred Lines (2026-07-17)

`gallery/hundred_lines.ps` — the first Gallery II piece, and the
showcase demo for the Stage 12 handwriting font: a chalkboard on
which the same sentence ("i will not cache my glyphs.") is written
nine times in /HandScript. Because every glyph is generated fresh by
BuildChar, no two lines — no two letters — render alike; the ninth
line is cut off mid-word by the bell, the chalk skidding away.

Two knobs the letter demo didn't pull, both exercised here: the
jitter amplitude `J` climbs line by line (12 → 40 — the writer gets
bored), and the pen width `W` (a small adaptation to the copied
font: `setlinewidth` reads `/W` instead of a literal) picks a
chalkier stroke. Both are overridden *from outside the font* by
writing into its scratch `work` dict, which stays writable after
definefont even in gs, where the font dict itself is locked — the
same scratch-dict trick Stage 12 pinned, used in the other
direction. Verified in both interpreters: gs runs the same file and
writes the board in its own hand (its `rand` stream differs, by
design). Still is 2× supersampled like the rest; piece is seeded
(`1985 srand`), so pscat's board is reproducible run to run.

Remaining Stage 10 items (spool mode, halftone screens) still open.

## Stage 12 — dynamic handwriting (2026-07-17)

`examples/handwriting.ps`: the /HandScript Type 3 font. Letterforms
are single-stroke polyline skeletons (lowercase, digits,
punctuation) on the 1000-unit grid; `drawstroke` jitters every point
through `rand` (±16 units) and smooths the run into quadratic-style
curves (interior points become control points, segments end at
midpoints — a pen slurring through corners). BuildChar adds
per-glyph slant with a rightward bias, baseline drift, and
pen-pressure width variation, stroking with round caps/joins.

The mechanics that make it work are all existing machinery: Type 3
glyphs are deliberately uncached (Stage 6 task 7), so every `show`
re-runs BuildChar and re-rolls the pen — repeated characters never
match; `rand` is a seeded LCG, so whole pages are reproducible run
to run (`tests/handwriting.rs` pins both properties). One
portability lesson pinned by running the file in gs: font dicts go
read-only at definefont there, so BuildChar's working defs live in
a scratch dict inside the font (invalidaccess otherwise; we don't
model access control, so our run never noticed).

## Stage 11 — performance parity investigation (2026-07-17)

**The harness** (`benches/vs_gs.rs`, `cargo bench --bench vs_gs`):
identical workloads through both binaries, wall time best-of-three
plus peak RSS via `/usr/bin/time -l`, startup row for netting out.

**Headline: pscat wins almost everywhere.** M-series numbers after
this stage's fix — pscat vs gs: startup 4ms/19ms, memory 3–5×
smaller on every workload (5.6–9.4MB vs 18–44MB), saveloop 3ms/23ms,
sierpinski 3ms/26ms, specimen 4ms/35ms, postcard 5ms/28ms, defloop
now *tied* 24ms/24ms. The two remaining gaps are fib 27 (124ms vs
54ms) and fern (247ms vs 149ms).

**What profiling found** (macOS `sample` on fib 30 / fern):
- A custom `Drop for Object` (the Stage-4 iterative-teardown guard)
  taxed ~18% of fib: every popped operand — integers included —
  paid a function call. **Fixed** by moving the impl to `PsArray`
  where it belongs: non-array values get the compiler's plain drop
  glue, arrays keep the last-handle iterative teardown. fib
  134→124ms, defloop 30→24ms (tie), fern 264→247ms. Bonus:
  `obj.value` is movable now — the old E0509 gotcha is gone.
- A **name-lookup cache was built, measured, and rejected** — twice
  (global write-generation, then per-name generations with dict-
  stack invalidation). fib gained only ~5% while def-heavy code
  (fern, defloop) paid more in bookkeeping than the two integer-hash
  probes it saved. The uncached walk is already near-optimal at a
  two-dict stack. Kept: nothing; the numbers said no.
- Both remaining gaps share one root: per-element machine-loop cost
  (step dispatch ~26%, dstack walk ~17%, per-element `PsArray::get`
  clone ~11%, drop glue ~9%). Fern is interpreter-bound, *not*
  raster-bound — the tiny-skia side is already fast, and fern's
  malloc traffic is its own `3 dict` per `sethsb` call.

**Deliberately not done** (the levers left, in order of expected
payoff, all representation changes): borrow-yield or bytecode-style
procedure bodies to kill the per-element clone; gs-style per-name
binding slots; NaN-boxed objects. For an interpreter whose real
workloads (rendering) already beat gs 5–7×, they're not worth the
architectural churn today.

## Stage 9 — output targets (2026-07-16)

**Multi-page** (`tests/pages.rs`): showpage snapshots the page, does
a full initgraphics (gs-pinned: CTM/color/width reset, font kept),
and erases *lazily* — the canvas keeps the finished page until the
next mark. That single idea resolves the Stage 2 "showpage doesn't
erase" deviation without defeating watching: single-page programs
keep their picture, multi-page programs erase exactly when page N+1
starts. All paint entry points funnel through `Gfx::prepare_paint`.
copypage/initgraphics ops added; `--png` numbers multi-page output;
the window browses pages with arrow keys.

**`--dpi`** — one scale factor in the base CTM
(`Interp::with_page_scaled`); verified pixel-identical to gs -r144.

**SVG export** (`--svg`, `src/svg.rs`): a recorder mirroring the
paint pipeline at six seams in Gfx — device-space paths serialize
1:1, glyphs arrive as outlines, clips became a *chain* (ClipState
now records path links, not just the newest) rendered as nested
clipPath groups, images embed as base64 PNG with the rasterizer's
exact transform. One document per page, lazy erase mirrored.
Verified by rasterizing postcard.ps's SVG in a browser.

**PDF export** (`--pdf`, `src/pdf.rs` — the design note lives in its
module doc): same recorder pattern, PDF syntax; the design decision
(mirror vs re-execution vs retained display list) is recorded there.
Content streams reuse our y-down device coordinates behind one flip
`cm`; each element carries its own clip chain in `q…Q`; images are
Flate-compressed RGB XObjects; imagemasks are 1-bit `/ImageMask`
stencils painted in the current color — PostScript's own semantics.
The strong test: **gs rasterizes our PDFs** and they block-match our
canvas (postcard, specimen).

If a third export target appears, promote the recorder seams into a
neutral display list and make SVG/PDF serializers of it.

## Stage 8 wrap — corpus round 2, Level 2 odds and ends (2026-07-16)

**Found-file corpus round 2** (`tests/corpus.rs`): the Ghostscript
classics — tiger, golfer, escher, colorcir, doretree render
block-identical to gs; snowflak and vasarely run clean but are
rand-driven art (implementation-specific sequences), asserted to run
rather than match. Files download on first run (gitignored;
AGPL-distributed, so fetched rather than vendored) and the test
skips offline. Two comparison lessons pinned: give gs
`-dGraphicsAlphaBits=4` or AA policy swamps the signal on edge-dense
art, and colors that "look wrong" at first glance are usually that
same AA thinning, not color math (sethsbcolor matched gs exactly).
What the corpus actually surfaced: `setflat`/`currentflat` (stored
hint; tiny-skia flattens itself) and `min`/`max`/`.min`/`.max` (gs
conveniences). waterfal.ps stays out — it reads gs's QUIET
command-line define.

**Task 5** (`ops/level2.rs`): packedarray (plain arrays; packing flag
tracked but inert), usertime/realtime, languagelevel = 2, and
resource basics — defineresource/findresource/resourcestatus/
undefineresource, Font category delegating to findfont/definefont,
other categories in a per-interp registry, writes journaled so
restore rolls them back. Status/size in resourcestatus are 0 0
fictions, like vmstatus's byte counts.

**Task 6, half of it**: `--pstack-on-error` prints the operand stack
(gs post-mortem style) after errors in both REPL and headless runs.
The `--interactive` flag (window + REPL together) remains; design
note: a stdin-reader thread ships raw lines through
`winit::EventLoopProxy::send_event` into the existing event loop,
which runs each line via `run_str` between frame budgets — the
interpreter stays single-threaded and owned by the loop; the reader
thread never touches it. Deferred until someone wants it enough to
test the windowed path properly.

## Stage 8 task 4 — Indexed and Separation color spaces (2026-07-16)

`ops/color.rs` + a real `ColorSpace` enum in the graphics state
(replacing the bare component count; part of the gsave snapshot, so
spaces roll back). `setcolor` dispatches on the current space.
Indexed: string lookup tables, index-0 initial color, out-of-range
indices *clamp* (gs renders where the PLRM says rangecheck — pinned);
indexed images map samples straight to palette entries with the
[0, 2^bits−1] default Decode. Separation: the tint transform is a
real PostScript procedure run through the machine via a new generic
continuation frame (`Frame::PostOp` — the reusable "operator needs a
proc's result" pattern the HANDOFF said to copy from ShowCtx; holds
no external state, so unwinding is free; `exit` can't cross it).

Gaps (documented in `ops/color.rs`): Indexed lookup procedures and
nested bases (typecheck); CIE spaces (undefined); Separation *images*
render as 1−tint gray, an approximation noted in `image.rs`.

## Stage 8 task 2 — name interning (2026-07-16)

`src/name.rs`: a `PsName` is an interned id plus the shared text
(`Deref<Target=str>` kept the churn tiny — 25 `Value::Name` sites,
most untouched). The interner is thread-local (the interpreter is
`!Send` by design; several Interps on one thread share ids
harmlessly; the table only grows, matching PS name semantics). Dicts
key name entries by `u32` through a one-multiply hasher; name
*execution* carries only the id (`load_id`), so the hot path does no
string hashing and no Rc traffic — `last_name` is now a bare id
resolved to text only at error time. `Dict::get(&str)` probes the
interner read-only so misses don't grow the table.

Measured (`cargo bench`, M-series): fib 27 214ms→137ms, defloop
33→25ms, fern 375→258ms. gs remains ~4.5× ahead on fib; profiling
says the rest is the frame loop (per-element RefCell borrow +
Object clone), noted in `benches/perf.rs` as the next lever and
deliberately left until something feels slow.

## Stage 8 tasks 1 + 3 — save/restore and the perf yardstick (2026-07-16)

**save/restore** (`VM.md` is the design doc — read it first; every
choice is pinned against gs there). Object-granularity copy-on-write
journaling: while a save is live, the first mutation of an array's or
dict's backing store at the current save level snapshots the whole
store into an undo journal; restore replays it backwards. Zero
overhead with no save live. The long-feared object-model reach was
one new `Value::Save(Rc<SaveHandle>)` variant (savetype) plus `Clone`
on `Dict`. Graphics rollback reuses the Type 3 glyph-snapshot
mechanism (boundary state lives in the save record, not on the gsave
stack). `vmstatus` (level real, byte counts fixed fictions) and
`grestoreall` came along.

Key facts pinned before design (they shaped it):
- **Strings are exempt from restore** (PLRM §3.7.3.2, confirmed in
  gs) — no string write path needs a journal barrier.
- restore = grestoreall to the save point (unbalanced gsaves inside
  the context are discarded).
- Write barriers audited into: put/putinterval/copy/astore (arrays),
  put/def/store/undef/copy (dicts), bind, the matrix-filling
  operators, definefont (FID + FontDirectory), findfont's
  substitution install, and the interpreter's own `$error` writes.

Deviations (documented in `VM.md` and tested as deviations):
- No invalidrestore scan of the stacks for post-save composites (gs
  errors; under `Rc` the objects stay valid — render, don't error).
- restore doesn't close files opened since the save.
- `grestore` can pop through a save boundary (balanced programs never
  notice); `grestoreall` respects it.

**Perf yardstick** (`benches/perf.rs`, `cargo bench`): fib 27 /
defloop / sierpinski / fern, best-of-three. Baseline: fib 214ms vs
gs ~30ms net of startup — the interning gap is ~7×, worse than the
~3× previously recorded; Stage 8 task 2 has measurable headroom.
save/restore added no regression (209ms after).

## Stage 7 truly complete — DCTDecode (2026-07-16)

**DCTDecode** (`Decoder::Dct`, zune-jpeg 0.5). The one decoder that
buffers: JPEG needs whole-stream context (Huffman tables, progressive
scans), so the first pull reads one complete image and decodes it all
at once. The buffering is *marker-aware* (`file.rs::buffer_jpeg`):
segment lengths are honored (an EXIF thumbnail's embedded EOI is
skipped, not mistaken for the end), entropy data is scanned respecting
FF00 stuffing and RST0–7, and reading stops exactly at the EOI — so
the shared-cursor exactness contract survives even though the decode
itself isn't streaming (verified by test: bytes after EOI stay
unread). Output matches PostScript sample expectations: grayscale
JPEGs yield one byte per sample, CMYK/YCCK four, everything else
(the usual YCbCr) RGB triples.

Tradeoffs / deviations:
- CCITTFaxDecode is now the only absent decode filter.
- CMYK JPEG output is zune's CMYK — Adobe's inverted-CMYK convention
  is untested (no real file in the corpus yet).
- Fixtures `tests/data/{gray8,red4}.jpg` were generated by gs
  (jpeggray/jpeg devices, APP segments stripped); decoded values
  pinned against gs rendering the same snippets (127-gray, 254-red).

## Stage 6 complete + Stage 7 (2026-07-13)

**Stage 6 task 6 — Type 1 fonts.** `src/type1.rs` is a hint-ignoring
charstring interpreter (numbers, moves/lines/curves, hsbw/sbw,
callsubr/return, div, seac with FreeType's sidebearing reading, flex
via OtherSubrs 0–2 as literal cubics, OtherSubr 3 threaded through
pop/callsubr). Glyph dispatch in `ShowCtx::step`: registry fid ≥ 0 →
bundled TTF; `BuildGlyph`/`BuildChar` in the dict → Type 3 frame;
`CharStrings` → Type 1 (decrypted per Private/lenIV with Subrs),
rendered synchronously through the same FontMatrix∘CTM transform as
every glyph source. `tests/type1.rs` *generates a complete canonical
PFA in memory* (eexec + charstring encryption, `N RD <binary>`
delivery, Private/CharStrings/definefont sequence, zeros,
cleartomark) — and Ghostscript accepts the identical bytes, agreeing
on metrics. Supporting work that made it possible:
- **File objects** (Stage 7 task 1, `src/file.rs` + `Value::File`):
  one shared read cursor between the scanner and data reads. The
  Lexer now pulls bytes through a `FileHandle`.
- **Filters** (task 2): files layered on files, decoding *on demand*
  so a filter consumes exactly the source bytes its consumer needed —
  the property `currentfile eexec` + `closefile` + cleartomark
  depends on. ASCIIHex/ASCII85/RunLength + the eexec cipher
  (hex/binary sniffed), later Flate (flate2, byte-fed) and LZW
  (hand-rolled, EarlyChange=1).
- **Scanner delimiter fix**: a token terminated by whitespace consumes
  that one character (CRLF as a unit), per PLRM and pinned against
  gs — without it `N RD <binary>` reads the wrong byte. `token`
  remainders changed accordingly (stage5 expectations updated to
  match gs).
- **`systemdict`/`userdict` by name** — simply missing until now;
  eexec pushes systemdict for its duration (fonts assume it).
- Access ops `executeonly readonly noaccess rcheck wcheck` as
  identities (every real font executes them).

**Stage 6 task 7 — glyph cache, measured.** Faces are now parsed once
per process (OnceLock over the `'static` bundled data). Measured
(release, M-series, 110k glyphs): show 0.35s→0.33s (rasterization
dominates), stringwidth 0.04s→0.02s. At ~330k painted glyphs/sec a
bitmap cache is not worth its complexity yet — same philosophy as
name interning: revisit when something feels slow. `setcachedevice`'s
bbox remains unused until then.

**Stage 7 tasks 1–3** shipped as described above (file objects,
filter framework, Flate/LZW). DCTDecode/CCITTFax deliberately absent
(`filter` reports them undefined).

**Stage 7 task 4 — images.** `src/image.rs` + `ops/image.rs`:
`image` (Level 1 five-operand and Level 2 dict forms), `imagemask`
(polarity pinned against gs: true ≡ Decode [1 0] ≡ paint the 1s),
`colorimage` (interleaved single source; 1/3/4 components).
`Frame::Image` accumulates data like the show frame: procedure
sources run as frames above and hand back strings (empty string =
EOD; premature EOD renders what arrived, missing samples read 0);
file/string sources drain synchronously. Rasterization inverse-maps
each device pixel through CTM⁻¹ then ImageMatrix (nearest neighbor),
blending through the clip mask. Minimal `setcolorspace`/
`currentcolorspace` for the three device spaces (the dict form reads
the current space's component count; color operators set it
implicitly per the PLRM).

**Tradeoffs / deviations (this whole block):**
- `file` op is read-only (`(r)`); write modes are invalidfileaccess.
- Filter `encode` variants don't exist; parameter dicts are accepted
  and ignored.
- `colorimage` with MultipleDataSources → limitcheck (documented gap).
- Images are nearest-neighbor, no interpolation (`/Interpolate`
  ignored); 12-bit samples unsupported (rangecheck).
- Type 1: hints ignored entirely; no `/Metrics` override; CID/CFF
  (Type 2 charstrings) out of scope.
- eexec's systemdict push is popped at source exhaustion but *not*
  restored on error unwinds (program is aborting anyway).

**Closing state:** `examples/postcard.ps` (inline hex/Flate images,
colorimage, imagemask sprites) joined the golden suite — the corpus
proof for the image pipeline. Bitmap fonts (Type 3 BuildChar using
imagemask) are tested working. **Everything else about continuing
this project — orientation, gotchas, and the recommended next-work
order (DCTDecode, found-file corpus round 2, then Stage 8's
save/restore) — lives in `HANDOFF.md`.**

## Stage 6, task 5 — Type 3 fonts and kshow (2026-07-13)

**Built** (the stage's [opus] exec-stack piece; design in `FONTS.md`
Decision 6, rewritten to match):
- **`Frame::Show`**: the entire show family (`show ashow widthshow
  awidthshow kshow stringwidth charpath`) now queues a frame and lets
  the machine process one glyph per step. Frame ordering is program
  ordering, so `stringwidth` pushing its result at frame-pop, nested
  shows inside BuildChar, and glyph-by-glyph live-window rendering all
  fall out for free. Outline glyphs still paint synchronously within
  their step.
- **Type 3 glyph contexts**: `BuildGlyph` (preferred, receives the
  encoded name) or `BuildChar` (receives the code) runs inside a
  sealed context — graphics-state snapshot + gsave watermark, CTM set
  to glyph space at the pen, fresh path — restored regardless of what
  the procedure does (unbalanced gsave included). `setcachedevice`,
  `setcachedevice2`, and `setcharwidth` record the width, which
  advances the pen through FontMatrix∘CTM. Unwinding (`stop`, uncaught
  errors, program abort) seals open contexts; `exit` across a show is
  `invalidexit`.
- **Metrics execute glyph procedures**: `stringwidth`/`charpath` on
  Type 3 run BuildChar with painting suppressed (a nestable counter on
  `Gfx`) — per the PLRM, that's where the width comes from.
- **`kshow`**: proc runs between character pairs with both codes
  pushed; the pen re-reads the current point after, so procs can kern
  or even swap fonts mid-string.
- **Demos**: `examples/type3_demo.ps` (deterministic, in the golden
  suite: bit-pattern CellFont, parametric GearFont, BuildGlyph
  SprigFont dingbats) and `examples/type3_ransom.ps` (dynamic
  generation: every glyph invented at show time — random face, size,
  tilt, paper scrap — via nested show inside BuildChar; deterministic
  per run, not gs-comparable since rand differs).
- **Tests**: 15 in `tests/type3.rs` — advances and kshow displacement
  pinned against Ghostscript, context sealing, catchable BuildChar
  errors, nesting, suppressed stringwidth, charpath-on-Type-3.

**Tradeoffs / deviations:** `charpath` on Type 3 advances without
capturing outlines; `setcachedevice`'s bbox is recorded nowhere (no
glyph cache yet — task 7); color restrictions on setcachedevice glyphs
are not enforced (lenient superset). `cshow` still absent (trivial
now; with Type 0 work if ever).

**Next:** Stage 6 task 6 (Type 1 parsing) and task 7 (glyph cache).

## Stage 6, tasks 1–4 — text and base fonts (2026-07-13)

**Built** (per `ROADMAP.md` Stage 6; tasks 5–7 remain open):
- **Architecture writeup** (`FONTS.md`): font dicts are ordinary Dicts;
  `FID` (an integer index — documented type deviation) is the seam to a
  Rust-side registry of 12 bundled Liberation faces (OFL, in `fonts/`,
  `include_bytes!`'d) covering the base-13 names minus Symbol.
  Process note: the task was tagged [opus+review]; the review pass was
  done in-session (this session ran on a stronger-than-opus model —
  no stronger reviewer available) and caught one real spec error
  (scalefont/makefont must *shallow*-copy the dict, sharing the
  Encoding array).
- **Plumbing**: `findfont` (lazy materialization into a real
  `FontDirectory`; unknown names substitute the default face,
  gs-style, instead of `invalidfont` — documented), `scalefont`/
  `makefont` (shallow copy + f64 matrix composition), `setfont`
  (validates, caches FontMatrix in the graphics state — gsave/grestore
  snapshot the font for free), `currentfont`, `definefont` (FID
  assignment; Type 3 dicts accepted but unrenderable until task 5),
  `selectfont`.
- **Show engine** (`src/font.rs`): byte → live Encoding lookup →
  glyph name → glyph id (font's `post` table, then Adobe-name→Unicode→
  `cmap` fallback) → outline through glyph→FontMatrix→CTM into a
  device-space path (quads elevated to cubics), filled through the
  normal paint+clip path without touching the current path; pen
  advances by the transformed advance vector. `show ashow widthshow
  awidthshow stringwidth charpath` all drive it; rotation/shear falls
  out of the matrix pipeline.
- **Encodings** (`src/encodings.rs`): Appendix E StandardEncoding and
  ISOLatin1Encoding, installed in systemdict and (fresh copy) in each
  built-in font dict; re-encoding idiom golden-tested.
- **Tests**: 21 in `tests/fonts.rs` (plumbing, metrics vs the 600/em
  Courier expectation, ink/clip/rotation pixels, live re-encoding);
  encoding-table unit tests; `examples/specimen.ps` in the golden
  suite (text compares with wider ink bands — Liberation vs URW
  shapes differ at glyph-edge granularity; metrics agree).

**Tradeoffs / deviations:** `FID` is integertype, not fonttype; Symbol
substitutes Helvetica; unknown `findfont` substitutes rather than
erroring; `charpath`'s bool operand is ignored (one outline form);
faces are re-parsed per show call (glyph-cache task will measure);
`show` is synchronous — Type 3/`kshow` need the ShowFrame exec-stack
design sketched in `FONTS.md` Decision 6.

The gallery gained its typography piece (`ring_of_type.ps` — the
stage-goal payoff: text on eleven shrinking rings around a charpath
ampersand).

**Next:** Stage 6 task 5 (Type 3 / BuildChar, the [opus] exec-stack
piece, plus `kshow`), task 6 (Type 1 parsing), task 7 (glyph cache).

## Stage 5 — Run found PostScript (2026-07-12)

**Built** (per `ROADMAP.md` Stage 5, all twelve items):
- **Views**: arrays and strings are now `PsArray`/`PsString` — shared
  backing plus offset/length — so `getinterval`/`cvs`/`search` results
  alias their source per the PLRM. The iterative-drop protection moved
  to the backing store.
- **General dict keys**: names/strings on the original fast path;
  integers (reals with integer value unify, per spec), booleans, marks,
  and composites-by-identity in a second map preserving original key
  objects for `forall`.
- **Data ops**: `length get put getinterval putinterval copy`
  (polymorphic incl. the stack form) `forall` (new frame type, all
  three container kinds) `array string aload astore`.
- **Dict ops**: `<< >> known where store undef currentdict
  countdictstack cleardictstack maxlength`.
- **Conversions**: `cvi cvr cvn cvs cvrs cvx cvlit type xcheck`
  (string→number uses the scanner's full syntax, radix included).
- **Error recovery**: `stop`/`stopped` as an exec-stack boundary frame;
  errors inside a stopped context record errorname/command into
  `$error` and resume with `true`; `handleerror` reports and resets.
  `exit` across a stopped boundary is `invalidexit` per the PLRM.
- **Strings as programs**: executable strings run as source
  (`(...) cvx exec`); `search anchorsearch token` (token reuses the
  real scanner, procedures included).
- **Graphics**: `clip eoclip initclip clippath pathbbox` (alpha-mask
  intersection; clip is part of the gsave snapshot and leaves the
  path), `setdash currentdash`, `sethsbcolor currenthsbcolor
  setcmykcolor currentrgbcolor currentgray currentlinewidth`.
- **Matrices**: full operand set (`matrix identmatrix defaultmatrix
  currentmatrix setmatrix initmatrix concat concatmatrix invertmatrix
  transform/itransform/dtransform/idtransform`) plus matrix-form
  dispatch for `translate/scale/rotate`.
- **Scanner**: `//immediate` names (resolved at scan time, value not
  executed), ASCII85 `<~...~>` literals.
- **Misc**: `rand srand rrand` (deterministic LCG), `bitshift`.
- **Corpus**: `examples/testcard.ps` — a found-style file (shortcut
  prolog, where-probe, forall charts, cvs geometry, eoclip ring, matrix
  bookkeeping) rendering identically in pscat and Ghostscript; in the
  golden suite. 90 tests total.

**Tradeoffs / deviations:** custom procedures installed in `errordict`
are not consulted (the `stopped`+`$error` path covers found-file usage);
the operand stack is *not* restored to pre-operator state when an error
is caught (PLRM handlers see pre-error operands — revisit with the
errordict work); `maxlength` reports current length (Level 2 dicts grow
anyway); `clippath` returns the most recent clip path rather than the
true intersection outline (the mask *is* the true intersection).

**Next:** Stage 6 — text and fonts, starting with the font architecture
writeup (`ROADMAP.md` Stage 6 item 1, [opus+review]).

## Stage 4 — Robustness and polish (2026-07-11)

**Audit result:** one real crash found and fixed. Dropping a deeply
nested array (10k levels of `[`) overflowed the Rust stack — `Rc` trees
drop recursively. Fixed with an iterative teardown in `Drop for Object`
(worklist reuses the array's own element vector; only last-owner array
drops pay anything). Also: `repr` is now depth-capped (deep nesting
prints `...` instead of recursing), `--page` is capped at 8000² so a typo
can't attempt a multi-GB allocation, and the REPL reads multi-line input
(brace/string-aware continuation prompt) so procedures can actually be
typed interactively.

**New tests** (61 total, 9 suites):
- `tests/robustness.rs` — deterministic fuzz (random bytes + random
  token soup over the whole operator set, step-budgeted so `loop` bombs
  terminate), pathological nesting, degenerate graphics (singular CTM,
  1e300 coordinates, negative radius), error-then-recover.
- `tests/golden.rs` — renders all four examples with pscat *and*
  Ghostscript, compares 10×10-block downsampled images: any block inked
  in one render and blank in the other fails (geometry error), mean
  block difference must stay < 6 (observed ≤ 2.1). Skips with a notice
  if gs isn't installed.

**Performance** (release, M-series): fib(27) ≈ 830k procedure calls in
~0.20s; 5M-iteration `for`+`add` in ~0.20s (~50M executed objects/sec);
depth-8 Sierpinski (6,561 fills) renders in 57ms. Ghostscript does
fib(27) ~3× faster — its names are interned, ours hash strings per
lookup. That's the known next lever, still not worth pulling until
something actually feels slow.

**Implemented today:** scanner (radix/reals/strings/hex/procs, bytes not
UTF-8), three-stack machine (steppable, tail-call-flat, depth-limited),
stack/arith/math ops, control flow (`if`/`ifelse`/`for`/`repeat`/`loop`/
`exit`/`exec`), `def`/`dict`/`begin`/`end`/`load`/`bind`, comparisons/
boolean/bitwise, paths (`moveto`…`arc`/`arcn`), painting (`fill`/
`eofill`/`stroke`), graphics state (`gsave`/`grestore`, gray/RGB color,
line attributes), transforms (`translate`/`rotate`/`scale`), live
window + headless PNG, PLRM error names throughout, LaserWriter-style
error reports.

**Not implemented (main gaps):** array/string element ops (`get`/`put`/
`getinterval`/`forall`/`array`/`string`/`aload`/`astore`), `cvi`/`cvr`/
`cvs`/`cvx`/`cvn`, `save`/`restore`, `stopped`/`stop`/errordict,
`clip`, `setdash`, matrix operands (`concat`/`setmatrix`/`transform`),
`sethsbcolor`/CMYK, text (`show`/fonts), images, `//name`, ASCII85,
`<<`/`>>` dict literals, multi-page semantics.

**Stage 5 recommendation — "run found PostScript":** the single most
valuable next chunk is data-structure completeness plus error recovery:
`get`/`put`/`forall`/`getinterval`/`array`/`string`/`aload`, the `cv*`
conversions, `<<`/`>>`, `where`/`known`, `stopped`/`stop`, `clip`, and
matrix operands. That's what stands between pscat and real-world `.ps`
files from the internet (they lean on arrays/strings/`forall`
constantly, and on `stopped` for prolog robustness). Fonts/`show` is the
bigger prize emotionally but is a stage of its own (font formats,
encoding vectors, glyph caching) and most found art still needs the data
ops first. Suggested order: Stage 5 = data + error recovery + clip;
Stage 6 = text with a bundled font (`show`, `stringwidth`, `charpath`);
Stage 7 = images and filters, which opens EPS previews and scanned art.

## Stage 3 — Control flow and procedures (2026-07-11)

**Built:** `def`, `dict`/`begin`/`end`/`load`, `if`/`ifelse`/`exec`,
`repeat`/`loop`/`for`/`exit`, comparisons (`eq`/`ne`/`lt`/`le`/`gt`/`ge`
with PLRM semantics: cross-type numeric equality, string content
comparison, name≡string, composite identity), boolean/bitwise
(`and`/`or`/`xor`/`not`), and `bind` (recursive, redefinition-proof —
tested). Loop operators are new **frame types on the execution stack**,
not host recursion, so they inherit stepping (live rendering), the depth
limit, and `exit` is a frame unwind that stops at source boundaries
(`invalidexit`). All three fractal examples now run and render. 15 new
control tests (recursion, 200k-deep tail recursion in constant space,
runaway recursion → `execstackoverflow` not a crash) + 3 fractal
integration tests with pixel assertions. 55 tests total.

**Found & fixed — example-file bugs, verified against Ghostscript:**
`koch_snowflake.ps` converted degrees to radians before calling
`sin`/`cos` (PostScript trig is degrees) which flattened the curve, and
its triangle headings/start point didn't close; `golden_spiral.ps` passed
a stray extra `4` in its recursive calls (shifting the whole operand
protocol) and its arc geometry didn't chain. In both cases **pscat and
Ghostscript rendered the broken files identically** (good news for
interpreter fidelity), and both render the fixed files identically —
a proper snowflake and a connected golden spiral.

**Tradeoffs:** dict `capacity` ignored (Level 2 dicts grow anyway); dict
keys still name-text only; `for` control values are f64 internally with
integer presentation when all operands were integers.

**Deferred:** `forall`, `put`/`get`/array ops, `where`/`known`/`store`,
`stopped`/`stop`, `cvx`/`cvi`/`cvr`/`cvs`, `//name` immediate names,
REPL multi-line procedure input.

**Next:** Stage 4 — robustness/error-handling audit, golden-image tests
against Ghostscript, performance pass, and the what's-next writeup.

## Stage 2 — Graphics core, live window (2026-07-11)

**Built:** `src/gfx.rs` (graphics state, device-space paths, arc→Bézier
flattening, tiny-skia fill/stroke, gsave stack); the full Stage 2 operator
set (`moveto`/`lineto`/`curveto`/`arc`/`arcn` + relatives, `fill`/
`eofill`/`stroke`, `gsave`/`grestore`, colors, line attributes,
`translate`/`rotate`/`scale`, `currentpoint`, `showpage`, `erasepage`);
a public `begin_source`/`step_n` stepping API on the interpreter; a live
winit+softbuffer window that steps the machine each frame (`--speed`
knob); headless `--png` output; `--page WxH`. 13 pixel-level render tests
plus the Stage 1 suite (40 total); demo at `examples/stage2_demo.ps`,
verified by rendering to PNG and inspecting.

**Tradeoffs:** stroke width scaled by √|det CTM| instead of a true
user-space pen (exact under uniform scale/rotation, wrong for
anisotropic `scale`); `showpage` leaves the image up instead of erasing;
window blit is nearest-neighbor on HiDPI. All noted in `ARCHITECTURE.md`.

**Deferred:** `clip`/`clippath`, `setdash`, `sethsbcolor`,
matrix-operand forms (`concat`, `setmatrix`, `transform`), multi-page
semantics, REPL-attached window.

**Next:** Stage 3 — `def`, `if`/`ifelse`, `for`/`repeat`/`loop`,
`bind`, dict operators (`begin`/`end`, `dict`), comparisons — the set
the three fractal examples actually need.

## Stage 1 — Foundation (2026-07-11)

**Built:** Cargo project (`pscat`, lib + bin); byte-oriented lexer with
full PostScript token syntax (radix numbers, nested/escaped strings, hex
strings, the "failed numbers are names" rule) and in-module unit tests;
object model (`Object` = `Value` + executable flag, `Rc<RefCell>`
composites); an exec-stack machine with operand/dict/exec stacks, tail-call
frame popping, and depth limits; stack-manipulation and arithmetic/math
operators per the PLRM (promotion on overflow, degrees for trig,
ties-round-to-greater, `mod` sign rules); `=`/`==`/`stack`/`pstack`/
`print`/`quit`; a CLI with file, `-e/--eval`, and REPL modes;
LaserWriter-style error reports. 27 tests; clippy clean.

**Tradeoffs:** `i64` integers instead of the PLRM's 32-bit (documented in
`ARCHITECTURE.md`); names as `Rc<str>` with interning deferred until a
benchmark justifies it; dict keys are name-text only until arbitrary-key
dict operators exist.

**Deferred:** `def` and all control flow (Stage 3 — though the dict stack
and procedure-call machinery already work, tested via the embedding API);
`//name`, ASCII85, `<<>>` construction; `save`/`restore` (flagged as the
one feature that could reach back into the object model); REPL multi-line
input.

**Next:** paused for the architecture-writeup checkpoint per `INIT.md`.
Stage 2 (graphics + live window) starts on approval; crate leanings are in
`ARCHITECTURE.md`.
