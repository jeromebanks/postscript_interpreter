//! Pressure-sensitive ribbon strokes (issue #41, `lib/paintkit.ps`):
//! `pkribbon` treats the current path as a centerline and fills a
//! variable-width band along it, built on artkit's `walkpath`
//! (issue #40). Ink placement is deterministic under the caller's own
//! `N srand` -- jitter is the only randomness involved -- so
//! determinism itself (same seed, same pixels) is asserted on
//! directly, alongside geometry (the three centerline shapes, the
//! three pressure profiles, cap styles) and the documented fallbacks
//! (a degenerate point, an empty path).
//!
//! A two-walkpath-pass bug (a proc that leaves an array on the
//! operand stack instead of consuming it corrupts `wkend`'s own
//! stack assumptions across a subpath boundary -- see the comment
//! above the collection loop in `lib/paintkit.ps`) only ever showed
//! up with more than one subpath in the source path, so
//! `multiple_subpaths_each_become_their_own_ribbon` below is a
//! regression test for that, not just a feature check.
//!
//! `pknib` (issue #42) tests start below the `pkribbon`-specific ones:
//! its own validation guards (single-open-subpath, Width/Pitch/Angle/
//! MinWidth/Pressure), the nib-angle response actually changing
//! measured width, composition with /Pressure and the tapers, seeded
//! jitter's determinism, and corners/a direction reversal rendering
//! without crashing.

use pscat::{Interp, PsError};

fn load(it: &mut Interp) {
    for path in ["lib/artkit.ps", "lib/paintkit.ps"] {
        let src = std::fs::read(path).unwrap_or_else(|e| panic!("{path}: {e}"));
        it.run_source(&src)
            .unwrap_or_else(|e| panic!("{path} failed to load: {}", it.error_report(&e)));
    }
}

fn fresh(w: u32, h: u32) -> Interp {
    let mut it = Interp::with_page(w, h).expect("page");
    load(&mut it);
    it.run_str("1 1 1 setrgbcolor clippath fill")
        .expect("white background");
    it
}

// Luminance, not a per-channel threshold -- same reasoning as
// tests/pagekit.rs's ink_count (a light-but-not-white color could dip
// a single channel low without reading as "ink" to a human).
fn luma(p: tiny_skia::PremultipliedColorU8) -> f32 {
    0.3 * p.red() as f32 + 0.59 * p.green() as f32 + 0.11 * p.blue() as f32
}

fn ink_count(it: &Interp) -> usize {
    it.gfx()
        .pixmap
        .pixels()
        .iter()
        .filter(|&&p| luma(p) < 180.0)
        .count()
}

fn column_height(it: &Interp, x: u32, h: u32) -> u32 {
    (0..h)
        .filter(|&y| {
            let p = it.gfx().pixmap.pixel(x, y).expect("in bounds");
            luma(p) < 180.0
        })
        .count() as u32
}

#[test]
fn loads_clean() {
    let it = fresh(50, 50);
    assert_eq!(ink_count(&it), 0, "paintkit drew on load");
}

#[test]
fn width_and_pitch_guards_reject_non_positive_values() {
    let mut it = Interp::new();
    load(&mut it);
    let cases = [
        (
            "newpath 0 0 moveto 100 0 lineto << /Width 0 >> pkribbon",
            "pkribbon-width-must-be-positive",
        ),
        (
            "newpath 0 0 moveto 100 0 lineto << /Width -3 >> pkribbon",
            "pkribbon-width-must-be-positive",
        ),
        (
            "newpath 0 0 moveto 100 0 lineto << /Width 10 /Pitch 0 >> pkribbon",
            "pkribbon-pitch-must-be-positive",
        ),
        (
            "newpath 0 0 moveto 100 0 lineto << /Width 10 /Pitch -1 >> pkribbon",
            "pkribbon-pitch-must-be-positive",
        ),
        // Regression tests for a Codex-round-2 finding: a non-procedure
        // /Pressure used to be silently accepted (never auto-executed,
        // just pushed) and corrupt every downstream computation instead
        // of raising a clean error.
        (
            "newpath 0 0 moveto 100 0 lineto << /Width 10 /Pressure 1 >> pkribbon",
            "pkribbon-pressure-must-be-a-procedure",
        ),
        (
            "newpath 0 0 moveto 100 0 lineto << /Width 10 /Pressure (nope) >> pkribbon",
            "pkribbon-pressure-must-be-a-procedure",
        ),
        // Regression test for a Codex-round-3 finding: xcheck alone
        // isn't sufficient -- an executable non-procedure (`2 cvx`)
        // passes xcheck but still isn't callable the way pkhalfwat
        // needs, corrupting the same way an unvalidated number did.
        (
            "newpath 0 0 moveto 100 0 lineto << /Width 10 /Pressure 2 cvx >> pkribbon",
            "pkribbon-pressure-must-be-a-procedure",
        ),
        // Regression tests for a Codex-round-2 finding: any /StartCap//
        // EndCap value other than the three documented ones used to
        // fall through to /flat silently instead of erroring.
        (
            "newpath 0 0 moveto 100 0 lineto << /Width 10 /StartCap /roudn >> pkribbon",
            "pkribbon-startcap-must-be-round-flat-or-pointed",
        ),
        (
            "newpath 0 0 moveto 100 0 lineto << /Width 10 /EndCap /squared >> pkribbon",
            "pkribbon-endcap-must-be-round-flat-or-pointed",
        ),
        // Regression tests for a Codex-round-5 finding: /StartTaper//
        // EndTaper outside the documented 0..1 range didn't error --
        // negative silently disabled the ramp, above 1 kept the whole
        // stroke short of full width.
        (
            "newpath 0 0 moveto 100 0 lineto << /Width 10 /StartTaper -0.1 >> pkribbon",
            "pkribbon-starttaper-must-be-0-to-1",
        ),
        (
            "newpath 0 0 moveto 100 0 lineto << /Width 10 /StartTaper 1.5 >> pkribbon",
            "pkribbon-starttaper-must-be-0-to-1",
        ),
        (
            "newpath 0 0 moveto 100 0 lineto << /Width 10 /EndTaper -0.1 >> pkribbon",
            "pkribbon-endtaper-must-be-0-to-1",
        ),
        (
            "newpath 0 0 moveto 100 0 lineto << /Width 10 /EndTaper 1.5 >> pkribbon",
            "pkribbon-endtaper-must-be-0-to-1",
        ),
        // Regression tests for a Codex-round-7 finding: binding a
        // value option straight to its own name (pkgetdef's normal
        // result) makes every later bare reference to that name
        // auto-execute it if the supplied value happens to be an
        // executable array -- e.g. a zero-push /Width { } silently
        // corrupts the stack instead of erroring, since these fields
        // are documented as plain values, not callbacks.
        (
            "newpath 0 0 moveto 100 0 lineto << /Width { 10 } >> pkribbon",
            "pkribbon-width-must-not-be-a-procedure",
        ),
        (
            "newpath 0 0 moveto 100 0 lineto << /Width { } >> pkribbon",
            "pkribbon-width-must-not-be-a-procedure",
        ),
        (
            "newpath 0 0 moveto 100 0 lineto << /Width 10 /Pitch { 5 } >> pkribbon",
            "pkribbon-pitch-must-not-be-a-procedure",
        ),
        (
            "newpath 0 0 moveto 100 0 lineto << /Width 10 /StartTaper { 0.1 } >> pkribbon",
            "pkribbon-starttaper-must-not-be-a-procedure",
        ),
        (
            "newpath 0 0 moveto 100 0 lineto << /Width 10 /EndTaper { 0.1 } >> pkribbon",
            "pkribbon-endtaper-must-not-be-a-procedure",
        ),
        (
            "newpath 0 0 moveto 100 0 lineto << /Width 10 /StartCap { /round } >> pkribbon",
            "pkribbon-startcap-must-not-be-a-procedure",
        ),
        (
            "newpath 0 0 moveto 100 0 lineto << /Width 10 /EndCap { /round } >> pkribbon",
            "pkribbon-endcap-must-not-be-a-procedure",
        ),
        (
            "newpath 0 0 moveto 100 0 lineto << /Width 10 /Jitter { 1 } >> pkribbon",
            "pkribbon-jitter-must-not-be-a-procedure",
        ),
    ];
    for (src, name) in cases {
        let err = it.run_str(src).unwrap_err();
        assert!(
            matches!(err, PsError::Undefined(ref n) if n == name),
            "{src}: got {err}"
        );
    }
}

#[test]
fn setpacking_true_does_not_break_pressure_validation() {
    // Partial regression coverage for a Codex-round-5 finding: under a
    // Level 2 interpreter with packing enabled, a plain procedure
    // literal like the /Pressure default itself has type
    // packedarraytype, not arraytype -- the original xcheck+type guard
    // rejected even pkribbon's own default under packing. pscat itself
    // doesn't actually produce packedarraytype under `setpacking`
    // (confirmed directly: `true setpacking { } type` is still
    // arraytype here), so this can only assert the call still works
    // with packing toggled on, not exercise the packedarraytype branch
    // itself -- ghostscript_accepts_paintkit's driver below does that
    // part, against real Ghostscript, where packing actually changes
    // the type.
    let mut it = fresh(220, 40);
    it.run_str(
        "0 0 0 setrgbcolor 1 srand true setpacking \
         newpath 10 20 moveto 210 20 lineto \
         << /Width 10 /Pressure { pktaper } >> pkribbon",
    )
    .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    assert!(ink_count(&it) > 500, "expected the ribbon to still render");
}

#[test]
fn straight_line_centerline_draws_a_continuous_band() {
    let mut it = fresh(220, 40);
    it.run_str(
        "0 0 0 setrgbcolor 1 srand \
         newpath 10 20 moveto 210 20 lineto << /Width 10 >> pkribbon",
    )
    .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    assert!(ink_count(&it) > 1000, "expected a substantial filled band");
}

#[test]
fn bezier_centerline_draws_a_continuous_band() {
    let mut it = fresh(220, 100);
    it.run_str(
        "0 0 0 setrgbcolor 1 srand \
         newpath 10 20 moveto 30 80 190 80 210 20 curveto \
         << /Width 10 >> pkribbon",
    )
    .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    assert!(ink_count(&it) > 1000, "expected a substantial filled band");
}

#[test]
fn closed_polygon_centerline_draws_a_ring_without_crashing() {
    let mut it = fresh(220, 220);
    it.run_str(
        "0 0 0 setrgbcolor 1 srand \
         newpath 40 40 moveto 180 40 lineto 180 180 lineto 40 180 lineto closepath \
         << /Width 10 >> pkribbon",
    )
    .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    assert!(ink_count(&it) > 1000, "expected a substantial ring");
}

#[test]
fn multiple_subpaths_each_become_their_own_ribbon() {
    let mut it = fresh(220, 220);
    it.run_str(
        "0 0 0 setrgbcolor 1 srand \
         newpath 10 10 moveto 100 10 lineto \
         30 60 moveto 90 60 lineto 90 120 lineto 30 120 lineto closepath \
         << /Width 8 >> pkribbon",
    )
    .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    assert!(ink_count(&it) > 500, "expected ink from both subpaths");
}

#[test]
fn closed_polygon_leaves_a_hole_in_the_middle() {
    // Regression test for a Codex-round-1 finding: both closed-loop
    // traversals (right and left offset of the same centerline) built
    // forward, giving them the same winding sign under nonzero fill,
    // so they added instead of one punching a hole in the other -- a
    // closed square rendered as a solid filled square, not a ring.
    let mut it = fresh(220, 220);
    it.run_str(
        "0 0 0 setrgbcolor 1 srand \
         newpath 40 40 moveto 180 40 lineto 180 180 lineto 40 180 lineto closepath \
         << /Width 20 >> pkribbon",
    )
    .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    let center = it.gfx().pixmap.pixel(110, 110).expect("in bounds");
    assert!(
        luma(center) > 200.0,
        "expected an unpainted hole at the ring's center, got luma {}",
        luma(center)
    );
    let edge = it.gfx().pixmap.pixel(110, 40).expect("in bounds");
    assert!(
        luma(edge) < 100.0,
        "expected ink on the ring itself, got luma {}",
        luma(edge)
    );
}

#[test]
fn closed_polygon_leaves_a_hole_under_a_large_scale() {
    // The exact-equality closed-path check (see the pkbrclosed comment
    // in lib/paintkit.ps) must still correctly recognize a *real*
    // closepath under a large CTM, not just at 1x scale -- a genuinely
    // closed subpath's guaranteed-end stop coincides with its start
    // bit-for-bit regardless of scale, since walkpath reports
    // pre-CTM coordinates untouched by the scale transform.
    let mut it = fresh(200, 200);
    it.run_str(
        "0 0 0 setrgbcolor 1 srand gsave 100 100 scale \
         newpath 0.4 0.4 moveto 1.8 0.4 lineto 1.8 1.8 lineto 0.4 1.8 lineto closepath \
         << /Width 0.2 >> pkribbon grestore",
    )
    .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    let center = it.gfx().pixmap.pixel(110, 110).expect("in bounds");
    assert!(
        luma(center) > 200.0,
        "expected an unpainted hole under scale, got luma {}",
        luma(center)
    );
}

#[test]
fn closed_polygon_ring_covers_its_whole_perimeter_not_just_a_dot_at_the_seam() {
    // Regression test for a bug in this round's own first attempt at
    // fixing a Codex-round-4 finding: a closed run's "does a real
    // interior sample exist" check compared pkbrend's *position*
    // against pkbrstart's -- but a closed subpath's guaranteed-end
    // stop *always* returns to the start's coordinates by definition,
    // regardless of how much real interior content the loop has. That
    // made every closed ribbon, at any size, look "degenerate" and
    // collapse to a single dot at the start/seam point instead of
    // following the whole perimeter -- caught visually before this
    // test was written, not by the existing hole-in-the-middle checks
    // (which happened to still pass against the collapsed dot). Fixed
    // by checking index separation instead of position. Sample all
    // four sides, not just one, so a dot-at-the-seam collapse (ink
    // only near start) can't slip past a single-point check either.
    let mut it = fresh(80, 60);
    it.run_str(
        "0 0 0 setrgbcolor 1 srand \
         newpath 20 10 moveto 60 10 lineto 60 50 lineto 20 50 lineto closepath \
         << /Width 16 >> pkribbon",
    )
    .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    for (x, y, side) in [
        (40, 10, "bottom"),
        (40, 50, "top"),
        (20, 30, "left"),
        (60, 30, "right"),
    ] {
        let p = it.gfx().pixmap.pixel(x, y).expect("in bounds");
        assert!(
            luma(p) < 100.0,
            "expected ink on the ring's {side} side ({x},{y}), got luma {}",
            luma(p)
        );
    }
}

#[test]
fn overlapping_start_and_end_taper_ramps_stay_continuous() {
    // Regression test for a Codex-round-1 finding: StartTaper/EndTaper
    // summing past 1 (so their ramp regions overlap) used to pick one
    // ramp by a mutually exclusive branch on t, jumping discontinuously
    // right at the branch boundary instead of blending. Sampled just
    // either side of the branch boundary the old code had (t=0.79 and
    // t=0.80 on a 200-unit-wide ribbon at StartTaper=EndTaper=0.8),
    // the measured width must be close, not a sharp jump.
    let mut it = fresh(220, 60);
    it.run_str(
        "0 0 0 setrgbcolor 1 srand newpath 10 30 moveto 210 30 lineto \
         << /Width 20 /StartTaper 0.8 /EndTaper 0.8 >> pkribbon",
    )
    .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    let just_before = column_height(&it, 168, 60); // t ~= 0.79
    let just_after = column_height(&it, 170, 60); // t ~= 0.80
    let diff = just_before.abs_diff(just_after);
    assert!(
        diff <= 2,
        "expected a continuous taper across the overlap, got {just_before} vs {just_after}"
    );
}

#[test]
fn small_width_cap_survives_a_large_scale_without_collapsing_to_a_point() {
    // Regression test for a Codex-round-1 finding: the cap-degeneracy
    // check used a fixed user-space half-width epsilon (0.001), so a
    // /Width of 0.0015 (half-width 0.00075, under the old threshold)
    // silently collapsed a /round cap to a point even though a large
    // CTM scale makes it many device pixels wide. A zero-radius `arc`
    // is safe in both pscat and Ghostscript (verified directly), so
    // the check now degrades only a truly zero half-width.
    let mut it = fresh(200, 100);
    it.run_str(
        "0 0 0 setrgbcolor 1 srand gsave 10000 10000 scale \
         newpath 0.001 0.005 moveto 0.019 0.005 lineto \
         << /Width 0.0015 /StartCap /round /EndCap /round >> pkribbon grestore",
    )
    .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    // Just inside the end cap (endpoint is at device x=190): a real
    // round cap is close to the full ~15px device width there; a
    // collapsed point would taper to near 0.
    let h = column_height(&it, 189, 100);
    assert!(
        h > 8,
        "expected a wide round cap near the end, got height {h}"
    );
}

#[test]
fn pressure_profiles_change_the_measured_width() {
    let mut flat = fresh(220, 60);
    flat.run_str(
        "0 0 0 setrgbcolor 1 srand newpath 10 30 moveto 210 30 lineto \
         << /Width 20 /Pressure { pkflat } >> pkribbon",
    )
    .unwrap_or_else(|e| panic!("{}", flat.error_report(&e)));
    let flat_start = column_height(&flat, 15, 60);
    let flat_end = column_height(&flat, 205, 60);
    assert!(
        flat_start > 10 && flat_end > 10,
        "constant pressure should be near full width at both ends: {flat_start} {flat_end}"
    );

    let mut taper = fresh(220, 60);
    taper
        .run_str(
            "0 0 0 setrgbcolor 1 srand newpath 10 30 moveto 210 30 lineto \
             << /Width 20 /Pressure { pktaper } >> pkribbon",
        )
        .unwrap_or_else(|e| panic!("{}", taper.error_report(&e)));
    let taper_start = column_height(&taper, 15, 60);
    let taper_end = column_height(&taper, 205, 60);
    assert!(
        taper_start < taper_end,
        "linear taper should be thinner at the start than the end: {taper_start} vs {taper_end}"
    );

    let mut bell = fresh(220, 60);
    bell.run_str(
        "0 0 0 setrgbcolor 1 srand newpath 10 30 moveto 210 30 lineto \
         << /Width 20 /Pressure { pkbell } /StartCap /pointed /EndCap /pointed >> pkribbon",
    )
    .unwrap_or_else(|e| panic!("{}", bell.error_report(&e)));
    let bell_start = column_height(&bell, 15, 60);
    let bell_mid = column_height(&bell, 110, 60);
    assert!(
        bell_mid > bell_start,
        "bell profile should be widest in the middle: mid {bell_mid} vs start {bell_start}"
    );
}

#[test]
fn start_and_end_taper_ramp_width_down_independently_of_pressure() {
    let mut it = fresh(220, 60);
    it.run_str(
        "0 0 0 setrgbcolor 1 srand newpath 10 30 moveto 210 30 lineto \
         << /Width 20 /StartTaper 0.3 /EndTaper 0.3 >> pkribbon",
    )
    .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    let start = column_height(&it, 15, 60);
    let mid = column_height(&it, 110, 60);
    let end = column_height(&it, 205, 60);
    assert!(
        start < mid && end < mid,
        "StartTaper/EndTaper should thin both ends relative to the middle: \
         start {start} mid {mid} end {end}"
    );
}

#[test]
fn full_taper_degrades_the_cap_to_a_point_without_a_zero_radius_arc() {
    // /StartTaper 1 makes the computed half-width at t=0 exactly 0 --
    // /StartCap /round must degrade to a point rather than emit (or
    // crash on) a zero-radius arc.
    let mut it = fresh(220, 60);
    it.run_str(
        "0 0 0 setrgbcolor 1 srand newpath 10 30 moveto 210 30 lineto \
         << /Width 20 /StartTaper 1 /StartCap /round >> pkribbon",
    )
    .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    assert!(
        ink_count(&it) > 500,
        "expected the tapered ribbon to still render"
    );
}

#[test]
fn short_pointed_stroke_renders_a_lens_not_a_blank_page() {
    // Regression test for a Codex-round-3 finding: a subpath shorter
    // than /Pitch gets only walkpath's start+end stops (no interior),
    // so both ends pointed used to collapse straight from tip to tip
    // with zero area -- a valid, generously-wide short stroke rendered
    // nothing. pkopenrun now synthesizes one interior sample at the
    // run's midpoint in exactly this case.
    let mut it = fresh(90, 60);
    it.run_str(
        "0 0 0 setrgbcolor 1 srand \
         newpath 40 30 moveto 44 30 lineto \
         << /Width 20 /StartCap /pointed /EndCap /pointed >> pkribbon",
    )
    .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    assert!(
        ink_count(&it) > 10,
        "expected a visible lens for a short pointed stroke, got {}",
        ink_count(&it)
    );
}

#[test]
fn stroke_exactly_one_pitch_long_still_bulges_with_a_zero_edge_pressure() {
    // Regression test for a Codex-round-4 finding: when a subpath's
    // length is an exact multiple of /Pitch, walkpath's regular
    // stepping already lands exactly on the endpoint, and the
    // guaranteed-end stop then duplicates it (sp == 0) -- so a naive
    // "is there an interior stop" check sees a distinct *index* that
    // isn't actually a distinct *position*, missing the case in
    // short_pointed_stroke_renders_a_lens_not_a_blank_page (which uses
    // a length well under /Pitch, no duplicate involved). Combined
    // with a pressure profile that's genuinely zero at the very edge
    // (pkbell at t=1), the only "interior" sample available also has
    // zero width, so the fix has to actually drop the duplicate before
    // deciding whether a real bulge is possible.
    let mut it = fresh(40, 60);
    it.run_str(
        "0 0 0 setrgbcolor 1 srand \
         newpath 10 30 moveto 20 30 lineto \
         << /Width 20 /Pitch 10 /Pressure { pkbell } \
            /StartCap /pointed /EndCap /pointed >> pkribbon",
    )
    .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    assert!(
        ink_count(&it) > 10,
        "expected a visible lens even at exactly one pitch step, got {}",
        ink_count(&it)
    );
}

#[test]
fn tiny_closed_path_falls_back_to_a_dot_not_a_blank_page() {
    // Regression test for a Codex-round-4 finding: a closed subpath
    // shorter than /Pitch gets only walkpath's start and guaranteed
    // end, both at the same position -- but the end's sp is the
    // subpath's own positive length, not 0, so the sp==0 duplicate
    // check alone doesn't catch it. Without a position-aware check,
    // pkloop built two coincident-point "loops" that fill painted as
    // nothing instead of the documented dot fallback.
    let mut it = fresh(40, 60);
    it.run_str(
        "0 0 0 setrgbcolor 1 srand \
         newpath 20 30 moveto 22 30 lineto 21 32 lineto closepath \
         << /Width 20 /Pitch 50 >> pkribbon",
    )
    .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    assert!(
        ink_count(&it) > 20,
        "expected a filled dot for a closed path shorter than Pitch, got {}",
        ink_count(&it)
    );
}

#[test]
fn open_path_returning_to_its_start_without_closepath_keeps_its_caps() {
    // Regression test for a Codex-round-4 finding: comparing walkpath's
    // start/end stop coordinates for equality can't distinguish a real
    // closepath from an open path that merely ends exactly where it
    // began (an explicit lineto back to the start, no closepath) --
    // both give bit-for-bit identical endpoint coordinates. Closure is
    // now tracked directly from the path via pkscanclosed (whether
    // pathforall actually reported a close segment), not inferred from
    // coordinates. Both the open (pointed-tip) and closed (smooth ring)
    // versions of this rectangle are hollow in the middle either way --
    // pointed caps still trace the same outline, they don't fill it --
    // so the real signature of "caps got applied" is at the seam
    // corner itself, not the center: comparing rendered ink counts
    // against the same path with a real closepath (where /StartCap//
    // EndCap must be ignored per the documented ring contract) should
    // show a real difference if this one kept its pointed corner.
    fn render(src: &str) -> usize {
        let mut it = fresh(80, 60);
        it.run_str(src)
            .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
        ink_count(&it)
    }
    let open_pointed = render(
        "0 0 0 setrgbcolor 1 srand \
         newpath 20 10 moveto 60 10 lineto 60 50 lineto 20 50 lineto 20 10 lineto \
         << /Width 16 /StartCap /pointed /EndCap /pointed >> pkribbon",
    );
    let truly_closed = render(
        "0 0 0 setrgbcolor 1 srand \
         newpath 20 10 moveto 60 10 lineto 60 50 lineto 20 50 lineto closepath \
         << /Width 16 /StartCap /pointed /EndCap /pointed >> pkribbon",
    );
    assert_ne!(
        open_pointed, truly_closed,
        "an explicit-lineto-back-to-start path and a real closepath \
         path should render differently (caps kept vs ignored), not \
         both get treated as closed"
    );
}

#[test]
fn open_path_coincident_with_its_own_start_falls_back_to_a_dot() {
    // Regression test for a Codex-round-6 finding: an open subpath
    // that returns exactly to its own starting coordinates (an
    // unclosed full-circle arc, no closepath) and is also shorter than
    // /Pitch has pkox0==pkoxe and pkoy0==pkoye -- the round-3 midpoint-
    // synthesis fix computed a chord direction via `atan` on that
    // (now-zero) delta, which is undefined in both pscat and
    // Ghostscript (confirmed directly against both). No chord exists
    // to synthesize a midpoint from in that case, so it falls back to
    // a dot instead, same as any other genuinely degenerate short run.
    let mut it = fresh(80, 80);
    it.run_str(
        "0 0 0 setrgbcolor 1 srand \
         newpath 40 40 30 0 360 arc \
         << /Width 100 /Pitch 200 /StartCap /pointed /EndCap /pointed >> pkribbon",
    )
    .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    assert!(
        ink_count(&it) > 50,
        "expected a filled dot for a coincident-endpoint short open path, got {}",
        ink_count(&it)
    );
}

#[test]
fn seeded_jitter_is_deterministic_and_actually_perturbs_the_edge() {
    fn render(jitter: f64, seed: i64) -> Vec<u8> {
        let mut it = fresh(320, 40);
        it.run_str(&format!(
            "0 0 0 setrgbcolor {seed} srand newpath 10 20 moveto 310 20 lineto \
             << /Width 10 /Jitter {jitter} >> pkribbon"
        ))
        .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
        it.gfx().pixmap.data().to_vec()
    }

    let a = render(4.0, 5);
    let b = render(4.0, 5);
    assert_eq!(a, b, "same seed, same Jitter -> identical pixels");

    // Negative control: this would pass trivially if /Jitter were
    // silently ignored, so assert the jittered render actually
    // differs from an unjittered one at the same seed.
    let unjittered = render(0.0, 5);
    assert_ne!(
        a, unjittered,
        "Jitter > 0 must perturb the edge, not render identically to Jitter 0"
    );

    let other_seed = render(4.0, 9);
    assert_ne!(a, other_seed, "a different seed should jitter differently");
}

#[test]
fn degenerate_single_point_falls_back_to_a_dot() {
    let mut it = fresh(60, 60);
    it.run_str("0 0 0 setrgbcolor newpath 30 30 moveto << /Width 12 >> pkribbon")
        .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    assert!(ink_count(&it) > 20, "expected a filled dot at the point");
}

#[test]
fn degenerate_single_point_ignores_taper_and_still_draws_a_dot() {
    // pkdot's diameter is Width*Pressure only -- StartTaper/EndTaper
    // are meaningless for a lone point (no path to ramp along), and
    // applying them anyway (pkhalfwat does, for a real path) would
    // zero the dot out for any nonzero /StartTaper at t=0.
    let mut it = fresh(60, 60);
    it.run_str(
        "0 0 0 setrgbcolor newpath 30 30 moveto \
         << /Width 12 /StartTaper 0.5 /EndTaper 0.5 >> pkribbon",
    )
    .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    assert!(
        ink_count(&it) > 20,
        "expected a filled dot despite StartTaper/EndTaper"
    );
}

#[test]
fn empty_path_is_a_no_op() {
    let mut it = fresh(60, 60);
    it.run_str("0 0 0 setrgbcolor newpath << /Width 12 >> pkribbon")
        .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    assert_eq!(ink_count(&it), 0, "empty path should draw nothing");
}

#[test]
fn ghostscript_accepts_paintkit() {
    let gs_ok = std::process::Command::new("gs")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success());
    if !gs_ok {
        eprintln!("skipping gs compatibility check: gs not installed");
        return;
    }
    let artkit = std::fs::read_to_string("lib/artkit.ps").expect("artkit");
    let paintkit = std::fs::read_to_string("lib/paintkit.ps").expect("paintkit");
    // `true setpacking`: real Ghostscript packs subsequently-parsed
    // procedure literals into packedarraytype under packing mode
    // (confirmed directly; pscat itself doesn't), so running the whole
    // driver -- including pkribbon's own `{ pkflat }` default and every
    // explicit /Pressure literal below -- with packing on is what
    // actually exercises the packedarraytype branch of the /Pressure
    // guard (Codex review, round 5), not just calling `pkribbon` at all.
    let driver = "true setpacking 3 srand \
        0 0 0 setrgbcolor \
        newpath 10 10 moveto 100 10 lineto << /Width 8 >> pkribbon \
        newpath 10 40 moveto 30 80 90 80 110 40 curveto \
            << /Width 8 /Pressure { pktaper } >> pkribbon \
        newpath 130 10 moveto 190 10 lineto 190 60 lineto 130 60 lineto closepath \
            << /Width 8 >> pkribbon \
        newpath 220 10 moveto 280 10 lineto \
            << /Width 12 /Pressure { pkbell } /StartCap /pointed /EndCap /pointed >> pkribbon \
        newpath 220 40 moveto 280 40 lineto << /Width 8 /Jitter 3 >> pkribbon \
        newpath 300 30 moveto << /Width 10 >> pkribbon";
    let dir = std::env::temp_dir().join(format!("pscat-paintkit-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let combined = dir.join("paintkit_gs.ps");
    std::fs::write(
        &combined,
        format!("{artkit}\n{paintkit}\n{driver}\nshowpage\n"),
    )
    .expect("write");
    let status = std::process::Command::new("gs")
        .args([
            "-dNOPAUSE",
            "-dBATCH",
            "-q",
            "-sDEVICE=png16m",
            "-g400x120",
            "-r72",
            "-o/dev/null",
        ])
        .arg(&combined)
        .status()
        .expect("run gs");
    assert!(status.success(), "gs rejected paintkit");
}

// --- pknib (issue #42): angled-nib calligraphy preset ------------------

#[test]
fn nib_guards_reject_malformed_input() {
    let mut it = Interp::new();
    load(&mut it);
    let cases = [
        (
            "newpath 0 0 moveto 10 10 lineto 30 30 moveto 40 40 lineto \
             << /Width 10 >> pknib",
            "pknib-path-must-be-a-single-subpath",
        ),
        (
            "newpath 0 0 moveto 10 0 lineto 10 10 lineto closepath \
             << /Width 10 >> pknib",
            "pknib-path-must-not-be-closed",
        ),
        (
            "newpath 0 0 moveto 10 10 lineto << /Width 0 >> pknib",
            "pknib-width-must-be-positive",
        ),
        (
            "newpath 0 0 moveto 10 10 lineto << /Width -3 >> pknib",
            "pknib-width-must-be-positive",
        ),
        (
            "newpath 0 0 moveto 10 10 lineto << /Width { 10 } >> pknib",
            "pknib-width-must-not-be-a-procedure",
        ),
        (
            "newpath 0 0 moveto 10 10 lineto << /Width 10 /Pitch 0 >> pknib",
            "pknib-pitch-must-be-positive",
        ),
        (
            "newpath 0 0 moveto 10 10 lineto << /Width 10 /Pitch -1 >> pknib",
            "pknib-pitch-must-be-positive",
        ),
        (
            "newpath 0 0 moveto 10 10 lineto << /Width 10 /Pitch { 5 } >> pknib",
            "pknib-pitch-must-not-be-a-procedure",
        ),
        (
            "newpath 0 0 moveto 10 10 lineto << /Width 10 /Angle { 5 } >> pknib",
            "pknib-angle-must-not-be-a-procedure",
        ),
        (
            "newpath 0 0 moveto 10 10 lineto << /Width 10 /MinWidth -0.1 >> pknib",
            "pknib-minwidth-must-be-0-to-1",
        ),
        (
            "newpath 0 0 moveto 10 10 lineto << /Width 10 /MinWidth 1.5 >> pknib",
            "pknib-minwidth-must-be-0-to-1",
        ),
        (
            "newpath 0 0 moveto 10 10 lineto << /Width 10 /MinWidth { 0.1 } >> pknib",
            "pknib-minwidth-must-not-be-a-procedure",
        ),
        // Same /Pressure-must-actually-be-callable trap pkribbon itself
        // guards against (Codex rounds 2/3 there): pknib's own composite
        // /Pressure calls the caller's proc by bare reference, so an
        // unvalidated non-procedure would silently corrupt the stack
        // instead of erroring.
        (
            "newpath 0 0 moveto 10 10 lineto << /Width 10 /Pressure 1 >> pknib",
            "pknib-pressure-must-be-a-procedure",
        ),
        (
            "newpath 0 0 moveto 10 10 lineto << /Width 10 /Pressure (nope) >> pknib",
            "pknib-pressure-must-be-a-procedure",
        ),
        (
            "newpath 0 0 moveto 10 10 lineto << /Width 10 /Pressure 2 cvx >> pknib",
            "pknib-pressure-must-be-a-procedure",
        ),
    ];
    for (src, name) in cases {
        let err = it.run_str(src).unwrap_err();
        assert!(
            matches!(err, PsError::Undefined(ref n) if n == name),
            "{src}: got {err}"
        );
    }
}

#[test]
fn nib_validates_opts_even_on_an_empty_path() {
    // Regression test for a Codex-review (PR #77) finding: validation
    // used to be nested entirely inside the `pnmoveto 0 gt` guard, so
    // an empty path skipped it -- a malformed, non-dict opts operand
    // (e.g. a bare number) never even got read, silently succeeding
    // instead of erroring the way pkribbon does for the same call.
    // pkribbon validates its whole opts dict unconditionally, before
    // ever looking at the path; pknib now does too.
    let mut it = Interp::new();
    load(&mut it);
    let err = it.run_str("newpath 42 pknib").unwrap_err();
    assert!(
        matches!(err, PsError::Typecheck),
        "expected a typecheck error for a non-dict opts operand on an \
         empty path, got {err}"
    );
}

#[test]
fn nib_degenerate_single_point_falls_back_to_a_visible_dot() {
    // Regression test for a Codex-review (PR #77) finding: a moveto-
    // only subpath has no direction of travel (walkpath reports a
    // synthetic ang=0 for it, not a real one), but the nib-angle
    // multiplier was applied anyway -- at Angle 0 with MinWidth 0 this
    // floored the response to exactly 0, silently rendering nothing
    // instead of the dot fallback pkribbon (and pknib's own header)
    // document. pnpressure now skips the nib multiplier entirely for a
    // single-sample pnangles table (walkpath's contract guarantees any
    // subpath with real length gets at least two stops, so this is an
    // unambiguous signal).
    let mut it = fresh(60, 60);
    it.run_str(
        "0 0 0 setrgbcolor newpath 30 30 moveto \
         << /Width 12 /Angle 0 /MinWidth 0 >> pknib",
    )
    .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    assert!(
        ink_count(&it) > 5,
        "expected a filled dot despite Angle 0 / MinWidth 0, got {}",
        ink_count(&it)
    );
}

#[test]
fn nib_empty_path_still_validates_fields_pknib_forwards_to_pkribbon() {
    // Regression test for a Codex-review (PR #77, round 2) finding:
    // pknib's own empty-path guard used to skip pkribbon entirely, so
    // fields pknib itself never validates (StartTaper/EndTaper/
    // StartCap/EndCap/Jitter -- only pkribbon checks these) went
    // unchecked on an empty path, unlike the equivalent pkribbon call.
    // pknib now always delegates to pkribbon, which validates
    // everything before its own `pkn 0 gt` guard decides to no-op.
    let mut it = Interp::new();
    load(&mut it);
    let cases = [
        (
            "newpath << /StartTaper -1 >> pknib",
            "pkribbon-starttaper-must-be-0-to-1",
        ),
        (
            "newpath << /StartCap /bogus >> pknib",
            "pkribbon-startcap-must-be-round-flat-or-pointed",
        ),
    ];
    for (src, name) in cases {
        let err = it.run_str(src).unwrap_err();
        assert!(
            matches!(err, PsError::Undefined(ref n) if n == name),
            "{src}: got {err}"
        );
    }
}

#[test]
fn nib_short_pointed_stroke_uses_the_chord_direction_for_its_synthesized_midpoint() {
    // Regression test for a Codex-review (PR #77, round 2) finding: a
    // curved stroke shorter than /Pitch with both caps pointed has no
    // interior walkpath sample -- pkopenrun synthesizes one interior
    // bulge point using the *chord's* own direction, but pnangleat's
    // plain nearest-t lookup had no sample there and picked whichever
    // endpoint happened to be closer in t, an arbitrary answer. This
    // curve's chord is horizontal (dx=3, dy=0): at /Angle 0 the
    // response should collapse toward the /MinWidth floor (near-
    // hairline, matching the chord), and at /Angle 90 it should render
    // near full width (perpendicular to the chord) -- the same
    // discriminator nib_angle_changes_the_measured_width uses, applied
    // to the synthesized-midpoint code path specifically.
    let curve = "newpath 20 30 moveto 21 31.5 22 31.5 23 30 curveto";

    let mut parallel = fresh(60, 60);
    parallel
        .run_str(&format!(
            "0 0 0 setrgbcolor 1 srand {curve} \
             << /Width 20 /Angle 0 /MinWidth 0 \
                /StartCap /pointed /EndCap /pointed >> pknib"
        ))
        .unwrap_or_else(|e| panic!("{}", parallel.error_report(&e)));
    let parallel_ink = ink_count(&parallel);

    let mut perpendicular = fresh(60, 60);
    perpendicular
        .run_str(&format!(
            "0 0 0 setrgbcolor 1 srand {curve} \
             << /Width 20 /Angle 90 /MinWidth 0 \
                /StartCap /pointed /EndCap /pointed >> pknib"
        ))
        .unwrap_or_else(|e| panic!("{}", perpendicular.error_report(&e)));
    let perpendicular_ink = ink_count(&perpendicular);

    assert!(
        perpendicular_ink > parallel_ink * 3,
        "Angle perpendicular to the chord should render much more ink \
         than Angle parallel to it at the synthesized midpoint: \
         parallel {parallel_ink} perpendicular {perpendicular_ink}"
    );
}

#[test]
fn nib_angle_changes_the_measured_width() {
    // Angle 0 on a horizontal stroke: travel runs parallel to the nib,
    // so the response floors at /MinWidth (near-hairline). Angle 90:
    // travel runs perpendicular to the nib, the response is 1.0 (full
    // /Width) -- the unambiguous discriminator, not a pair that happens
    // to be symmetric under |sin|.
    let mut thin = fresh(220, 60);
    thin.run_str(
        "0 0 0 setrgbcolor 1 srand newpath 10 30 moveto 210 30 lineto \
         << /Width 20 /Angle 0 >> pknib",
    )
    .unwrap_or_else(|e| panic!("{}", thin.error_report(&e)));
    let thin_h = column_height(&thin, 110, 60);

    let mut thick = fresh(220, 60);
    thick
        .run_str(
            "0 0 0 setrgbcolor 1 srand newpath 10 30 moveto 210 30 lineto \
             << /Width 20 /Angle 90 >> pknib",
        )
        .unwrap_or_else(|e| panic!("{}", thick.error_report(&e)));
    let thick_h = column_height(&thick, 110, 60);

    assert!(
        thick_h > thin_h * 3,
        "Angle perpendicular to travel should render much wider than \
         Angle parallel to it: thin {thin_h} thick {thick_h}"
    );
}

#[test]
fn nib_min_width_floors_the_near_hairline_response() {
    // Same parallel-to-nib case as above, but MinWidth 0 -- the floor
    // is off, so the stroke should be even thinner (ideally near
    // invisible) than the default-MinWidth version.
    let mut floored = fresh(220, 60);
    floored
        .run_str(
            "0 0 0 setrgbcolor 1 srand newpath 10 30 moveto 210 30 lineto \
             << /Width 20 /Angle 0 >> pknib",
        )
        .unwrap_or_else(|e| panic!("{}", floored.error_report(&e)));
    let floored_h = column_height(&floored, 110, 60);

    let mut zeroed = fresh(220, 60);
    zeroed
        .run_str(
            "0 0 0 setrgbcolor 1 srand newpath 10 30 moveto 210 30 lineto \
             << /Width 20 /Angle 0 /MinWidth 0 >> pknib",
        )
        .unwrap_or_else(|e| panic!("{}", zeroed.error_report(&e)));
    let zeroed_h = column_height(&zeroed, 110, 60);

    assert!(
        floored_h > 0,
        "expected the default MinWidth floor to render *something*, \
         not a vacuous zero-vs-zero pass; got {floored_h}"
    );
    assert!(
        zeroed_h <= floored_h,
        "MinWidth 0 should never render wider than the default floor: \
         zeroed {zeroed_h} floored {floored_h}"
    );
}

#[test]
fn taper_composes_with_the_nib_angle_response() {
    // Angle 90 keeps the nib-angle multiplier pinned at 1.0 along this
    // horizontal stroke, isolating StartTaper/EndTaper's own effect on
    // top of it.
    let mut it = fresh(220, 60);
    it.run_str(
        "0 0 0 setrgbcolor 1 srand newpath 10 30 moveto 210 30 lineto \
         << /Width 20 /Angle 90 /StartTaper 0.3 /EndTaper 0.3 >> pknib",
    )
    .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    let start = column_height(&it, 15, 60);
    let mid = column_height(&it, 110, 60);
    let end = column_height(&it, 205, 60);
    assert!(
        start < mid && end < mid,
        "StartTaper/EndTaper should still thin both ends relative to \
         the middle when composed with the nib-angle response: \
         start {start} mid {mid} end {end}"
    );
}

#[test]
fn pressure_composes_with_the_nib_angle_response() {
    let mut it = fresh(220, 60);
    it.run_str(
        "0 0 0 setrgbcolor 1 srand newpath 10 30 moveto 210 30 lineto \
         << /Width 20 /Angle 90 /Pressure { pktaper } >> pknib",
    )
    .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    let start = column_height(&it, 15, 60);
    let end = column_height(&it, 205, 60);
    assert!(
        start < end,
        "a linear /Pressure taper should still thin the start relative \
         to the end when composed with the nib-angle response: \
         start {start} end {end}"
    );
}

#[test]
fn nib_jitter_is_deterministic_and_perturbs_the_edge() {
    fn render(jitter: f64, seed: i64) -> Vec<u8> {
        let mut it = fresh(320, 60);
        it.run_str(&format!(
            "0 0 0 setrgbcolor {seed} srand newpath 10 30 moveto 310 30 lineto \
             << /Width 16 /Angle 90 /Jitter {jitter} >> pknib"
        ))
        .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
        it.gfx().pixmap.data().to_vec()
    }

    let a = render(4.0, 5);
    let b = render(4.0, 5);
    assert_eq!(a, b, "same seed, same Jitter -> identical pixels");

    let unjittered = render(0.0, 5);
    assert_ne!(
        a, unjittered,
        "Jitter > 0 must perturb the edge, not render identically to Jitter 0"
    );

    let other_seed = render(4.0, 9);
    assert_ne!(a, other_seed, "a different seed should jitter differently");
}

#[test]
fn corners_render_without_crashing() {
    let mut it = fresh(220, 220);
    it.run_str(
        "0 0 0 setrgbcolor 1 srand \
         newpath 20 20 moveto 100 180 lineto 180 20 lineto \
         << /Width 14 /Angle 30 >> pknib",
    )
    .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    assert!(ink_count(&it) > 200, "expected ink across the corners");
}

#[test]
fn direction_reversal_renders_without_crashing() {
    // A sharp cusp: travel direction reverses roughly 180 degrees
    // partway along the stroke -- a real chisel nib's width genuinely
    // jumps there, not a bug pknib needs to smooth over.
    let mut it = fresh(220, 220);
    it.run_str(
        "0 0 0 setrgbcolor 1 srand \
         newpath 20 20 moveto 100 180 lineto 20 20 lineto \
         << /Width 14 /Angle 30 >> pknib",
    )
    .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    assert!(
        ink_count(&it) > 100,
        "expected ink along the reversed stroke"
    );
}

#[test]
fn bezier_stroke_renders_without_crashing() {
    let mut it = fresh(220, 100);
    it.run_str(
        "0 0 0 setrgbcolor 1 srand \
         newpath 10 20 moveto 30 80 190 80 210 20 curveto \
         << /Width 14 /Angle 45 >> pknib",
    )
    .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    assert!(ink_count(&it) > 200, "expected a substantial filled band");
}

#[test]
fn nib_degenerate_single_point_falls_back_to_a_dot() {
    let mut it = fresh(60, 60);
    it.run_str("0 0 0 setrgbcolor newpath 30 30 moveto << /Width 12 >> pknib")
        .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    assert!(ink_count(&it) > 5, "expected a filled dot at the point");
}

#[test]
fn nib_empty_path_is_a_no_op() {
    let mut it = fresh(60, 60);
    it.run_str("0 0 0 setrgbcolor newpath << /Width 12 >> pknib")
        .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    assert_eq!(ink_count(&it), 0, "empty path should draw nothing");
}

#[test]
fn ghostscript_accepts_paintkit_nib() {
    let gs_ok = std::process::Command::new("gs")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success());
    if !gs_ok {
        eprintln!("skipping gs compatibility check: gs not installed");
        return;
    }
    let artkit = std::fs::read_to_string("lib/artkit.ps").expect("artkit");
    let paintkit = std::fs::read_to_string("lib/paintkit.ps").expect("paintkit");
    let driver = "true setpacking 3 srand \
        0 0 0 setrgbcolor \
        newpath 10 10 moveto 100 10 lineto << /Width 12 /Angle 30 >> pknib \
        newpath 10 40 moveto 90 40 lineto 90 80 lineto 10 80 lineto \
            << /Width 10 /Angle 45 >> pknib \
        newpath 130 10 moveto 190 60 lineto 130 10 lineto \
            << /Width 10 /Angle 60 >> pknib \
        newpath 220 10 moveto 240 60 260 60 280 10 curveto \
            << /Width 10 /Angle 30 /StartTaper 0.2 /EndTaper 0.2 \
               /Pressure { pkbell } /Jitter 2 >> pknib \
        newpath 300 30 moveto << /Width 10 /Angle 20 >> pknib";
    let dir = std::env::temp_dir().join(format!("pscat-paintkit-nib-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let combined = dir.join("paintkit_nib_gs.ps");
    std::fs::write(
        &combined,
        format!("{artkit}\n{paintkit}\n{driver}\nshowpage\n"),
    )
    .expect("write");
    let status = std::process::Command::new("gs")
        .args([
            "-dNOPAUSE",
            "-dBATCH",
            "-q",
            "-sDEVICE=png16m",
            "-g400x120",
            "-r72",
            "-o/dev/null",
        ])
        .arg(&combined)
        .status()
        .expect("run gs");
    assert!(status.success(), "gs rejected paintkit's pknib");
}

// --- pkdry (issue #43): dry-bristle brush with broken coverage --------
//
// A bounded family of thin offset bristles scattered across the
// centerline, each broken into ink/no-ink runs by a seeded two-state
// Markov chain (see the model doc above `/pkdry` in lib/paintkit.ps).
// Coverage (ink_count) is the primary discriminator below since the
// output isn't one continuous band -- loaded vs. very-dry differ in
// how *much* of the path stays inked, not in a simple width measure.

#[test]
fn dry_guards_reject_malformed_input() {
    let mut it = Interp::new();
    load(&mut it);
    let cases = [
        (
            "newpath 0 0 moveto 100 0 lineto << /Width 0 >> pkdry",
            "pkdry-width-must-be-positive",
        ),
        (
            "newpath 0 0 moveto 100 0 lineto << /Width -3 >> pkdry",
            "pkdry-width-must-be-positive",
        ),
        (
            "newpath 0 0 moveto 100 0 lineto << /Width { 10 } >> pkdry",
            "pkdry-width-must-not-be-a-procedure",
        ),
        (
            "newpath 0 0 moveto 100 0 lineto << /Bristles 0 >> pkdry",
            "pkdry-bristles-must-be-1-to-100",
        ),
        (
            "newpath 0 0 moveto 100 0 lineto << /Bristles 101 >> pkdry",
            "pkdry-bristles-must-be-1-to-100",
        ),
        (
            // Regression test for a Codex-review finding: a fractional
            // /Bristles used to pass the range check and silently draw
            // only one bristle through `1 1 pbbristles for`'s own
            // truncating semantics, instead of erroring on malformed
            // input.
            "newpath 0 0 moveto 100 0 lineto << /Bristles 1.5 >> pkdry",
            "pkdry-bristles-must-be-1-to-100",
        ),
        (
            "newpath 0 0 moveto 100 0 lineto << /Bristles { 10 } >> pkdry",
            "pkdry-bristles-must-not-be-a-procedure",
        ),
        (
            "newpath 0 0 moveto 100 0 lineto << /Spread -0.1 >> pkdry",
            "pkdry-spread-must-be-0-to-1",
        ),
        (
            "newpath 0 0 moveto 100 0 lineto << /Spread 1.1 >> pkdry",
            "pkdry-spread-must-be-0-to-1",
        ),
        (
            "newpath 0 0 moveto 100 0 lineto << /BristleWidth 0 >> pkdry",
            "pkdry-bristlewidth-must-be-positive",
        ),
        (
            "newpath 0 0 moveto 100 0 lineto << /WidthJitter -0.1 >> pkdry",
            "pkdry-widthjitter-must-be-0-to-1",
        ),
        (
            "newpath 0 0 moveto 100 0 lineto << /WidthJitter 1.1 >> pkdry",
            "pkdry-widthjitter-must-be-0-to-1",
        ),
        (
            "newpath 0 0 moveto 100 0 lineto << /Load -0.1 >> pkdry",
            "pkdry-load-must-be-0-to-1",
        ),
        (
            "newpath 0 0 moveto 100 0 lineto << /Load 1.1 >> pkdry",
            "pkdry-load-must-be-0-to-1",
        ),
        (
            "newpath 0 0 moveto 100 0 lineto << /Dropout -0.1 >> pkdry",
            "pkdry-dropout-must-be-0-to-1",
        ),
        (
            "newpath 0 0 moveto 100 0 lineto << /Dropout 1.1 >> pkdry",
            "pkdry-dropout-must-be-0-to-1",
        ),
        (
            "newpath 0 0 moveto 100 0 lineto << /Jitter { 1 } >> pkdry",
            "pkdry-jitter-must-not-be-a-procedure",
        ),
        (
            "newpath 0 0 moveto 100 0 lineto << /Pitch 0 >> pkdry",
            "pkdry-pitch-must-be-positive",
        ),
        (
            "newpath 0 0 moveto 100 0 lineto << /Pitch -1 >> pkdry",
            "pkdry-pitch-must-be-positive",
        ),
        (
            "newpath 0 0 moveto 100 0 lineto << /ColorJitter -0.1 >> pkdry",
            "pkdry-colorjitter-must-be-0-to-1",
        ),
        (
            "newpath 0 0 moveto 100 0 lineto << /ColorJitter 1.1 >> pkdry",
            "pkdry-colorjitter-must-be-0-to-1",
        ),
    ];
    for (src, name) in cases {
        let err = it.run_str(src).unwrap_err();
        assert!(
            matches!(err, PsError::Undefined(ref n) if n == name),
            "{src}: got {err}"
        );
    }
}

#[test]
fn dry_deposit_budget_guard_rejects_bristles_times_samples_over_the_limit() {
    // 100 bristles (the /Bristles cap) on a long path at a fine custom
    // /Pitch multiplies out past the 150000 deposit budget even though
    // /Bristles alone is within its own range guard -- the second,
    // independent safety limit the issue's acceptance criteria ask for.
    let mut it = Interp::new();
    load(&mut it);
    let err = it
        .run_str(
            "newpath 0 0 moveto 3000 0 lineto \
             << /Width 10 /Bristles 100 /Pitch 1 >> pkdry",
        )
        .unwrap_err();
    assert!(
        matches!(err, PsError::Undefined(ref n) if n == "pkdry-deposit-count-exceeds-safety-limit"),
        "got {err}"
    );
}

#[test]
fn dry_deposit_budget_guard_rejects_quickly_even_on_a_huge_path() {
    // Regression test for a Codex-review finding: the budget check
    // originally ran only *after* the counting walkpath pass finished,
    // so a pathologically long path at a tiny /Pitch could spend an
    // enormous number of interpreted iterations counting stops before
    // ever getting rejected -- the advertised "before any drawing
    // starts" safety limit didn't actually bound that pass itself. The
    // budget check now runs inside the counting callback, aborting as
    // soon as the running Bristles*count product crosses the limit.
    // This uses a path far too long to finish a full count in
    // reasonable time if the abort weren't working -- the test itself
    // completing at all (within the harness's normal timeout) is the
    // assertion, not just the error kind.
    let mut it = Interp::new();
    load(&mut it);
    let err = it
        .run_str(
            "newpath 0 0 moveto 5000000 0 lineto \
             << /Width 10 /Bristles 100 /Pitch 0.01 >> pkdry",
        )
        .unwrap_err();
    assert!(
        matches!(err, PsError::Undefined(ref n) if n == "pkdry-deposit-count-exceeds-safety-limit"),
        "got {err}"
    );
}

#[test]
fn dry_each_subpath_draws_its_own_independent_bristle_scatter() {
    // Regression test for a Codex-review finding: bristle offset,
    // width, and color used to be drawn once per bristle and reused
    // verbatim across every subpath, contradicting pkribbon's own
    // "each subpath becomes an independent [mark]" contract that
    // pkdry's own header claims too. A single bristle (isolating the
    // effect from cross-bristle averaging) on two identical, vertically
    // separated horizontal subpaths with heavy /ColorJitter: with the
    // bug, both subpaths draw the exact same color (one shared draw);
    // fixed, each subpath's independent `shade` draw lands in
    // continuous RGB space, where two independent rolls coinciding
    // exactly is vanishingly unlikely -- a far higher-resolution signal
    // than comparing pixel-quantized band heights (which collapse a
    // continuous width into only a handful of possible pixel counts,
    // and coincided in an earlier draft of this test).
    // The most-inked (lowest-luma) pixel in the range, not a fixed
    // luma threshold: a lightening ColorJitter draw (shade's k > 1
    // branch) can push even a fully-covered pixel's luma well above a
    // fixed cutoff tuned to the base color, so a threshold-based search
    // can spuriously find nothing for one of the two draws.
    fn most_inked_color(it: &Interp, x: u32, y_lo: u32, y_hi: u32) -> (u8, u8, u8) {
        (y_lo..y_hi)
            .map(|y| it.gfx().pixmap.pixel(x, y).expect("in bounds"))
            .min_by(|a, b| luma(*a).partial_cmp(&luma(*b)).expect("not NaN"))
            .map(|p| (p.red(), p.green(), p.blue()))
            .expect("non-empty y range")
    }

    let mut it = fresh(220, 100);
    it.run_str(
        "0.2 0.3 0.8 setrgbcolor 21 srand \
         newpath 10 25 moveto 210 25 lineto \
         10 75 moveto 210 75 lineto \
         << /Width 20 /Bristles 1 /Spread 0 \
            /Load 1 /Dropout 0 /ColorJitter 0.4 >> pkdry",
    )
    .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));

    let top = most_inked_color(&it, 110, 0, 50);
    let bottom = most_inked_color(&it, 110, 50, 100);
    assert_ne!(
        top, bottom,
        "each subpath's bristle should draw its own independent color \
         roll, not reuse the same one: top {top:?} bottom {bottom:?}"
    );
}

#[test]
fn dry_load_1_forces_contact_even_on_the_frnd_exactly_1_edge_case() {
    // Regression test for a Codex-review finding: `frnd` (rand's high
    // bits divided down) can land on exactly 1.0, which made a bare
    // `frnd pbload lt` false even at /Load 1 -- silently breaking the
    // documented "rate 1 means certain" contract. Seed 5659 with a
    // single bristle is the exact case Codex's review reported drawing
    // nothing before the fix (`pbroll`'s explicit `rate >= 1` -> always
    // true branch).
    let mut it = fresh(60, 60);
    it.run_str(
        "0 0 0 setrgbcolor 5659 srand newpath 30 30 moveto \
         << /Width 12 /Bristles 1 /Load 1 /Dropout 0 >> pkdry",
    )
    .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    assert!(
        ink_count(&it) > 0,
        "expected /Load 1 to force contact even under this seed's \
         exactly-1.0 frnd roll"
    );
}

#[test]
fn dry_final_short_stop_uses_its_own_travel_distance() {
    // Regression test for a Codex-review finding: each transition's
    // probability used to be derived from the *nominal* /Pitch for
    // every sample, including walkpath's guaranteed final stop of a
    // subpath, whose actual `sp` (arc-length since the previous stop)
    // can be much shorter than a full pitch step -- overstating that
    // one transition's probability specifically at subpath ends,
    // breaking the "per one Width of travel" contract there. /Pitch 10
    // over a 25-unit path gives stops at 0, 10, 20, then a guaranteed
    // final stop at 25 with sp=5 (half a pitch step) -- this exercises
    // that exact shape end to end (extracting `sp` via `pbraw`'s shared
    // 6-field layout, `4 get`) without crashing, at a /Dropout high
    // enough that the old, nominal-pitch-scaled bug would have made the
    // final short stop drop out almost as often as a full-pitch one.
    let mut it = fresh(60, 60);
    it.run_str(
        "0 0 0 setrgbcolor 15 srand newpath 10 30 moveto 35 30 lineto \
         << /Width 100 /Bristles 1 /Pitch 10 /Load 1 /Dropout 0.99 \
            /StartCap /pointed /EndCap /pointed >> pkdry",
    )
    .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    assert!(
        ink_count(&it) > 0,
        "expected the short-leftover-final-stop path to render without \
         crashing"
    );
}

#[test]
fn dry_loaded_preset_covers_more_of_the_stroke_than_very_dry() {
    fn render(load_v: f64, dropout_v: f64) -> usize {
        let mut it = fresh(320, 40);
        it.run_str(&format!(
            "0 0 0 setrgbcolor 7 srand newpath 10 20 moveto 310 20 lineto \
             << /Width 14 /Bristles 24 /Load {load_v} /Dropout {dropout_v} >> pkdry"
        ))
        .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
        ink_count(&it)
    }

    let loaded = render(0.95, 0.05);
    let very_dry = render(0.1, 0.9);
    assert!(
        loaded > very_dry * 2,
        "a loaded brush should cover substantially more of the stroke \
         than a very-dry one: loaded {loaded} very_dry {very_dry}"
    );
}

#[test]
fn dry_seeded_render_is_deterministic() {
    fn render(seed: i64) -> Vec<u8> {
        let mut it = fresh(320, 40);
        it.run_str(&format!(
            "0 0 0 setrgbcolor {seed} srand newpath 10 20 moveto 310 20 lineto \
             << /Width 14 /Bristles 20 /Load 0.6 /Dropout 0.4 >> pkdry"
        ))
        .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
        it.gfx().pixmap.data().to_vec()
    }

    let a = render(11);
    let b = render(11);
    assert_eq!(a, b, "same seed, same opts -> identical pixels");

    let other = render(12);
    assert_ne!(a, other, "a different seed should render differently");
}

#[test]
fn dry_spread_widens_the_inked_band_around_the_centerline() {
    // The issue names "bristle count and spread" as required
    // configurables, and "coverage breakup follows the path rather
    // than looking like unrelated page noise" as an acceptance
    // criterion. A narrow /Spread should keep ink confined close to
    // the centerline (a real signal of "follows the path", not
    // scattered noise); a wide /Spread should visibly widen that band.
    // High /Load and low /Dropout keep both renders solidly covered so
    // the measured column height reflects /Spread, not Markov holes. A
    // small, explicit /BristleWidth keeps each individual bristle's own
    // mark from itself dominating the measured band height (the default
    // BristleWidth, Width*0.12, is wide enough at Width 40 to swamp a
    // narrow /Spread's own contribution).
    fn inked_band_height(spread: f64) -> u32 {
        let mut it = fresh(220, 100);
        it.run_str(&format!(
            "0 0 0 setrgbcolor 6 srand newpath 10 50 moveto 210 50 lineto \
             << /Width 40 /BristleWidth 1 /Bristles 40 /Spread {spread} \
                /Load 0.97 /Dropout 0.03 >> pkdry"
        ))
        .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
        column_height(&it, 110, 100)
    }

    let narrow = inked_band_height(0.15);
    let wide = inked_band_height(1.0);
    assert!(
        wide > narrow * 2,
        "a wide /Spread should visibly widen the inked band around the \
         centerline compared to a narrow one: narrow {narrow} wide {wide}"
    );
    assert!(
        (narrow as f64) < 40.0 * 0.15 * 2.0 + 3.0,
        "a narrow /Spread should keep ink close to the centerline, not \
         scattered across the full envelope: narrow {narrow}"
    );
}

#[test]
fn dry_multiple_subpaths_each_get_their_own_bristle_scatter() {
    let mut it = fresh(220, 220);
    it.run_str(
        "0 0 0 setrgbcolor 3 srand \
         newpath 10 10 moveto 100 10 lineto \
         30 60 moveto 90 60 lineto 90 120 lineto 30 120 lineto closepath \
         << /Width 10 /Bristles 20 /Load 0.9 /Dropout 0.1 >> pkdry",
    )
    .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    assert!(ink_count(&it) > 100, "expected ink from both subpaths");
}

#[test]
fn dry_empty_path_is_a_no_op() {
    let mut it = fresh(60, 60);
    it.run_str("0 0 0 setrgbcolor newpath << /Width 12 >> pkdry")
        .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    assert_eq!(ink_count(&it), 0, "empty path should draw nothing");
}

#[test]
fn dry_degenerate_single_point_scatters_isotropically_not_in_a_line() {
    // Regression test for the fix documented in pkdry's own header: a
    // bare moveto has no real direction of travel (walkpath's ang=0 is
    // synthetic), so bristles must not fan out perpendicular to that
    // arbitrary angle (which would draw a straight vertical line of
    // dots) -- they scatter isotropically around the point instead.
    // /Load 1 /Dropout 0 forces every bristle to mark (the initial-
    // contact roll uses raw, unscaled /Load -- see the header comment
    // above pbdrstate's definition in lib/paintkit.ps), so the ink's
    // own bounding box should be a genuine 2D cluster, not a 1D line:
    // both its width and height should be a substantial fraction of
    // the configured scatter width, not one collapsed near zero.
    let mut it = fresh(120, 120);
    it.run_str(
        "1 0 0 setrgbcolor 6 srand newpath 60 60 moveto \
         << /Width 40 /BristleWidth 4 /Bristles 30 \
            /Load 1 /Dropout 0 >> pkdry",
    )
    .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));

    let (mut min_x, mut max_x, mut min_y, mut max_y) = (120u32, 0u32, 120u32, 0u32);
    let mut any = false;
    for y in 0..120u32 {
        for x in 0..120u32 {
            let p = it.gfx().pixmap.pixel(x, y).expect("in bounds");
            if luma(p) < 180.0 {
                any = true;
                min_x = min_x.min(x);
                max_x = max_x.max(x);
                min_y = min_y.min(y);
                max_y = max_y.max(y);
            }
        }
    }
    assert!(any, "expected some ink around the point");
    let (w, h) = (max_x - min_x, max_y - min_y);
    assert!(
        w > 10 && h > 10,
        "expected a 2D scatter cluster, not a line: bounding box {w}x{h}"
    );
}

#[test]
fn dry_color_jitter_varies_per_bristle_but_zero_stays_uniform() {
    // Counting distinct RGB triples directly is too noisy: with 24
    // overlapping bristles there are many internal edge-to-edge seams,
    // each contributing its own partial-coverage antialiased blend
    // regardless of /ColorJitter, not just the outer envelope edge.
    // Variance of the red channel among solidly-inked pixels is robust
    // to that geometric AA noise (present, roughly equally, in both
    // renders) while still picking up the much larger spread
    // /ColorJitter itself adds.
    fn red_variance(color_jitter: f64, seed: i64) -> f64 {
        let mut it = fresh(220, 60);
        it.run_str(&format!(
            "0.2 0.3 0.8 setrgbcolor {seed} srand \
             newpath 10 30 moveto 210 30 lineto \
             << /Width 20 /Bristles 24 /Load 0.95 /Dropout 0.05 \
                /ColorJitter {color_jitter} >> pkdry"
        ))
        .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
        // Strict threshold, well under the test color's own solid-fill
        // luma (~82 for 0.2/0.3/0.8), to favor solidly-covered pixels
        // over antialiased edges.
        let reds: Vec<f64> = it
            .gfx()
            .pixmap
            .pixels()
            .iter()
            .filter(|&&p| luma(p) < 100.0)
            .map(|p| p.red() as f64)
            .collect();
        assert!(!reds.is_empty(), "expected some solidly-inked pixels");
        let mean = reds.iter().sum::<f64>() / reds.len() as f64;
        reds.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / reds.len() as f64
    }

    let uniform_var = red_variance(0.0, 9);
    let varied_var = red_variance(0.3, 9);
    assert!(
        varied_var > uniform_var * 3.0 && varied_var > 1.0,
        "ColorJitter > 0 should spread the red channel noticeably more \
         than ColorJitter 0's own antialiasing noise: uniform_var \
         {uniform_var} varied_var {varied_var}"
    );
}

#[test]
fn dry_restores_the_callers_current_color_before_returning() {
    // pkdry sets a jittered color per bristle while it draws -- the
    // caller's own color must not leak the last bristle's variation
    // once pkdry returns (color is deliberately not a settable key
    // here, same doctrine as pkribbon, so it must not be an ambient
    // side effect either).
    let mut it = Interp::new();
    load(&mut it);
    it.run_str(
        "0.4 0.5 0.6 setrgbcolor 2 srand \
         newpath 10 10 moveto 100 10 lineto \
         << /Width 10 /Bristles 20 /ColorJitter 0.3 >> pkdry",
    )
    .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    let (r, g, b) = it.gfx().rgb();
    assert!(
        (r - 0.4).abs() < 1e-6 && (g - 0.5).abs() < 1e-6 && (b - 0.6).abs() < 1e-6,
        "expected the caller's own color restored, got ({r}, {g}, {b})"
    );
}

#[test]
fn dry_widthjitter_changes_the_render_relative_to_no_jitter() {
    fn render(width_jitter: f64) -> Vec<u8> {
        let mut it = fresh(220, 60);
        it.run_str(&format!(
            "0 0 0 setrgbcolor 4 srand newpath 10 30 moveto 210 30 lineto \
             << /Width 20 /Bristles 24 /Load 0.95 /Dropout 0.05 \
                /WidthJitter {width_jitter} >> pkdry"
        ))
        .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
        it.gfx().pixmap.data().to_vec()
    }

    let none = render(0.0);
    let jittered = render(0.8);
    assert_ne!(
        none, jittered,
        "WidthJitter should visibly change the render, not be ignored"
    );
}

#[test]
fn dry_dryness_reads_similarly_across_different_pitch_values() {
    // Regression test for the /Pitch-coupling fix: /Load and /Dropout
    // are a rate per one /Width of travel, scaled by (Pitch/Width) into
    // a per-sample transition probability.
    //
    // Total *ink fraction* is the wrong thing to assert on here: for a
    // two-state Markov chain the stationary ink fraction is a/(a+b)
    // (a = on-rate, b = off-rate), which is unchanged by scaling both
    // rates by the same factor -- it would pass identically whether or
    // not the /Pitch scaling is applied, so an ink_count-based assertion
    // can't actually detect a regression here. What the scaling protects
    // is *mean run length in user-space units*: without it, run length
    // is pitch/a (grows/shrinks with /Pitch); with it, Width/a (pitch-
    // independent). So the number of separate ink runs a fixed stroke
    // length produces is the real discriminator -- a finer /Pitch
    // without the fix produces visibly *more, shorter* runs. Counted
    // here as white-to-ink transitions along each pixel row.
    //
    // A single bristle (/Bristles 1) and a thin /BristleWidth: with many
    // overlapping bristles, one bristle's gap is frequently covered by a
    // neighbor's ink at a slightly different offset, and a wide
    // /BristleWidth's own round caps can visually bridge a short gap
    // even for one bristle -- both mask the per-bristle run-length
    // signal this test needs at the pixel level. Confirmed empirically:
    // an earlier 24-bristle, default-/BristleWidth version of this test
    // still passed after deliberately reverting the (Pitch/Width)
    // scaling in lib/paintkit.ps (using raw pbload/pbdropout as
    // pbdronrate/pbdroffrate directly), i.e. it couldn't detect the
    // regression it was meant to catch. One thin bristle removes both
    // sources of masking; with that reverted scaling, the fine-pitch
    // transition count came out ~3-4x the coarse-pitch count, well
    // outside the ratio bound below. Restoring the scaling (the actual
    // shipped code) brings it back within bounds.
    fn transition_count(pitch: f64) -> usize {
        let mut it = fresh(320, 40);
        it.run_str(&format!(
            "0 0 0 setrgbcolor 8 srand newpath 10 20 moveto 310 20 lineto \
             << /Width 14 /BristleWidth 0.6 /Bristles 1 \
                /Load 0.6 /Dropout 0.35 /Pitch {pitch} >> pkdry"
        ))
        .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
        let mut count = 0usize;
        for y in 0..40u32 {
            let mut prev_ink = false;
            for x in 0..320u32 {
                let p = it.gfx().pixmap.pixel(x, y).expect("in bounds");
                let ink = luma(p) < 180.0;
                if ink && !prev_ink {
                    count += 1;
                }
                prev_ink = ink;
            }
        }
        count
    }

    let fine = transition_count(1.0);
    let coarse = transition_count(4.0);
    assert!(
        fine > 0 && coarse > 0,
        "expected some ink runs at both pitches"
    );
    let ratio = fine.max(coarse) as f64 / fine.min(coarse) as f64;
    assert!(
        ratio < 2.0,
        "ink run count should stay in the same ballpark across /Pitch \
         values, not swing by multiples: fine {fine} coarse {coarse} \
         ratio {ratio}"
    );
}

#[test]
fn ghostscript_accepts_paintkit_dry() {
    let gs_ok = std::process::Command::new("gs")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success());
    if !gs_ok {
        eprintln!("skipping gs compatibility check: gs not installed");
        return;
    }
    let artkit = std::fs::read_to_string("lib/artkit.ps").expect("artkit");
    let paintkit = std::fs::read_to_string("lib/paintkit.ps").expect("paintkit");
    let driver = "true setpacking 3 srand \
        0.1 0.1 0.1 setrgbcolor \
        newpath 10 10 moveto 150 10 lineto \
            << /Width 10 /Bristles 20 /Load 0.8 /Dropout 0.2 >> pkdry \
        newpath 10 40 moveto 90 40 lineto 90 80 lineto 10 80 lineto closepath \
            << /Width 8 /Bristles 16 /Load 0.4 /Dropout 0.6 \
               /ColorJitter 0.1 >> pkdry \
        newpath 220 10 moveto 240 60 260 60 280 10 curveto \
            << /Width 10 /Bristles 12 /Jitter 1.5 >> pkdry \
        newpath 300 30 moveto << /Width 10 /Bristles 10 >> pkdry";
    let dir = std::env::temp_dir().join(format!("pscat-paintkit-dry-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let combined = dir.join("paintkit_dry_gs.ps");
    std::fs::write(
        &combined,
        format!("{artkit}\n{paintkit}\n{driver}\nshowpage\n"),
    )
    .expect("write");
    let status = std::process::Command::new("gs")
        .args([
            "-dNOPAUSE",
            "-dBATCH",
            "-q",
            "-sDEVICE=png16m",
            "-g400x120",
            "-r72",
            "-o/dev/null",
        ])
        .arg(&combined)
        .status()
        .expect("run gs");
    assert!(status.success(), "gs rejected paintkit's pkdry");
}

#[test]
fn ghostscript_accepts_the_actual_dry_demo_file() {
    let gs_ok = std::process::Command::new("gs")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success());
    if !gs_ok {
        eprintln!("skipping gs compatibility check: gs not installed");
        return;
    }
    let status = std::process::Command::new("gs")
        .args([
            "-dNOSAFER",
            "-dNOPAUSE",
            "-dBATCH",
            "-q",
            "-sDEVICE=png16m",
            "-g620x760",
            "-r72",
            "-o/dev/null",
            "examples/paintkit_dry_demo.ps",
        ])
        .status()
        .expect("run gs");
    assert!(
        status.success(),
        "gs rejected examples/paintkit_dry_demo.ps"
    );
}

#[test]
fn ghostscript_accepts_the_actual_nib_demo_file() {
    // ghostscript_accepts_paintkit_nib above exercises pknib itself
    // through a synthetic driver, but the acceptance criterion is that
    // the *example* -- what a human actually runs -- works unchanged in
    // both interpreters, and the demo additionally exercises artkit's
    // `pal`, `findfont`/`show`, and its own local helper procs, none of
    // which the synthetic driver touches. Run the real file directly.
    // `-dNOSAFER` is required because the demo does `(lib/artkit.ps)
    // run` from disk, which gs's default sandbox blocks -- fine here,
    // the file is repo-owned, not untrusted input.
    let gs_ok = std::process::Command::new("gs")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success());
    if !gs_ok {
        eprintln!("skipping gs compatibility check: gs not installed");
        return;
    }
    let status = std::process::Command::new("gs")
        .args([
            "-dNOSAFER",
            "-dNOPAUSE",
            "-dBATCH",
            "-q",
            "-sDEVICE=png16m",
            "-g620x760",
            "-r72",
            "-o/dev/null",
            "examples/paintkit_nib_demo.ps",
        ])
        .status()
        .expect("run gs");
    assert!(
        status.success(),
        "gs rejected examples/paintkit_nib_demo.ps"
    );
}

// --- pkspray (issue #44): seeded particle spray around the centerline --

#[test]
fn spray_loads_clean() {
    let it = fresh(50, 50);
    assert_eq!(ink_count(&it), 0, "paintkit drew on load");
}

#[test]
fn spray_validation_guards_reject_bad_values() {
    // Same convention as pkribbon/pknib/pkdry: every documented range
    // constraint gets a self-documenting undefined-name error, and
    // executable values are rejected outright (no proc-valued options
    // exist here, so `load xcheck` is the whole guard -- the same
    // shape as pkribbon's /Width check).
    let mut it = Interp::new();
    load(&mut it);
    let cases = [
        (
            "newpath 0 0 moveto 100 0 lineto << /Nozzle 0 >> pkspray",
            "pkspray-nozzle-must-be-positive",
        ),
        (
            "newpath 0 0 moveto 100 0 lineto << /Nozzle -3 >> pkspray",
            "pkspray-nozzle-must-be-positive",
        ),
        (
            "newpath 0 0 moveto 100 0 lineto << /Nozzle { 8 } >> pkspray",
            "pkspray-nozzle-must-not-be-a-procedure",
        ),
        (
            "newpath 0 0 moveto 100 0 lineto << /Density -1 >> pkspray",
            "pkspray-density-must-be-non-negative",
        ),
        (
            "newpath 0 0 moveto 100 0 lineto << /Falloff -0.1 >> pkspray",
            "pkspray-falloff-must-be-0-to-1",
        ),
        (
            "newpath 0 0 moveto 100 0 lineto << /Falloff 1.1 >> pkspray",
            "pkspray-falloff-must-be-0-to-1",
        ),
        (
            "newpath 0 0 moveto 100 0 lineto << /Overspray -0.5 >> pkspray",
            "pkspray-overspray-must-be-0-to-1",
        ),
        (
            "newpath 0 0 moveto 100 0 lineto << /Overspray 2 >> pkspray",
            "pkspray-overspray-must-be-0-to-1",
        ),
        (
            "newpath 0 0 moveto 100 0 lineto << /Speck 0 >> pkspray",
            "pkspray-speck-must-be-positive",
        ),
        (
            "newpath 0 0 moveto 100 0 lineto << /Speckle -0.1 >> pkspray",
            "pkspray-speckle-must-be-0-to-1",
        ),
        (
            "newpath 0 0 moveto 100 0 lineto << /Speckle 1.5 >> pkspray",
            "pkspray-speckle-must-be-0-to-1",
        ),
        (
            "newpath 0 0 moveto 100 0 lineto << /Pitch 0 >> pkspray",
            "pkspray-pitch-must-be-positive",
        ),
        (
            "newpath 0 0 moveto 100 0 lineto << /StartBurst -0.1 >> pkspray",
            "pkspray-startburst-must-be-0-to-1",
        ),
        (
            "newpath 0 0 moveto 100 0 lineto << /StartBurst 1.2 >> pkspray",
            "pkspray-startburst-must-be-0-to-1",
        ),
        (
            "newpath 0 0 moveto 100 0 lineto << /EndBurst -1 >> pkspray",
            "pkspray-endburst-must-be-0-to-1",
        ),
        (
            "newpath 0 0 moveto 100 0 lineto << /EndBurst 7 >> pkspray",
            "pkspray-endburst-must-be-0-to-1",
        ),
        // Validation must run even when the path is empty (the PR #77
        // lesson): the malformed dict errors the same way either way.
        ("<< /Nozzle 0 >> pkspray", "pkspray-nozzle-must-be-positive"),
    ];
    for (src, want) in cases {
        let err = it.run_str(src).unwrap_err();
        assert!(
            matches!(err, PsError::Undefined(ref n) if n == want),
            "{src}: expected {want}, got {err}"
        );
    }
}

#[test]
fn spray_same_seed_renders_identically_and_different_seed_differs() {
    fn render(seed: u32) -> Vec<u8> {
        let mut it = fresh(300, 60);
        it.run_str(&format!(
            "0.2 0.2 0.2 setrgbcolor {seed} srand \
             newpath 10 30 moveto 290 30 lineto \
             << /Nozzle 12 /Density 40 >> pkspray"
        ))
        .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
        it.gfx().pixmap.data().to_vec()
    }
    let a = render(9);
    let b = render(9);
    assert_eq!(a, b, "same seed and options must render identically");
    let c = render(10);
    assert_ne!(a, c, "different seeds should move particles");
}

#[test]
fn spray_falloff_concentrates_ink_near_the_centerline() {
    // The acceptance criterion: particle density falls off predictably
    // from the gesture centerline. Fraction of ink within a narrow band
    // around the centerline must be clearly higher at /Falloff 1
    // (quadratic thinning outward) than at /Falloff 0 (uniform in
    // radius), same seed and everything else.
    fn band_fraction(falloff: f64) -> f64 {
        let mut it = fresh(300, 80);
        it.run_str(&format!(
            "0 0 0 setrgbcolor 17 srand \
             newpath 20 40 moveto 280 40 lineto \
             << /Nozzle 24 /Density 60 /Speckle 0.15 /Falloff {falloff} >> pkspray"
        ))
        .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
        let mut in_band = 0usize;
        let mut total = 0usize;
        for y in 8..72u32 {
            for x in 16..284u32 {
                let p = it.gfx().pixmap.pixel(x, y).expect("in bounds");
                if luma(p) < 180.0 {
                    total += 1;
                    if (y as i32 - 40).abs() <= 6 {
                        in_band += 1;
                    }
                }
            }
        }
        assert!(
            total > 200,
            "expected real ink coverage at falloff {falloff}"
        );
        in_band as f64 / total as f64
    }

    let low = band_fraction(0.0);
    let high = band_fraction(1.0);
    assert!(
        high > low * 1.15,
        "falloff should concentrate ink near the centerline: low {low:.3} high {high:.3}"
    );
}

#[test]
fn spray_overspray_extends_mist_past_the_nozzle_edge() {
    // With a generous nozzle the discriminator is wide: interior
    // particles stop at Nozzle (+ speck/2 + antialiasing slack), while
    // /Overspray 1 scatters uniformly out to 2*Nozzle. Baseline must
    // stay inside the nozzle; oversprayed must reach clearly past it.
    fn max_offset(overspray: f64) -> i32 {
        let mut it = fresh(240, 160);
        it.run_str(&format!(
            "0 0 0 setrgbcolor 23 srand \
             newpath 30 80 moveto 210 80 lineto \
             << /Nozzle 30 /Density 45 /Speck 1.4 /Overspray {overspray} >> pkspray"
        ))
        .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
        let mut max_off = 0i32;
        for y in 0..160u32 {
            for x in 34..206u32 {
                let p = it.gfx().pixmap.pixel(x, y).expect("in bounds");
                if luma(p) < 180.0 {
                    max_off = max_off.max((y as i32 - 80).abs());
                }
            }
        }
        max_off
    }
    let base = max_offset(0.0);
    let mist = max_offset(1.0);
    assert!(
        base <= 35,
        "overspray 0 should keep all ink within the nozzle (+speck/AA slack), got {base}"
    );
    assert!(
        mist >= 40,
        "overspray 1 should throw mist well past the nozzle edge, got {mist}"
    );
}

#[test]
fn spray_end_burst_pools_ink_at_the_stroke_end() {
    fn tail_ink(end_burst: f64) -> usize {
        let mut it = fresh(260, 120);
        it.run_str(&format!(
            "0 0 0 setrgbcolor 31 srand \
             newpath 20 60 moveto 180 60 lineto \
             << /Nozzle 12 /Density 26 /EndBurst {end_burst} >> pkspray"
        ))
        .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
        let mut count = 0usize;
        for y in 0..120u32 {
            for x in 150..259u32 {
                let p = it.gfx().pixmap.pixel(x, y).expect("in bounds");
                if luma(p) < 180.0 {
                    count += 1;
                }
            }
        }
        count
    }
    let plain = tail_ink(0.0);
    let burst = tail_ink(1.0);
    assert!(
        burst > plain * 3 / 2,
        "end burst should pool extra ink past the stroke end: plain {plain} burst {burst}"
    );
}

#[test]
fn spray_empty_path_is_a_noop() {
    let mut it = fresh(50, 50);
    it.run_str("<< /Nozzle 12 /Density 40 /StartBurst 1 /EndBurst 1 >> pkspray")
        .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    assert_eq!(ink_count(&it), 0, "empty path drew something");
}

#[test]
fn spray_single_point_deposits_a_dab_cluster() {
    // A bare moveto has zero-length sp, so without the degenerate-dab
    // fallback the emission accumulator would deposit nothing at all.
    // With it, the point reads as a pressed-down spray dot: a genuine
    // 2D cluster, not a collapsed line.
    let mut it = fresh(140, 140);
    it.run_str(
        "0 0 0 setrgbcolor 41 srand newpath 70 70 moveto \
         << /Nozzle 18 /Density 40 >> pkspray",
    )
    .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));

    let (mut min_x, mut max_x, mut min_y, mut max_y) = (140u32, 0u32, 140u32, 0u32);
    let mut any = false;
    for y in 0..140u32 {
        for x in 0..140u32 {
            let p = it.gfx().pixmap.pixel(x, y).expect("in bounds");
            if luma(p) < 180.0 {
                any = true;
                min_x = min_x.min(x);
                max_x = max_x.max(x);
                min_y = min_y.min(y);
                max_y = max_y.max(y);
            }
        }
    }
    assert!(any, "expected some ink around the lone point");
    let (w, h) = (max_x - min_x, max_y - min_y);
    assert!(
        w > 8 && h > 8,
        "expected a 2D dab cluster, not a line: bounding box {w}x{h}"
    );
}

#[test]
fn spray_deposit_budget_guard_rejects_uncapped_density_dabs() {
    // Regression test for a review finding on this very feature's
    // plan: the budget estimate originally counted only accumulated
    // per-stop emissions (+ truncation spares + bursts). A degenerate
    // single-point stop reports sp=0, so its dab of
    // truncate(Density*2) particles was invisible to the estimate --
    // and /Density is uncapped, so a page of bare movetos with huge
    // /Density slipped arbitrarily many deposits past the limit.
    // The dab count now goes into the estimate explicitly.
    let mut it = Interp::new();
    load(&mut it);
    let err = it
        .run_str(
            "newpath 1 1 30 { pop 10 10 moveto } for \
             << /Density 100000 >> pkspray",
        )
        .unwrap_err();
    assert!(
        matches!(err, PsError::Undefined(ref n) if n == "pkspray-deposit-count-exceeds-safety-limit"),
        "got {err}"
    );
}

#[test]
fn spray_deposit_budget_guard_rejects_quickly_even_on_a_huge_path() {
    // The check runs inside the counting callback (every stop adds at
    // least 1 spare to the estimate), so a pathological fine /Pitch on
    // a long path is rejected within ~budget-many callbacks instead of
    // walking the whole path first -- same placement argument as
    // pkdry's own quick-reject test above.
    let mut it = Interp::new();
    load(&mut it);
    let err = it
        .run_str(
            "newpath 0 0 moveto 5000000 0 lineto \
             << /Density 200000 /Pitch 0.01 >> pkspray",
        )
        .unwrap_err();
    assert!(
        matches!(err, PsError::Undefined(ref n) if n == "pkspray-deposit-count-exceeds-safety-limit"),
        "got {err}"
    );
}

#[test]
fn spray_multiple_subpaths_each_get_their_own_scatter() {
    // walkpath's stack-discipline trap (a callback that leaves operands
    // behind corrupts wkend across subpath boundaries) only ever bites
    // with more than one subpath -- every sibling brush carries a
    // multi-subpath regression test for exactly that.
    let mut it = fresh(320, 150);
    it.run_str(
        "0 0 0 setrgbcolor 13 srand \
         newpath 20 30 moveto 300 30 lineto \
                 20 75 moveto 300 75 lineto \
                 20 120 moveto 300 120 lineto \
         << /Nozzle 9 /Density 40 >> pkspray",
    )
    .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    for (name, lo, hi) in [("top", 0u32, 52), ("middle", 53, 97), ("bottom", 98, 150)] {
        let mut band = 0usize;
        for y in lo..hi {
            for x in 15..305u32 {
                let p = it.gfx().pixmap.pixel(x, y).expect("in bounds");
                if luma(p) < 180.0 {
                    band += 1;
                }
            }
        }
        assert!(
            band > 100,
            "{name} subpath should carry its own scatter, got {band} ink pixels"
        );
    }
}

#[test]
fn spray_total_deposits_are_pitch_independent() {
    // The accumulator's headline claim: total particles track arc
    // length (about Density per nozzle-diameter of travel), not the
    // stop count, so the same options at different /Pitch values leave
    // comparable ink. A buggy fixed-per-stop implementation passes
    // every other test here but fails this one.
    fn ink_at_pitch(pitch: f64) -> usize {
        let mut it = fresh(320, 60);
        it.run_str(&format!(
            "0 0 0 setrgbcolor 47 srand \
             newpath 15 30 moveto 305 30 lineto \
             << /Nozzle 10 /Density 50 /Pitch {pitch} >> pkspray"
        ))
        .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
        ink_count(&it)
    }
    let fine = ink_at_pitch(1.0);
    let coarse = ink_at_pitch(4.0);
    assert!(fine > 0 && coarse > 0, "expected ink at both pitches");
    let ratio = fine.max(coarse) as f64 / fine.min(coarse) as f64;
    assert!(
        ratio < 2.0,
        "total deposits should stay in the same ballpark across /Pitch values: \
         fine {fine} coarse {coarse} ratio {ratio}"
    );
}

#[test]
fn spray_respects_the_active_clip() {
    // Stencil support is just PostScript clipping: particles are plain
    // fills inside whatever clip is active. Nothing may land outside
    // the clip even though the sprayed line crosses far past it.
    let mut it = fresh(320, 80);
    it.run_str(
        "0 0 0 setrgbcolor 19 srand \
         newpath 0 0 moveto 150 0 lineto 150 80 lineto 0 80 lineto closepath clip \
         newpath 10 40 moveto 310 40 lineto \
         << /Nozzle 12 /Density 40 >> pkspray",
    )
    .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    let mut outside = 0usize;
    for y in 0..80u32 {
        for x in 154..320u32 {
            let p = it.gfx().pixmap.pixel(x, y).expect("in bounds");
            if luma(p) < 180.0 {
                outside += 1;
            }
        }
    }
    assert_eq!(outside, 0, "ink leaked past the active clip");
    let mut inside = 0usize;
    for y in 0..80u32 {
        for x in 0..150u32 {
            let p = it.gfx().pixmap.pixel(x, y).expect("in bounds");
            if luma(p) < 180.0 {
                inside += 1;
            }
        }
    }
    assert!(inside > 100, "expected ink inside the clip, got {inside}");
}

#[test]
fn spray_closed_subpath_renders_without_error() {
    // Closed subpaths need no ring special-casing -- their stops are
    // walked once linearly and the end burst lands where the nozzle
    // lifts. Assert it renders real ink rather than erroring or
    // drawing nothing.
    let mut it = fresh(160, 160);
    it.run_str(
        "0 0 0 setrgbcolor 29 srand \
         newpath 30 30 moveto 130 30 lineto 130 130 lineto 30 130 lineto closepath \
         << /Nozzle 8 /Density 30 /EndBurst 0.6 >> pkspray",
    )
    .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    assert!(ink_count(&it) > 200, "closed subpath should leave real ink");
}

#[test]
fn ghostscript_accepts_paintkit_spray() {
    let gs_ok = std::process::Command::new("gs")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success());
    if !gs_ok {
        eprintln!("skipping gs compatibility check: gs not installed");
        return;
    }
    let artkit = std::fs::read_to_string("lib/artkit.ps").expect("artkit");
    let paintkit = std::fs::read_to_string("lib/paintkit.ps").expect("paintkit");
    let driver = "true setpacking 5 srand \
        0.1 0.1 0.1 setrgbcolor \
        newpath 10 10 moveto 150 10 lineto \
            << /Nozzle 10 /Density 30 /Overspray 0.4 /Speckle 0.5 >> pkspray \
        newpath 10 40 moveto 90 40 lineto 90 80 lineto 10 80 lineto closepath \
            << /Nozzle 9 /Density 26 /Falloff 0 /EndBurst 0.7 >> pkspray \
        newpath 220 10 moveto 240 60 260 60 280 10 curveto \
            << /Nozzle 12 /Density 24 /StartBurst 1 >> pkspray \
        newpath 300 30 moveto << /Nozzle 10 /Density 18 >> pkspray";
    let dir = std::env::temp_dir().join(format!("pscat-paintkit-spray-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let combined = dir.join("paintkit_spray_gs.ps");
    std::fs::write(
        &combined,
        format!("{artkit}\n{paintkit}\n{driver}\nshowpage\n"),
    )
    .expect("write");
    let status = std::process::Command::new("gs")
        .args([
            "-dNOPAUSE",
            "-dBATCH",
            "-q",
            "-sDEVICE=png16m",
            "-g400x120",
            "-r72",
            "-o/dev/null",
        ])
        .arg(&combined)
        .status()
        .expect("run gs");
    assert!(status.success(), "gs rejected paintkit's pkspray");
}

#[test]
fn ghostscript_accepts_the_actual_spray_demo_file() {
    // Same pattern as the nib/dry demo checks: the synthetic driver
    // exercises pkspray itself, but the demo additionally exercises
    // artkit's pal/star, charpath clipping, findfont/show, and its own
    // local helper procs -- run the real file directly. -dNOSAFER
    // because the demo loads lib files from disk.
    let gs_ok = std::process::Command::new("gs")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success());
    if !gs_ok {
        eprintln!("skipping gs compatibility check: gs not installed");
        return;
    }
    let status = std::process::Command::new("gs")
        .args([
            "-dNOSAFER",
            "-dNOPAUSE",
            "-dBATCH",
            "-q",
            "-sDEVICE=png16m",
            "-g620x760",
            "-r72",
            "-o/dev/null",
            "examples/paintkit_spray_demo.ps",
        ])
        .status()
        .expect("run gs");
    assert!(
        status.success(),
        "gs rejected examples/paintkit_spray_demo.ps"
    );
}

#[test]
fn spray_start_burst_pools_ink_at_the_stroke_start() {
    // Mirror of the end-burst test: the two burst paths are near-
    // copies, not shared code, and the end one already caught a
    // stranded-operand bug the start one shared -- pin both.
    fn head_ink(start_burst: f64) -> usize {
        let mut it = fresh(260, 120);
        it.run_str(&format!(
            "0 0 0 setrgbcolor 37 srand \
             newpath 80 60 moveto 240 60 lineto \
             << /Nozzle 12 /Density 26 /StartBurst {start_burst} >> pkspray"
        ))
        .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
        let mut count = 0usize;
        for y in 0..120u32 {
            for x in 0..110u32 {
                let p = it.gfx().pixmap.pixel(x, y).expect("in bounds");
                if luma(p) < 180.0 {
                    count += 1;
                }
            }
        }
        count
    }
    let plain = head_ink(0.0);
    let burst = head_ink(1.0);
    assert!(
        burst > plain * 3 / 2,
        "start burst should pool extra ink before the stroke starts: \
         plain {plain} burst {burst}"
    );
}

#[test]
fn spray_roll_clamps_rate_1_and_0() {
    // pzroll's documented contract: rate >= 1 is certain, rate <= 0
    // impossible -- not left to a bare `frnd rate lt`, which frnd's
    // exactly-1.0 draws (rare but real; pkdry's seed-5659 case) would
    // break at rate 1. Unit-tested directly on the clamped endpoints
    // rather than statistically through /Overspray, where the exactly-
    // 1.0 draw is far too rare to pin.
    let mut it = Interp::new();
    load(&mut it);
    it.run_str("7 srand 1 pzroll 1 pzroll 0 pzroll 0.5 pzroll pop")
        .expect("pzroll calls");
    let stack: Vec<String> = it.operand_stack().iter().map(|o| o.repr()).collect();
    assert_eq!(
        stack,
        vec!["true", "true", "false"],
        "rate 1 must be certainly true, rate 0 certainly false (0.5 consumed)"
    );
}

#[test]
fn spray_overspray_band_scales_with_the_burst_radius() {
    // Codex review, PR #83: the overspray band used to be computed from
    // the nozzle radius even for burst particles, whose base radius is
    // the bloomed Nozzle*(1+Burst*0.5) -- so with /Overspray 1 every
    // burst particle ignored the spatial bloom entirely. The band must
    // scale off the particle's own base radius: with a start burst the
    // escaped mist reaches well past the plain burst bloom.
    fn burst_reach(overspray: f64) -> i32 {
        let mut it = fresh(240, 200);
        it.run_str(&format!(
            "0 0 0 setrgbcolor 53 srand \
             newpath 120 100 moveto \
             << /Nozzle 20 /Density 30 /StartBurst 1 /Overspray {overspray} >> pkspray"
        ))
        .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
        // Chebyshev distance of the farthest ink pixel from the burst
        // center; the point path deposits nothing but dab + burst.
        let mut max_off = 0i32;
        for y in 0..200u32 {
            for x in 0..240u32 {
                let p = it.gfx().pixmap.pixel(x, y).expect("in bounds");
                if luma(p) < 180.0 {
                    let dy = (y as i32 - 100).abs();
                    let dx = (x as i32 - 120).abs();
                    max_off = max_off.max(dx.max(dy));
                }
            }
        }
        max_off
    }
    let plain = burst_reach(0.0);
    let mist = burst_reach(1.0);
    // Plain burst bloom: radius 1.5*Nozzle = 30 (+speck slack).
    assert!(
        plain <= 34,
        "overspray 0 should stay within the bloom, got {plain}"
    );
    // Oversprayed burst mist scales off the bloom: out to 2*30 = 60.
    assert!(
        mist >= 42,
        "oversprayed burst should throw mist past the plain bloom, got {mist}"
    );
}

#[test]
fn oil_renders_loaded_impasto() {
    let mut it = fresh(200, 200);
    it.run_str("0 0 0 setrgbcolor 7 srand newpath 20 100 moveto 180 100 lineto << /Width 14 /Ridges 8 >> pkoil")
        .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    // A loaded oil stroke should put ink on the centerline
    assert!(ink_count(&it) > 200, "oil should paint a solid impasto band");
}

#[test]
fn oil_determinism_fixed_seed() {
    fn render_once() -> usize {
        let mut it = fresh(200, 200);
        it.run_str("0 0 0 setrgbcolor 42 srand newpath 20 100 moveto 180 100 lineto << /Width 14 /Ridges 8 /ColorJitter 0.08 >> pkoil")
            .unwrap();
        ink_count(&it)
    }
    let a = render_once();
    let b = render_once();
    assert_eq!(a, b, "pkoil must be deterministic under fixed seed");
}

#[test]
fn oil_validation_and_safety() {
    fn err(src: &str) -> String {
        let mut it = fresh(100, 100);
        let e = it.run_str(src).unwrap_err();
        format!("{}", it.error_report(&e))
    }
    assert!(err("newpath 0 0 moveto 100 0 lineto << /Width 0 >> pkoil").contains("pkoil-width-must-be-positive"));
    assert!(err("newpath 0 0 moveto 100 0 lineto << /Ridges 0 >> pkoil").contains("pkoil-ridges-must-be-1-to-40"));
    assert!(err("newpath 0 0 moveto 100 0 lineto << /Ridges 41 >> pkoil").contains("pkoil-ridges-must-be-1-to-40"));
    assert!(err("newpath 0 0 moveto 2000 0 lineto << /Width 14 /Ridges 40 /Pitch 0.5 >> pkoil").contains("pkoil-deposit-count-exceeds-safety-limit"));
}

#[test]
fn spray_degenerate_point_honors_endpoint_bursts() {
    // Codex review, PR #83: a bare moveto reports atend==3 (both first
    // and last stop), but the dab-only branch skipped both burst
    // options entirely -- enabling either had no effect on a single-
    // point subpath. Bursts must land on top of the dab.
    fn dab_ink(end_burst: f64) -> usize {
        let mut it = fresh(160, 160);
        it.run_str(&format!(
            "0 0 0 setrgbcolor 59 srand newpath 80 80 moveto \
             << /Nozzle 12 /Density 24 /EndBurst {end_burst} >> pkspray"
        ))
        .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
        ink_count(&it)
    }
    let plain = dab_ink(0.0);
    let burst = dab_ink(1.0);
    assert!(plain > 0, "dab should mark on its own");
    assert!(
        burst > plain * 3 / 2,
        "endpoint bursts must apply to a single-point subpath: plain {plain} burst {burst}"
    );
}
