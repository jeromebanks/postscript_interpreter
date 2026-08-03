# NOTES.md — stage summaries

Newest first. Per `AGENTS.md`, each stage ends with a summary here: what
was built, tradeoffs made, what's explicitly deferred.

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

Verified against `gs` throughout, not just at the end: both the
Ghostscript acceptance driver (`tests/artkit.rs`) and every render done
while building this (`examples/hyperbolic.ps`, `gallery/
infinite_descent.ps`) were run under both `pscat` and `gs` and compared
by eye. Structurally identical in both; `gs`'s arcs facet more coarsely
near the rim on the gallery piece, the same documented `flattenpath`
tolerance difference `HANDOFF.md` already records (fixed quarter-pixel
device-space tolerance vs. `gs`'s `setflat`-driven one — chord counts
differ, shapes agree).

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
(scale-invariant collinearity) both pass unchanged, a new
`horthocircle_does_not_special_case_points_merely_near_the_origin` pins
the round-five near-origin counterexample, and
`httile_survives_catastrophic_cancellation_at_high_p_q` pins the
`{10,10}` depth-4 tile count — now 8,191, not the 7,612 the round-four
band-aid produced (that number reflected the band-aid's own
approximation error, not real geometry) and not the 8,201 round five's
review estimated by hand from the tiling's plain reflection-tree growth
without accounting for what the BFS's dedup step exists to do at all.
8,191 is what the numerically stable solve actually computes for this
case, cross-checked against the existing regular tests
(`{5..8, 3..4}` at depths 3-5, all unchanged) and the fact that the
`{7,3}` depth-4 gallery piece's render is byte-identical before and
after this rewrite — the reformulation only changes behavior in the
extreme regime that exposed it. `{10,10}` depth 4 also stays well under
`htmax` (20,000), so this isn't a silent-truncation artifact either. The
`gs`-compat driver in `tests/artkit.rs` runs a `{10,10}` depth-4
`httile` directly — the exact configuration that produced the original
crash — so this is covered under `gs` itself, not just `pscat`.

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
