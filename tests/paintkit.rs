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

// Issue #79: a sharp cusp with a huge Width/Pitch ratio makes adjacent
// dash offsets O(Width) apart, which the Bristles*stops guard never
// sees; the geometric cusp guard in the counting pass must reject it
// before any drawing. This is the issue's literal repro.
#[test]
fn dry_cusp_with_huge_width_over_pitch_is_rejected_before_drawing() {
    let mut it = Interp::new();
    load(&mut it);
    let err = it
        .run_str(
            "newpath 0 0 moveto 1 0 lineto 1 1 lineto \
             << /Width 100000 /Bristles 2 /Pitch 1 /Load 1 /Dropout 0 >> pkdry",
        )
        .unwrap_err();
    assert!(
        matches!(err, PsError::Undefined(ref n) if n == "pkdry-cusp-resample-count-exceeds-safety-limit"),
        "got {err}"
    );
}

// The same huge Width on a smooth/straight stroke must stay accepted:
// the guard is geometric, not a static Width/Pitch ratio cap.
#[test]
fn dry_large_width_on_straight_stroke_is_not_rejected() {
    let mut it = Interp::new();
    load(&mut it);
    it.run_str(
        "newpath 0 0 moveto 100 0 lineto \
         << /Width 100000 /Bristles 2 /Pitch 1 >> pkdry",
    )
    .unwrap();
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
    assert!(
        ink_count(&it) > 200,
        "oil should paint a solid impasto band"
    );
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
        it.error_report(&e).to_string()
    }
    assert!(
        err("newpath 0 0 moveto 100 0 lineto << /Width 0 >> pkoil")
            .contains("pkoil-width-must-be-positive")
    );
    assert!(
        err("newpath 0 0 moveto 100 0 lineto << /Ridges 0 >> pkoil")
            .contains("pkoil-ridges-must-be-1-to-40")
    );
    assert!(
        err("newpath 0 0 moveto 100 0 lineto << /Ridges 41 >> pkoil")
            .contains("pkoil-ridges-must-be-1-to-40")
    );
    assert!(
        err("newpath 0 0 moveto 2000 0 lineto << /Width 14 /Ridges 40 /Pitch 0.5 >> pkoil")
            .contains("pkoil-deposit-count-exceeds-safety-limit")
    );
}

// Issue #98: /Pressure must shape the whole composite oil mark (ridges,
// highlight, shadow), not just the hidden base ribbon.
fn oil_repro(pressure: &str, path: &str) -> Interp {
    let mut it = fresh(240, 120);
    let src = format!(
        "0.8 0.2 0.1 setrgbcolor 7 srand {path} << /Width 30 /Pitch 4 /Ridges 12 \
         /RidgeSpread 0.75 /RidgeWidth 4 /WidthJitter 0 /Load 1 /Dropout 0 \
         /ColorJitter 0 /EdgePickup 0 /Highlight 0.12 /Shadow 0.1 /Jitter 0 \
         /Pressure {{ {pressure} }} >> pkoil"
    );
    it.run_str(&src)
        .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    it
}

#[test]
fn oil_bell_pressure_tapers_the_whole_composite_mark() {
    let path = "newpath 20 60 moveto 220 60 lineto";
    let flat = oil_repro("pkflat", path);
    let bell = oil_repro("pkbell", path);
    // Near the endpoint the bell profile is almost closed; layers with
    // fixed offsets/widths used to leave thick flat ends.
    let (flat_end, bell_end) = (column_height(&flat, 24, 120), column_height(&bell, 24, 120));
    assert!(
        flat_end >= 25,
        "flat stays full width at the end: {flat_end}"
    );
    assert!(bell_end <= 8, "bell must taper every layer: {bell_end}");
    // The middle is unchanged by the profile (bell peaks at 1).
    let (flat_mid, bell_mid) = (
        column_height(&flat, 120, 120),
        column_height(&bell, 120, 120),
    );
    assert!(bell_mid + 3 >= flat_mid, "mid {bell_mid} vs {flat_mid}");
    // And nothing protrudes beyond the base ribbon's own envelope.
    let mut base = fresh(240, 120);
    base.run_str("0.8 0.2 0.1 setrgbcolor newpath 20 60 moveto 220 60 lineto << /Width 30 /Pitch 4 /Pressure { pkbell } >> pkribbon")
        .unwrap();
    for x in [24, 40, 80, 120, 160, 200, 216] {
        assert!(
            column_height(&bell, x, 120) <= column_height(&base, x, 120) + 2,
            "x={x}: oil {} vs base {}",
            column_height(&bell, x, 120),
            column_height(&base, x, 120)
        );
    }
}

#[test]
fn oil_pressure_progress_is_global_across_broken_ridge_runs() {
    // Wide ridges plus heavy dropout break each ridge into short dashes.
    // A dash that restarted its own 0..1 pressure domain would bulge to
    // full width near the low-pressure ends of a bell profile; every
    // column must stay inside the base ribbon's own envelope.
    let src = |pressure: &str, kind: &str| {
        format!(
            "0.8 0.2 0.1 setrgbcolor 7 srand newpath 20 60 moveto 220 60 lineto \
             << /Width 30 /Pitch 4 /Ridges 12 /RidgeWidth 12 /WidthJitter 0 \
             /Load 0.4 /Dropout 0.8 /ColorJitter 0 /EdgePickup 0 \
             /Highlight 0.2 /Shadow 0.2 /Pressure {{ {pressure} }} >> {kind}"
        )
    };
    // Wide ridges legitimately poke past the base at full pressure, so
    // the bound is relative: the bell mark at x may be no taller than
    // the flat mark scaled by the bell profile at x.
    let render = |pressure: &str, kind: &str| {
        let mut it = fresh(240, 120);
        it.run_str(&src(pressure, kind)).unwrap();
        it
    };
    let (flat, bell) = (render("pkflat", "pkoil"), render("pkbell", "pkoil"));
    let base = render("pkbell", "pkribbon");
    for x in (22..220).step_by(2) {
        let m = column_height(&base, x, 120) as f32 / 30.0;
        let bound = column_height(&flat, x, 120) as f32 * m + 3.0;
        assert!(
            column_height(&bell, x, 120) as f32 <= bound,
            "x={x}: bell {} vs scaled flat {bound}",
            column_height(&bell, x, 120),
        );
    }
}

#[test]
fn oil_single_point_dab_keeps_its_ridges_under_any_pressure() {
    // A dab has no progress; its ridges ignore /Pressure so a zero-at-
    // start profile does not erase the pressed cluster (pre-#98 look).
    let dab = |p: &str| {
        let it = oil_repro(p, "newpath 120 60 moveto");
        ink_count(&it)
    };
    // Bell zeroes the base dot (Pressure(0)); only the ridge cluster remains.
    assert!(dab("pkflat") > dab("pkbell"));
    assert!(
        dab("pkbell") > 40,
        "ridge cluster must survive: {}",
        dab("pkbell")
    );
}

#[test]
fn ghostscript_accepts_oil_pressure() {
    let gs = std::process::Command::new("gs").arg("-v").output();
    if gs.is_err() {
        return;
    }
    let src = "(lib/artkit.ps) run (lib/paintkit.ps) run 1 1 1 setrgbcolor clippath fill \
        0.8 0.2 0.1 setrgbcolor 7 srand newpath 20 60 moveto 220 60 lineto \
        << /Width 30 /Pitch 4 /Pressure { pkbell } >> pkoil showpage";
    let dir = std::env::temp_dir().join("pscat_oil_pressure_gs.ps");
    std::fs::write(&dir, src).unwrap();
    let out = std::process::Command::new("gs")
        .args([
            "-q",
            "-dNOPAUSE",
            "-dBATCH",
            "-dNOSAFER",
            "-sDEVICE=nullpage",
        ])
        .arg(&dir)
        .output()
        .unwrap();
    let text =
        String::from_utf8_lossy(&out.stdout).to_string() + &String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success() && !text.contains("Error:"), "{text}");
}

#[test]
fn oil_pressure_runs_per_subpath() {
    let bell = oil_repro(
        "pkbell",
        "newpath 20 30 moveto 120 30 lineto 20 90 moveto 120 90 lineto",
    );
    // Second subpath restarts its own 0..1 progress.
    assert!(column_height(&bell, 24, 120) <= 16);
    assert!(column_height(&bell, 70, 120) >= 40);
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

// --- pkwash / pkpaper: the watercolor medium (issue #47) -------------
//
// Two things separate these from the presets above and drive what's
// asserted here. First, they're the only ones that depend on a pscat
// operator Ghostscript doesn't have (`setalpha`), so the *fallback*
// path — flattening each mark against white when `pkalphaok` is false
// — is a first-class code path with its own tests, not an afterthought.
// Second, their randomness comes from the section's own generator
// rather than `rand`, so /Seed reproducibility and the caller's-stream
// default are both directly checkable.

fn wash_pixmap(src: &str) -> Interp {
    let mut it = fresh(200, 200);
    it.run_str(src)
        .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    it
}

fn pixels(it: &Interp) -> Vec<u8> {
    it.gfx().pixmap.data().to_vec()
}

const BLOB: &str = "newpath 100 100 60 0 360 arc closepath";

#[test]
fn a_wash_is_translucent_not_opaque() {
    // The same blob, filled flat and washed. The wash has to land
    // strictly between the paper and the solid color, in the middle of
    // the shape where no wobble or rim reaches.
    let flat = wash_pixmap(&format!("0 0 0 setrgbcolor {BLOB} fill"));
    let washed = wash_pixmap(&format!(
        "0 0 0 setrgbcolor {BLOB} << /Alpha 0.3 /Layers 1 /Wet 0 /Bloom 0 /Seed 1 >> pkwash"
    ));
    let at = |it: &Interp| it.gfx().pixmap.pixel(100, 100).expect("pixel").red();
    assert_eq!(at(&flat), 0, "a plain fill is opaque");
    let w = at(&washed);
    assert!(w > 20 && w < 230, "wash should be translucent, got {w}");
}

#[test]
fn layers_build_up_darker() {
    let at = |n: u32| {
        let it = wash_pixmap(&format!(
            "0 0 0 setrgbcolor {BLOB} << /Alpha 0.25 /Layers {n} /Wet 0 /Bloom 0 /Seed 2 >> pkwash"
        ));
        it.gfx().pixmap.pixel(100, 100).expect("pixel").red()
    };
    let (one, two, four) = (at(1), at(2), at(4));
    assert!(two < one, "two layers darker than one: {two} vs {one}");
    assert!(four < two, "four layers darker than two: {four} vs {two}");
}

#[test]
fn the_same_seed_paints_the_same_pixels() {
    let run = |seed: u32| {
        pixels(&wash_pixmap(&format!(
            "0 0 0 setrgbcolor {BLOB} \
             << /Alpha 0.3 /Layers 3 /Wet 8 /Bloom 0.5 /Grain 0.4 /Seed {seed} >> pkwash"
        )))
    };
    assert_eq!(run(7), run(7), "same seed, same pixels");
    assert_ne!(run(7), run(8), "a different seed should differ");
}

/// Without /Seed a wash follows the caller's own `srand`, like every
/// other preset in this file — *and* it consumes only that stream, so
/// the wash's texture can't depend on how much randomness ran before
/// it beyond that one draw.
#[test]
fn without_a_seed_the_wash_follows_the_callers_srand() {
    let run = |seed: u32| {
        pixels(&wash_pixmap(&format!(
            "{seed} srand 0 0 0 setrgbcolor {BLOB} \
             << /Alpha 0.3 /Wet 8 /Bloom 0.5 >> pkwash"
        )))
    };
    assert_eq!(run(5), run(5));
    assert_ne!(run(5), run(6));
}

/// /Seed must not move the caller's own `rand` stream, which is what
/// lets an artist re-roll one wash without redrawing the rest of the
/// piece. (The obvious `rrand`/`srand` save-restore implementation
/// would pass this test but break under `--sweep-seed`; see the
/// section header in lib/paintkit.ps for why it isn't used.)
#[test]
fn a_seeded_wash_leaves_the_callers_rand_stream_alone() {
    let mut it = fresh(200, 200);
    it.run_str("3 srand rand rand rand").expect("baseline");
    let baseline: Vec<String> = it.operand_stack().iter().map(|o| o.repr()).collect();

    let mut it = fresh(200, 200);
    it.run_str(&format!(
        "3 srand rand \
         0 0 0 setrgbcolor {BLOB} << /Alpha 0.3 /Grain 0.5 /Seed 99 >> pkwash \
         rand rand"
    ))
    .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    let after: Vec<String> = it.operand_stack().iter().map(|o| o.repr()).collect();
    assert_eq!(baseline, after, "a /Seed wash consumed the caller's stream");
}

#[test]
fn bloom_darkens_the_edge_relative_to_the_middle() {
    let it = wash_pixmap(&format!(
        "0 0 0 setrgbcolor {BLOB} \
         << /Alpha 0.25 /Layers 1 /Wet 0 /Bloom 1 /BloomWidth 10 /Seed 3 >> pkwash"
    ));
    let middle = it.gfx().pixmap.pixel(100, 100).expect("pixel").red();
    // Just inside the rim, on the horizontal through the center.
    let rim = it.gfx().pixmap.pixel(46, 100).expect("pixel").red();
    assert!(
        rim < middle,
        "rim {rim} should be darker than middle {middle}"
    );
}

#[test]
fn wet_pushes_the_boundary_off_the_path() {
    // A dry wash stays inside the circle; a very wet one reaches past
    // it. Sampled a few points outside the nominal radius.
    let outside_ink = |wet: u32| {
        let it = wash_pixmap(&format!(
            "0 0 0 setrgbcolor {BLOB} \
             << /Alpha 0.6 /Layers 2 /Wet {wet} /Bloom 0 /Seed 4 >> pkwash"
        ));
        let pm = &it.gfx().pixmap;
        (0..360)
            .step_by(3)
            .filter(|deg| {
                let a = (*deg as f32).to_radians();
                let x = (100.0 + 64.0 * a.cos()) as u32;
                let y = (100.0 + 64.0 * a.sin()) as u32;
                pm.pixel(x, y).map(|p| p.red() < 240).unwrap_or(false)
            })
            .count()
    };
    assert_eq!(outside_ink(0), 0, "a dry wash stays on its path");
    assert!(outside_ink(14) > 5, "a wet wash should wander outside");
}

#[test]
fn multiply_washes_commute_and_normal_ones_do_not() {
    // Sampled in the middle of the overlap rather than over the whole
    // pixmap: an antialiased boundary pixel is a partial-coverage
    // blend whose *coverage* still depends on paint order, so
    // commutativity is a statement about the composited color, not
    // about every pixel of the raster.
    let overlap = |blend: &str, reversed: bool| {
        let a = "0.9 0.8 0.2 setrgbcolor newpath 80 100 45 0 360 arc closepath";
        let b = "0.2 0.4 0.8 setrgbcolor newpath 120 100 45 0 360 arc closepath";
        let opts = format!("<< /Alpha 0.5 /Layers 1 /Wet 0 /Bloom 0 /Blend /{blend} /Seed 6 >>");
        let (first, second) = if reversed { (b, a) } else { (a, b) };
        let it = wash_pixmap(&format!("{first} {opts} pkwash {second} {opts} pkwash"));
        let p = it.gfx().pixmap.pixel(100, 100).expect("pixel");
        (p.red(), p.green(), p.blue())
    };
    assert_eq!(overlap("Multiply", false), overlap("Multiply", true));
    assert_ne!(overlap("Normal", false), overlap("Normal", true));
}

#[test]
fn grain_adds_marks_inside_the_wash() {
    let ink = |grain: &str| {
        let it = wash_pixmap(&format!(
            "0 0 0 setrgbcolor {BLOB} \
             << /Alpha 0.2 /Layers 1 /Wet 0 /Bloom 0 /Grain {grain} /Seed 5 >> pkwash"
        ));
        ink_count(&it)
    };
    assert!(ink("0.9") > ink("0"), "granulation should darken the wash");
}

#[test]
fn an_empty_path_is_a_no_op() {
    let mut it = fresh(120, 120);
    it.run_str("0 0 0 setrgbcolor newpath << /Alpha 0.5 >> pkwash")
        .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    assert_eq!(ink_count(&it), 0, "an empty path should draw nothing");
}

#[test]
fn pkwash_rejects_malformed_options() {
    // Validation happens before the path is looked at, so an empty
    // path still has to reject a bad dict — the same trap pknib hit.
    for bad in [
        "<< /Alpha 2 >>",
        "<< /Alpha -1 >>",
        "<< /Alpha { 0.5 } >>",
        "<< /Layers 0 >>",
        "<< /Layers 9 >>",
        "<< /Wet -1 >>",
        "<< /Bloom 1.5 >>",
        "<< /BloomWidth 0 >>",
        "<< /Grain 2 >>",
        "<< /Blend /Screen >>",
        "<< /Blend { /Multiply } >>",
        "<< /Pitch 0 >>",
        "<< /Pitch -2 >>",
    ] {
        let mut it = fresh(60, 60);
        assert!(
            it.run_str(&format!("newpath {bad} pkwash")).is_err(),
            "{bad} should have been rejected"
        );
    }
}

/// A pitch fine enough to blow the boundary-sample budget must be
/// refused *before* anything is walked or drawn — the same doctrine as
/// pkdry's and pkspray's deposit budgets.
///
/// The pitch here is deliberately absurd (0.0001 on a ~1200-point
/// perimeter is 12 million stops). Counting first and checking after —
/// the shape this originally had — passes a check at `/Pitch 0.01` and
/// still hangs at this one, because the counting walk *is* the
/// unbounded work; the wall-clock bound below is what actually
/// distinguishes the two implementations (Codex review, PR #109).
#[test]
fn pkwash_bounds_its_own_work_before_doing_any_of_it() {
    let mut it = fresh(400, 400);
    let started = std::time::Instant::now();
    let err = it.run_str(
        "0 0 0 setrgbcolor newpath 200 200 190 0 360 arc closepath \
         << /Alpha 0.3 /Pitch 0.0001 >> pkwash",
    );
    let elapsed = started.elapsed();
    assert!(err.is_err(), "an unbounded sample count should be refused");
    assert_eq!(ink_count(&it), 0, "and nothing should have been drawn");
    assert!(
        elapsed < std::time::Duration::from_secs(2),
        "the budget must be arithmetic, not discovered by walking: took {elapsed:?}"
    );
}

/// The same doctrine for the granulation budget, which used to be
/// checked after the layers and the bloom had already been painted: a
/// rejected wash has to leave the canvas untouched, not half-finished.
#[test]
fn a_rejected_grain_budget_leaves_the_canvas_clean() {
    let mut it = fresh(400, 400);
    let err = it.run_str(
        "0 0 0 setrgbcolor newpath 200 200 190 0 360 arc closepath \
         << /Alpha 0.5 /Wet 4000 /Grain 1 >> pkwash",
    );
    assert!(err.is_err(), "an unbounded grain count should be refused");
    assert_eq!(
        ink_count(&it),
        0,
        "no layer or bloom should have been painted first"
    );
}

/// And for `pkpaper`, whose grain check sat after its tone fill.
#[test]
fn a_rejected_pkpaper_grain_budget_leaves_the_canvas_clean() {
    let mut it = fresh(400, 400);
    let err = it.run_str("0 0 4000 4000 << /Grain 1 /Tone [0.2 0.2 0.2] >> pkpaper");
    assert!(err.is_err(), "an unbounded grain count should be refused");
    assert_eq!(ink_count(&it), 0, "the tone fill should not have landed");
}

#[test]
fn pkpaper_lays_a_ground_and_validates() {
    let mut it = fresh(120, 120);
    it.run_str("0 0 120 120 << /Tone [0.9 0.85 0.8] /Grain 0.4 /Seed 3 >> pkpaper")
        .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    let p = it.gfx().pixmap.pixel(60, 60).expect("pixel");
    assert!(p.red() > 150 && p.red() < 250, "toned, not white or black");

    for bad in [
        "<< /Tone [0.9 0.85] >>",
        "<< /Tone 0.9 >>",
        "<< /Tone { [0.9 0.8 0.7] } >>",
        "<< /Grain 2 >>",
        "<< /Alpha -1 >>",
        "<< /Depth 3 >>",
        "<< /Fiber 2 >>",
        "<< /Blend /Screen >>",
        "<< /Blend { /Multiply } >>",
    ] {
        let mut it = fresh(60, 60);
        assert!(
            it.run_str(&format!("0 0 60 60 {bad} pkpaper")).is_err(),
            "{bad} should have been rejected"
        );
    }

    // A degenerate rectangle draws nothing rather than erroring.
    let mut it = fresh(60, 60);
    it.run_str("10 10 0 40 << >> pkpaper")
        .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    assert_eq!(ink_count(&it), 0);
}

/// The documented fallback for interpreters without `setalpha`. Forcing
/// `pkalphaok` false is exactly what a Ghostscript run does on its own,
/// so this exercises the same branch `gs` takes without needing `gs`.
#[test]
fn the_no_alpha_fallback_still_paints_a_recognizable_wash() {
    let src = format!(
        "0 0 0 setrgbcolor {BLOB} \
         << /Alpha 0.3 /Layers 3 /Wet 6 /Bloom 0.5 /Seed 8 >> pkwash"
    );
    let real = wash_pixmap(&src);
    let mut fb = fresh(200, 200);
    fb.run_str("/pkalphaok false def").expect("force fallback");
    fb.run_str(&src)
        .unwrap_or_else(|e| panic!("{}", fb.error_report(&e)));

    let mid = |it: &Interp| it.gfx().pixmap.pixel(100, 100).expect("pixel").red();
    // Same build-up in the middle: three 0.3 passes over white paper
    // flatten to the same value the compositor arrives at.
    assert!(
        mid(&real).abs_diff(mid(&fb)) <= 6,
        "fallback {} vs real {}",
        mid(&fb),
        mid(&real)
    );
    assert!(ink_count(&fb) > 0, "the fallback still paints");
}

/// What the fallback provably *cannot* do, asserted so the gap stays a
/// documented limitation rather than a surprise: paint underneath a
/// wash shows through with real alpha and is hidden without it.
#[test]
fn the_no_alpha_fallback_cannot_show_what_is_underneath() {
    let src = format!(
        "1 0 0 setrgbcolor newpath 0 0 moveto 200 0 lineto 200 200 lineto 0 200 lineto \
           closepath fill \
         0 0 1 setrgbcolor {BLOB} \
         << /Alpha 0.4 /Layers 1 /Wet 0 /Bloom 0 /Seed 9 >> pkwash"
    );
    let real = wash_pixmap(&src);
    let mut fb = fresh(200, 200);
    fb.run_str("/pkalphaok false def").expect("force fallback");
    fb.run_str(&src)
        .unwrap_or_else(|e| panic!("{}", fb.error_report(&e)));

    // Green is the channel that tells the two apart. The red ground is
    // (1,0,0) and the wash is (0,0,1) at 0.4: composited for real, the
    // green stays at the ground's 0; flattened against white instead,
    // it comes out at the paper's own 0.6.
    let green = |it: &Interp| it.gfx().pixmap.pixel(100, 100).expect("pixel").green();
    assert!(
        green(&real) < 20,
        "with real alpha the wash composites over the red ground, got {}",
        green(&real)
    );
    assert!(
        green(&fb) > 100,
        "the flattened fallback paints as if the ground were white — \
         the documented limitation; got {}",
        green(&fb)
    );
}

/// `lib/paintkit.ps` itself must still load and run under real
/// Ghostscript, alpha section included: the file is one `run`, so a
/// parse or definition-time error in the watercolor section would take
/// every other preset in it down too. What this asserts is that gs
/// *accepts and draws* the fallback, not that gs renders watercolor —
/// flattening against white is visibly wrong the moment a wash sits
/// over `pkpaper`'s ground or two washes overlap. Verified alpha output
/// goes through `--pdf` (tests/pdf.rs), not through `gs file.ps`.
#[test]
fn ghostscript_accepts_paintkit_wash_via_the_fallback() {
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
        0 0 300 200 << /Grain 0.5 /Seed 2 >> pkpaper \
        0.2 0.3 0.7 setrgbcolor \
        newpath 90 100 50 0 360 arc closepath \
        << /Alpha 0.3 /Layers 3 /Wet 7 /Bloom 0.6 /Grain 0.4 /Seed 3 >> pkwash \
        0.8 0.3 0.2 setrgbcolor \
        newpath 160 100 50 0 360 arc closepath \
        << /Alpha 0.3 /Layers 2 /Wet 5 /Blend /Multiply /Seed 4 >> pkwash";
    let dir = std::env::temp_dir().join(format!("pscat-paintkit-wash-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let combined = dir.join("paintkit_wash_gs.ps");
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
            "-g300x200",
            "-r72",
            "-o/dev/null",
        ])
        .arg(&combined)
        .status()
        .expect("run gs");
    assert!(
        status.success(),
        "gs rejected paintkit's watercolor section"
    );
}

#[test]
fn ghostscript_accepts_the_actual_wash_demo_file() {
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
            // -dNOSAFER because the demo `run`s lib/artkit.ps and
            // lib/paintkit.ps itself, same as the sibling demo-file
            // checks above.
            "-dNOSAFER",
            "-dNOPAUSE",
            "-dBATCH",
            "-q",
            "-sDEVICE=png16m",
            "-g620x820",
            "-r72",
            "-o/dev/null",
            "examples/paintkit_wash_demo.ps",
        ])
        .status()
        .expect("run gs");
    assert!(status.success(), "gs rejected the wash demo");
}

/// The watercolor section's generator is a Schrage-decomposed minimal
/// standard LCG specifically so its intermediate products stay inside
/// 32-bit signed range and the sequence is therefore identical under
/// Ghostscript, whose integers are 32-bit and whose `mul` silently
/// promotes to real on overflow. That claim is load-bearing — it's the
/// stated reason for not using `rand` — so it's pinned to the actual
/// values here rather than left as a comment. Confirmed against gs
/// 10.07.1 directly; the two extreme seeds are the wrap-around and
/// fixed-point cases the seeding step exists to handle.
#[test]
fn the_wash_generator_matches_ghostscripts_arithmetic() {
    let mut it = fresh(60, 60);
    it.run_str("1000 pwsrand 1 1 8 { pop pwrand 1000000 mul cvi } for")
        .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    let got: Vec<String> = it.operand_stack().iter().map(|o| o.repr()).collect();
    assert_eq!(
        got,
        [
            "7834", "669325", "360927", "108782", "300004", "178145", "91660", "543581"
        ]
    );

    // The most negative representable integer must reduce without
    // overflowing (`mod` before `abs`, not after), and 0 must not land
    // on the Lehmer generator's one fixed point.
    let mut it = fresh(60, 60);
    it.run_str(
        "-2147483648 pwsrand pwrand 1000000 mul cvi          0 pwsrand pwrand 1000000 mul cvi",
    )
    .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    let got: Vec<String> = it.operand_stack().iter().map(|o| o.repr()).collect();
    assert_eq!(got, ["23", "7"]);
}

/// `/Blend` must mean what it says regardless of the graphics state it
/// was called in. A caller who left `/Multiply setblendmode` in force
/// used to get a Multiply wash out of `/Blend /Normal`, because the
/// mode was only ever *set* for /Multiply and otherwise inherited
/// (Codex review, PR #109).
#[test]
fn blend_does_not_inherit_the_callers_mode() {
    let wash = |prelude: &str, blend: &str| {
        pixels(&wash_pixmap(&format!(
            "{prelude} 0.9 0.7 0.2 setrgbcolor \
             newpath 0 0 moveto 200 0 lineto 200 200 lineto 0 200 lineto closepath fill \
             0.2 0.4 0.9 setrgbcolor {BLOB} \
             << /Alpha 0.5 /Layers 1 /Wet 0 /Bloom 0 /Blend /{blend} /Seed 12 >> pkwash"
        )))
    };
    assert_eq!(
        wash("/Multiply setblendmode", "Normal"),
        wash("", "Normal"),
        "an ambient Multiply leaked into a /Normal wash"
    );
    assert_eq!(
        wash("/Normal setblendmode", "Multiply"),
        wash("", "Multiply"),
        "an ambient Normal suppressed a /Multiply wash"
    );
    assert_ne!(wash("", "Normal"), wash("", "Multiply"));
}

/// The caller's own blend mode has to survive the call, since `pkwash`
/// only borrows it inside its own gsave.
#[test]
fn a_wash_restores_the_callers_blend_mode_and_alpha() {
    let mut it = fresh(120, 120);
    it.run_str(
        "/Multiply setblendmode 0.4 setalpha \
         0 0 0 setrgbcolor newpath 60 60 30 0 360 arc closepath \
         << /Alpha 0.3 /Blend /Normal >> pkwash \
         currentblendmode currentalpha",
    )
    .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    let got: Vec<String> = it.operand_stack().iter().map(|o| o.repr()).collect();
    // 0.4 widened back out of the f32 the graphics state stores, the
    // same round-trip `currentgray` has always shown.
    assert_eq!(got, ["/Multiply", "0.4000000059604645"]);
}

/// A malformed `/Tone` must raise, not *run*. `[{ ... } 0.8 0.7]` clears
/// the array/type/length checks, and the procedure then executes the
/// moment the name it was bound to is referenced — arbitrary side
/// effects from a color option (Codex review, PR #109).
#[test]
fn pkpaper_tone_components_are_validated_individually() {
    for bad in [
        "[{ 0.9 } 0.8 0.7]",
        "[0.9 { 0.8 } 0.7]",
        "[0.9 0.8 (blue)]",
        "[0.9 0.8 [0.7]]",
    ] {
        let mut it = fresh(60, 60);
        assert!(
            it.run_str(&format!("0 0 60 60 << /Tone {bad} >> pkpaper"))
                .is_err(),
            "/Tone {bad} should have been rejected"
        );
        assert_eq!(ink_count(&it), 0, "/Tone {bad} painted before rejecting");
    }
    // An integer component is still a number, and still fine.
    let mut it = fresh(60, 60);
    it.run_str("0 0 60 60 << /Tone [1 0.8 0] >> pkpaper")
        .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
}

/// `pkpaper`'s ground is a ground: opaque and source-over whatever the
/// caller had set. /Alpha and /Blend are documented as *grain* options,
/// so an ambient `setalpha` used to make the paper itself translucent
/// with nothing in the contract saying so (Codex review, PR #109).
#[test]
fn the_paper_ground_does_not_inherit_the_callers_compositing() {
    let ground = |prelude: &str| {
        let it = wash_pixmap(&format!(
            "0 0 0 setrgbcolor newpath 0 0 moveto 200 0 lineto 200 200 lineto 0 200 lineto \
               closepath fill \
             {prelude} 0 0 200 200 << /Tone [0.9 0.85 0.8] /Grain 0 >> pkpaper"
        ));
        it.gfx().pixmap.pixel(100, 100).expect("pixel").red()
    };
    let plain = ground("");
    assert!(plain > 200, "the ground covers the black under it: {plain}");
    assert_eq!(ground("0.2 setalpha"), plain, "ambient alpha leaked in");
    assert_eq!(
        ground("/Multiply setblendmode"),
        plain,
        "ambient blend mode leaked in"
    );
}

/// The bloom rim's whole job is to be continuous, so it must not
/// inherit the caller's dash pattern — the one stroke in this section
/// where that would be visible (Codex review, PR #109).
#[test]
fn the_bloom_rim_ignores_an_inherited_dash() {
    let rim = |prelude: &str| {
        pixels(&wash_pixmap(&format!(
            "{prelude} 0 0 0 setrgbcolor {BLOB} \
             << /Alpha 0.2 /Layers 1 /Wet 0 /Bloom 1 /BloomWidth 8 /Seed 21 >> pkwash"
        )))
    };
    assert_eq!(rim("[6 6] 0 setdash"), rim(""));
}

/// Forcing `/pkalphaok false` is a request to preview what Ghostscript
/// does, and gs has no `setalpha` for an ambient value to leak out of —
/// so the preview must not inherit one either. Neutralizing therefore
/// keys off whether the operators *exist* (`pwhasalpha`), not off
/// whether this library is using them (`pkalphaok`), which are the same
/// question until someone moves the dial (Codex review, PR #109).
#[test]
fn a_forced_fallback_preview_ignores_ambient_compositing() {
    let fallback = |prelude: &str| {
        let mut it = fresh(200, 200);
        it.run_str("/pkalphaok false def").expect("force fallback");
        it.run_str(&format!(
            "{prelude} \
             0 0 200 200 << /Tone [0.9 0.85 0.8] /Grain 0.3 /Seed 2 >> pkpaper \
             0 0 0 setrgbcolor {BLOB} \
             << /Alpha 0.3 /Layers 2 /Wet 4 /Bloom 0.5 /Grain 0.3 /Seed 3 >> pkwash"
        ))
        .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
        pixels(&it)
    };
    let plain = fallback("");
    assert_eq!(fallback("0.2 setalpha"), plain, "ambient alpha leaked in");
    assert_eq!(
        fallback("/Multiply setblendmode"),
        plain,
        "ambient blend mode leaked in"
    );
    assert_eq!(
        fallback("0.2 setalpha /Multiply setblendmode"),
        plain,
        "ambient compositing leaked in"
    );
}

/// `pkpaper`'s fallback backdrop isn't a guess the way `pkwash`'s
/// white-paper assumption is — a grain mark sits directly on a ground
/// this same procedure just painted — so both blend modes have an exact
/// closed form and `/Blend` has to be honored rather than collapsed
/// into the source-over one (Codex review, PR #109, round 5).
#[test]
fn pkpaper_honors_blend_in_the_fallback_too() {
    // Rendered under a 4x CTM so each speck covers whole device pixels:
    // the darkest pixel is then exactly the mark color, which is what
    // the two blend modes disagree about. At 1x the marks are subpixel
    // and every pixel is a partial blend, which measures antialiasing
    // rather than the formula.
    let mark_color = |forced_fallback: bool, blend: &str| {
        let mut it = fresh(200, 200);
        if forced_fallback {
            it.run_str("/pkalphaok false def").expect("force fallback");
        }
        it.run_str(&format!(
            "gsave 4 4 scale 0 0 50 50 \
             << /Tone [0.8 0.7 0.6] /Grain 1 /Alpha 0.9 /Depth 0.8 /Fiber 0 \
                /Blend /{blend} /Seed 5 >> pkpaper grestore"
        ))
        .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
        it.gfx()
            .pixmap
            .pixels()
            .iter()
            .map(|p| p.red())
            .min()
            .expect("pixels")
    };

    // The closed forms, on the red channel: tone 0.8, alpha 0.9,
    // depth 0.8.
    //   Normal:   0.8*(1 - 0.8*0.9)                = 0.224 -> 57
    //   Multiply: 0.8*(1 - 0.9 + 0.9*0.8*(1-0.8))  = 0.195 -> 50
    let fb_normal = mark_color(true, "Normal");
    let fb_multiply = mark_color(true, "Multiply");
    assert!(
        fb_multiply < fb_normal,
        "fallback /Multiply should darken more than /Normal: {fb_multiply} vs {fb_normal}"
    );
    assert!(fb_normal.abs_diff(57) <= 2, "fallback Normal: {fb_normal}");
    assert!(
        fb_multiply.abs_diff(50) <= 2,
        "fallback Multiply: {fb_multiply}"
    );

    // And because the backdrop is exactly known — a mark sits on a
    // ground this same procedure just painted — the fallback lands
    // where real alpha compositing lands, for both modes.
    for blend in ["Normal", "Multiply"] {
        let real = mark_color(false, blend);
        let fallback = mark_color(true, blend);
        assert!(
            real.abs_diff(fallback) <= 2,
            "{blend}: fallback {fallback} vs real {real}"
        );
    }
}

/// Grain centers are sampled right up to the rectangle's edges, and a
/// speck reaches ~0.8 points past its center while a fiber reaches
/// several — so an inset or adjacent `pkpaper` used to scatter texture
/// onto its neighbour (Codex review, PR #109, round 5).
#[test]
fn pkpaper_keeps_its_texture_inside_its_rectangle() {
    let mut it = fresh(200, 200);
    it.run_str(
        "0 0 1 setrgbcolor newpath 0 0 moveto 200 0 lineto 200 200 lineto 0 200 lineto \
           closepath fill \
         60 60 80 80 << /Tone [0.9 0.9 0.9] /Grain 1 /Fiber 0.5 /Seed 7 >> pkpaper",
    )
    .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));

    let pm = &it.gfx().pixmap;
    // A band just outside the ground on every side must still be the
    // pure blue it was painted, untouched by any stray mark.
    let mut strays = 0;
    for d in 0..200u32 {
        for (x, y) in [(d, 55), (d, 145), (55, d), (145, d)] {
            if let Some(p) = pm.pixel(x, y)
                && (p.red(), p.green(), p.blue()) != (0, 0, 255)
            {
                strays += 1;
            }
        }
    }
    assert_eq!(strays, 0, "{strays} pixels of texture escaped the ground");

    // ...and the ground itself is genuinely textured, so the check
    // above isn't passing because nothing was drawn.
    let inside = pm.pixel(100, 100).expect("pixel");
    assert!(inside.red() > 150, "the ground was painted");
}

/// An invalid explicit `/Pitch` must be rejected before the O(length)
/// measurement traversal, not after it (Codex review, PR #109, round 5).
#[test]
fn an_invalid_explicit_pitch_is_rejected_before_walking() {
    let mut it = fresh(400, 400);
    // A path expensive enough to walk that doing so before rejecting is
    // measurable: 60,000 tiny segments.
    let started = std::time::Instant::now();
    let err = it.run_str(
        "0 0 0 setrgbcolor newpath 0 0 moveto 1 1 60000 { pop 0.002 0.002 rlineto } for \
           closepath \
         << /Alpha 0.3 /Pitch 0 >> pkwash",
    );
    let elapsed = started.elapsed();
    assert!(err.is_err(), "/Pitch 0 should be rejected");
    assert_eq!(ink_count(&it), 0);
    assert!(
        elapsed < std::time::Duration::from_millis(900),
        "rejection walked the path first: took {elapsed:?}"
    );
}

// --- pktrowel: the trowel / palette knife (issue #111) ----------------
//
// The tool's whole claim is that it is *not* a wide brush: the deposit
// is swept along the blade direction rather than the path normal, and
// there is no solid base pass, so broken coverage and the underlayer
// showing through are load-bearing rather than decorative. The tests
// below therefore measure each artistic control on its own axis --
// /Load as deposit *width*, /Coverage as fill *within* that width,
// /Viscosity as run *length*, /Scrape as lengthwise streaking -- since
// all four would otherwise read as a single "amount of ink" number and
// a regression in one could hide behind another.

/// One horizontal scanline's worth of contiguous ink runs. `/Viscosity`
/// is defined as the contact chain's *rate*, so run count along the
/// stroke is the measurement that distinguishes it from `/Coverage`
/// (which moves ink totals without necessarily moving run counts).
fn row_runs(it: &Interp, y: u32, w: u32) -> usize {
    let mut runs = 0;
    let mut prev = false;
    for x in 0..w {
        let inked = it.gfx().pixmap.pixel(x, y).is_some_and(|p| luma(p) < 180.0);
        if inked && !prev {
            runs += 1;
        }
        prev = inked;
    }
    runs
}

/// The same count down a column, i.e. *across* the blade. This is the
/// lengthwise-streak measurement: `/Scrape` narrows every lane toward
/// its own centre, so a column crossing the band meets more separate
/// bands as scrape rises.
fn column_runs(it: &Interp, x: u32, h: u32) -> usize {
    let mut runs = 0;
    let mut prev = false;
    for y in 0..h {
        let inked = it.gfx().pixmap.pixel(x, y).is_some_and(|p| luma(p) < 180.0);
        if inked && !prev {
            runs += 1;
        }
        prev = inked;
    }
    runs
}

/// Horizontal extent of anything inked -- how far the mark reaches
/// along its own direction of travel, which is what `/Drag` extends.
/// Leftmost and rightmost inked column, or None if nothing was drawn.
/// `ink_x_extent` gives only the width, which cannot tell a mark in the
/// right place from one the same size beside it.
fn ink_x_bounds(it: &Interp, w: u32, h: u32) -> Option<(u32, u32)> {
    let mut lo = u32::MAX;
    let mut hi = 0u32;
    for y in 0..h {
        for x in 0..w {
            if it.gfx().pixmap.pixel(x, y).is_some_and(|p| luma(p) < 180.0) {
                lo = lo.min(x);
                hi = hi.max(x);
            }
        }
    }
    if lo == u32::MAX { None } else { Some((lo, hi)) }
}

fn ink_x_extent(it: &Interp, w: u32, h: u32) -> u32 {
    let mut lo = u32::MAX;
    let mut hi = 0u32;
    for y in 0..h {
        for x in 0..w {
            if it.gfx().pixmap.pixel(x, y).is_some_and(|p| luma(p) < 180.0) {
                lo = lo.min(x);
                hi = hi.max(x);
            }
        }
    }
    if lo == u32::MAX { 0 } else { hi - lo }
}

const TROWEL_PATH: &str = "newpath 60 100 moveto 340 100 lineto";

fn trowel(opts: &str) -> Interp {
    let mut it = fresh(400, 200);
    it.run_str(&format!(
        "0.6 0.6 0.6 setrgbcolor 11 srand {TROWEL_PATH} << {opts} >> pktrowel"
    ))
    .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    it
}

#[test]
fn trowel_lays_a_broken_flat_blade_mark() {
    let it = trowel("/Width 60");
    assert!(
        ink_count(&it) > 3000,
        "a loaded trowel stroke should deposit a substantial mass, got {}",
        ink_count(&it)
    );
}

#[test]
fn trowel_seeded_render_is_deterministic() {
    let run = || pixels(&trowel("/Width 60 /Jitter 2 /Scrape 0.3"));
    assert_eq!(run(), run(), "pktrowel must be deterministic under a seed");
}

#[test]
fn trowel_a_different_seed_paints_a_different_mark() {
    let run = |seed: u32| {
        let mut it = fresh(400, 200);
        it.run_str(&format!(
            "0.6 0.6 0.6 setrgbcolor {seed} srand {TROWEL_PATH} \
             << /Width 60 /Jitter 2 /Scrape 0.3 >> pktrowel"
        ))
        .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
        pixels(&it)
    };
    assert_ne!(run(11), run(12), "a different seed should differ");
}

/// /Load is the *width* control: a nearly empty blade deposits a narrow
/// band, a loaded one the whole blade. Measured as band thickness, not
/// ink total, so it can't be satisfied by /Coverage's axis.
#[test]
fn trowel_load_widens_the_deposit_rather_than_filling_it() {
    let thin = column_height(
        &trowel("/Width 60 /Load 0.05 /Coverage 1 /Scrape 0 /EdgeBuildup 0"),
        200,
        200,
    );
    let full = column_height(
        &trowel("/Width 60 /Load 1 /Coverage 1 /Scrape 0 /EdgeBuildup 0"),
        200,
        200,
    );
    assert!(thin > 0, "even a nearly empty blade should mark");
    assert!(
        full > thin * 3 / 2,
        "Load must widen the deposit: thin {thin} full {full}"
    );
}

/// /Coverage is the contact chain's steady state -- holes *within* the
/// band, at an unchanged band width.
#[test]
fn trowel_coverage_fills_more_of_the_contact_area() {
    let sparse = ink_count(&trowel("/Width 60 /Coverage 0.3 /Scrape 0 /EdgeBuildup 0"));
    let dense = ink_count(&trowel("/Width 60 /Coverage 0.98 /Scrape 0 /EdgeBuildup 0"));
    assert!(
        dense > sparse * 3 / 2,
        "Coverage must fill the contact area: sparse {sparse} dense {dense}"
    );
}

/// /Viscosity is the chain's *rate*, not its steady state: low
/// viscosity chatters into many short runs, high viscosity holds paint
/// together into few long ones. Ink totals are deliberately not the
/// measurement here -- both settings share a /Coverage, so they should
/// deposit comparable amounts of ink in very different arrangements.
#[test]
fn trowel_viscosity_trades_run_count_for_run_length() {
    let runs = |visc: f64| {
        let it = trowel(&format!(
            "/Width 60 /Coverage 0.6 /Viscosity {visc} /Scrape 0 /EdgeBuildup 0"
        ));
        (90..110).map(|y| row_runs(&it, y, 400)).sum::<usize>()
    };
    let chattery = runs(0.02);
    let chunky = runs(0.98);
    assert!(
        chattery > chunky * 2,
        "low viscosity should break into many more runs: chattery {chattery} chunky {chunky}"
    );
}

/// /Scrape is static lengthwise thinning plus whole-lane kills, so it
/// both removes ink and opens streaks a column crossing the band can
/// count -- the "reveals the underlayer" behavior.
#[test]
fn trowel_scrape_opens_lengthwise_streaks() {
    let smooth = trowel("/Width 60 /Coverage 1 /Scrape 0 /EdgeBuildup 0");
    let scraped = trowel("/Width 60 /Coverage 1 /Scrape 0.9 /EdgeBuildup 0");
    assert!(
        ink_count(&scraped) < ink_count(&smooth),
        "scraping must remove paint: smooth {} scraped {}",
        ink_count(&smooth),
        ink_count(&scraped)
    );
    let smooth_bands: usize = (150..250).map(|x| column_runs(&smooth, x, 200)).sum();
    let scraped_bands: usize = (150..250).map(|x| column_runs(&scraped, x, 200)).sum();
    assert!(
        scraped_bands > smooth_bands * 2,
        "scraping must open lengthwise streaks: smooth {smooth_bands} scraped {scraped_bands}"
    );
}

/// Footprint thickness goes as cos(Angle) -- a blade held near-parallel
/// to its own travel collapses toward a sliver, which is exactly why
/// the guard stops at 80 rather than 90.
#[test]
fn trowel_angle_thins_the_footprint() {
    let square = column_height(
        &trowel("/Width 60 /Angle 0 /Coverage 1 /Scrape 0 /EdgeBuildup 0"),
        200,
        200,
    );
    let raked = column_height(
        &trowel("/Width 60 /Angle 75 /Coverage 1 /Scrape 0 /EdgeBuildup 0"),
        200,
        200,
    );
    assert!(raked > 0, "a raked blade should still mark");
    assert!(
        raked * 3 / 2 < square,
        "Angle must thin the footprint: square {square} raked {raked}"
    );
}

/// The ridges are painted in a *darker* shade than the caller's color,
/// so their presence is asserted on tone, not just on ink volume --
/// otherwise the test would pass on any extra geometry at all.
#[test]
fn trowel_edge_buildup_piles_darker_paint_at_the_blade_edges() {
    let very_dark = |it: &Interp| {
        it.gfx()
            .pixmap
            .pixels()
            .iter()
            .filter(|&&p| luma(p) < 100.0)
            .count()
    };
    let flat = trowel("/Width 60 /EdgeBuildup 0 /ColorJitter 0");
    let ridged = trowel("/Width 60 /EdgeBuildup 1 /ColorJitter 0");
    assert_eq!(
        very_dark(&flat),
        0,
        "without edge buildup nothing should be darker than the caller's color"
    );
    assert!(
        very_dark(&ridged) > 200,
        "edge buildup must lay a darker ridge, got {}",
        very_dark(&ridged)
    );
}

/// /Drag is the smear past where the blade actually lifted, so it shows
/// up as reach along the direction of travel.
#[test]
fn trowel_drag_smears_the_mark_along_its_travel() {
    let dry = ink_x_extent(&trowel("/Width 60 /Drag 0 /Coverage 1 /Scrape 0"), 400, 200);
    let smeared = ink_x_extent(&trowel("/Width 60 /Drag 1 /Coverage 1 /Scrape 0"), 400, 200);
    assert!(
        smeared > dry + 20,
        "Drag must extend the mark along travel: dry {dry} smeared {smeared}"
    );
}

#[test]
fn trowel_jitter_roughens_the_lane_boundaries() {
    let clean = pixels(&trowel("/Width 60 /Jitter 0"));
    let rough = pixels(&trowel("/Width 60 /Jitter 3"));
    assert_ne!(clean, rough, "Jitter must perturb the lane boundaries");
}

/// /Pressure multiplies the blade's half-extent over path progress, and
/// is resolved once per stop rather than once per (lane, stop) -- see
/// the comment on the collecting pass in lib/paintkit.ps.
#[test]
fn trowel_pressure_profile_shapes_the_blade_along_the_stroke() {
    let it = trowel("/Width 60 /Pressure { pkbell } /Coverage 1 /Scrape 0 /EdgeBuildup 0");
    let near_start = column_height(&it, 80, 200);
    let middle = column_height(&it, 200, 200);
    assert!(
        middle > near_start * 2,
        "pkbell should be widest in the middle: start {near_start} middle {middle}"
    );
}

/// A caller's /Pressure proc may legitimately consume randomness, so
/// calling it once per (lane, stop) instead of once per stop would make
/// lane texture depend on the pressure profile and break same-seed-
/// same-picture. This pins the call count indirectly: a stochastic
/// pressure proc still has to reproduce exactly under a fixed seed.
#[test]
fn trowel_a_stochastic_pressure_proc_still_reproduces_under_a_seed() {
    let run = || pixels(&trowel("/Width 60 /Pressure { pop 0.4 frnd 0.6 mul add }"));
    assert_eq!(
        run(),
        run(),
        "a randomness-consuming /Pressure must still be deterministic"
    );
}

/// A degenerate single-point subpath is a *pressed* knife: contact is
/// forced on and the one-stop run gets a minimum along-travel extent,
/// or the forward-and-back strip through a single point would be a
/// zero-area polygon and the dab would silently vanish.
#[test]
fn trowel_degenerate_single_point_presses_a_blade_patch() {
    let mut it = fresh(200, 200);
    it.run_str(
        "0 0 0 setrgbcolor 5 srand newpath 100 100 moveto << /Width 60 /Angle 0 >> pktrowel",
    )
    .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    assert!(
        ink_count(&it) > 400,
        "a pressed knife should leave a blade-shaped patch, got {}",
        ink_count(&it)
    );
    // Blade square across a zero-direction stop means the patch is
    // taller than it is long -- a patch, not a line.
    assert!(
        column_height(&it, 100, 200) > 30,
        "the patch should span most of the blade's length"
    );
}

/// The same minimum is what keeps low-/Viscosity chatter visible: at
/// that end of the dial most contact runs are a single stop long.
#[test]
fn trowel_extreme_chatter_still_deposits_paint() {
    let it = trowel("/Width 60 /Viscosity 0 /Coverage 0.5 /Drag 0");
    assert!(
        ink_count(&it) > 500,
        "single-stop runs must still mark, got {}",
        ink_count(&it)
    );
}

#[test]
fn trowel_multiple_subpaths_each_get_their_own_contact_chain() {
    let mut it = fresh(400, 200);
    it.run_str(
        "0 0 0 setrgbcolor 3 srand newpath 40 60 moveto 360 60 lineto \
         40 140 moveto 360 140 lineto << /Width 30 >> pktrowel",
    )
    .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    let lower: usize = (0..100)
        .map(|y| {
            (0..400)
                .filter(|&x| it.gfx().pixmap.pixel(x, y).is_some_and(|p| luma(p) < 180.0))
                .count()
        })
        .sum();
    let upper: usize = (100..200)
        .map(|y| {
            (0..400)
                .filter(|&x| it.gfx().pixmap.pixel(x, y).is_some_and(|p| luma(p) < 180.0))
                .count()
        })
        .sum();
    assert!(lower > 500, "first subpath should be painted, got {lower}");
    assert!(upper > 500, "second subpath should be painted, got {upper}");
}

#[test]
fn trowel_empty_path_is_a_no_op() {
    let mut it = fresh(120, 120);
    it.run_str("0 0 0 setrgbcolor newpath << /Width 30 >> pktrowel")
        .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    assert_eq!(ink_count(&it), 0, "an empty path should paint nothing");
}

#[test]
fn trowel_restores_the_callers_current_color_before_returning() {
    let mut it = fresh(200, 200);
    it.run_str(
        "0.2 0.4 0.8 setrgbcolor 9 srand newpath 40 100 moveto 160 100 lineto \
         << /Width 20 /ColorJitter 0.3 /EdgeBuildup 0.8 >> pktrowel",
    )
    .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    let (r, g, b) = it.gfx().rgb();
    assert!(
        (r - 0.2).abs() < 1e-6 && (g - 0.4).abs() < 1e-6 && (b - 0.8).abs() < 1e-6,
        "pktrowel must leave the caller's color alone, got {r} {g} {b}"
    );
}

/// "Produces a visibly different mark from pkribbon and pkoil" is an
/// acceptance criterion, and a specimen page cannot fail. pkoil lays a
/// solid base ribbon and puts ridges on top of it; pktrowel lays no
/// base at all, so a column crossing the band meets many separate
/// masses rather than one continuous one.
#[test]
fn trowel_and_oil_produce_structurally_different_marks() {
    let other = |preset: &str| {
        let mut it = fresh(400, 200);
        it.run_str(&format!(
            "0.6 0.6 0.6 setrgbcolor 11 srand {TROWEL_PATH} << /Width 60 >> {preset}"
        ))
        .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
        (150..250).map(|x| column_runs(&it, x, 200)).sum::<usize>()
    };
    let trowel_mark = trowel("/Width 60");
    let trowel_bands: usize = (150..250).map(|x| column_runs(&trowel_mark, x, 200)).sum();
    for preset in ["pkoil", "pkribbon"] {
        let bands = other(preset);
        assert!(
            trowel_bands > bands * 2,
            "the trowel must read as separate masses next to {preset}'s solid band: \
             {preset} {bands} trowel {trowel_bands}"
        );
    }
}

/// The header promises the number of random draws depends on (Lanes,
/// stops) alone, so that turning one control cannot silently re-roll
/// another's texture. `ptroll` is deliberately not a copy of `poroll`
/// for exactly this reason: `poroll` short-circuits its clamps and
/// draws nothing at rate <= 0 or >= 1, which at /Coverage 1 would drop
/// two draws per (lane, stop) relative to /Coverage 0.98.
///
/// Asserted from the *outside*, on the only thing that can observe it:
/// a mark placed from the caller's own stream after pktrowel returns
/// moves if and only if the number of draws consumed changed.
#[test]
fn trowel_consumes_a_fixed_number_of_random_draws() {
    let downstream_mark = |opts: &str| {
        let mut it = fresh(400, 240);
        it.run_str(&format!(
            "0.6 0.6 0.6 setrgbcolor 11 srand {TROWEL_PATH} << /Width 60 {opts} >> pktrowel \
             0 0 0 setrgbcolor newpath 20 frnd 340 mul add 20 moveto 0 12 rlineto \
             6 setlinewidth stroke"
        ))
        .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
        // Only the marker band, well clear of the stroke itself. The
        // mark sits at PostScript y 20..32, i.e. pixel rows near the
        // *bottom* of the pixmap.
        (0..400)
            .find(|&x| {
                (205..225).any(|y| it.gfx().pixmap.pixel(x, y).is_some_and(|p| luma(p) < 180.0))
            })
            .expect("marker drawn")
    };
    let baseline = downstream_mark("/Coverage 0.98");
    for opts in [
        "/Coverage 1", // rate >= 1 on the seed draw, 0 on the on->off check
        "/Coverage 0", // rate 0 on the seed draw and on the off->on check
        "/Scrape 0",   // the lane-kill draw must happen anyway
        "/Scrape 0.9",
        "/Viscosity 0", // drives the per-step rate past 1
        "/EdgeBuildup 0",
        "/Jitter 4",
    ] {
        let got = downstream_mark(&format!("/Coverage 0.98 {opts}"));
        assert_eq!(
            got, baseline,
            "{opts} changed how much of the caller's random stream pktrowel consumed"
        );
    }
}

/// Every other preset in this file pins closed-subpath behavior.
/// `pktrowel` does not treat a closed subpath specially the way
/// `pkribbon` does (concentric loops, no caps): it walks it as an open
/// path that happens to return to where it began. The per-lane trim
/// therefore leaves the join at the start point *ragged* rather than
/// seamless -- each lane starts and stops at its own point around
/// there. That is deliberate and worth knowing, but it isn't asserted:
/// how visible it is depends on the seed and the lane count, since a
/// gap only appears where every lane's trim happens to coincide.
/// What is pinned is what a caller can rely on -- the loop is walked
/// all the way round, and its interior stays clear.
#[test]
fn trowel_closed_subpath_walks_the_whole_loop() {
    let mut it = fresh(240, 240);
    // `arc` from 0 degrees starts at the rightmost point, (190, 120).
    it.run_str(
        "0 0 0 setrgbcolor 4 srand newpath 120 120 70 0 360 arc closepath \
         << /Width 22 /Coverage 1 /Scrape 0 /EdgeBuildup 0 >> pktrowel",
    )
    .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    assert!(
        ink_count(&it) > 1500,
        "a closed loop should paint all the way round, got {}",
        ink_count(&it)
    );
    let inked = |x: u32, y: u32| it.gfx().pixmap.pixel(x, y).is_some_and(|p| luma(p) < 180.0);
    let on_ring = |cx: u32, cy: u32| (0..14).any(|d| inked(cx, cy.saturating_sub(7) + d));
    // All four sides, the start point included (pixel y is flipped from
    // PostScript y, but the ring is symmetric about both axes).
    for (x, y) in [(120u32, 50u32), (120, 190), (50, 120), (190, 120)] {
        assert!(on_ring(x, y), "no ink near ({x}, {y}) -- the walk broke");
    }
    // A ring, not a disc: the middle is untouched.
    let centre_ink = (100..140)
        .flat_map(|x| (100..140).map(move |y| (x, y)))
        .filter(|&(x, y)| inked(x, y))
        .count();
    assert_eq!(centre_ink, 0, "the loop's interior should stay clear");
}

/// The file's convention (`nib_validates_opts_even_on_an_empty_path`):
/// malformed options are rejected before the path is walked, so a bad
/// dict fails the same way whether or not there is anything to paint.
#[test]
fn trowel_validates_opts_even_on_an_empty_path() {
    let mut it = fresh(100, 100);
    let e = it.run_str("newpath << /Lanes 99 >> pktrowel").unwrap_err();
    assert!(
        it.error_report(&e)
            .contains("pktrowel-lanes-must-be-1-to-40"),
        "an empty path must still validate its options, got {}",
        it.error_report(&e)
    );
}

#[test]
fn trowel_validation_and_safety() {
    fn err(src: &str) -> String {
        let mut it = fresh(100, 100);
        let e = it.run_str(src).unwrap_err();
        it.error_report(&e).to_string()
    }
    let p = "newpath 0 0 moveto 100 0 lineto";
    for (opts, want) in [
        ("/Width 0", "pktrowel-width-must-be-positive"),
        ("/Width { 3 }", "pktrowel-width-must-not-be-a-procedure"),
        ("/Pitch 0", "pktrowel-pitch-must-be-positive"),
        ("/Lanes 0", "pktrowel-lanes-must-be-1-to-40"),
        ("/Lanes 41", "pktrowel-lanes-must-be-1-to-40"),
        ("/Lanes 3.5", "pktrowel-lanes-must-be-1-to-40"),
        ("/Load 1.4", "pktrowel-load-must-be-0-to-1"),
        ("/Angle 90", "pktrowel-angle-must-be-minus80-to-80"),
        ("/Angle -90", "pktrowel-angle-must-be-minus80-to-80"),
        ("/Drag 2", "pktrowel-drag-must-be-0-to-1"),
        ("/Viscosity -0.1", "pktrowel-viscosity-must-be-0-to-1"),
        ("/Coverage 1.2", "pktrowel-coverage-must-be-0-to-1"),
        ("/Scrape 3", "pktrowel-scrape-must-be-0-to-1"),
        ("/EdgeBuildup 9", "pktrowel-edgebuildup-must-be-0-to-1"),
        ("/ColorJitter 5", "pktrowel-colorjitter-must-be-0-to-1"),
        ("/Jitter { 1 }", "pktrowel-jitter-must-not-be-a-procedure"),
        ("/Pressure 0.5", "pktrowel-pressure-must-be-a-procedure"),
    ] {
        let report = err(&format!("{p} << {opts} >> pktrowel"));
        assert!(
            report.contains(want),
            "expected {want} for {opts}, got {report}"
        );
    }
}

/// Bounded like pkoil's: Lanes * stops is checked *inside* the counting
/// walk, so a pathological path is rejected before anything is
/// allocated or drawn rather than after the whole walk completes.
#[test]
fn trowel_deposit_budget_guard_rejects_lanes_times_samples_over_the_limit() {
    let mut it = fresh(100, 100);
    let e = it
        .run_str("newpath 0 0 moveto 400000 0 lineto << /Width 20 /Lanes 40 /Pitch 0.5 >> pktrowel")
        .unwrap_err();
    assert!(
        it.error_report(&e)
            .contains("pktrowel-deposit-count-exceeds-safety-limit"),
        "expected the deposit budget guard, got {}",
        it.error_report(&e)
    );
    assert_eq!(
        ink_count(&it),
        0,
        "a rejected budget must leave the canvas clean"
    );
}

#[test]
fn ghostscript_accepts_paintkit_trowel() {
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
            "examples/paintkit_trowel_demo.ps",
        ])
        .status()
        .expect("run gs");
    assert!(
        status.success(),
        "gs rejected examples/paintkit_trowel_demo.ps"
    );
}

/// Codex review of PR #137: `ptemit` derived one drag extent from the
/// run's *first* stop and reused it at both ends. Under a varying
/// /Pressure the two ends are different widths, so `{ pktaper }` --
/// which starts at nearly zero and ends fully loaded -- made /Drag do
/// almost nothing at the exit end however high it was set.
#[test]
fn trowel_drag_smears_both_ends_under_a_varying_pressure() {
    let reach = |drag: f64| {
        let mut it = fresh(400, 200);
        it.run_str(&format!(
            "0.6 0.6 0.6 setrgbcolor 11 srand {TROWEL_PATH} \
             << /Width 60 /Pressure {{ pktaper }} /Drag {drag} /Coverage 1 /Scrape 0 >> pktrowel"
        ))
        .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
        // The *loaded* end of a taper is the far end of the stroke.
        (0..400)
            .rev()
            .find(|&x| {
                (0..200).any(|y| it.gfx().pixmap.pixel(x, y).is_some_and(|p| luma(p) < 180.0))
            })
            .expect("stroke drawn")
    };
    let dry = reach(0.0);
    let smeared = reach(1.0);
    assert!(
        smeared > dry + 15,
        "the loaded end of a tapered stroke must smear too: dry {dry} smeared {smeared}"
    );
}

/// Codex review of PR #137 found /Viscosity moving ink coverage, which
/// is precisely what separating these two controls is for. Chasing it
/// turned up *two* independent causes, and both had to be fixed:
///
/// 1. The chain's transition probabilities are `Coverage*step` and
///    `(1-Coverage)*step`, each clamped to 1 independently. Once `step`
///    is large enough for the bigger one to clamp, their ratio changes
///    and /Coverage stops being the steady state. `ptstepmax` caps the
///    step so the ratio survives.
/// 2. A run of k in-contact stops was drawn as a strip from the first
///    to the last, spanning only (k-1) pitches of the k it stands for.
///    Every run lost a pitch, which costs a chattery stroke far more
///    than a chunky one. `ptemit` now extends each end by half a pitch.
///
/// Measured at /Lanes 1 (with more lanes the 18% inter-lane overlap
/// masks it -- the union of 14 chains saturates), /Coverage 0.75:
///
/// | build                  | Viscosity 0 | Viscosity 0.95 |
/// |------------------------|-------------|----------------|
/// | before either fix      | 0.434       | 0.609          |
/// | half-pitch fix only    | 0.625       | 0.750          |
/// | both fixes             | 0.716       | 0.750          |
///
/// so the 8% bound below fails on either fix alone, not just on both
/// reverted.
#[test]
fn trowel_viscosity_does_not_drag_coverage_along_with_it() {
    // Fraction of the path that is inked, along the stroke's centre
    // line. Sampled inside the path's own span so the half-pitch
    // contact extension past each endpoint isn't counted.
    let realized = |visc: f64| {
        let mut it = fresh(400, 120);
        it.run_str(&format!(
            "0 0 0 setrgbcolor 11 srand newpath 40 60 moveto 360 60 lineto \
             << /Width 30 /Pitch 15 /Lanes 1 /Coverage 0.75 /Viscosity {visc} \
                /Scrape 0 /EdgeBuildup 0 /Jitter 0 /Drag 0 >> pktrowel"
        ))
        .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
        let inked = (45..355)
            .filter(|&x| {
                it.gfx()
                    .pixmap
                    .pixel(x, 60)
                    .is_some_and(|p| luma(p) < 180.0)
            })
            .count();
        inked as f64 / 310.0
    };
    let chattery = realized(0.0);
    let chunky = realized(0.95);
    for (name, got) in [("chattery", chattery), ("chunky", chunky)] {
        assert!(
            (got - 0.75).abs() < 0.10,
            "{name} paint realized {got:.3} coverage against the 0.75 asked for"
        );
    }
    let drift = (chattery - chunky).abs() / chunky;
    assert!(
        drift < 0.08,
        "Viscosity must not move coverage: chattery {chattery:.3} chunky {chunky:.3} \
         (drift {:.1}%)",
        drift * 100.0
    );
}

// --- pkfan: the fan brush (issue #114) -------------------------------
//
// A fan brush is `pkdry` plus one term: the bristles leave a flattened
// ferrule and *splay* apart as the stroke travels, instead of running
// as parallel tracks. `/Splay` is therefore the property most of these
// tests are about -- if it stopped working, pkfan would silently become
// a pkdry reskin and every other assertion here would still pass.

/// Width of the inked band in a single pixel column -- topmost inked
/// row to bottommost. A fan's band grows from ferrule to tip; parallel
/// bristles' does not.
fn band_span(it: &Interp, x: u32, h: u32) -> u32 {
    let rows: Vec<u32> = (0..h)
        .filter(|&y| it.gfx().pixmap.pixel(x, y).is_some_and(|p| luma(p) < 180.0))
        .collect();
    match (rows.first(), rows.last()) {
        (Some(&a), Some(&b)) => b - a,
        _ => 0,
    }
}

const FAN_PATH: &str = "newpath 50 100 moveto 350 100 lineto";

fn fan(opts: &str) -> Interp {
    let mut it = fresh(400, 200);
    it.run_str(&format!(
        "0 0 0 setrgbcolor 31 srand {FAN_PATH} << {opts} >> pkfan"
    ))
    .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    it
}

#[test]
fn fan_lays_a_bundle_of_bristles() {
    let it = fan("/Width 70");
    assert!(
        ink_count(&it) > 1500,
        "a fan stroke should deposit a bundle of bristles, got {}",
        ink_count(&it)
    );
}

/// The defining property. At /Splay 0 the bristles are parallel, so the
/// band is the same width at both ends; as /Splay rises the ferrule
/// narrows while the tip stays put, so the band opens along the stroke.
#[test]
fn fan_splay_opens_the_bundle_along_the_stroke() {
    let opening = |splay: f64| {
        let it = fan(&format!(
            "/Width 70 /Splay {splay} /Load 1 /Dropout 0 /Ragged 0 /Flick 0"
        ));
        let near = band_span(&it, 70, 200) as f64;
        let far = band_span(&it, 330, 200) as f64;
        (near, far)
    };
    let (pn, pf) = opening(0.0);
    assert!(
        (pn - pf).abs() < pf * 0.2,
        "Splay 0 must keep the bristles parallel: near {pn} far {pf}"
    );
    let (sn, sf) = opening(1.0);
    assert!(
        sf > sn * 1.8,
        "Splay 1 must open the bundle along the stroke: near {sn} far {sf}"
    );
}

/// ...and the ferrule never collapses to a point, however high /Splay
/// is. A version without that floor put every bristle on the centerline
/// at t=0 and rendered the stroke's first third as one opaque blob.
#[test]
fn fan_ferrule_keeps_its_width_at_full_splay() {
    let it = fan("/Width 70 /Splay 1 /Load 1 /Dropout 0 /Ragged 0 /Flick 0");
    let near = band_span(&it, 60, 200);
    let far = band_span(&it, 340, 200);
    assert!(
        near as f64 > far as f64 * 0.15,
        "the ferrule should stay open, not collapse: near {near} far {far}"
    );
}

/// The two mark families the acceptance criteria name. Measured as
/// contiguous ink runs down a column crossing the stroke: a feathered
/// mark is near-continuous, a separated one is many distinct bristles.
#[test]
fn fan_makes_both_feathered_and_separated_marks() {
    let gaps = |opts: &str| {
        let it = fan(opts);
        (100..300).map(|x| column_runs(&it, x, 200)).sum::<usize>()
    };
    let feathered = gaps("/Width 70 /Bristles 48 /BristleWidth 1.4 /Load 0.99 /Dropout 0.01");
    let separated = gaps("/Width 70 /Bristles 10 /BristleWidth 3 /Load 0.25 /Dropout 0.6");
    assert!(
        feathered > 0 && separated > 0,
        "both settings should mark: feathered {feathered} separated {separated}"
    );
    assert!(
        ink_count(&fan(
            "/Width 70 /Bristles 48 /BristleWidth 1.4 /Load 0.99 /Dropout 0.01"
        )) > ink_count(&fan(
            "/Width 70 /Bristles 10 /BristleWidth 3 /Load 0.25 /Dropout 0.6"
        )) * 2,
        "a loaded feathered fan should carry far more paint than a separated one"
    );
}

/// Without /Ragged every bristle stops at the same arc length and the
/// mark ends on one straight edge, which reads as a comb. With it the
/// tips feather, so the stroke's end is no longer a single column.
#[test]
fn fan_ragged_feathers_the_tips() {
    let end_spread = |ragged: f64| {
        let it = fan(&format!(
            "/Width 70 /Ragged {ragged} /Load 1 /Dropout 0 /Flick 0 /Bristles 24"
        ));
        // How many columns near the end carry *some* but not all of the
        // bristles: a straight edge has almost none.
        let full = band_span(&it, 300, 200);
        (300..380)
            .filter(|&x| {
                let s = band_span(&it, x, 200);
                s > 0 && s < full
            })
            .count()
    };
    let square = end_spread(0.0);
    let feathered = end_spread(0.8);
    assert!(
        feathered > square,
        "Ragged must feather the tips: square {square} feathered {feathered}"
    );
}

/// A degenerate single-point subpath is a *pressed* fan radiating about
/// the point. Without that branch the dab collapses: at t=0 every
/// bristle sits at the ferrule offset, which at a high /Splay is nearly
/// the centerline, so they all land on top of each other.
#[test]
fn fan_single_point_presses_a_radiating_dab() {
    let mut it = fresh(200, 200);
    it.run_str(
        "0 0 0 setrgbcolor 3 srand newpath 100 100 moveto \
         << /Width 70 /Splay 1 /Bristles 16 >> pkfan",
    )
    .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    assert!(
        ink_count(&it) > 300,
        "a pressed fan should leave a real mark, got {}",
        ink_count(&it)
    );
    // It must radiate, not stack up in a line: ink spread across a wide
    // band of columns, not concentrated in one.
    let wide = (60..140).filter(|&x| band_span(&it, x, 200) > 0).count();
    assert!(
        wide > 40,
        "the dab should radiate across the fan's width, got {wide} inked columns"
    );
}

#[test]
fn fan_is_deterministic_under_a_seed() {
    let run = || pixels(&fan("/Width 70 /Jitter 1.5 /Ragged 0.5"));
    assert_eq!(run(), run(), "pkfan must be deterministic under a seed");
}

#[test]
fn fan_multiple_subpaths_each_open_their_own_fan() {
    let mut it = fresh(400, 220);
    it.run_str(
        "0 0 0 setrgbcolor 3 srand newpath 40 60 moveto 360 60 lineto \
         40 160 moveto 360 160 lineto << /Width 40 /Splay 1 >> pkfan",
    )
    .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    for (lo, hi) in [(0u32, 110u32), (110, 220)] {
        let n: usize = (lo..hi)
            .map(|y| {
                (0..400)
                    .filter(|&x| it.gfx().pixmap.pixel(x, y).is_some_and(|p| luma(p) < 180.0))
                    .count()
            })
            .sum();
        assert!(n > 300, "subpath in rows {lo}..{hi} did not paint: {n}");
    }
}

#[test]
fn fan_empty_path_is_a_no_op() {
    let mut it = fresh(120, 120);
    it.run_str("0 0 0 setrgbcolor newpath << /Width 30 >> pkfan")
        .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    assert_eq!(ink_count(&it), 0, "an empty path should paint nothing");
}

#[test]
fn fan_restores_the_callers_color() {
    let mut it = fresh(200, 200);
    it.run_str(
        "0.2 0.4 0.8 setrgbcolor 9 srand newpath 40 100 moveto 160 100 lineto \
         << /Width 30 /ColorJitter 0.4 >> pkfan",
    )
    .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    let (r, g, b) = it.gfx().rgb();
    assert!(
        (r - 0.2).abs() < 1e-6 && (g - 0.4).abs() < 1e-6 && (b - 0.8).abs() < 1e-6,
        "pkfan must leave the caller's color alone, got {r} {g} {b}"
    );
}

#[test]
fn fan_validation_and_safety() {
    fn err(src: &str) -> String {
        let mut it = fresh(100, 100);
        let e = it.run_str(src).unwrap_err();
        it.error_report(&e).to_string()
    }
    let p = "newpath 0 0 moveto 100 0 lineto";
    for (opts, want) in [
        ("/Width 0", "pkfan-width-must-be-positive"),
        ("/Width { 3 }", "pkfan-width-must-not-be-a-procedure"),
        ("/Bristles 0", "pkfan-bristles-must-be-1-to-60"),
        ("/Bristles 61", "pkfan-bristles-must-be-1-to-60"),
        ("/Bristles 4.5", "pkfan-bristles-must-be-1-to-60"),
        ("/Spread 1.4", "pkfan-spread-must-be-0-to-1"),
        ("/Splay -0.2", "pkfan-splay-must-be-0-to-1"),
        ("/Splay 3", "pkfan-splay-must-be-0-to-1"),
        ("/BristleWidth 0", "pkfan-bristlewidth-must-be-positive"),
        ("/WidthJitter 2", "pkfan-widthjitter-must-be-0-to-1"),
        ("/Load 1.1", "pkfan-load-must-be-0-to-1"),
        ("/Dropout -1", "pkfan-dropout-must-be-0-to-1"),
        ("/Ragged 5", "pkfan-ragged-must-be-0-to-1"),
        ("/Flick 5", "pkfan-flick-must-be-0-to-1"),
        ("/Pitch 0", "pkfan-pitch-must-be-positive"),
        ("/ColorJitter 9", "pkfan-colorjitter-must-be-0-to-1"),
        ("/Jitter { 1 }", "pkfan-jitter-must-not-be-a-procedure"),
    ] {
        let report = err(&format!("{p} << {opts} >> pkfan"));
        assert!(
            report.contains(want),
            "expected {want} for {opts}, got {report}"
        );
    }
}

/// Bounded like pkdry's: Bristles * stops is checked *inside* the
/// counting walk, so a pathological path is rejected before anything is
/// allocated or drawn.
#[test]
fn fan_deposit_budget_guard_rejects_bristles_times_samples_over_the_limit() {
    let mut it = fresh(100, 100);
    let e = it
        .run_str("newpath 0 0 moveto 500000 0 lineto << /Width 20 /Bristles 60 /Pitch 0.5 >> pkfan")
        .unwrap_err();
    assert!(
        it.error_report(&e)
            .contains("pkfan-deposit-count-exceeds-safety-limit"),
        "expected the deposit budget guard, got {}",
        it.error_report(&e)
    );
    assert_eq!(
        ink_count(&it),
        0,
        "a rejected budget must leave the canvas clean"
    );
}

#[test]
fn ghostscript_accepts_paintkit_fan() {
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
            "-g620x700",
            "-r72",
            "-o/dev/null",
            "examples/paintkit_fan_demo.ps",
        ])
        .status()
        .expect("run gs");
    assert!(
        status.success(),
        "gs rejected examples/paintkit_fan_demo.ps"
    );
}

/// Codex review of PR #139: `/Load` was ignored for a pressed dab --
/// `pfdab` emitted every bristle without consulting the contact state,
/// so `<< /Load 0 >>` still painted a full fan. A dry brush pressed to
/// the page leaves nothing.
#[test]
fn fan_pressed_dab_honors_load() {
    let dab = |load: f64| {
        let mut it = fresh(200, 200);
        it.run_str(&format!(
            "0 0 0 setrgbcolor 3 srand newpath 100 100 moveto \
             << /Width 70 /Bristles 20 /Load {load} /Dropout 0 >> pkfan"
        ))
        .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
        ink_count(&it)
    };
    assert_eq!(
        dab(0.0),
        0,
        "an unloaded brush pressed to the page marks nothing"
    );
    assert!(dab(1.0) > 300, "a loaded brush pressed to the page marks");
}

/// Codex review of PR #139: the pressed-fan branch keyed off the
/// *whole path* being one stop, so a path mixing an ordinary subpath
/// with a bare `moveto` took the generic branch for the latter. walkpath
/// reports angle 0 at a degenerate stop, so those bristles came out
/// along one straight line instead of radiating.
#[test]
fn fan_a_degenerate_subpath_radiates_even_beside_a_normal_one() {
    let mut it = fresh(400, 200);
    it.run_str(
        "0 0 0 setrgbcolor 3 srand \
         newpath 40 40 moveto 200 40 lineto \
         300 120 moveto \
         << /Width 70 /Bristles 20 /Splay 1 /Load 1 /Dropout 0 >> pkfan",
    )
    .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    // The dab lives around x=300. A radiating fan spans many columns
    // there; bristles collapsed onto one line span almost none.
    let inked_cols = (255..350).filter(|&x| band_span(&it, x, 200) > 0).count();
    assert!(
        inked_cols > 40,
        "the trailing bare moveto should radiate, not collapse to a line: \
         {inked_cols} inked columns"
    );
}

/// Codex review of PR #139: a degenerate subpath contributes exactly one
/// stop to the outer safety walk however fine `/Pitch` is, so the ray's
/// own resampling was the one nested cost that budget did not bound --
/// `/Pitch 0.001` sampled a fixed-length ray millions of times while
/// passing the guard. The ray's pitch is now floored relative to its own
/// length. Asserted as a wall-clock bound, since the failure mode is a
/// hang rather than a wrong pixel.
#[test]
fn fan_a_pressed_dab_cannot_be_made_unboundedly_expensive() {
    let start = std::time::Instant::now();
    let mut it = fresh(200, 200);
    it.run_str(
        "0 0 0 setrgbcolor 3 srand newpath 100 100 moveto \
         << /Width 70 /Bristles 60 /Pitch 0.001 /Ragged 0 >> pkfan",
    )
    .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    let elapsed = start.elapsed();
    assert!(ink_count(&it) > 300, "the dab should still paint");
    assert!(
        elapsed < std::time::Duration::from_secs(5),
        "a fine /Pitch must not make a fixed-length ray unboundedly \
         expensive: took {elapsed:?}"
    );
}

/// Codex review of PR #139, round 2: `/Jitter` displaces each
/// centerline sample *before* pkribbon resamples it, so consecutive
/// samples can end up O(Jitter) apart while the outer `Bristles*stops`
/// guard still counts only the original stops. Resampling that inflated
/// path at the original pitch was nested work the budget did not bound.
/// Same class as the pressed-ray case, and asserted the same way --
/// wall clock, because the failure mode is a hang.
#[test]
fn fan_a_huge_jitter_cannot_inflate_the_nested_resampling() {
    let start = std::time::Instant::now();
    let mut it = fresh(200, 200);
    it.run_str(
        "0 0 0 setrgbcolor 3 srand newpath 100 100 moveto 101 100 lineto \
         << /Width 20 /Bristles 1 /Pitch 1 /Jitter 10000 >> pkfan",
    )
    .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    let elapsed = start.elapsed();
    assert!(
        elapsed < std::time::Duration::from_secs(5),
        "a large /Jitter must not multiply the nested resampling without \
         bound: took {elapsed:?}"
    );
}

/// Codex review of PR #139, round 2: adding pkfan's `pfroll` initially
/// rewrote pkoil's own transition calls to use it -- a file-wide rename
/// that reached past the new preset. pkoil must stay entirely inside its
/// declared `po-` namespace, or redefining one preset's helper silently
/// changes another's marks.
#[test]
fn each_preset_keeps_its_own_roll_helper() {
    let src = std::fs::read_to_string("lib/paintkit.ps").expect("read paintkit");
    // Locate pkoil's section by its dash emitter and check the whole
    // body of it refers only to poroll.
    let start = src.find("/porad {").expect("porad present");
    let end = src[start..].find("\n/pkoil ").expect("pkoil follows porad") + start;
    let porad = &src[start..end];
    assert!(
        porad.contains("poroll"),
        "pkoil's dash emitter should use poroll"
    );
    assert!(
        !porad.contains("pfroll") && !porad.contains("ptroll"),
        "pkoil's dash emitter must not reach into another preset's namespace"
    );
}

/// Codex review of PR #139, round 3: the first nested-resampling floor
/// accounted for `/Jitter` but not for the splay itself, whose offset
/// moves O(Width) between consecutive stops with no jitter at all -- so
/// a huge `/Width` at `/Splay 1` still passed the deposit guard while
/// making pkribbon resample hundreds of thousands of points. The floor
/// is now derived from the run's *measured* length rather than an
/// analytic worst case, which also keeps it from coarsening ordinary
/// strokes.
#[test]
fn fan_a_huge_width_at_full_splay_cannot_inflate_the_nested_resampling() {
    let start = std::time::Instant::now();
    let mut it = fresh(200, 200);
    it.run_str(
        "0 0 0 setrgbcolor 3 srand newpath 100 100 moveto 101 100 lineto \
         << /Width 1000000 /Bristles 2 /BristleWidth 1 /Spread 1 /Splay 1 \
            /Pitch 1 /Load 1 /Dropout 0 /Ragged 0 /Jitter 0 >> pkfan",
    )
    .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    let elapsed = start.elapsed();
    assert!(
        elapsed < std::time::Duration::from_secs(5),
        "splay displacement must be covered by the nested bound too: \
         took {elapsed:?}"
    );
}

/// ...and the floor must not engage on ordinary strokes. An analytic
/// worst-case bound would have coarsened every fan by roughly 3x; a
/// measured-length one leaves the normal case at exactly /Pitch.
///
/// Asserted on the effective pitch itself rather than on pixels (Codex
/// review of PR #139, round 4: the first version of this test compared
/// one render against an identical one, so it only retested fixed-seed
/// determinism and would have passed even if the floor coarsened
/// everything). `pfrun` leaves the pitch it actually handed pkribbon in
/// `pfrp`, and `pfPitch` is what the caller asked for, so the two can be
/// read back and compared directly.
#[test]
fn fan_the_nested_bound_engages_only_where_it_is_needed() {
    let effective_vs_requested = |opts: &str| {
        let mut it = fresh(400, 200);
        it.run_str(&format!(
            "0 0 0 setrgbcolor 31 srand {FAN_PATH} << {opts} >> pkfan pfrp pfPitch"
        ))
        .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
        let st = it.operand_stack();
        let requested: f64 = st[st.len() - 1].repr().parse().expect("pfPitch");
        let effective: f64 = st[st.len() - 2].repr().parse().expect("pfrp");
        (effective, requested)
    };

    // An ordinary fan: run length is about stops*Pitch, so the floor
    // works out to Pitch/8 and must not bind at all.
    let (eff, req) = effective_vs_requested("/Width 60 /Jitter 0 /Splay 0.6 /Pitch 1.5");
    assert!(
        (eff - req).abs() < 1e-9,
        "the bound must not coarsen an ordinary stroke: \
         effective {eff} vs requested {req}"
    );

    // A pathological one: the floor is exactly what stops pkribbon
    // resampling a wildly displaced centerline at the original pitch.
    let (peff, preq) = effective_vs_requested(
        "/Width 1000000 /Bristles 2 /BristleWidth 1 /Spread 1 /Splay 1 \
         /Pitch 1 /Load 1 /Dropout 0 /Ragged 0 /Jitter 0",
    );
    assert!(
        peff > preq * 10.0,
        "the bound must engage on a hugely displaced centerline: \
         effective {peff} vs requested {preq}"
    );
}

/// Codex review of PR #139, round 5 [P1]: a degenerate subpath becomes
/// one pkribbon call per bristle, each resampling a ray, so counting it
/// as a single stop let a path batching many bare movetos run millions
/// of nested samples while the budget counter saw a few thousand. Each
/// degenerate stop is now charged its real relative cost.
#[test]
fn fan_batched_pressed_dabs_are_charged_against_the_budget() {
    // 100 degenerate subpaths at 60 bristles: ~24s of nested work before
    // the fix, and the counter saw only 6000 of a 150000 ceiling.
    let mut movetos = String::from("newpath");
    for i in 0..100 {
        movetos.push_str(&format!(" {} {} moveto closepath", 10 + i, 50 + (i % 7)));
    }
    let mut it = fresh(200, 200);
    let start = std::time::Instant::now();
    let res = it.run_str(&format!(
        "0 0 0 setrgbcolor 3 srand {movetos} \
         << /Width 40 /Bristles 60 /Load 1 /Dropout 0 >> pkfan"
    ));
    let elapsed = start.elapsed();
    let e = res.expect_err("batched dabs at 60 bristles must exceed the budget");
    assert!(
        it.error_report(&e)
            .contains("pkfan-deposit-count-exceeds-safety-limit"),
        "expected the deposit budget guard, got {}",
        it.error_report(&e)
    );
    assert!(
        elapsed < std::time::Duration::from_secs(5),
        "the guard must reject before doing the work: took {elapsed:?}"
    );
    assert_eq!(
        ink_count(&it),
        0,
        "a rejected budget must leave the canvas clean"
    );
}

/// ...but a realistic foliage cluster -- the workload the pressed dab
/// exists for -- must still be comfortably inside the budget. A safety
/// limit that rejects the tool's own primary use case is a bug, so this
/// pins the headroom rather than only the rejection.
#[test]
fn fan_a_realistic_foliage_cluster_stays_within_the_budget() {
    let mut dabs = String::from("newpath");
    for i in 0..12 {
        dabs.push_str(&format!(
            " {} {} moveto closepath",
            30 + i * 12,
            60 + (i % 3) * 20
        ));
    }
    let mut it = fresh(240, 200);
    it.run_str(&format!(
        "0 0 0 setrgbcolor 3 srand {dabs} \
         << /Width 44 /Bristles 13 /Ragged 0.5 >> pkfan"
    ))
    .unwrap_or_else(|e| {
        panic!(
            "a 12-dab foliage cluster should be allowed: {}",
            it.error_report(&e)
        )
    });
    assert!(ink_count(&it) > 500, "the cluster should paint");
}

/// Codex review of PR #139, round 5 [P2]: the header claimed the
/// pressed fan radiates "isotropically", but the rays span roughly
/// 20..160 degrees. The *code* is right -- a fan brush pressed to the
/// page leaves a fan, and shrubs open upward -- so the documentation was
/// corrected to match. This pins the actual contract so the two can't
/// drift apart again.
#[test]
fn fan_a_pressed_dab_opens_upward_rather_than_all_round() {
    let mut it = fresh(200, 200);
    it.run_str(
        "0 0 0 setrgbcolor 3 srand newpath 100 100 moveto \
         << /Width 80 /Bristles 24 /Load 1 /Dropout 0 /Ragged 0 >> pkfan",
    )
    .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    let inked_rows = |lo: u32, hi: u32| {
        (lo..hi)
            .map(|y| {
                (0..200)
                    .filter(|&x| it.gfx().pixmap.pixel(x, y).is_some_and(|p| luma(p) < 180.0))
                    .count()
            })
            .sum::<usize>()
    };
    // PostScript y=100 is pixel row 100; the fan opens toward +y, i.e.
    // toward *lower* pixel rows.
    let above = inked_rows(0, 99);
    let below = inked_rows(101, 200);
    assert!(above > 200, "the fan should open upward, got {above}");
    assert!(
        below * 4 < above,
        "a pressed fan is a fan, not a full circle: above {above} below {below}"
    );
}

// --- pkbroad: the broad flat brush (issue #115) ----------------------
//
// Three presets here already make a wide mark, so most of these tests
// are about the one thing that makes this its own tool: a brush runs
// out of paint. `/Depletion` is that fade, and if it stopped working
// pkbroad would be pkribbon with striations.

const BROAD_PATH: &str = "newpath 50 100 moveto 350 106 lineto";

fn broad(opts: &str) -> Interp {
    let mut it = fresh(400, 200);
    it.run_str(&format!(
        "0 0 0 setrgbcolor 29 srand {BROAD_PATH} << {opts} >> pkbroad"
    ))
    .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    it
}

/// Ink in a vertical slice, as a fraction of the band's height. The
/// depletion fade is exactly a fall in this along the stroke.
fn slice_ink(it: &Interp, x0: u32, x1: u32, h: u32) -> usize {
    (x0..x1)
        .map(|x| {
            (0..h)
                .filter(|&y| it.gfx().pixmap.pixel(x, y).is_some_and(|p| luma(p) < 180.0))
                .count()
        })
        .sum()
}

#[test]
fn broad_lays_a_wide_striated_band() {
    let it = broad("/Width 50 /Depletion 0");
    assert!(
        ink_count(&it) > 6000,
        "a loaded block-in should be a solid wide band, got {}",
        ink_count(&it)
    );
}

/// The whole point of the tool: the mark is solid where the brush lands
/// and progressively drier as it travels.
#[test]
fn broad_depletes_along_the_stroke() {
    let it = broad("/Width 50 /Depletion 0.9");
    let near = slice_ink(&it, 60, 130, 200);
    let far = slice_ink(&it, 270, 340, 200);
    assert!(
        near > 0 && far < near / 2,
        "the brush must run drier as it goes: near {near} far {far}"
    );
}

/// ...and /Depletion 0 turns it off, giving the even block-in that is
/// the other end of the tool's range.
#[test]
fn broad_depletion_zero_stays_even() {
    let it = broad("/Width 50 /Depletion 0");
    let near = slice_ink(&it, 60, 130, 200) as f64;
    let far = slice_ink(&it, 270, 340, 200) as f64;
    assert!(
        (near - far).abs() < near * 0.15,
        "Depletion 0 must not fade: near {near} far {far}"
    );
}

/// /Charge is how much paint the brush started with, which is a
/// different axis from how fast it runs out: a low charge skips from
/// the very beginning.
#[test]
fn broad_charge_controls_how_much_lands_at_all() {
    let full = slice_ink(&broad("/Width 50 /Charge 1 /Depletion 0"), 60, 130, 200);
    let scant = slice_ink(&broad("/Width 50 /Charge 0.35 /Depletion 0"), 60, 130, 200);
    assert!(
        scant * 2 < full,
        "a barely loaded brush should skip from the start: full {full} scant {scant}"
    );
}

/// /Angle is pktrowel's reading -- brush rotation relative to travel,
/// constant along a curve -- deliberately not pknib's fixed world
/// angle. Footprint thickness therefore goes as its cosine.
#[test]
fn broad_angle_thins_the_footprint() {
    let square = column_height(
        &broad("/Width 50 /Angle 0 /Depletion 0 /Charge 1"),
        200,
        200,
    );
    let raked = column_height(
        &broad("/Width 50 /Angle 75 /Depletion 0 /Charge 1"),
        200,
        200,
    );
    assert!(raked > 0, "a raked brush should still mark");
    assert!(
        raked * 3 / 2 < square,
        "Angle must thin the footprint: square {square} raked {raked}"
    );
}

#[test]
fn broad_edge_roughens_the_sides() {
    let crisp = pixels(&broad("/Width 50 /Edge 0 /Depletion 0"));
    let rough = pixels(&broad("/Width 50 /Edge 1 /Depletion 0"));
    assert_ne!(crisp, rough, "/Edge must perturb the band's sides");
}

#[test]
fn broad_is_deterministic_under_a_seed() {
    let run = || pixels(&broad("/Width 50 /Depletion 0.6 /Edge 0.4"));
    assert_eq!(run(), run(), "pkbroad must be deterministic under a seed");
}

/// The charge is per *subpath*: three parallel strokes in one call are
/// three brush loads, each fading independently. That is what an artist
/// blocking in a sky does, and it follows from walkpath's `t` restarting
/// per subpath -- worth pinning rather than leaving to be discovered.
#[test]
fn broad_each_subpath_gets_its_own_charge() {
    let mut it = fresh(400, 240);
    it.run_str(
        "0 0 0 setrgbcolor 29 srand \
         newpath 50 60 moveto 350 60 lineto \
         50 180 moveto 350 180 lineto \
         << /Width 40 /Depletion 0.9 >> pkbroad",
    )
    .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    // Both strokes must be solid at their own start and thin at their
    // own end -- not one continuous fade across the pair.
    for (lo, hi) in [(0u32, 120u32), (120, 240)] {
        let near: usize = (60..120)
            .map(|x| {
                (lo..hi)
                    .filter(|&y| it.gfx().pixmap.pixel(x, y).is_some_and(|p| luma(p) < 180.0))
                    .count()
            })
            .sum();
        let far: usize = (280..340)
            .map(|x| {
                (lo..hi)
                    .filter(|&y| it.gfx().pixmap.pixel(x, y).is_some_and(|p| luma(p) < 180.0))
                    .count()
            })
            .sum();
        assert!(
            near > 0 && far < near,
            "each subpath should fade from its own start: rows {lo}..{hi} near {near} far {far}"
        );
    }
}

#[test]
fn broad_empty_path_is_a_no_op() {
    let mut it = fresh(120, 120);
    it.run_str("0 0 0 setrgbcolor newpath << /Width 30 >> pkbroad")
        .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    assert_eq!(ink_count(&it), 0, "an empty path should paint nothing");
}

#[test]
fn broad_restores_the_callers_color() {
    let mut it = fresh(200, 200);
    it.run_str(
        "0.2 0.4 0.8 setrgbcolor 9 srand newpath 40 100 moveto 160 100 lineto \
         << /Width 24 /ColorJitter 0.4 >> pkbroad",
    )
    .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    let (r, g, b) = it.gfx().rgb();
    assert!(
        (r - 0.2).abs() < 1e-6 && (g - 0.4).abs() < 1e-6 && (b - 0.8).abs() < 1e-6,
        "pkbroad must leave the caller's color alone, got {r} {g} {b}"
    );
}

/// `pkflat` was already taken -- it is pkribbon's constant-width
/// *pressure* preset, and every preset here defaults `/Pressure` to
/// `{ pkflat }`. Defining a brush over it broke pkribbon, pktrowel,
/// pkoil and pkfan at once, silently, because the failure surfaced as a
/// `typecheck` inside pkgetdef rather than anywhere near the cause.
#[test]
fn the_pressure_presets_are_not_shadowed_by_a_brush() {
    let mut it = fresh(200, 200);
    it.run_str("/pkflat load type /pktaper load type /pkbell load type")
        .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    let st = it.operand_stack();
    for (i, name) in ["pkbell", "pktaper", "pkflat"].iter().enumerate() {
        assert_eq!(
            st[st.len() - 1 - i].repr(),
            "arraytype",
            "{name} should still be a procedure"
        );
    }
    // And the thing they are used for still works.
    let mut it2 = fresh(200, 200);
    it2.run_str(
        "0 0 0 setrgbcolor 3 srand newpath 40 100 moveto 160 100 lineto \
         << /Width 20 /Pressure { pkflat } >> pkribbon",
    )
    .unwrap_or_else(|e| panic!("{}", it2.error_report(&e)));
    assert!(
        ink_count(&it2) > 100,
        "the default pressure preset should still paint"
    );
}

#[test]
fn broad_validation_and_safety() {
    fn err(src: &str) -> String {
        let mut it = fresh(100, 100);
        let e = it.run_str(src).unwrap_err();
        it.error_report(&e).to_string()
    }
    let p = "newpath 0 0 moveto 100 0 lineto";
    for (opts, want) in [
        ("/Width 0", "pkbroad-width-must-be-positive"),
        ("/Width { 3 }", "pkbroad-width-must-not-be-a-procedure"),
        ("/Angle 90", "pkbroad-angle-must-be-minus80-to-80"),
        ("/Charge 1.5", "pkbroad-charge-must-be-0-to-1"),
        ("/Depletion -1", "pkbroad-depletion-must-be-0-to-1"),
        ("/Grain 0", "pkbroad-grain-must-be-1-to-40"),
        ("/Grain 41", "pkbroad-grain-must-be-1-to-40"),
        ("/Grain 2.5", "pkbroad-grain-must-be-1-to-40"),
        ("/Edge 3", "pkbroad-edge-must-be-0-to-1"),
        ("/Pitch 0", "pkbroad-pitch-must-be-positive"),
        ("/ColorJitter 9", "pkbroad-colorjitter-must-be-0-to-1"),
    ] {
        let report = err(&format!("{p} << {opts} >> pkbroad"));
        assert!(
            report.contains(want),
            "expected {want} for {opts}, got {report}"
        );
    }
    // A whole-valued real is a reasonable thing to compute; it must work
    // rather than typecheck later on `real array`.
    let mut ok = fresh(120, 120);
    ok.run_str("0 0 0 setrgbcolor 3 srand newpath 10 60 moveto 110 60 lineto << /Width 20 /Grain 6.0 >> pkbroad")
        .unwrap_or_else(|e| panic!("/Grain 6.0 should work: {}", ok.error_report(&e)));
}

#[test]
fn broad_deposit_budget_guard_rejects_grain_times_samples_over_the_limit() {
    let mut it = fresh(100, 100);
    let e = it
        .run_str("newpath 0 0 moveto 400000 0 lineto << /Width 20 /Grain 40 /Pitch 0.5 >> pkbroad")
        .unwrap_err();
    assert!(
        it.error_report(&e)
            .contains("pkbroad-deposit-count-exceeds-safety-limit"),
        "expected the deposit budget guard, got {}",
        it.error_report(&e)
    );
    assert_eq!(
        ink_count(&it),
        0,
        "a rejected budget must leave the canvas clean"
    );
}

#[test]
fn ghostscript_accepts_paintkit_broad() {
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
            "-g620x600",
            "-r72",
            "-o/dev/null",
            "examples/paintkit_broad_demo.ps",
        ])
        .status()
        .expect("run gs");
    assert!(
        status.success(),
        "gs rejected examples/paintkit_broad_demo.ps"
    );
}

/// Review of PR #140 found `/Jitter` parsed, validated, and advertised
/// in `pscat --capabilities` while being read nowhere in the body -- a
/// complete no-op that shipped because the only test touching it
/// asserted that it *rejected a procedure*. Validation coverage is not
/// effect coverage.
///
/// This is the structural guard against that whole class: every
/// parameter the catalog advertises must measurably change the render.
/// It is deliberately a loop over the documented list rather than one
/// test per parameter, so adding a parameter without an effect fails
/// here without anyone remembering to write a new test.
#[test]
fn broad_every_documented_parameter_changes_the_render() {
    let baseline = pixels(&broad("/Width 44"));
    for (param, value) in [
        ("Width", "70"),
        ("Angle", "60"),
        ("Charge", "0.3"),
        ("Depletion", "1"),
        ("Grain", "3"),
        ("Edge", "0.9"),
        ("Pitch", "8"),
        ("ColorJitter", "0.9"),
    ] {
        let varied = pixels(&broad(&format!("/Width 44 /{param} {value}")));
        assert_ne!(
            varied, baseline,
            "/{param} {value} produced an identical render -- it is a no-op"
        );
    }
}

/// ...and the catalog must not advertise anything beyond that list, so
/// a future parameter cannot slip in undocumented-and-untested either.
#[test]
fn broad_advertises_exactly_the_parameters_it_implements() {
    let src = std::fs::read_to_string("lib/paintkit.ps").expect("read paintkit");
    let start = src
        .find("% @example: newpath 40 120 moveto 300 130 lineto")
        .expect("pkbroad tags");
    let end = src[start..].find("\n/pkbroad ").expect("pkbroad def") + start;
    let mut advertised: Vec<String> = src[start..end]
        .lines()
        .filter_map(|l| l.strip_prefix("% @param: /"))
        .map(|l| l.split_whitespace().next().unwrap_or("").to_string())
        .collect();
    advertised.sort();
    let mut expected = vec![
        "Angle",
        "Charge",
        "ColorJitter",
        "Depletion",
        "Edge",
        "Grain",
        "Pitch",
        "Width",
    ];
    expected.sort();
    assert_eq!(
        advertised, expected,
        "pkbroad's advertised parameters drifted from the tested set"
    );
}

/// /Grain is the mechanism the header calls the whole tool -- the band
/// is divided into striations -- yet nothing checked it did anything.
/// Counted as separate ink bands down a column crossing the stroke.
#[test]
fn broad_grain_sets_the_striation_count() {
    let bands = |grain: u32| {
        let it = broad(&format!(
            "/Width 60 /Grain {grain} /Charge 0.6 /Depletion 0 /Edge 0"
        ));
        (120..280).map(|x| column_runs(&it, x, 200)).sum::<usize>()
    };
    let coarse = bands(3);
    let fine = bands(30);
    assert!(
        fine > coarse,
        "more striations should read as more separate bands: coarse {coarse} fine {fine}"
    );
}

/// A degenerate subpath used to draw nothing at all, while every other
/// wide preset here leaves a mark. A flat brush set down and lifted
/// stamps its own footprint.
#[test]
fn broad_degenerate_subpath_presses_a_footprint() {
    let mut it = fresh(200, 200);
    it.run_str(
        "0 0 0 setrgbcolor 29 srand newpath 100 100 moveto \
         << /Width 60 /Charge 1 /Depletion 0 >> pkbroad",
    )
    .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    assert!(
        ink_count(&it) > 200,
        "a pressed flat brush should stamp its footprint, got {}",
        ink_count(&it)
    );
    // The footprint spans the band's width, not a hairline.
    assert!(
        column_height(&it, 100, 200) > 40,
        "the stamp should be the full band width"
    );
}

/// ...and it honors the charge, like every other stop: an unloaded
/// brush pressed to the page leaves nothing.
#[test]
fn broad_a_pressed_footprint_honors_the_charge() {
    let mut it = fresh(200, 200);
    it.run_str(
        "0 0 0 setrgbcolor 29 srand newpath 100 100 moveto \
         << /Width 60 /Charge 0 >> pkbroad",
    )
    .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    assert_eq!(ink_count(&it), 0, "an unloaded brush stamps nothing");
}

/// The footprint's long axis is the stop's travel direction turned by
/// /Angle. It used to be hard-coded along +x, which made the stamp a
/// sheared parallelogram at nonzero /Angle instead of the rotated
/// rectangle a flat brush actually presses. Both other degenerate tests
/// run at /Angle 0, where the two agree, so this is the one that pins
/// the rotation.
#[test]
fn broad_a_pressed_footprint_turns_with_the_angle() {
    let stamp = |angle: &str| {
        let mut it = fresh(200, 200);
        it.run_str(&format!(
            "0 0 0 setrgbcolor 29 srand newpath 100 100 moveto \
             << /Width 60 /Charge 1 /Depletion 0 /Angle {angle} >> pkbroad"
        ))
        .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
        it
    };
    let flat = stamp("0");
    let turned = stamp("60");
    assert_ne!(
        pixels(&flat),
        pixels(&turned),
        "/Angle must turn the pressed footprint"
    );
    // The discriminator is area, not extent. Turning the brush is a
    // *rotation*, which preserves the footprint's area at every angle.
    // Shearing it -- holding the long axis along +x while the width
    // swings to Angle+90 -- scales the area by cos(Angle), so the old
    // hard-coded version lost half its ink at /Angle 60 while still
    // looking like a plausible slanted mark.
    let (flat_ink, turned_ink) = (ink_count(&flat) as f64, ink_count(&turned) as f64);
    assert!(
        (turned_ink - flat_ink).abs() < flat_ink * 0.15,
        "turning the brush must preserve the footprint's area (a rotation), \
         not scale it by cos(Angle) (a shear): flat {flat_ink} turned {turned_ink}"
    );
}

/// Codex review of PR #140: `frnd` really can return exactly 1.0 --
/// seed 51463 does -- and the strict `charge > affinity` comparison
/// then rejected that striation at `/Charge 1`, which is documented as
/// fully loaded. A one-striation brush at full charge painted nothing.
#[test]
fn broad_full_charge_is_certain_contact() {
    let mut it = fresh(200, 120);
    it.run_str(
        "0 0 0 setrgbcolor 51463 srand newpath 20 60 moveto 180 60 lineto \
         << /Width 40 /Grain 1 /Charge 1 /Depletion 0 >> pkbroad",
    )
    .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    assert!(
        ink_count(&it) > 1000,
        "a fully loaded brush must deposit whatever affinity it drew, got {}",
        ink_count(&it)
    );
    // The other end still holds: an unloaded brush deposits nothing,
    // including when it draws an affinity of exactly 0.
    let mut it = fresh(200, 120);
    it.run_str(
        "0 0 0 setrgbcolor 51463 srand newpath 20 60 moveto 180 60 lineto \
         << /Width 40 /Grain 1 /Charge 0 >> pkbroad",
    )
    .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    assert_eq!(ink_count(&it), 0, "an unloaded brush deposits nothing");
}

/// Codex review of PR #140: the striation affinities were drawn inside
/// the same loop that emits geometry, and `pmemit` calls `pkjit` per
/// boundary point. So the number of points a striation deposited --
/// which is exactly what `/Charge` and `/Depletion` control -- shifted
/// the random stream for every later striation, and turning either knob
/// re-rolled the band into a *different* mark instead of re-shaping the
/// same one. That invariant is the preset's advertised behavior.
///
/// Read the affinities off the interpreter rather than from pixels: the
/// claim is about which lanes were chosen, not about what they look
/// like.
#[test]
fn broad_charge_does_not_reroll_the_striations() {
    let affinity = |charge: &str| {
        let mut it = fresh(400, 200);
        it.run_str(&format!(
            "0 0 0 setrgbcolor 3 srand newpath 50 100 moveto 350 106 lineto \
             << /Width 50 /Grain 6 /Depletion 0 /Charge {charge} >> pkbroad \
             pmaffs aload pop pmcjs aload pop"
        ))
        .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
        it.operand_stack()
            .iter()
            .map(|o| o.repr().to_string())
            .collect::<Vec<_>>()
    };
    let low = affinity("0.4");
    assert_eq!(
        low.len(),
        12,
        "six affinities and six per-striation colors -- the documented claim \
         is about which lanes deposit *and what color they are*, and an \
         affinity-only probe passed with the color draw moved back onto \
         the drawing stream (review of PR #140)"
    );
    // Equality alone is not enough for the colors: if the color draw is
    // moved back onto the drawing stream, `pmcjs` is simply never
    // written, and two runs of six nulls compare equal. Require real
    // numbers, so "planned up front" is what is actually asserted.
    for (i, v) in low.iter().enumerate() {
        assert!(
            v.parse::<f64>().is_ok(),
            "slot {i} of the plan is {v:?}, not a number -- the per-striation \
             values must be drawn before any geometry, not left unwritten"
        );
    }
    for charge in ["0.8", "1", "0.05"] {
        assert_eq!(
            affinity(charge),
            low,
            "/Charge {charge} re-rolled the striation affinities; \
             it must re-shape the same mark, not choose different lanes"
        );
    }
    // ...and the same for the other half of the pair.
    for depletion in ["0", "0.5", "1"] {
        let mut it = fresh(400, 200);
        it.run_str(&format!(
            "0 0 0 setrgbcolor 3 srand newpath 50 100 moveto 350 106 lineto \
             << /Width 50 /Grain 6 /Depletion {depletion} /Charge 0.4 >> pkbroad \
             pmaffs aload pop pmcjs aload pop"
        ))
        .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
        let got: Vec<String> = it
            .operand_stack()
            .iter()
            .map(|o| o.repr().to_string())
            .collect();
        assert_eq!(
            got, low,
            "/Depletion {depletion} re-rolled the striation affinities"
        );
    }
}

/// The taper's *use*, in pixels. Pinning only the helper's arithmetic
/// (below) let both `pmtaper` calls be deleted from `pmemit` with every
/// broad_ test still green, while the specimen changed by 8552 pixels
/// and the "staircase of hard vertical cutoffs" the taper exists to
/// remove came back (review of PR #140).
///
/// One striation, fully depleting: near the end of its run the band must
/// be measurably thinner than mid-run. Untapered it keeps full width and
/// stops dead.
#[test]
fn broad_the_run_out_taper_is_actually_applied() {
    let mut it = fresh(400, 200);
    it.run_str(
        "0 0 0 setrgbcolor 29 srand newpath 40 100 moveto 360 100 lineto \
         << /Width 60 /Grain 1 /Charge 1 /Depletion 1 /Edge 0 /ColorJitter 0 >> pkbroad",
    )
    .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    let (x0, x1) = ink_x_bounds(&it, 400, 200).expect("the stroke must ink");
    let mid = x0 + (x1 - x0) / 2;
    let late = x0 + (x1 - x0) * 9 / 10;
    let (h_mid, h_late) = (column_height(&it, mid, 200), column_height(&it, late, 200));
    assert!(
        h_mid > 0 && h_late > 0,
        "both sample columns must be inside the mark: mid {h_mid} late {h_late}"
    );
    assert!(
        (h_late as f64) < (h_mid as f64) * 0.8,
        "a striation that ran out must thin away over its last stretch, not \
         stop at full width: height {h_mid} at mid-run, {h_late} at 90%"
    );
}

/// Codex review of PR #140 [P1]: the run-out taper has to narrow a
/// striation around *its own* center, not scale its offsets from the
/// band centerline. Scaling the absolute offsets translates the whole
/// striation inward as it fades, so an outer one sweeps diagonally
/// across its neighbours -- visible in the demo as the depleted end
/// collapsing into a chevron pointing at the centerline.
///
/// Read off `pmtaper` directly rather than from pixels: the invariant is
/// exactly "the tapered midpoint is still the striation's own center,"
/// and a render can only show that indirectly.
///
/// That precision is also this test's limit, which is why
/// `broad_the_run_out_taper_is_actually_applied` sits beside it: pinning
/// the helper's arithmetic said nothing about `pmemit` still calling it.
#[test]
fn broad_the_run_out_taper_narrows_in_place() {
    let mut it = fresh(100, 100);
    // An outer striation: center 0.75, spanning 0.5..1.0 of the band.
    // pmei/pmstart/pmspan put it at the very end of its run, where the
    // taper is deepest and the defect is largest.
    it.run_str(
        "/pmnc 0.75 def /pmran true def /pmstart 0 def /pmspan 10 def \
         /pmei 10 def 0.5 pmtaper 1.0 pmtaper",
    )
    .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    let vals: Vec<f64> = it
        .operand_stack()
        .iter()
        .map(|o| o.repr().parse().expect("pmtaper should push numbers"))
        .collect();
    assert_eq!(vals.len(), 2, "pmtaper should push one value per boundary");
    let (inner, outer) = (vals[0], vals[1]);
    let mid = (inner + outer) / 2.0;
    assert!(
        (mid - 0.75).abs() < 1e-6,
        "a tapered striation must stay centered on 0.75, got {mid} \
         (boundaries {inner}..{outer}) -- it is sliding toward the \
         band centerline instead of thinning in place"
    );
    assert!(
        outer - inner < 0.5,
        "the taper must actually narrow the striation, got a span of {}",
        outer - inner
    );
}

/// Codex review of PR #140 [P2]: a subpath shorter than /Pitch gets two
/// stops -- a loaded one at the start and a dry one at the end -- so
/// every striation's run collapsed to a single sample, and `pmemit`'s
/// `end > start` guard discarded all of them. A fully charged brush
/// painted nothing at all.
#[test]
fn broad_a_stroke_shorter_than_the_pitch_still_paints() {
    let mut it = fresh(120, 120);
    it.run_str(
        "0 0 0 setrgbcolor 1 srand newpath 50 50 moveto 53 50 lineto \
         << /Width 40 /Charge 1 /Depletion 1 >> pkbroad",
    )
    .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    assert!(
        ink_count(&it) > 50,
        "a fully loaded brush must leave a mark on a short stroke, got {}",
        ink_count(&it)
    );
    // ...and it must leave it *where the stroke is*. Asserting only
    // that ink exists passed on a mark 2pt to the left of the stroke,
    // which is what reusing the degenerate footprint did: that one is
    // centred on its stop, correct for a bare `moveto` and wrong for a
    // run that knows which way the brush was travelling (review of
    // PR #140).
    let (x0, x1) = ink_x_bounds(&it, 120, 120).expect("the short stroke must ink");
    assert!(
        x0 >= 49 && x1 <= 54,
        "the mark must sit on the 50..53 stroke, got x {x0}..{x1}"
    );
}

/// The same placement bug is unbounded through `/Pitch`, which is
/// user-supplied and validated only as positive: the footprint was a
/// full pitch long, so `/Pitch 200` on a 30pt path inked 200pt of band
/// starting 50pt before the stroke.
#[test]
fn broad_a_short_stroke_does_not_scale_its_mark_with_pitch() {
    let mut it = fresh(300, 300);
    it.run_str(
        "0 0 0 setrgbcolor 1 srand newpath 50 150 moveto 80 150 lineto \
         << /Width 20 /Pitch 200 /Charge 1 /Depletion 1 >> pkbroad",
    )
    .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    let (x0, x1) = ink_x_bounds(&it, 300, 300).expect("must ink");
    assert!(
        x0 >= 49 && x1 <= 81,
        "a large /Pitch must not stretch the mark past its own stroke \
         (50..80), got x {x0}..{x1}"
    );
}

// --- pkliner: the liner / detail brush (issue #116) -------------------
//
// The tool is defined by three things nothing else in the file does
// together: the line leaves the centerline it was drawn from, it whips
// from full width to a point, and it breaks into shortening dashes as
// the paint runs out. Most of what follows is about keeping those three
// separable -- /Taper is width only, /Charge and /Depletion are coverage
// only -- because that separation is the whole artist-facing story.

const LINER_PATH: &str = "newpath 40 100 moveto 360 100 lineto";

fn liner(opts: &str) -> Interp {
    let mut it = fresh(400, 200);
    it.run_str(&format!(
        "0 0 0 setrgbcolor 17 srand {LINER_PATH} << {opts} >> pkliner"
    ))
    .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    it
}

/// Inked rows in one column, as a set of y values -- the vertical extent
/// of the mark where it crosses x, which is what /Waver moves and
/// /Taper shrinks.
fn inked_rows_at(it: &Interp, x: u32, h: u32) -> Vec<u32> {
    (0..h)
        .filter(|&y| it.gfx().pixmap.pixel(x, y).is_some_and(|p| luma(p) < 180.0))
        .collect()
}

/// Separate inked segments along x within a band of rows: how many
/// dashes the line has been broken into there.
fn dashes_in(it: &Interp, x0: u32, x1: u32, y0: u32, y1: u32) -> usize {
    let mut runs = 0;
    let mut prev = false;
    for x in x0..x1 {
        let inked = (y0..y1).any(|y| it.gfx().pixmap.pixel(x, y).is_some_and(|p| luma(p) < 180.0));
        if inked && !prev {
            runs += 1;
        }
        prev = inked;
    }
    runs
}

fn region_pixels(it: &Interp, x0: u32, x1: u32, y0: u32, y1: u32) -> Vec<u8> {
    let mut out = Vec::new();
    for y in y0..y1 {
        for x in x0..x1 {
            let p = it.gfx().pixmap.pixel(x, y).expect("in bounds");
            out.extend_from_slice(&[p.red(), p.green(), p.blue(), p.alpha()]);
        }
    }
    out
}

#[test]
fn liner_draws_a_thin_line() {
    let it = liner("/Taper 0 /Depletion 0 /Waver 0");
    assert!(ink_count(&it) > 300, "the liner must mark at all");
    // A liner is a *fine* brush: the default 2.4pt line has to stay a
    // couple of pixels tall, not quietly render as a band.
    let tall = (60..340)
        .map(|x| column_height(&it, x, 200))
        .max()
        .expect("columns");
    assert!(tall <= 5, "a 2.4pt liner line should be thin, got {tall}px");
}

/// The lift: full width where the stroke starts, a point where it ends.
#[test]
fn liner_taper_narrows_toward_the_end() {
    let it = liner("/Width 9 /Taper 0.9 /Depletion 0 /Waver 0");
    let near = column_height(&it, 60, 200);
    let far = column_height(&it, 340, 200);
    assert!(
        near >= 8 && far * 2 < near,
        "the line must whip toward a point: near {near} far {far}"
    );
}

#[test]
fn liner_taper_zero_keeps_one_width() {
    let it = liner("/Width 9 /Taper 0 /Depletion 0 /Waver 0");
    let near = column_height(&it, 60, 200) as i32;
    let far = column_height(&it, 340, 200) as i32;
    assert!(
        (near - far).abs() <= 1,
        "/Taper 0 must not narrow: near {near} far {far}"
    );
}

/// The capability nothing else in this file has: the mark leaves the
/// centerline the caller drew. `pkribbon`'s /Jitter roughens the
/// ribbon's own edges around the path; `pkdry`'s bristles ride at fixed
/// parallel offsets from it. Neither *wanders*.
#[test]
fn liner_waver_leaves_the_ruled_centerline() {
    // The path is y=100 in user space, which is row 100 of a 200-tall
    // page. How far the ink strays from that row is the waver.
    let stray = |opts: &str| {
        let it = liner(opts);
        (60..340)
            .flat_map(|x| inked_rows_at(&it, x, 200))
            .map(|y| (y as i32 - 100).abs())
            .max()
            .expect("ink")
    };
    let ruled = stray("/Width 4 /Taper 0 /Depletion 0 /Waver 0");
    let wandered = stray("/Width 4 /Taper 0 /Depletion 0 /Waver 4");
    assert!(
        ruled <= 3,
        "/Waver 0 must be a mechanically straight line, strayed {ruled}px"
    );
    assert!(
        wandered > ruled + 5,
        "/Waver must take the line off the drawn path: ruled {ruled} wandered {wandered}"
    );
}

/// The ribbon's normals come from the *displaced* centerline, not from
/// each stop's own reported angle. The difference is measurable rather
/// than cosmetic: offsetting a wandering line along the original
/// tangent's normal gives a band of constant *vertical* extent, whose
/// perpendicular thickness is therefore Width*cos(slope) -- pinched
/// wherever the waver turns. Total ink is the discriminator, since a
/// pinched band's ink stays equal to the straight line's (the extra arc
/// length is exactly cancelled by the thinning) while a correctly
/// offset one grows with the arc length it actually travels.
#[test]
fn liner_a_wavered_line_keeps_its_thickness() {
    let ink = |opts: &str| ink_count(&liner(opts)) as f64;
    let straight = ink("/Width 6 /Taper 0 /Depletion 0 /Waver 0");
    let wavy = ink("/Width 6 /Taper 0 /Depletion 0 /Waver 10");
    assert!(
        wavy > straight * 1.03,
        "a wandering line travels further and must lay down more paint, \
         not the same amount thinner: straight {straight} wavy {wavy}"
    );
}

/// The reservoir: coverage falls along the stroke and the line breaks
/// into dashes, which is what running out of paint looks like on a mark
/// one bristle wide.
#[test]
fn liner_depletion_breaks_the_tail() {
    let it = liner("/Width 5 /Taper 0 /Depletion 1 /Waver 0");
    let near = (50..140)
        .filter(|&x| column_height(&it, x, 200) > 0)
        .count();
    let far = (260..350)
        .filter(|&x| column_height(&it, x, 200) > 0)
        .count();
    assert!(
        near > 70 && far * 2 < near,
        "the brush must run out as it travels: near {near} far {far}"
    );
    assert!(
        dashes_in(&it, 180, 360, 90, 112) > 1,
        "a depleting liner should break into dashes, not just stop"
    );
}

#[test]
fn liner_depletion_zero_never_runs_out() {
    let it = liner("/Width 5 /Taper 0 /Depletion 0 /Waver 0");
    let near = (50..140)
        .filter(|&x| column_height(&it, x, 200) > 0)
        .count();
    let far = (260..350)
        .filter(|&x| column_height(&it, x, 200) > 0)
        .count();
    assert!(
        far * 10 > near * 9,
        "/Depletion 0 must carry paint the whole way: near {near} far {far}"
    );
}

/// /Charge is a different axis from /Depletion: how much the brush
/// started with, so a low one skips from the very beginning.
#[test]
fn liner_charge_controls_how_much_lands_at_all() {
    let covered = |opts: &str| {
        let it = liner(opts);
        (50..140)
            .filter(|&x| column_height(&it, x, 200) > 0)
            .count()
    };
    let full = covered("/Width 5 /Taper 0 /Depletion 0 /Charge 1 /Waver 0");
    // 0.12 rather than something mid-range: coverage responds fastest in
    // the lower part of /Charge, because the tooth pool is squared (the
    // library header says why) and so has its median near 0.25. A
    // half-charged brush is documented as still mostly touching, and
    // measuring here is what put that sentence in the docs.
    let scant = covered("/Width 5 /Taper 0 /Depletion 0 /Charge 0.12 /Waver 0");
    assert!(
        scant * 2 < full,
        "a barely loaded liner should skip from the start: full {full} scant {scant}"
    );
}

/// `frnd` really returns exactly 1.0 for some seeds, so a strict
/// `charge gt affinity` would make a *fully loaded* brush skip wherever
/// the tooth peaks. /Charge 1 is documented as fully loaded, so it means
/// certain contact -- checked across seeds, since one seed proves
/// nothing about a rare draw.
#[test]
fn liner_full_charge_is_certain_contact() {
    for seed in [1, 7, 51463, 90210] {
        let mut it = fresh(400, 200);
        it.run_str(&format!(
            "0 0 0 setrgbcolor {seed} srand {LINER_PATH} \
             << /Width 5 /Taper 0 /Charge 1 /Depletion 0 /Waver 0 >> pkliner"
        ))
        .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
        let gaps = (50..350)
            .filter(|&x| column_height(&it, x, 200) == 0)
            .count();
        assert_eq!(
            gaps, 0,
            "/Charge 1 must not skip (seed {seed}): {gaps} gaps"
        );
    }
}

#[test]
fn liner_is_deterministic_under_a_seed() {
    let run = || pixels(&liner("/Width 4 /Depletion 0.7 /Waver 2"));
    assert_eq!(run(), run(), "pkliner must be deterministic under a seed");
}

/// The contract the header claims and none of the other seeded presets
/// here can: the number of random draws per subpath is fixed, so turning
/// any knob re-*shapes* this mark rather than re-rolling everything
/// downstream of it. Measured with the downstream-marker technique
/// #111/#140 use -- a second, unrelated mark drawn after the call, whose
/// own randomness comes off the same stream.
#[test]
fn liner_turning_the_knobs_does_not_reroll_the_stroke() {
    let marker = |opts: &str| {
        let mut it = fresh(420, 200);
        it.run_str(&format!(
            "0 0 0 setrgbcolor 17 srand \
             newpath 20 150 moveto 190 150 lineto << {opts} >> pkliner \
             newpath 230 60 moveto 400 60 lineto << /Width 12 >> pkdry"
        ))
        .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
        region_pixels(&it, 210, 420, 0, 200)
    };
    let baseline = marker("/Width 4");
    for opts in [
        "/Width 9",
        "/Width 4 /Taper 0.2",
        "/Width 4 /Charge 0.4",
        "/Width 4 /Depletion 1",
        "/Width 4 /Waver 5",
        "/Width 4 /ColorJitter 0.5",
        "/Width 4 /Pitch 2.5",
    ] {
        assert_eq!(
            marker(opts),
            baseline,
            "{opts} shifted the random stream: everything drawn after a \
             pkliner call would move as a side effect of retuning it"
        );
    }
}

/// Each subpath is its own dip of the brush -- its own charge, its own
/// waver, its own tint -- which is what drawing a spray of branches in
/// one call has to mean.
#[test]
fn liner_each_subpath_gets_its_own_charge() {
    let mut it = fresh(400, 240);
    it.run_str(
        "0 0 0 setrgbcolor 17 srand \
         newpath 40 60 moveto 360 60 lineto \
         40 180 moveto 360 180 lineto \
         << /Width 5 /Taper 0 /Depletion 1 /Waver 0 >> pkliner",
    )
    .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    for (lo, hi) in [(0u32, 120u32), (120, 240)] {
        let seg = |x0: u32, x1: u32| {
            (x0..x1)
                .filter(|&x| {
                    (lo..hi).any(|y| it.gfx().pixmap.pixel(x, y).is_some_and(|p| luma(p) < 180.0))
                })
                .count()
        };
        // The claim is "its own charge", and the sharp form of that is
        // that the *second* stroke starts as solid as the first: were
        // the charge shared across the call, it would start already
        // spent. So the near window is asserted to be unbroken, not
        // merely denser than the far one -- a coverage *ratio* between
        // two windows is a much weaker statement here, since each
        // subpath draws its own tooth pool and a low spot in one of them
        // legitimately leaves a dash alive in the last tenth of a stroke
        // whose charge is nearly gone.
        let near = seg(45, 75);
        let far = seg(325, 355);
        assert!(
            near >= 29,
            "each subpath must start fully loaded: rows {lo}..{hi} near {near}"
        );
        assert!(
            (far as f64) < near as f64 * 0.8,
            "...and run out within itself: rows {lo}..{hi} near {near} far {far}"
        );
    }
}

/// The detail end of the tool: one dab per `moveto`.
#[test]
fn liner_a_pressed_dot_lands_at_the_moveto() {
    let mut it = fresh(120, 120);
    it.run_str("0 0 0 setrgbcolor 17 srand newpath 60 60 moveto << /Width 10 >> pkliner")
        .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    let (x0, x1) = ink_x_bounds(&it, 120, 120).expect("the pressed tip must mark");
    assert!(
        x0 >= 54 && x1 <= 66,
        "the dot must sit on its moveto, got x {x0}..{x1}"
    );
    let h = column_height(&it, 60, 120);
    assert!(
        (8..=12).contains(&h),
        "a pressed dot should be about Width across, got {h}px"
    );
}

/// ...and unlike `pkbroad`'s pressed footprint it does not consult the
/// tooth: a pressed tip makes contact by pressure, and a detail dab that
/// randomly fails to land is a bad primitive. Only having no paint at
/// all stops it.
#[test]
fn liner_a_pressed_dot_lands_on_every_seed_but_needs_paint() {
    for seed in [1, 2, 3, 51463, 90210] {
        let mut it = fresh(80, 80);
        it.run_str(&format!(
            "0 0 0 setrgbcolor {seed} srand newpath 40 40 moveto \
             << /Width 8 /Charge 0.05 >> pkliner"
        ))
        .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
        assert!(ink_count(&it) > 20, "the dab must land (seed {seed})");
    }
    let mut dry = fresh(80, 80);
    dry.run_str("0 0 0 setrgbcolor 17 srand newpath 40 40 moveto << /Width 8 /Charge 0 >> pkliner")
        .unwrap_or_else(|e| panic!("{}", dry.error_report(&e)));
    assert_eq!(ink_count(&dry), 0, "no paint means no dab");
}

/// A subpath shorter than /Pitch gives two stops, so every wet run
/// between them collapses to a single sample and there is no ribbon to
/// fill. `pkbroad` painted nothing at all in this case until PR #140's
/// review, and a liner hits it constantly -- twigs and grass stems are
/// mostly shorter than a pitch.
#[test]
fn liner_a_stroke_shorter_than_the_pitch_still_paints() {
    let mut it = fresh(120, 120);
    it.run_str(
        // /Waver 0: this is a test about *where* the mark lands, and a
        // wavered stroke legitimately tilts its end cap out past the
        // stroke's own x range by the half width.
        "0 0 0 setrgbcolor 17 srand newpath 50 60 moveto 53 60 lineto \
         << /Width 6 /Waver 0 /Charge 1 /Depletion 1 >> pkliner",
    )
    .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    assert!(
        ink_count(&it) > 10,
        "a short stroke must still leave a mark"
    );
    let (x0, x1) = ink_x_bounds(&it, 120, 120).expect("must ink");
    assert!(
        x0 >= 49 && x1 <= 55,
        "the mark must sit on the 50..53 stroke, got x {x0}..{x1}"
    );
}

/// ...and the same placement is unbounded through /Pitch, which the
/// caller supplies and which is validated only as positive.
#[test]
fn liner_a_short_stroke_does_not_scale_its_mark_with_pitch() {
    let mut it = fresh(300, 300);
    it.run_str(
        "0 0 0 setrgbcolor 17 srand newpath 50 150 moveto 80 150 lineto \
         << /Width 5 /Waver 0 /Pitch 200 /Charge 1 /Depletion 1 >> pkliner",
    )
    .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    let (x0, x1) = ink_x_bounds(&it, 300, 300).expect("must ink");
    assert!(
        x0 >= 49 && x1 <= 81,
        "a large /Pitch must not stretch the mark past its own stroke \
         (50..80), got x {x0}..{x1}"
    );
}

#[test]
fn liner_empty_path_is_a_no_op() {
    let mut it = fresh(120, 120);
    it.run_str("0 0 0 setrgbcolor newpath << /Width 4 >> pkliner")
        .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    assert_eq!(ink_count(&it), 0, "an empty path should paint nothing");
}

#[test]
fn liner_restores_the_callers_color() {
    let mut it = fresh(200, 200);
    it.run_str(
        "0.2 0.4 0.8 setrgbcolor 9 srand newpath 40 100 moveto 160 100 lineto \
         << /Width 4 /ColorJitter 0.5 >> pkliner",
    )
    .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    let (r, g, b) = it.gfx().rgb();
    assert!(
        (r - 0.2).abs() < 1e-6 && (g - 0.4).abs() < 1e-6 && (b - 0.8).abs() < 1e-6,
        "pkliner must leave the caller's color alone, got {r} {g} {b}"
    );
}

#[test]
fn liner_validation_and_safety() {
    fn err(src: &str) -> String {
        let mut it = fresh(100, 100);
        let e = it.run_str(src).unwrap_err();
        it.error_report(&e).to_string()
    }
    let p = "newpath 0 0 moveto 100 0 lineto";
    for (opts, want) in [
        ("/Width 0", "pkliner-width-must-be-positive"),
        ("/Width { 3 }", "pkliner-width-must-not-be-a-procedure"),
        ("/Taper 1.5", "pkliner-taper-must-be-0-to-1"),
        ("/Taper { 1 }", "pkliner-taper-must-not-be-a-procedure"),
        ("/Charge 1.5", "pkliner-charge-must-be-0-to-1"),
        ("/Depletion -1", "pkliner-depletion-must-be-0-to-1"),
        ("/Waver -1", "pkliner-waver-must-be-0-to-10"),
        ("/Waver 11", "pkliner-waver-must-be-0-to-10"),
        ("/Pitch 0", "pkliner-pitch-must-be-positive"),
        ("/ColorJitter 9", "pkliner-colorjitter-must-be-0-to-1"),
    ] {
        let report = err(&format!("{p} << {opts} >> pkliner"));
        assert!(
            report.contains(want),
            "expected {want} for {opts}, got {report}"
        );
    }
}

#[test]
fn liner_deposit_budget_guard_rejects_a_path_with_too_many_samples() {
    let mut it = fresh(100, 100);
    let e = it
        .run_str("newpath 0 0 moveto 400000 0 lineto << /Width 4 /Pitch 0.5 >> pkliner")
        .unwrap_err();
    assert!(
        it.error_report(&e)
            .contains("pkliner-deposit-count-exceeds-safety-limit"),
        "expected the deposit budget guard, got {}",
        it.error_report(&e)
    );
    assert_eq!(
        ink_count(&it),
        0,
        "a rejected budget must leave the canvas clean"
    );
}

/// Every parameter the catalog advertises must measurably change the
/// render -- `pkbroad`'s structural guard against the /Jitter incident,
/// where a parameter was parsed, validated and advertised while being
/// read nowhere. The baseline path is long enough that /Charge and
/// /Depletion are genuinely in play; on a short one both ends would come
/// back fully inked and the test would pass vacuously.
#[test]
fn liner_every_documented_parameter_changes_the_render() {
    // Painted in a mid tone, not the black the other tests use: artkit's
    // `shade` moves toward black below 1 and toward white above it, so
    // *black* paint cannot show a negative tint at all, and this seed's
    // first tint draw is negative. On black, /ColorJitter 0.05 and 0.9
    // render identically -- which is a real property of the parameter
    // (now documented), not a no-op, and a sweep that can't tell the two
    // apart would have to be silenced rather than believed.
    let tinted = |opts: &str| {
        let mut it = fresh(400, 200);
        it.run_str(&format!(
            "0.35 0.42 0.58 setrgbcolor 17 srand {LINER_PATH} << {opts} >> pkliner"
        ))
        .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
        pixels(&it)
    };
    let baseline = tinted("/Width 5");
    for (param, value) in [
        ("Width", "12"),
        ("Taper", "0"),
        ("Charge", "0.3"),
        ("Depletion", "1"),
        ("Waver", "6"),
        ("Pitch", "2.5"),
        ("ColorJitter", "0.9"),
    ] {
        let varied = tinted(&format!("/Width 5 /{param} {value}"));
        assert_ne!(
            varied, baseline,
            "/{param} {value} produced an identical render -- it is a no-op"
        );
    }
}

/// ...and the catalog must not advertise anything beyond that list.
#[test]
fn liner_advertises_exactly_the_parameters_it_implements() {
    let src = std::fs::read_to_string("lib/paintkit.ps").expect("read paintkit");
    let start = src
        .find("% @example: newpath 60 40 moveto 92 150 118 214 150 300 curveto")
        .expect("pkliner tags");
    let end = src[start..].find("\n/pkliner ").expect("pkliner def") + start;
    let mut advertised: Vec<String> = src[start..end]
        .lines()
        .filter_map(|l| l.strip_prefix("% @param: /"))
        .map(|l| l.split_whitespace().next().unwrap_or("").to_string())
        .collect();
    advertised.sort();
    let mut expected = vec![
        "Charge",
        "ColorJitter",
        "Depletion",
        "Pitch",
        "Taper",
        "Waver",
        "Width",
    ];
    expected.sort();
    assert_eq!(
        advertised, expected,
        "pkliner's advertised parameters drifted from the tested set"
    );
}

/// Acceptance, not pixel parity: seeded `rand` diverges under gs, so the
/// claim is that the specimen runs there, which is what the issue's
/// portability criterion asks for.
#[test]
fn ghostscript_accepts_paintkit_liner() {
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
            "-g620x600",
            "-r72",
            "-o/dev/null",
            "examples/paintkit_liner_demo.ps",
        ])
        .status()
        .expect("run gs");
    assert!(
        status.success(),
        "gs rejected examples/paintkit_liner_demo.ps"
    );
}

/// A run's lift-on/lift-off taper is a fraction of the run, so a *short*
/// run gets the full lift at both of its ends at once and renders at a
/// quarter of its width and under. Nothing about a brief touch of a wet
/// tip says it should thin at all -- it has no room to -- so the lift is
/// scaled by how long the run actually is.
///
/// Pinned as total ink at a coarse pitch, which is where runs are short
/// in *stops*: this case measures 683 with the scaling and 457 without.
/// Swept across 30 seeds x 3 widths before choosing it -- the median
/// case moves 7% and the worst 50%, so a threshold between the two is a
/// real gap rather than one seed's luck. Not measurable as a *vanishing*
/// dash: the count of dashes is the same either way, since a run of
/// three or more stops still reaches full width in its middle.
#[test]
fn liner_a_brief_touch_keeps_its_width() {
    let mut it = fresh(400, 200);
    it.run_str(&format!(
        "0 0 0 setrgbcolor 5 srand {LINER_PATH} \
         << /Width 9 /Taper 0 /Waver 0 /Charge 0.45 /Depletion 0 /Pitch 22 >> pkliner"
    ))
    .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    let ink = ink_count(&it);
    assert!(
        ink > 600,
        "short runs must keep their width rather than thinning at both \
         ends at once, got {ink} (457 without the run-length scaling)"
    );
}

/// A pressed tip has no direction of travel, so there is nothing for it
/// to wander along -- and the preset promises the dab lands on its
/// `moveto`. Displacing it anyway moved a dot asked for at y=80 to y=97
/// at `/Width 10 /Waver 10` (Codex review of PR #143). The default waver
/// is small enough that the original test's tolerance hid this.
#[test]
fn liner_a_pressed_dot_ignores_the_waver() {
    for waver in ["0", "0.8", "10"] {
        let mut it = fresh(160, 160);
        it.run_str(&format!(
            "0 0 0 setrgbcolor 17 srand newpath 80 80 moveto \
             << /Width 10 /Waver {waver} >> pkliner"
        ))
        .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
        let rows: Vec<u32> = (0..160)
            .filter(|&y| {
                (0..160).any(|x| it.gfx().pixmap.pixel(x, y).is_some_and(|p| luma(p) < 180.0))
            })
            .collect();
        let (lo, hi) = (
            *rows.first().expect("the dab must land"),
            *rows.last().unwrap(),
        );
        let mid = (lo + hi) / 2;
        assert!(
            (79..=81).contains(&mid),
            "/Waver {waver} moved the dab off its moveto: rows {lo}..{hi}"
        );
    }
}

/// A lone deposit at a subpath's *last* stop has no travel ahead of it,
/// and reaching forward by half a pitch put the whole mark outside the
/// path -- a line ending at x=370 at `/Pitch 100` inked x=370..419 and
/// nothing else (Codex review of PR #143). Unbounded, since `/Pitch` is
/// the caller's and validated only as positive. The paint arrived from
/// behind, so that is the direction the mark occupies.
#[test]
fn liner_a_deposit_at_the_last_stop_stays_inside_the_path() {
    for seed in 1..=12 {
        let mut it = fresh(500, 200);
        it.run_str(&format!(
            "0 0 0 setrgbcolor {seed} srand newpath 40 100 moveto 370 100 lineto \
             << /Width 8 /Waver 0 /Taper 0 /Charge 0.5 /Depletion 0 /Pitch 100 >> pkliner"
        ))
        .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
        if let Some((x0, x1)) = ink_x_bounds(&it, 500, 200) {
            assert!(
                x0 >= 39 && x1 <= 371,
                "seed {seed} inked x {x0}..{x1}, outside the 40..370 path"
            );
        }
    }
}

/// An isolated deposit occupies the travel either side of its stop, and
/// that travel comes from the *displaced* centerline -- not from the
/// stop's own reported angle, which at a corner is the direction the
/// brush arrived from. Extending that by the distance to the next stop,
/// which lies along the direction it leaves in, shot the mark 100pt past
/// the corner: this path's maximum x is 140 and it inked through 242
/// (Codex review of PR #143, round 2).
#[test]
fn liner_an_isolated_deposit_follows_the_outgoing_path() {
    let mut it = fresh(400, 300);
    it.run_str(
        "0 0 0 setrgbcolor 3 srand newpath 40 40 moveto 140 40 lineto 140 140 lineto \
         << /Width 8 /Pitch 100 /Charge 0.2 /Waver 0 /Taper 0 >> pkliner",
    )
    .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    let (x0, x1) = ink_x_bounds(&it, 400, 300).expect("the corner deposit must mark");
    assert!(
        x0 >= 35 && x1 <= 145,
        "the mark must stay on the path (x 40..140, plus half a width), got {x0}..{x1}"
    );
}

/// walkpath emits a guaranteed final stop for every subpath, so a
/// subpath whose length is an exact multiple of /Pitch ends with two
/// stops at the *same* coordinates. A run made only of those has two
/// distinct indexes and no extent, and filling it as a ribbon produced a
/// zero-area polygon -- the deposit vanished with no error. Seeds 20 and
/// 29 here painted nothing at all before the emit branch started asking
/// about geometric span rather than index span; 11, 50 and 56 lost one
/// deposit of two (Codex review of PR #143, round 2).
#[test]
fn liner_a_deposit_on_a_duplicated_endpoint_is_not_lost() {
    for seed in [20, 29] {
        let mut it = fresh(500, 200);
        it.run_str(&format!(
            "0 0 0 setrgbcolor {seed} srand newpath 40 100 moveto 340 100 lineto \
             << /Width 8 /Pitch 100 /Charge 0.2 /Waver 0 /Taper 0 >> pkliner"
        ))
        .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
        assert!(
            ink_count(&it) > 400,
            "seed {seed}: a deposit landing on the duplicated endpoint \
             must still paint, got {}",
            ink_count(&it)
        );
    }
}

// --- pkwet: wet interaction for soft layered paint (issue #113) ------
//
// pkwet is the one entry point in this file that draws no mark of its
// own -- it re-runs the caller's mark-drawing procedure, each pass
// displaced further and mixed further toward a declared backdrop. The
// tests below pin the three things that makes load-bearing: that
// /Soft 0 is *exactly* a plain call (so the wrapper is free when it's
// switched off), that softening actually produces the intermediate
// tones a graded edge is made of, and that it touches no alpha (which
// is what spares it pkwash's Ghostscript fallback).

/// Pixels that are neither the mark's own color nor the backdrop --
/// i.e. the graded edge itself. With a black mark on white and
/// /Under white, that's everything in between.
fn midtone_count(it: &Interp) -> usize {
    it.gfx()
        .pixmap
        .pixels()
        .iter()
        .filter(|&&p| {
            let l = luma(p);
            l > 60.0 && l < 200.0
        })
        .count()
}

const WET_MARK: &str = "newpath 60 100 moveto 340 100 lineto << /Width 30 /Bristles 24 >> pkdry";

fn wet(opts: &str) -> Interp {
    let mut it = fresh(400, 200);
    it.run_str(&format!(
        "0 0 0 setrgbcolor 11 srand {{ {WET_MARK} }} << {opts} >> pkwet"
    ))
    .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    it
}

/// The contract that makes the wrapper free when it's switched off: at
/// /Soft 0 pkwet draws one undisplaced pass in the caller's exact color
/// *and takes no random draws of its own*, so the result is
/// byte-identical to calling the procedure directly. The displacement
/// draw is deliberately made only when there is a displacement to make
/// -- take it unconditionally and this test fails on the shifted
/// random stream alone.
#[test]
fn wet_soft_zero_is_identical_to_calling_the_proc() {
    let wrapped = pixels(&wet("/Soft 0"));
    let mut plain = fresh(400, 200);
    plain
        .run_str(&format!("0 0 0 setrgbcolor 11 srand {WET_MARK}"))
        .unwrap_or_else(|e| panic!("{}", plain.error_report(&e)));
    assert_eq!(
        wrapped,
        pixels(&plain),
        "/Soft 0 must be byte-identical to calling the proc directly"
    );
}

/// The acceptance criterion: "visibly softer interaction than an
/// ordinary opaque paintkit mark". A hard mark on a plain backdrop has
/// midtones only where its own edge is anti-aliased; a softened one is
/// mostly midtone, because that *is* the graded edge.
#[test]
fn wet_softens_the_mark_against_its_backdrop() {
    let hard = midtone_count(&wet("/Soft 0"));
    let soft = midtone_count(&wet("/Soft 0.9"));
    assert!(
        soft > hard * 3,
        "a wet mark must grade into its backdrop: hard {hard} soft {soft}"
    );
}

/// /Soft is the single knob: more of it means more grading.
#[test]
fn wet_soft_is_monotonic() {
    let a = midtone_count(&wet("/Soft 0.2"));
    let b = midtone_count(&wet("/Soft 0.6"));
    let c = midtone_count(&wet("/Soft 1"));
    assert!(
        a < b && b < c,
        "grading should increase with /Soft: {a} {b} {c}"
    );
}

/// /Pickup is how far the outermost pass mixes toward /Under. At 0 every
/// pass keeps the caller's own color, so displacing them just makes a
/// bigger solid mark rather than a graded one.
#[test]
fn wet_pickup_controls_how_far_the_edge_dissolves() {
    let none = midtone_count(&wet("/Soft 0.9 /Pickup 0"));
    let full = midtone_count(&wet("/Soft 0.9 /Pickup 0.95"));
    assert!(
        full > none * 2,
        "Pickup must drive the dissolve: none {none} full {full}"
    );
}

/// /Under is a *declared* backdrop -- pkwet cannot read the canvas
/// (that needs a pixel-sample operator, issue #134), so this asserts the
/// declaration is honored: with a strongly colored /Under the outer
/// passes must actually carry that hue.
#[test]
fn wet_outer_passes_carry_the_declared_under_color() {
    let mut it = fresh(400, 200);
    it.run_str(&format!(
        "0 0 0 setrgbcolor 11 srand {{ {WET_MARK} }} \
         << /Soft 1 /Pickup 0.9 /Under [1 0 0] >> pkwet"
    ))
    .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    let reddish = it
        .gfx()
        .pixmap
        .pixels()
        .iter()
        .filter(|&&p| p.red() as i32 - p.green() as i32 > 60 && p.red() > 90)
        .count();
    assert!(
        reddish > 300,
        "the outer passes should carry /Under's hue, got {reddish} reddish pixels"
    );
}

/// /Spread is how far the outermost pass is displaced, so it widens the
/// area the mark affects.
#[test]
fn wet_spread_widens_the_affected_area() {
    let ink = |spread: f64| {
        let it = wet(&format!("/Soft 0.9 /Spread {spread} /Pickup 0.4"));
        ink_count(&it)
    };
    let tight = ink(2.0);
    let wide = ink(30.0);
    assert!(
        wide > tight * 5 / 4,
        "Spread must widen the mark's reach: tight {tight} wide {wide}"
    );
}

#[test]
fn wet_is_deterministic_under_a_seed() {
    let run = || pixels(&wet("/Soft 0.8"));
    assert_eq!(run(), run(), "pkwet must be deterministic under a seed");
}

/// pkwet's whole portability story is that it uses no alpha, so unlike
/// pkwash it needs no Ghostscript fallback. Assert that directly rather
/// than trusting the comment: an alpha left set would also leak into
/// everything the caller drew afterwards.
#[test]
fn wet_never_touches_alpha() {
    let alpha_after = |prelude: &str| {
        let mut it = fresh(400, 200);
        it.run_str(&format!(
            "0 0 0 setrgbcolor 11 srand {prelude} \
             {{ {WET_MARK} }} << /Soft 1 >> pkwet currentalpha"
        ))
        .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
        it.operand_stack()
            .last()
            .expect("currentalpha left a value")
            .repr()
    };
    // Default: pkwet neither sets an alpha nor leaves one behind.
    assert_eq!(alpha_after(""), "1.0", "pkwet must leave alpha alone");
    // And an alpha the caller set survives it: pkwet has no alpha
    // handling of its own to clobber it with. (0.5 round-trips exactly
    // through the f32 the graphics state stores.)
    assert_eq!(
        alpha_after("0.5 setalpha"),
        "0.5",
        "pkwet must not clobber an alpha the caller set"
    );
}

#[test]
fn wet_restores_the_callers_color() {
    let mut it = fresh(200, 200);
    it.run_str(
        "0.2 0.4 0.8 setrgbcolor 9 srand \
         { newpath 40 100 moveto 160 100 lineto << /Width 20 >> pkribbon } \
         << /Soft 1 /Under [1 0 0] >> pkwet",
    )
    .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    let (r, g, b) = it.gfx().rgb();
    assert!(
        (r - 0.2).abs() < 1e-6 && (g - 0.4).abs() < 1e-6 && (b - 0.8).abs() < 1e-6,
        "pkwet must leave the caller's color alone, got {r} {g} {b}"
    );
}

/// The reason pkwet is a wrapper rather than a /Wet key on each preset:
/// one implementation has to serve every stroke family, including any
/// added later.
#[test]
fn wet_works_with_every_stroke_family() {
    for mark in [
        "<< /Width 24 >> pkribbon",
        "<< /Width 24 /Angle 30 >> pknib",
        "<< /Width 24 /Bristles 20 >> pkdry",
        "<< /Nozzle 12 /Density 30 >> pkspray",
        "<< /Width 24 /Ridges 8 >> pkoil",
        "<< /Width 24 >> pktrowel",
        "<< /Width 24 /Bristles 14 >> pkfan",
        "<< /Width 24 /Grain 8 >> pkbroad",
    ] {
        let mut it = fresh(300, 160);
        it.run_str(&format!(
            "0 0 0 setrgbcolor 5 srand \
             {{ newpath 50 80 moveto 250 80 lineto {mark} }} \
             << /Soft 0.8 /Under [0.9 0.9 0.9] >> pkwet"
        ))
        .unwrap_or_else(|e| panic!("{mark} under pkwet: {}", it.error_report(&e)));
        assert!(ink_count(&it) > 100, "{mark} under pkwet painted nothing");
    }
}

/// A single-layer call has no outermost/core distinction to grade, and
/// the obvious `pqi / (Layers-1)` divides by zero there -- an
/// `undefinedresult` this found the first time it was rendered.
#[test]
fn wet_a_single_layer_does_not_divide_by_zero() {
    let mut it = fresh(300, 160);
    it.run_str(
        "0 0 0 setrgbcolor 5 srand \
         { newpath 50 80 moveto 250 80 lineto << /Width 24 >> pkribbon } \
         << /Layers 1 /Spread 20 >> pkwet",
    )
    .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    assert!(
        ink_count(&it) > 100,
        "a one-layer wet call should still paint"
    );
}

#[test]
fn wet_validation_guards() {
    fn err(src: &str) -> String {
        let mut it = fresh(120, 120);
        let e = it.run_str(src).unwrap_err();
        it.error_report(&e).to_string()
    }
    let m = "{ newpath 10 10 moveto 100 10 lineto << /Width 10 >> pkribbon }";
    for (opts, want) in [
        ("/Soft 1.5", "pkwet-soft-must-be-0-to-1"),
        ("/Soft -0.1", "pkwet-soft-must-be-0-to-1"),
        ("/Layers 0", "pkwet-layers-must-be-1-to-6"),
        ("/Layers 7", "pkwet-layers-must-be-1-to-6"),
        ("/Layers 2.5", "pkwet-layers-must-be-1-to-6"),
        ("/Spread -1", "pkwet-spread-must-not-be-negative"),
        ("/Pickup 2", "pkwet-pickup-must-be-0-to-1"),
        ("/Under 4", "pkwet-under-must-be-a-3-element-array"),
        ("/Under [0 0]", "pkwet-under-must-be-a-3-element-array"),
        ("/Under [0 0 0 0]", "pkwet-under-must-be-a-3-element-array"),
        ("/Under [(a) 0 0]", "pkwet-under-components-must-be-numbers"),
        ("/Under [2 0 0]", "pkwet-under-components-must-be-0-to-1"),
        ("/Soft { 1 }", "pkwet-soft-must-not-be-a-procedure"),
    ] {
        let report = err(&format!("{m} << {opts} >> pkwet"));
        assert!(
            report.contains(want),
            "expected {want} for {opts}, got {report}"
        );
    }
    // The first operand really must be a procedure, not just anything.
    assert!(
        err("42 << /Soft 0.5 >> pkwet").contains("pkwet-first-operand-must-be-a-procedure"),
        "a non-procedure first operand must be rejected"
    );
}

#[test]
fn ghostscript_accepts_paintkit_wet() {
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
            "-g620x560",
            "-r72",
            "-o/dev/null",
            "examples/paintkit_wet_demo.ps",
        ])
        .status()
        .expect("run gs");
    assert!(
        status.success(),
        "gs rejected examples/paintkit_wet_demo.ps"
    );
}

/// pkwet must leave the operand stack exactly as it found it. Its
/// /Under validation checks each component's type and then its range,
/// and the two checks have to consume between them exactly the value
/// `get` produced -- a shape that is correct here but easy to break,
/// and whose failure mode is a mystery `typecheck` several calls later
/// rather than anything pointing back at pkwet.
#[test]
fn wet_leaves_the_operand_stack_balanced() {
    for opts in [
        "/Soft 0.8",                      // default /Under, the pkgetdef path
        "/Soft 0.8 /Under [0.1 0.2 0.3]", // reals
        "/Soft 0.8 /Under [0 1 0]",       // integers, the integertype branch
        "/Soft 0",                        // the single-pass short path
    ] {
        let mut it = fresh(200, 200);
        it.run_str(&format!(
            "1 1 1 setrgbcolor \
             {{ newpath 20 100 moveto 180 100 lineto << /Width 12 >> pkribbon }} \
             << {opts} >> pkwet"
        ))
        .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
        assert_eq!(
            it.operand_stack().len(),
            0,
            "{opts} left operands behind: {:?}",
            it.operand_stack()
                .iter()
                .map(|o| o.repr())
                .collect::<Vec<_>>()
        );
    }
}

/// The contract an artist actually needs: turning one knob must not
/// re-roll the others' texture. /Layers legitimately changes how many
/// times the proc runs, so it changes consumption -- but /Pickup,
/// /Under and /Spread must not, and neither must a /Soft that resolves
/// to the same /Layers. Same downstream-marker technique as
/// `trowel_consumes_a_fixed_number_of_random_draws`.
#[test]
fn wet_consumes_a_fixed_number_of_draws_at_a_fixed_layer_count() {
    let downstream_mark = |opts: &str| {
        let mut it = fresh(400, 240);
        it.run_str(&format!(
            "0 0 0 setrgbcolor 11 srand \
             {{ newpath 60 140 moveto 340 140 lineto << /Width 24 >> pkribbon }} \
             << /Layers 4 {opts} >> pkwet \
             0 0 0 setrgbcolor newpath 20 frnd 340 mul add 20 moveto 0 12 rlineto \
             6 setlinewidth stroke"
        ))
        .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
        (0..400)
            .find(|&x| {
                (205..225).any(|y| it.gfx().pixmap.pixel(x, y).is_some_and(|p| luma(p) < 180.0))
            })
            .expect("marker drawn")
    };
    let baseline = downstream_mark("/Spread 10");
    for opts in [
        "/Spread 10 /Pickup 0",
        "/Spread 10 /Pickup 1",
        "/Spread 10 /Under [0.2 0.4 0.9]",
        "/Spread 40",
        "/Spread 10 /Soft 0.9",
    ] {
        assert_eq!(
            downstream_mark(opts),
            baseline,
            "{opts} changed how much of the caller's random stream pkwet consumed"
        );
    }
}

/// pkwet re-enters the caller's procedure in a loop, which no other
/// preset here does -- so a wrapped proc that defines its own names
/// must not disturb the passes still to come. (The `pq-` prefix
/// reservation is what makes this hold; this pins the realistic case,
/// a proc keeping its own state across passes.)
#[test]
fn wet_survives_a_proc_that_defines_its_own_names() {
    let mut it = fresh(300, 200);
    it.run_str(
        "0 0 0 setrgbcolor 5 srand /passes 0 def \
         { /passes passes 1 add def \
           /myw 20 def \
           newpath 50 100 moveto 250 100 lineto << /Width myw >> pkribbon } \
         << /Layers 5 /Spread 12 /Under [0.9 0.9 0.9] >> pkwet \
         passes",
    )
    .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    assert_eq!(
        it.operand_stack().last().expect("passes").repr(),
        "5",
        "every planned pass should have run the caller's proc"
    );
}

/// pkwet's own displacement must not depend on how much randomness the
/// wrapped brush consumes -- otherwise swapping the brush inside the
/// braces would move the halo as well as change the mark. The whole
/// pass plan is therefore drawn before any caller code runs.
#[test]
fn wet_geometry_does_not_depend_on_the_procs_own_random_appetite() {
    let plan_marker = |extra_draws: &str| {
        let mut it = fresh(300, 200);
        it.run_str(&format!(
            "0 0 0 setrgbcolor 5 srand \
             {{ {extra_draws} newpath 50 100 moveto 250 100 lineto \
                << /Width 20 >> pkribbon }} \
             << /Layers 5 /Spread 14 /Pickup 0 >> pkwet"
        ))
        .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
        pixels(&it)
    };
    // /Pickup 0 keeps every pass the same color, so the only thing the
    // pixels can differ by is where the passes landed.
    assert_eq!(
        plan_marker(""),
        plan_marker("frnd pop frnd pop frnd pop"),
        "the pass plan must be fixed before the wrapped proc runs"
    );
}

/// Codex review of PR #138: pkwet ended by restoring the caller's color
/// with `setrgbcolor`, which forces the graphics state into DeviceRGB
/// and silently loses the caller's color space. Every pass already runs
/// inside its own gsave/grestore, so the restore was redundant as well
/// as harmful.
#[test]
fn wet_leaves_the_callers_color_space_alone() {
    let space_after = |prelude: &str| {
        let mut it = fresh(300, 200);
        it.run_str(&format!(
            "{prelude} 5 srand \
             {{ newpath 60 100 moveto 240 100 lineto << /Width 20 >> pkribbon }} \
             << /Soft 0.8 /Under [0.9 0.9 0.9] >> pkwet \
             currentcolorspace"
        ))
        .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
        it.operand_stack().last().expect("currentcolorspace").repr()
    };
    assert_eq!(
        space_after("0.35 setgray"),
        "[/DeviceGray]",
        "a DeviceGray caller must still be in DeviceGray after pkwet"
    );
    assert_eq!(
        space_after("0.1 0.2 0.3 0.05 setcmykcolor"),
        "[/DeviceCMYK]",
        "a DeviceCMYK caller must still be in DeviceCMYK after pkwet"
    );
}

/// Codex review of PR #138: pkwet is the only preset here that
/// re-enters caller code in a loop, so a wrapped procedure that itself
/// calls pkwet overwrote the outer call's `pqplan` and `pqproc`, and the
/// outer loop resumed against the inner one's plan. Prefix reservation
/// can't help, because the clashing name belongs to pkwet itself -- the
/// loop now carries its state on the operand stack instead.
#[test]
fn wet_can_be_nested_inside_its_own_wrapped_procedure() {
    let mut it = fresh(300, 220);
    it.run_str(
        "0 0 0 setrgbcolor 5 srand /inner 0 def /outer 0 def \
         { /outer outer 1 add def \
           { /inner inner 1 add def \
             newpath 60 110 moveto 240 110 lineto << /Width 16 >> pkribbon } \
           << /Layers 3 /Spread 6 /Under [0.9 0.9 0.9] >> pkwet } \
         << /Layers 4 /Spread 14 /Under [0.9 0.9 0.9] >> pkwet \
         outer inner",
    )
    .unwrap_or_else(|e| {
        panic!(
            "nested pkwet must not corrupt itself: {}",
            it.error_report(&e)
        )
    });
    let st = it.operand_stack();
    let inner: i64 = st[st.len() - 1].repr().parse().expect("inner");
    let outer: i64 = st[st.len() - 2].repr().parse().expect("outer");
    assert_eq!(outer, 4, "the outer pkwet must run all 4 of its passes");
    assert_eq!(inner, 12, "each outer pass must run all 3 inner passes");
    assert!(ink_count(&it) > 500, "the nested mark should paint");
}

/// Codex review of PR #138: whether a displacement draw happens was
/// keyed off the *magnitude* rather than the pass's position, so
/// `/Spread 0` with several layers skipped every draw while any
/// positive spread took one per non-core pass -- meaning turning
/// /Spread alone re-rolled everything drawn afterwards, which is exactly
/// what this preset's contract forbids. /Spread 0 is now on the same
/// stream as any other spread.
#[test]
fn wet_zero_spread_stays_on_the_same_random_stream() {
    let downstream_mark = |opts: &str| {
        let mut it = fresh(400, 240);
        it.run_str(&format!(
            "0 0 0 setrgbcolor 11 srand \
             {{ newpath 60 140 moveto 340 140 lineto << /Width 24 >> pkribbon }} \
             << /Layers 4 {opts} >> pkwet \
             0 0 0 setrgbcolor newpath 20 frnd 340 mul add 20 moveto 0 12 rlineto \
             6 setlinewidth stroke"
        ))
        .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
        (0..400)
            .find(|&x| {
                (205..225).any(|y| it.gfx().pixmap.pixel(x, y).is_some_and(|p| luma(p) < 180.0))
            })
            .expect("marker drawn")
    };
    assert_eq!(
        downstream_mark("/Spread 0"),
        downstream_mark("/Spread 18"),
        "/Spread must not change how much of the caller's stream pkwet consumes"
    );
}

/// Codex review of PR #138, round 2: even after dropping the trailing
/// `setrgbcolor`, every pass set an RGB color -- including the core
/// pass, whose pickup mix is the identity. So the wrapped procedure ran
/// in DeviceRGB whatever space the caller chose, and `/Soft 0` was not
/// the plain call it claims to be: a DeviceGray procedure using
/// `setcolor` works when called directly and raised `typecheck` under
/// pkwet. The core pass now carries no color at all.
#[test]
fn wet_runs_the_core_pass_in_the_callers_color_space() {
    // `setcolor` takes one operand in DeviceGray and three in DeviceRGB,
    // so a gray procedure using it is a direct probe of which space the
    // wrapped code actually ran in.
    let mut it = fresh(300, 200);
    it.run_str(
        "0.35 setgray 5 srand \
         { 0.2 setcolor newpath 60 100 moveto 240 100 lineto \
           << /Width 20 >> pkribbon } \
         << /Soft 0 >> pkwet",
    )
    .unwrap_or_else(|e| {
        panic!(
            "a DeviceGray procedure must run in DeviceGray under /Soft 0: {}",
            it.error_report(&e)
        )
    });
    assert!(ink_count(&it) > 100, "the gray-space mark should paint");
}

/// Codex review of PR #138, round 2: `/Layers 3.0` clears the
/// whole-number check but is still a real, and `real array` raises
/// typecheck in both pscat and Ghostscript. A value that passed the
/// documented contract must not fail later on its representation.
#[test]
fn wet_accepts_a_whole_valued_real_layer_count() {
    for layers in ["3", "3.0"] {
        let mut it = fresh(300, 200);
        it.run_str(&format!(
            "0 0 0 setrgbcolor 5 srand \
             {{ newpath 60 100 moveto 240 100 lineto << /Width 20 >> pkribbon }} \
             << /Layers {layers} /Spread 8 /Under [0.9 0.9 0.9] >> pkwet"
        ))
        .unwrap_or_else(|e| panic!("/Layers {layers} should work: {}", it.error_report(&e)));
        assert!(ink_count(&it) > 100, "/Layers {layers} should paint");
    }
}

/// Codex review of PR #138, round 3: carrying pkwet's loop state on the
/// operand stack fixed recursion but exposed that state *beneath* the
/// wrapped procedure, so a stack-balanced procedure that inspects
/// `count` behaved differently under pkwet than called directly -- and
/// one containing `clear` broke the loop outright. Both contradict the
/// /Soft 0 promise as badly as the corruption did. The state now lives
/// in a frame indexed by nesting depth, so nothing of pkwet's is on the
/// operand stack while caller code runs.
#[test]
fn wet_shows_the_wrapped_procedure_an_untouched_operand_stack() {
    // A procedure that only draws when the stack is empty: it must
    // behave the same wrapped as unwrapped.
    let ink_for = |wrapped: bool| {
        let body = "{ count 0 eq \
                     { newpath 60 100 moveto 240 100 lineto << /Width 20 >> pkribbon } if }";
        let src = if wrapped {
            format!("0 0 0 setrgbcolor 5 srand {body} << /Soft 0 >> pkwet")
        } else {
            format!("0 0 0 setrgbcolor 5 srand {body} exec")
        };
        let mut it = fresh(300, 200);
        it.run_str(&src)
            .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
        ink_count(&it)
    };
    let plain = ink_for(false);
    assert!(
        plain > 100,
        "the probe procedure should draw when called directly"
    );
    assert_eq!(
        ink_for(true),
        plain,
        "a stack-inspecting procedure must see the same stack under pkwet"
    );
}

/// Nesting is capped rather than growing the frame array without bound.
#[test]
fn wet_rejects_nesting_past_its_frame_depth() {
    // Nine deep: one past the cap.
    let mut src = String::from("newpath 60 100 moveto 240 100 lineto << /Width 12 >> pkribbon");
    for _ in 0..9 {
        src = format!("{{ {src} }} << /Layers 1 >> pkwet");
    }
    let mut it = fresh(300, 200);
    let e = it
        .run_str(&format!("0 0 0 setrgbcolor 5 srand {src}"))
        .unwrap_err();
    assert!(
        it.error_report(&e).contains("pkwet-nesting-too-deep"),
        "expected the nesting cap, got {}",
        it.error_report(&e)
    );
}

/// Codex review of PR #138, round 4: the nesting guard incremented the
/// depth and *then* signalled, so a caller catching the error with
/// `stopped` was left with the counter stuck above the limit. The guard
/// now rolls back before it signals.
///
/// Note the narrower scope: this covers the guard's *own* rejection,
/// which is the case fully in pkwet's control. An error raised inside a
/// wrapped procedure and caught still leaves enclosing invocations'
/// depth unreleased -- see the header for why running the procedure
/// under `stopped` and re-raising is not available here (pscat's
/// top-level `stop` ends execution silently, turning a hard error into
/// no error).
#[test]
fn wet_nesting_guard_rolls_back_the_depth_it_claimed() {
    let mut it = fresh(300, 200);
    it.run_str(
        "0 0 0 setrgbcolor 5 srand \
         { { newpath 60 100 moveto 240 100 lineto << /Width 12 >> pkribbon } \
           << /Layers 1 >> pkwet } \
         << /Layers 1 >> pkwet \
         pqdepth",
    )
    .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    assert_eq!(
        it.operand_stack().last().expect("pqdepth").repr(),
        "0",
        "a completed nest must leave the depth counter where it found it"
    );
}

/// Codex review of PR #138: the wrapped procedure must build its own
/// path. A PostScript path is in device space once constructed, so a
/// path built before the call cannot be moved by the per-pass
/// `translate`, every pass walks identical geometry, and the mark comes
/// out unsoftened -- silently. Documented as a precondition rather than
/// worked around with a `pathforall` replay; this pins the difference so
/// the documentation cannot quietly stop being true.
#[test]
fn wet_softens_only_when_the_procedure_builds_its_own_path() {
    let ink = |src: &str| {
        let mut it = fresh(200, 120);
        it.run_str(src)
            .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
        ink_count(&it)
    };
    let opts = "<< /Soft 0.9 /Under [1 1 1] >>";
    let mark = "newpath 20 60 moveto 180 60 lineto";
    let inside = ink(&format!(
        "0 0 0 setrgbcolor 5 srand \
         {{ {mark} << /Width 12 >> pkribbon }} {opts} pkwet"
    ));
    let outside = ink(&format!(
        "0 0 0 setrgbcolor 5 srand {mark} \
         {{ << /Width 12 >> pkribbon }} {opts} pkwet"
    ));
    assert!(
        inside > outside * 2,
        "building the path inside the procedure is what lets the passes \
         spread: inside {inside}, outside {outside}"
    );
}

/// Codex review of PR #138: an error caught by the caller used to leave
/// `pqdepth` naming the *inner* frame, so an outer invocation's next
/// pass read that frame and executed the inner call's procedure --
/// turning an error the caller had already handled into an uncaught
/// one, and never completing the outer call.
#[test]
fn wet_a_caught_nested_error_leaves_the_outer_call_intact() {
    let mut it = fresh(300, 200);
    it.run_str(
        "0 0 0 setrgbcolor 5 srand /seen 0 def \
         { { { nosuchname } << /Layers 2 >> pkwet } stopped { /seen seen 1 add def } if \
           newpath 60 100 moveto 240 100 lineto << /Width 12 >> pkribbon } \
         << /Layers 2 >> pkwet \
         seen pqdepth",
    )
    .unwrap_or_else(|e| panic!("the outer call must survive: {}", it.error_report(&e)));
    let stack: Vec<String> = it
        .operand_stack()
        .iter()
        .map(|o| o.repr().to_string())
        .collect();
    assert_eq!(
        stack,
        vec!["2".to_string(), "0".to_string()],
        "both outer passes should run and catch their own inner error, \
         and the depth must come back to 0"
    );
    assert!(ink_count(&it) > 100, "the outer call should still paint");
}

/// ...and the graphics state comes back untouched, which is the other
/// half of the same unwind. Before this, each abandoned pass leaked one
/// gsave frame, so the caller resumed translated and in pkwet's mixed
/// color rather than its own.
#[test]
fn wet_a_caught_error_restores_the_graphics_state() {
    let mut it = fresh(200, 120);
    it.run_str(
        "0.2 0.4 0.8 setrgbcolor 5 srand \
         matrix currentmatrix \
         { { nosuchname } << /Soft 0.8 /Layers 3 >> pkwet } stopped pop \
         matrix currentmatrix",
    )
    .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    let stack: Vec<String> = it
        .operand_stack()
        .iter()
        .map(|o| o.repr().to_string())
        .collect();
    assert_eq!(
        stack[0], stack[1],
        "a caught error must leave the CTM where it found it, got {stack:?}"
    );
    let (r, g, b) = it.gfx().rgb();
    assert!(
        (r - 0.2).abs() < 1e-6 && (g - 0.4).abs() < 1e-6 && (b - 0.8).abs() < 1e-6,
        "a caught error must leave the caller's color alone, got {r} {g} {b}"
    );
}

/// `pkwet` runs each pass under `stopped`, which makes it a boundary
/// for `exit` as well: per the PLRM, `exit` looks for the innermost
/// enclosing loop and a `stopped` context reached first is an error. So
/// a wrapped procedure cannot `exit` a loop the caller owns through
/// pkwet -- it gets a loud `invalidexit`.
///
/// Two rejected alternatives, both worse. Swallowing the exit silently
/// (the original behavior) let a caller's `loop` run forever. Telling
/// exits from errors apart by reading `$error /errorname` inferred a
/// control operation from a name that can be stale: a procedure doing
/// its own cleanup, `{ { exit } stopped { stop } if }`, re-raises a
/// `stop` while errorname still reads `/invalidexit`, and the heuristic
/// turned that deliberate `stop` into an exit of the caller's loop --
/// reporting `false` to their `stopped` (Codex review, round 4).
/// The boundary applies at *every* `/Soft`, `0` included -- the pass
/// wrapper is unconditional. An earlier revision of this test dropped
/// the `/Soft 0` case, and `/Soft 0` is exactly where the claim of
/// equivalence to a direct call makes the difference easiest to miss.
#[test]
fn wet_blocks_a_callers_exit_loudly_rather_than_swallowing_it() {
    for opts in ["/Soft 0", "/Soft 0.8 /Layers 4"] {
        let mut it = fresh(120, 60);
        let e = it
            .run_str(&format!(
                "0 0 0 setrgbcolor 5 srand /n 0 def \
                 0 1 4 {{ pop \
                   {{ newpath 10 10 moveto 100 10 lineto << /Width 5 >> pkribbon exit }} \
                   << {opts} >> pkwet \
                   /n n 1 add def }} for"
            ))
            .unwrap_err();
        assert!(
            it.error_report(&e).contains("invalidexit"),
            "<< {opts} >>: an exit across pkwet's stopped boundary must raise \
             invalidexit, got {}",
            it.error_report(&e)
        );
    }
}

/// What `/Soft 0` does and does not promise. It marks the page exactly
/// as a direct call would -- that is what
/// `wet_soft_zero_is_identical_to_calling_the_proc` pins. It does *not*
/// hand back the procedure's leftover graphics state, because every
/// pass runs in its own gsave, and the docs used to say "byte-identical"
/// which is stronger than that (review of PR #138).
#[test]
fn wet_does_not_hand_back_the_procedures_graphics_state() {
    // The current path is graphics state too, so a proc that builds a
    // path and leaves it for the caller to stroke gets nothing back.
    let ink = |src: &str| {
        let mut it = fresh(200, 120);
        it.run_str(src)
            .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
        ink_count(&it)
    };
    let build = "{ newpath 20 60 moveto 180 60 lineto }";
    let direct = ink(&format!(
        "0 0 0 setrgbcolor 3 setlinewidth {build} exec stroke"
    ));
    let wrapped = ink(&format!(
        "0 0 0 setrgbcolor 3 setlinewidth {build} << /Soft 0 >> pkwet stroke"
    ));
    assert!(direct > 100, "the direct call should stroke a line");
    assert_eq!(
        wrapped, 0,
        "pkwet runs each pass in a gsave, so a path left behind does not \
         survive -- direct {direct}, wrapped {wrapped}"
    );

    // ...and neither does a color the procedure sets.
    let mut it = fresh(60, 60);
    it.run_str("0 1 0 setrgbcolor { 1 0 0 setrgbcolor } << /Soft 0 >> pkwet")
        .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    let (r, g, b) = it.gfx().rgb();
    assert!(
        (r - 0.0).abs() < 1e-6 && (g - 1.0).abs() < 1e-6 && (b - 0.0).abs() < 1e-6,
        "the caller's color should survive the call, got {r} {g} {b}"
    );
}

/// `def` writes to the *current* dict and `stopped` does not restore the
/// dict stack, so a wrapped procedure that raised with a dict still
/// `begin`-ned had pkwet's decrement land in that dict while userdict's
/// counter stayed high. Eight of those and every later top-level call
/// dies with `pkwet-nesting-too-deep` -- exactly the poisoning the
/// nesting guard claims cannot happen (review of PR #138).
#[test]
fn wet_survives_a_proc_that_raises_with_a_dict_still_open() {
    let mut it = fresh(120, 60);
    it.run_str(
        "0 0 0 setrgbcolor 5 srand \
         { { 5 dict begin nosuchname } << /Layers 2 >> pkwet } stopped pop \
         countdictstack 3 eq { end } if \
         userdict /pqdepth get",
    )
    .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    assert_eq!(
        it.operand_stack().last().expect("pqdepth").repr(),
        "0",
        "the depth counter must come back even when the procedure left a \
         dict open, or eight of these brick pkwet"
    );
}

/// ...and the case that heuristic got wrong: a procedure that catches
/// its own `exit` and re-raises with `stop` must reach the caller's
/// `stopped` as a stop, not be mistaken for a loop exit.
#[test]
fn wet_forwards_a_procedures_own_stop_as_a_stop() {
    let mut it = fresh(200, 120);
    it.run_str(
        "0 0 0 setrgbcolor 5 srand \
         { { { exit } stopped { stop } if \
             newpath 20 60 moveto 180 60 lineto << /Width 8 >> pkribbon } \
           << /Soft 0.5 >> pkwet } stopped",
    )
    .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    assert_eq!(
        it.operand_stack().last().expect("stopped result").repr(),
        "true",
        "the caller's stopped must see the procedure's deliberate stop"
    );
}

/// A procedure with its *own* loop keeps its exit -- pkwet's boundary
/// only blocks an exit that would have to cross it.
#[test]
fn wet_re_propagates_an_exit_without_inventing_or_hiding_one() {
    let mut it = fresh(120, 60);
    it.run_str(
        "0 0 0 setrgbcolor 5 srand \
         { 0 1 9 { pop exit } for \
           newpath 10 10 moveto 100 10 lineto << /Width 5 >> pkribbon } \
         << /Soft 0.8 >> pkwet (reached) ",
    )
    .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    assert_eq!(
        it.operand_stack().last().expect("marker").repr(),
        "(reached)",
        "an exit belonging to the procedure's own loop must not escape"
    );

    let mut it = fresh(120, 60);
    let e = it
        .run_str(
            "0 0 0 setrgbcolor 5 srand \
             { newpath 10 10 moveto 100 10 lineto << /Width 5 >> pkribbon exit } \
             << /Soft 0.5 >> pkwet",
        )
        .unwrap_err();
    assert!(
        it.error_report(&e).contains("invalidexit"),
        "an exit with no enclosing loop must still raise invalidexit, got {}",
        it.error_report(&e)
    );
}

// --- pkwet's directional mode (issue #181) ----------------------------
//
// /Direction lines pkwet's passes up along one axis instead of
// scattering them round the mark: the pull-down, the lift and the
// sweep. What these pin: the passes really go one way (or both), the
// cross-axis wobble really narrows with /Stretch, the random stream is
// consumed exactly as an isotropic call consumes it, the default depth
// keeps the steps fine, and the pass budget bounds a nest -- without
// touching any call that never names /Direction.

/// Ink rows (pixmap y, top = 0) in the given band of a 400x200 page.
fn ink_rows_in(it: &Interp, rows: std::ops::Range<u32>) -> usize {
    rows.map(|y| {
        (0..400)
            .filter(|&x| it.gfx().pixmap.pixel(x, y).is_some_and(|p| luma(p) < 180.0))
            .count()
    })
    .sum()
}

/// A thin black ribbon along user y = 100, so device rows 95..105.
const PULL_MARK: &str = "newpath 60 100 moveto 340 100 lineto << /Width 8 >> pkribbon";

fn pulled(opts: &str) -> Interp {
    let mut it = fresh(400, 200);
    it.run_str(&format!(
        "0 0 0 setrgbcolor 11 srand {{ {PULL_MARK} }} << {opts} >> pkwet"
    ))
    .unwrap_or_else(|e| panic!("{opts}: {}", it.error_report(&e)));
    it
}

/// The pull-down is the point of the issue: a one-sided pull toward 270
/// leaves ink below the mark and none above it, and 90 is its mirror.
#[test]
fn wet_one_sided_pull_lands_on_one_side_only() {
    let opts = "/Spread 40 /Pickup 0.4 /OneSided true";
    let down = pulled(&format!("/Direction 270 {opts}"));
    // user y < 90 is device rows > 110; user y > 110 is rows < 90
    let (below, above) = (ink_rows_in(&down, 112..150), ink_rows_in(&down, 50..88));
    assert!(below > 2000, "a pull down should leave ink below: {below}");
    assert_eq!(above, 0, "a one-sided pull down must leave nothing above");
    let up = pulled(&format!("/Direction 90 {opts}"));
    let (below, above) = (ink_rows_in(&up, 112..150), ink_rows_in(&up, 50..88));
    assert!(above > 2000, "a lift should leave ink above: {above}");
    assert_eq!(below, 0, "a one-sided lift must leave nothing below");
}

/// Without /OneSided the passes alternate sides: a sweep goes both ways.
#[test]
fn wet_two_sided_direction_sweeps_both_ways() {
    let it = pulled("/Direction 90 /Spread 40 /Pickup 0.4 /Layers 6");
    let (below, above) = (ink_rows_in(&it, 112..150), ink_rows_in(&it, 50..88));
    assert!(
        below > 1000 && above > 1000,
        "a two-sided pull should reach both sides: below {below} above {above}"
    );
}

/// The outermost pass of a two-sided call is on the + side at any depth
/// parity -- at /Layers 3 it is the only pass at the full /Spread, so
/// full reach shows up above the mark and not below it.
#[test]
fn wet_two_sided_outermost_pass_is_on_the_plus_side_at_odd_depth() {
    // Spread 40, Layers 3: outermost at +40 (user y 140, rows ~56..64),
    // the middle pass at -20 (user y 80, rows ~116..124).
    let it = pulled("/Direction 90 /Spread 40 /Pickup 0.4 /Layers 3 /Stretch 40");
    let (plus, minus) = (ink_rows_in(&it, 52..68), ink_rows_in(&it, 132..148));
    assert!(
        plus > 1000,
        "the outermost pass should reach +Spread: {plus}"
    );
    assert_eq!(
        minus, 0,
        "nothing should reach -Spread at Layers 3: {minus}"
    );
}

/// /Stretch is the anisotropy: the cross-axis wobble is the along-axis
/// reach divided by it. A horizontal sweep of a horizontal mark at
/// /Stretch 1 wanders well off the line; at /Stretch 40 it stays on it.
#[test]
fn wet_stretch_narrows_the_cross_axis_spread() {
    let off_line = |stretch: u32| {
        let it = pulled(&format!(
            "/Direction 0 /Stretch {stretch} /Spread 40 /Pickup 0.4 /Layers 6"
        ));
        ink_rows_in(&it, 0..90) + ink_rows_in(&it, 111..200)
    };
    let (loose, tight) = (off_line(1), off_line(40));
    assert!(
        loose > 1000,
        "Stretch 1 should wander off the line: {loose}"
    );
    assert_eq!(tight, 0, "Stretch 40 should keep a sweep on its line");
}

/// The outer passes are still graded toward /Under, as in isotropic
/// mode -- a pull is a soft edge, not a hard duplicate.
#[test]
fn wet_directional_outer_passes_carry_the_declared_under_color() {
    let mut it = fresh(400, 200);
    it.run_str(&format!(
        "0 0 0 setrgbcolor 11 srand {{ {PULL_MARK} }} \
         << /Direction 270 /OneSided true /Spread 40 /Pickup 0.9 /Under [1 0 0] >> pkwet"
    ))
    .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    let reddish = it
        .gfx()
        .pixmap
        .pixels()
        .iter()
        .filter(|&&p| p.red() as i32 - p.green() as i32 > 60 && p.red() > 90)
        .count();
    assert!(
        reddish > 1000,
        "a pull's outer passes should carry /Under's hue, got {reddish}"
    );
}

/// A directional call takes exactly one draw per non-core pass, like an
/// isotropic one, so *at a fixed /Layers* switching /Direction on, or
/// turning /Direction, /Stretch, /OneSided or /Spread, never re-rolls
/// anything drawn afterwards. (With /Layers defaulted, a directional
/// call's depth follows /Spread, which does change consumption -- see
/// `wet_directional_default_depth_keeps_the_steps_fine`.)
#[test]
fn wet_direction_keys_do_not_change_random_consumption() {
    let downstream_mark = |opts: &str| {
        let mut it = fresh(400, 240);
        it.run_str(&format!(
            "0 0 0 setrgbcolor 11 srand \
             {{ newpath 60 140 moveto 340 140 lineto << /Width 24 >> pkribbon }} \
             << /Layers 4 /Spread 10 {opts} >> pkwet \
             0 0 0 setrgbcolor newpath 20 frnd 340 mul add 20 moveto 0 12 rlineto \
             6 setlinewidth stroke"
        ))
        .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
        (0..400)
            .find(|&x| {
                (205..225).any(|y| it.gfx().pixmap.pixel(x, y).is_some_and(|p| luma(p) < 180.0))
            })
            .expect("marker drawn")
    };
    let isotropic = downstream_mark("");
    for opts in [
        "/Direction 270",
        "/Direction 0 /Stretch 1",
        "/Direction 45.5 /Stretch 30 /OneSided true",
        "/Direction 270 /Spread 40",
    ] {
        assert_eq!(
            downstream_mark(opts),
            isotropic,
            "{opts} consumed the caller's stream differently from an isotropic call"
        );
    }
}

/// Lined-up identical copies read as stepped echoes past ~2pt apart, so
/// a directional call's *default* depth keeps the step at or under 2pt
/// (up to its cap of 16). An explicit /Layers is honored, and a
/// single-layer call stays a single pass.
#[test]
fn wet_directional_default_depth_keeps_the_steps_fine() {
    let passes = |opts: &str| {
        let mut it = fresh(200, 120);
        it.run_str(&format!(
            "0 0 0 setrgbcolor 5 srand /n 0 def {{ /n n 1 add def }} << {opts} >> pkwet n"
        ))
        .unwrap_or_else(|e| panic!("{opts}: {}", it.error_report(&e)));
        it.operand_stack().last().expect("n").repr().to_string()
    };
    // Soft 0.6 alone would give 4; a 22pt one-sided pull needs
    // ceil(22/2)+1 = 12, and a two-sided sweep twice as fine, ceil(12)+1.
    assert_eq!(passes("/Direction 270 /Spread 22 /OneSided true"), "12");
    assert_eq!(passes("/Direction 0 /Spread 12"), "13");
    assert_eq!(passes("/Direction 270 /Spread 80"), "16", "capped at 16");
    // an absurd /Spread must clamp, not rangecheck in cvi
    assert_eq!(passes("/Direction 270 /Spread 1e30"), "16");
    assert_eq!(passes("/Direction 270 /Spread 22 /Layers 3"), "3");
    assert_eq!(passes("/Direction 270 /Soft 0 /Spread 22"), "1");
    // ...and none of that reaches an isotropic call.
    assert_eq!(passes("/Spread 22"), "4");
}

/// The step that reads as an echo is between neighbours on the *same
/// side*, and a two-sided sweep alternates sides -- so its default depth
/// has to be twice as fine (Codex review, round 2: /Spread 12 at 7
/// layers put a sweep's copies 4pt apart on each side). Measured from
/// the passes' actual translations, not their count.
#[test]
fn wet_directional_default_depth_keeps_same_side_steps_within_2pt() {
    let offsets = |opts: &str| -> Vec<f64> {
        let mut it = fresh(200, 120);
        it.run_str(&format!(
            "/xs [] def {{ /xs [ xs aload pop matrix currentmatrix 4 get ] def }} \
             << {opts} >> pkwet xs aload pop"
        ))
        .unwrap_or_else(|e| panic!("{opts}: {}", it.error_report(&e)));
        it.operand_stack()
            .iter()
            .map(|o| o.repr().parse::<f64>().expect("a number"))
            .collect()
    };
    for opts in [
        "/Direction 0 /Spread 12",
        "/Direction 0 /Spread 12 /OneSided true",
        "/Direction 0 /Spread 22 /OneSided true",
    ] {
        let xs = offsets(opts);
        for side in [1.0, -1.0] {
            // the core (0) plus every pass on this side, in order
            let mut mine: Vec<f64> = xs.iter().copied().filter(|x| x * side >= 0.0).collect();
            mine.sort_by(|a, b| a.partial_cmp(b).expect("finite"));
            for w in mine.windows(2) {
                assert!(
                    w[1] - w[0] <= 2.0 + 1e-6,
                    "{opts}: same-side passes {} and {} are more than 2pt apart (all: {xs:?})",
                    w[0],
                    w[1]
                );
            }
        }
    }
}

/// /Soft 0 is the plain call in directional mode too.
#[test]
fn wet_directional_soft_zero_is_identical_to_calling_the_proc() {
    let wrapped = pixels(&pulled("/Soft 0 /Direction 270 /OneSided true"));
    let mut plain = fresh(400, 200);
    plain
        .run_str(&format!("0 0 0 setrgbcolor 11 srand {PULL_MARK}"))
        .unwrap_or_else(|e| panic!("{}", plain.error_report(&e)));
    assert_eq!(wrapped, pixels(&plain));
}

/// The pass budget: a chain containing a directional call may multiply
/// its innermost mark at most 64 times. The rejection comes before any
/// pass runs, and rolls the depth back so it cannot poison later calls.
#[test]
fn wet_directional_nest_is_bounded_by_the_pass_budget() {
    // Like every guard here, the rejection is an undefined name, so the
    // guard's own name is $error's /command.
    let mut it = fresh(200, 120);
    it.run_str(
        "/n 0 def \
         { { { /n n 1 add def } << /Direction 0 /Layers 16 >> pkwet } \
           << /Direction 270 /Layers 5 >> pkwet } stopped \
         $error /command get n userdict /pqdepth get",
    )
    .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    let st: Vec<String> = it
        .operand_stack()
        .iter()
        .map(|o| o.repr().to_string())
        .collect();
    let n = st.len();
    assert_eq!(st[n - 4], "true", "the over-budget nest should raise");
    assert!(
        st[n - 3].contains("pkwet-too-many-passes"),
        "expected pkwet-too-many-passes, got {}",
        st[n - 3]
    );
    // The outer call's first pass ran its proc (the inner call), which
    // was rejected before running any of its own passes.
    assert_eq!(st[n - 2], "0", "no inner pass may run once over budget");
    assert_eq!(st[n - 1], "0", "the rejection must roll the depth back");

    // A pull grazed across -- the specimen's nest -- fits.
    let mut it = fresh(200, 120);
    it.run_str(
        "/n 0 def \
         { { /n n 1 add def } << /Direction 270 /Layers 16 >> pkwet } \
         << /Direction 0 /Layers 4 >> pkwet n",
    )
    .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    assert_eq!(it.operand_stack().last().expect("n").repr(), "64");
}

/// The budget is scoped to chains with a directional call in them, so
/// a purely isotropic nest that works today still works.
#[test]
fn wet_isotropic_nests_are_not_subject_to_the_pass_budget() {
    let mut it = fresh(200, 120);
    it.run_str(
        "/n 0 def \
         { { { /n n 1 add def } << /Layers 6 >> pkwet } << /Layers 6 >> pkwet } \
         << /Layers 6 >> pkwet n",
    )
    .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    assert_eq!(it.operand_stack().last().expect("n").repr(), "216");
}

/// Directional mode shares the frame machinery, so a caught error inside
/// a nested directional call still unwinds depth and graphics state.
#[test]
fn wet_directional_caught_error_unwinds_like_the_isotropic_one() {
    let mut it = fresh(300, 200);
    it.run_str(
        "0.2 0.4 0.8 setrgbcolor 5 srand /seen 0 def matrix currentmatrix \
         { { { nosuchname } << /Direction 270 /OneSided true /Layers 3 >> pkwet } \
           stopped { /seen seen 1 add def } if \
           newpath 60 100 moveto 240 100 lineto << /Width 12 >> pkribbon } \
         << /Direction 0 /Layers 2 >> pkwet \
         matrix currentmatrix seen userdict /pqdepth get",
    )
    .unwrap_or_else(|e| panic!("the outer call must survive: {}", it.error_report(&e)));
    let st: Vec<String> = it
        .operand_stack()
        .iter()
        .map(|o| o.repr().to_string())
        .collect();
    assert_eq!(st[0], st[1], "the CTM must come back");
    assert_eq!(st[2], "2", "both outer passes should run and catch");
    assert_eq!(st[3], "0", "the depth must come back to 0");
    let (r, g, b) = it.gfx().rgb();
    assert!(
        (r - 0.2).abs() < 1e-6 && (g - 0.4).abs() < 1e-6 && (b - 0.8).abs() < 1e-6,
        "the caller's color must survive, got {r} {g} {b}"
    );
}

// The directional option guards and the pass budget's rejection live
// in `%%SelfTest` blocks beside pkwet in lib/paintkit.ps (AGENTS.md:
// guards belong next to the guard). What stays here needs pixels, pass
// counts or a real `gs`.

#[test]
fn ghostscript_accepts_the_directional_wet_demo() {
    let gs_ok = std::process::Command::new("gs")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success());
    if !gs_ok {
        eprintln!("skipping gs compatibility check: gs not installed");
        return;
    }
    let out = std::process::Command::new("gs")
        .args([
            "-dNOSAFER",
            "-dNOPAUSE",
            "-dBATCH",
            "-q",
            "-sDEVICE=png16m",
            "-g780x640",
            "-r72",
            "-o/dev/null",
            "examples/paintkit_wet_pull_demo.ps",
        ])
        .output()
        .expect("run gs");
    assert!(
        out.status.success() && !String::from_utf8_lossy(&out.stdout).contains("Error"),
        "gs rejected examples/paintkit_wet_pull_demo.ps: {}",
        String::from_utf8_lossy(&out.stdout)
    );
}

// --- pkdab: the foliage / dab brush (issue #117) ----------------------
//
// What the tool is defined by: clustered irregular dabs; one set of
// geometry painted in up to three layers; coverage that belongs to the
// path, not to /Pitch; and a fixed draw count per dab slot, so the
// layer and colour knobs re-shape the mark without re-rolling it.

const DAB_PATH: &str = "newpath 60 100 moveto 340 100 lineto";

fn dab_with(colour: &str, path: &str, opts: &str) -> Interp {
    let mut it = fresh(400, 200);
    it.run_str(&format!(
        "{colour} setrgbcolor 23 srand {path} << {opts} >> pkdab"
    ))
    .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    it
}

fn dab(opts: &str) -> Interp {
    dab_with("0 0 0", DAB_PATH, opts)
}

/// Pixels that differ from the white page at all -- unlike `ink_count`,
/// this still sees a highlight lighter than the ink threshold.
fn painted_count(it: &Interp) -> usize {
    it.gfx()
        .pixmap
        .pixels()
        .iter()
        .filter(|&&p| luma(p) < 250.0)
        .count()
}

fn dab_pixels(it: &Interp) -> Vec<u8> {
    region_pixels(it, 0, 400, 0, 200)
}

/// Pixels whose luminance sits in a narrow band: the flat interior of a
/// layer painted in one known colour. The anti-aliased fringe between a
/// layer and the page spreads over the whole range, so extremes would
/// not do, but a flat colour piles up in one bin.
fn band_count(it: &Interp, lo: f32, hi: f32) -> usize {
    it.gfx()
        .pixmap
        .pixels()
        .iter()
        .filter(|&&p| (lo..=hi).contains(&luma(p)))
        .count()
}

#[test]
fn dab_paints_clustered_marks_around_the_path() {
    let it = dab("/Size 6 /Spread 20");
    assert!(
        ink_count(&it) > 1500,
        "the dabs must mark: {}",
        ink_count(&it)
    );
    // Dabs scatter on both sides of the spine, not just along it.
    let above = (0..100)
        .any(|y| (40..360).any(|x| it.gfx().pixmap.pixel(x, y).is_some_and(|p| luma(p) < 180.0)));
    let below = (101..200)
        .any(|y| (40..360).any(|x| it.gfx().pixmap.pixel(x, y).is_some_and(|p| luma(p) < 180.0)));
    assert!(above && below, "a cluster should surround its spine");
}

#[test]
fn dab_ink_stays_within_spread_plus_dab_radius() {
    let it = dab("/Size 6 /Spread 20");
    // Spread + the largest dab (Size*1.4 * 1.12 underlayer, irregular
    // vertices up to ~1.65x) comfortably bounds this.
    let reach = 20.0 + 6.0 * 1.4 * 1.65 * 1.12 + 4.0;
    let rows: Vec<u32> = (0..200)
        .filter(|&y| (0..400).any(|x| it.gfx().pixmap.pixel(x, y).is_some_and(|p| luma(p) < 180.0)))
        .collect();
    let (ymin, ymax) = (*rows.first().unwrap() as f32, *rows.last().unwrap() as f32);
    assert!(
        100.0 - ymin <= reach && ymax - 100.0 <= reach,
        "ink strayed: {ymin}..{ymax}"
    );
}

#[test]
fn dab_is_deterministic_and_seed_sensitive() {
    let a = dab_pixels(&dab("/Size 6"));
    let b = dab_pixels(&dab("/Size 6"));
    assert_eq!(a, b, "same seed, same pixels");
    let mut it = fresh(400, 200);
    it.run_str(&format!(
        "0 0 0 setrgbcolor 24 srand {DAB_PATH} << /Size 6 >> pkdab"
    ))
    .expect("render");
    assert_ne!(a, dab_pixels(&it), "a different seed must differ");
}

#[test]
fn dab_charge_scales_coverage() {
    assert_eq!(
        ink_count(&dab("/Charge 0")),
        0,
        "an empty brush paints nothing"
    );
    let half = ink_count(&dab("/Charge 0.5"));
    let full = ink_count(&dab("/Charge 1"));
    assert!(
        full > half && half > 0,
        "charge should scale coverage: {half} vs {full}"
    );
}

#[test]
fn dab_coverage_follows_the_path_not_the_pitch() {
    let fine = ink_count(&dab("/Size 6 /Pitch 2"));
    let coarse = ink_count(&dab("/Size 6 /Pitch 24"));
    let ratio = fine as f64 / coarse as f64;
    assert!(
        (0.7..1.4).contains(&ratio),
        "pitch changed coverage: {fine} vs {coarse}"
    );
}

#[test]
fn dab_density_scales_coverage() {
    let sparse = ink_count(&dab("/Size 5 /Spread 5 /Density 0.5"));
    let dense = ink_count(&dab("/Size 5 /Spread 5 /Density 4"));
    assert!(dense > sparse * 2, "density: {sparse} -> {dense}");
    assert_eq!(
        ink_count(&dab("/Density 0")),
        0,
        "density 0 paints nothing along a line"
    );
}

#[test]
fn dab_layers_darken_and_lighten_around_the_body_colour() {
    // Base grey 0.5: shadow k = 1 - 0.55 -> 0.225 (luma ~57), highlight
    // k = 1.6 -> 0.8 (luma ~204).
    let plain = dab_with(
        "0.5 0.5 0.5",
        DAB_PATH,
        "/Shadow 0 /Highlight 0 /ColorJitter 0",
    );
    let layered = dab_with(
        "0.5 0.5 0.5",
        DAB_PATH,
        "/Shadow 1 /Highlight 1 /ColorJitter 0",
    );
    assert!(
        band_count(&layered, 54.0, 60.0) > band_count(&plain, 54.0, 60.0) + 200,
        "no shadow layer"
    );
    assert!(
        band_count(&layered, 201.0, 207.0) > band_count(&plain, 201.0, 207.0) + 100,
        "no highlight layer"
    );
    assert!(
        band_count(&plain, 124.0, 131.0) > 1000,
        "the body is the caller's colour"
    );
}

#[test]
fn dab_highlights_sit_on_the_painted_mass() {
    // Highlight dabs are derived from the body dabs' own geometry, so
    // enabling them can only add lighter pixels inside or at the rim of
    // what the body already covers: total inked area barely moves.
    let off = painted_count(&dab_with("0.5 0.5 0.5", DAB_PATH, "/Shadow 0 /Highlight 0"));
    let on = painted_count(&dab_with("0.5 0.5 0.5", DAB_PATH, "/Shadow 0 /Highlight 1"));
    let drift = (on as f64 - off as f64).abs() / off as f64;
    assert!(
        drift < 0.12,
        "highlights strayed off the mass: {off} -> {on}"
    );
}

/// The next `rand` after a pkdab call: equal across two runs exactly when
/// the call consumed the same number of draws from the stream.
fn dab_stream_after(opts: &str, path: &str) -> String {
    let mut it = fresh(400, 200);
    it.run_str(&format!(
        "0 0 0 setrgbcolor 23 srand {path} << {opts} >> pkdab rand"
    ))
    .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    it.operand_stack()
        .last()
        .map(|o| o.repr())
        .expect("a rand value")
}

#[test]
fn dab_layout_knobs_leave_the_random_stream_alone() {
    let base = dab_stream_after("/Size 6", DAB_PATH);
    for knob in [
        "/Charge 0.2",
        "/Shadow 0",
        "/Highlight 1",
        "/Light 45",
        "/ColorJitter 0.9",
        "/Irregular 1",
        "/Spread 40",
    ] {
        assert_eq!(
            dab_stream_after(&format!("/Size 6 {knob}"), DAB_PATH),
            base,
            "{knob} changed how many draws pkdab takes"
        );
    }
    // ...and the knobs documented as changing the layout do.
    assert_ne!(dab_stream_after("/Size 6 /Density 6", DAB_PATH), base);
}

#[test]
fn dab_short_strokes_and_taps_still_land() {
    // Shorter than Size/Density: accumulates under one dab, must not vanish.
    let twig = dab_with("0 0 0", "newpath 100 100 moveto 101 100 lineto", "/Size 6");
    assert!(ink_count(&twig) > 40, "a 1pt stroke painted nothing");
    // No extent at all: a tap, routed to the clump.
    let tap = dab_with(
        "0 0 0",
        "newpath 100 100 moveto 100 100 lineto",
        "/Size 6 /Clump 12",
    );
    assert!(
        ink_count(&tap) > 200,
        "a zero-extent stroke painted nothing"
    );
    // Density 0 is the caller asking for none along lines.
    let none = dab_with(
        "0 0 0",
        "newpath 100 100 moveto 101 100 lineto",
        "/Density 0",
    );
    assert_eq!(ink_count(&none), 0);
}

#[test]
fn dab_tiny_pitch_does_not_trip_the_budget_by_itself() {
    let mut it = fresh(400, 200);
    it.run_str(&format!(
        "0 0 0 setrgbcolor {DAB_PATH} << /Size 6 /Pitch 0.01 /Density 0 >> pkdab"
    ))
    .expect("density 0 on a finely sampled spine is not a budget problem");
}

#[test]
fn dab_light_moves_the_underlayer_and_highlights() {
    let a = dab_pixels(&dab_with("0.5 0.5 0.5", DAB_PATH, "/Light 0"));
    let b = dab_pixels(&dab_with("0.5 0.5 0.5", DAB_PATH, "/Light 180"));
    assert_ne!(a, b, "the light direction must matter");
}

#[test]
fn dab_single_point_is_a_bounded_clump() {
    let it = dab_with(
        "0 0 0",
        "newpath 200 100 moveto",
        "/Size 6 /Spread 20 /Clump 20",
    );
    let ink = ink_count(&it);
    assert!(ink > 300, "a point must paint a clump: {ink}");
    let (x0, x1) = ink_x_bounds(&it, 400, 200).expect("ink");
    assert!(
        x0 >= 200 - 70 && x1 <= 200 + 70,
        "clump strayed: {x0}..{x1}"
    );
}

#[test]
fn dab_empty_path_is_a_no_op_but_a_bad_dict_still_fails() {
    let mut it = fresh(100, 100);
    it.run_str("0 0 0 setrgbcolor newpath << /Size 6 >> pkdab")
        .expect("empty path");
    assert_eq!(ink_count(&it), 0);
    let mut bad = Interp::new();
    load(&mut bad);
    let err = bad
        .run_str("newpath << /Size -1 >> pkdab")
        .expect_err("must reject");
    assert!(
        bad.error_report(&err)
            .contains("pkdab-size-must-be-positive")
    );
}

#[test]
fn dab_rejects_malformed_options() {
    let cases = [
        ("/Size 0", "pkdab-size-must-be-positive"),
        ("/Size { 5 }", "pkdab-size-must-not-be-a-procedure"),
        ("/Spread 0", "pkdab-spread-must-be-positive"),
        ("/Density -1", "pkdab-density-must-be-non-negative"),
        ("/Clump 0", "pkdab-clump-must-be-1-to-400"),
        ("/Clump 2.5", "pkdab-clump-must-be-1-to-400"),
        ("/Irregular 2", "pkdab-irregular-must-be-0-to-1"),
        ("/Charge 1.5", "pkdab-charge-must-be-0-to-1"),
        ("/Shadow -0.1", "pkdab-shadow-must-be-0-to-1"),
        ("/Highlight 2", "pkdab-highlight-must-be-0-to-1"),
        ("/Pitch 0", "pkdab-pitch-must-be-positive"),
        ("/ColorJitter 3", "pkdab-colorjitter-must-be-0-to-1"),
        ("/Light { 1 }", "pkdab-light-must-not-be-a-procedure"),
    ];
    for (opts, name) in cases {
        let mut it = Interp::new();
        load(&mut it);
        let err = it
            .run_str(&format!(
                "newpath 0 0 moveto 10 0 lineto << {opts} >> pkdab"
            ))
            .expect_err(opts);
        assert!(
            it.error_report(&err).contains(name),
            "{opts}: {}",
            it.error_report(&err)
        );
    }
}

#[test]
fn dab_budget_fires_before_anything_is_painted() {
    let mut it = fresh(200, 200);
    let err = it
        .run_str("0 0 0 setrgbcolor newpath 0 0 moveto 100000 0 lineto << /Size 0.5 /Density 500 >> pkdab")
        .expect_err("budget");
    assert!(
        it.error_report(&err)
            .contains("pkdab-dab-count-exceeds-safety-limit")
    );
    assert_eq!(
        ink_count(&it),
        0,
        "nothing may paint before the budget check"
    );
}

#[test]
fn dab_preserves_the_callers_path_and_colour() {
    let mut it = fresh(400, 200);
    it.run_str(&format!(
        "0.2 0.4 0.6 setrgbcolor {DAB_PATH} << /Size 6 >> pkdab \
         currentrgbcolor 3 array astore /c exch def \
         flattenpath pathbbox /y1 exch def /x1 exch def /y0 exch def /x0 exch def"
    ))
    .expect("render");
    it.run_str(
        "c 0 get 0.2 sub abs 0.001 lt c 1 get 0.4 sub abs 0.001 lt and \
                x0 60 sub abs 0.5 lt and x1 340 sub abs 0.5 lt and \
                { } { dab-state-clobbered } ifelse",
    )
    .expect("colour and path survive");
}

#[test]
fn dab_composes_with_other_presets() {
    let mut it = fresh(400, 200);
    it.run_str(&format!(
        "0 0 0 setrgbcolor 5 srand {DAB_PATH} << /Size 6 >> pkdab \
         newpath 60 150 moveto 340 150 lineto << /Width 6 >> pkoil \
         newpath 60 40 moveto 340 40 lineto << /Width 6 >> pkribbon"
    ))
    .expect("pkdab must not clobber the scratch names of its neighbours");
    assert!(ink_count(&it) > 3000);
}

#[test]
fn dab_advertises_exactly_its_tested_parameters() {
    let src = std::fs::read_to_string("lib/paintkit.ps").expect("read paintkit");
    let start = src
        .find("% @example: newpath 80 60 moveto 180 90 280 60 380 70 curveto")
        .expect("pkdab tags");
    let end = src[start..].find("\n/pkdab ").expect("pkdab def") + start;
    let mut advertised: Vec<String> = src[start..end]
        .lines()
        .filter_map(|l| l.strip_prefix("% @param: /"))
        .map(|l| l.split_whitespace().next().unwrap_or("").to_string())
        .collect();
    advertised.sort();
    let mut expected = vec![
        "Charge",
        "Clump",
        "ColorJitter",
        "Density",
        "Highlight",
        "Irregular",
        "Light",
        "Pitch",
        "Shadow",
        "Size",
        "Spread",
    ];
    expected.sort();
    assert_eq!(
        advertised, expected,
        "pkdab's advertised parameters drifted from the tested set"
    );
}

#[test]
fn ghostscript_accepts_paintkit_dab() {
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
            "-g620x520",
            "-r72",
            "-o/dev/null",
            "examples/paintkit_dab_demo.ps",
        ])
        .status()
        .expect("run gs");
    assert!(
        status.success(),
        "gs rejected examples/paintkit_dab_demo.ps"
    );
}

/// frnd can return exactly 1.0, so a bare `frnd charge lt` rejected a slot
/// at /Charge 1. Seed 5659 with a one-dab clump is the reproduction.
#[test]
fn dab_full_charge_never_drops_a_slot() {
    let it = dab_with("0 0 0", "newpath 100 100 moveto", "/Clump 1 /Charge 1");
    // (23 srand is fixed in dab_with; re-seed to the reproducing seed.)
    let mut seeded = fresh(400, 200);
    seeded
        .run_str(
            "0 0 0 setrgbcolor 5659 srand newpath 100 100 moveto << /Clump 1 /Charge 1 >> pkdab",
        )
        .expect("render");
    assert!(
        ink_count(&seeded) > 20,
        "a full charge dropped its only dab"
    );
    assert!(ink_count(&it) > 20);
}

#[test]
fn dab_sample_count_is_bounded_independently_of_the_dab_budget() {
    let mut it = fresh(200, 200);
    let err = it
        .run_str("0 0 0 setrgbcolor newpath 0 0 moveto 100 0 lineto << /Density 0 /Pitch 0.0005 >> pkdab")
        .expect_err("sample budget");
    assert!(
        it.error_report(&err)
            .contains("pkdab-sample-count-exceeds-safety-limit")
    );
}

// ---------------------------------------------------------------------------
// Brush motions (issue #184): pkcrisscross, pkscumblein, pkscumble, pktap,
// pkpull, pkzigzag. Path builders, so most of these read the *path* they
// leave rather than pixels; the composition tests then paint it.
// ---------------------------------------------------------------------------

/// One path segment as `pathforall` reports it: 0 move, 1 line, 2 curve (end
/// point only), 3 close (coordinates are meaningless).
type Seg = (i32, f64, f64);

const DUMP_PATH: &str = "[ { 0 3 1 roll } { 1 3 1 roll } \
     { pop pop pop pop 2 3 1 roll } { 3 0 0 } pathforall ]";

fn motion_segs(code: &str) -> Vec<Seg> {
    let mut it = fresh(400, 200);
    it.run_str(&format!("{code} {DUMP_PATH}"))
        .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    parse_segs(&it)
}

fn parse_segs(it: &Interp) -> Vec<Seg> {
    let repr = it
        .operand_stack()
        .last()
        .map(|o| o.repr())
        .expect("a path dump");
    let nums: Vec<f64> = repr
        .trim_matches(|c| c == '[' || c == ']')
        .split_whitespace()
        .map(|t| t.parse().unwrap_or_else(|_| panic!("number: {t}")))
        .collect();
    nums.chunks(3).map(|c| (c[0] as i32, c[1], c[2])).collect()
}

/// The strokes of a path made only of two-point open subpaths.
fn strokes(segs: &[Seg]) -> Vec<((f64, f64), (f64, f64))> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < segs.len() {
        assert_eq!(segs[i].0, 0, "a stroke starts with a moveto: {segs:?}");
        assert!(
            i + 1 < segs.len() && segs[i + 1].0 == 1,
            "a stroke is moveto + lineto: {segs:?}"
        );
        out.push(((segs[i].1, segs[i].2), (segs[i + 1].1, segs[i + 1].2)));
        i += 2;
    }
    out
}

fn heading(s: &((f64, f64), (f64, f64))) -> f64 {
    (s.1.1 - s.0.1).atan2(s.1.0 - s.0.0).to_degrees()
}

fn stroke_len(s: &((f64, f64), (f64, f64))) -> f64 {
    ((s.1.0 - s.0.0).powi(2) + (s.1.1 - s.0.1).powi(2)).sqrt()
}

const SPINE: &str = "newpath 0 100 moveto 100 100 lineto";

#[test]
fn crisscross_runs_at_two_mirrored_headings() {
    let segs = motion_segs(
        "5 srand 0 0 200 100 screct \
         << /Count 60 /Length 20 /LengthJitter 0 /Angle 30 /Spread 0 >> pkcrisscross",
    );
    let s = strokes(&segs);
    assert!(s.len() > 40, "got {} strokes", s.len());
    let (mut up, mut down) = (0, 0);
    for st in &s {
        assert!(
            (stroke_len(st) - 20.0).abs() < 1e-3,
            "length {}",
            stroke_len(st)
        );
        let h = heading(st);
        if (h - 30.0).abs() < 1e-3 {
            up += 1;
        } else if (h + 30.0).abs() < 1e-3 {
            down += 1;
        } else {
            panic!("heading {h} is neither +30 nor -30");
        }
    }
    assert!(up > 10 && down > 10, "up {up} down {down}: not an X");
}

#[test]
fn crisscross_count_tracks_the_requested_count() {
    let n = strokes(&motion_segs(
        "5 srand 0 0 300 120 screct << /Count 100 >> pkcrisscross",
    ))
    .len();
    assert!((80..=140).contains(&n), "asked for about 100, got {n}");
    // A non-rectangular region keeps about Count too, because the grid is
    // sized from the region's area and not its bounding box.
    let tri = strokes(&motion_segs(
        "5 srand newpath 0 0 moveto 300 0 lineto 0 150 lineto closepath scpath \
         << /Count 100 >> pkcrisscross",
    ))
    .len();
    assert!((60..=150).contains(&tri), "asked for about 100, got {tri}");
    // Thin bands are the headline use: each axis used to round up on its
    // own, which nearly doubled the count (a 700 x 40 band asked for 20
    // drew 38).
    for (w, h) in [
        (700, 40),
        (700, 10),
        (700, 100),
        (40, 700),
        (1000, 1),
        (10, 10000),
        (10000, 10),
    ] {
        let n = strokes(&motion_segs(&format!(
            "5 srand 0 0 {w} {h} screct << /Count 20 >> pkcrisscross"
        )))
        .len();
        assert!(
            (13..=27).contains(&n),
            "a {w} x {h} band asked for about 20, got {n}"
        );
    }
    let circles = |w: u32, h: u32| {
        motion_segs(&format!(
            "5 srand 0 0 {w} {h} screct << /Count 20 >> pkscumblein"
        ))
        .iter()
        .filter(|s| s.0 == 3)
        .count()
    };
    for (w, h) in [(700, 40), (700, 10), (10, 10000)] {
        let n = circles(w, h);
        assert!((13..=27).contains(&n), "{w} x {h} circles: {n}");
    }
}

#[test]
fn crisscross_stays_inside_a_path_region() {
    // Triangle 0,0 - 300,0 - 0,150: x/300 + y/150 <= 1.
    let s = strokes(&motion_segs(
        "5 srand newpath 0 0 moveto 300 0 lineto 0 150 lineto closepath scpath \
         << /Count 120 /Length 12 >> pkcrisscross",
    ));
    assert!(s.len() > 50);
    for st in &s {
        let (cx, cy) = ((st.0.0 + st.1.0) / 2.0, (st.0.1 + st.1.1) / 2.0);
        assert!(
            cx >= -1e-9 && cy >= -1e-9 && cx / 300.0 + cy / 150.0 <= 1.0 + 1e-9,
            "stroke centre ({cx}, {cy}) is outside the region"
        );
    }
}

#[test]
fn motions_replace_the_path_so_a_guide_or_outline_is_never_painted() {
    // scpath leaves the region's outline as the current path; the motion
    // must not keep it, or the brush would paint it.
    let segs = motion_segs(
        "5 srand newpath 0 0 moveto 300 0 lineto 300 100 lineto closepath scpath \
         << /Count 30 >> pkcrisscross",
    );
    assert!(segs.iter().all(|s| s.0 != 3), "outline survived: closepath");
    strokes(&segs);
    for (name, code) in [
        ("pktap", "<< /Spacing 10 >> pktap"),
        ("pkpull", "<< /Spacing 10 >> pkpull"),
    ] {
        let s = motion_segs(&format!("5 srand {SPINE} {code}"));
        assert!(s.iter().all(|s| s.0 != 3), "{name}");
        let st = strokes(&s);
        // the 100-long guide itself would be a stroke 100 long
        assert!(
            st.iter().all(|x| stroke_len(x) < 90.0),
            "{name} kept the guide"
        );
    }
}

#[test]
fn crisscross_bow_makes_curves_and_only_when_asked() {
    let straight = motion_segs("5 srand 0 0 200 100 screct << /Count 20 >> pkcrisscross");
    assert!(straight.iter().all(|s| s.0 <= 1));
    let bowed = motion_segs("5 srand 0 0 200 100 screct << /Count 20 /Bow 0.3 >> pkcrisscross");
    assert!(bowed.iter().filter(|s| s.0 == 2).count() > 10);
}

#[test]
fn motions_are_deterministic_and_seed_sensitive() {
    for code in [
        "0 0 200 100 screct << /Count 30 >> pkcrisscross",
        "0 0 200 100 screct << /Count 30 >> pkscumblein",
        "newpath 0 50 moveto 150 80 lineto << >> pkscumble",
        "newpath 0 50 moveto 150 80 lineto << >> pktap",
        "newpath 0 50 moveto 150 80 lineto << >> pkpull",
        "newpath 0 50 moveto 150 80 lineto << >> pkzigzag",
    ] {
        let a = motion_segs(&format!("7 srand {code}"));
        let b = motion_segs(&format!("7 srand {code}"));
        let c = motion_segs(&format!("8 srand {code}"));
        assert!(!a.is_empty(), "{code} built nothing");
        assert_eq!(a, b, "{code} is not deterministic");
        assert_ne!(a, c, "{code} ignores the seed");
    }
}

fn motion_stream_after(code: &str) -> String {
    let mut it = fresh(400, 200);
    it.run_str(&format!("9 srand {code} rand"))
        .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    it.operand_stack()
        .last()
        .map(|o| o.repr())
        .expect("a rand value")
}

#[test]
fn motion_shaping_knobs_leave_the_random_stream_alone() {
    // A knob that only re-shapes a stroke must not re-roll where the strokes
    // land, so each slot draws the same number of values whatever it is set to.
    let region = |opts: &str| format!("0 0 200 100 screct << /Count 30 {opts} >> pkcrisscross");
    let base = motion_stream_after(&region(""));
    for knob in [
        "/Length 60",
        "/Angle 70",
        "/Spread 30",
        "/LengthJitter 1",
        "/Bow 0.4",
    ] {
        assert_eq!(
            motion_stream_after(&region(knob)),
            base,
            "crisscross {knob} changed the draws"
        );
    }
    let circles = |opts: &str| format!("0 0 200 100 screct << /Count 30 {opts} >> pkscumblein");
    let base = motion_stream_after(&circles(""));
    for knob in ["/Radius 12", "/RadiusJitter 1", "/Squash 0.3"] {
        assert_eq!(
            motion_stream_after(&circles(knob)),
            base,
            "scumblein {knob}"
        );
    }
    // /Step fixed: it sets the walk pitch, so it would move the stop count.
    let spine =
        |m: &str, opts: &str| format!("newpath 0 50 moveto 150 80 lineto << /Step 6 {opts} >> {m}");
    for (m, knobs) in [
        (
            "pktap",
            &[
                "/Length 30",
                "/Direction 10",
                "/Lean 60",
                "/Scatter 9",
                "/LengthJitter 1",
            ][..],
        ),
        (
            "pkpull",
            &[
                "/Length 80",
                "/LengthJitter 1",
                "/Direction 0",
                "/Lean 40",
                "/Bow 0.4",
            ][..],
        ),
        ("pkzigzag", &["/Amplitude 20", "/AmpJitter 1"][..]),
        (
            "pkscumble",
            &["/Radius 9", "/RadiusJitter 1", "/Squash 0.4"][..],
        ),
    ] {
        let base = motion_stream_after(&spine(m, ""));
        for knob in knobs {
            assert_eq!(motion_stream_after(&spine(m, knob)), base, "{m} {knob}");
        }
    }
}

#[test]
fn scumblein_makes_closed_squashed_circles() {
    // One circle, no jitter: a 2r wide, 2r*Squash tall closed loop, centred
    // inside the region.
    let segs = motion_segs(
        "5 srand 0 0 100 100 screct \
         << /Count 1 /Radius 10 /RadiusJitter 0 /Squash 0.5 >> pkscumblein flattenpath",
    );
    assert_eq!(segs.iter().filter(|s| s.0 == 0).count(), 1);
    assert_eq!(segs.iter().filter(|s| s.0 == 3).count(), 1, "closed");
    let pts: Vec<_> = segs.iter().filter(|s| s.0 <= 1).collect();
    let (x0, x1) = pts
        .iter()
        .fold((1e9f64, -1e9f64), |a, p| (a.0.min(p.1), a.1.max(p.1)));
    let (y0, y1) = pts
        .iter()
        .fold((1e9f64, -1e9f64), |a, p| (a.0.min(p.2), a.1.max(p.2)));
    assert!((x1 - x0 - 20.0).abs() < 0.5, "width {}", x1 - x0);
    assert!((y1 - y0 - 10.0).abs() < 0.5, "height {}", y1 - y0);
}

#[test]
fn scumblein_makes_many_circles_in_a_region() {
    let segs = motion_segs("5 srand 0 0 300 120 screct << /Count 80 /Radius 5 >> pkscumblein");
    let closed = segs.iter().filter(|s| s.0 == 3).count();
    assert!((60..=110).contains(&closed), "got {closed} circles");
    assert_eq!(closed, segs.iter().filter(|s| s.0 == 0).count());
}

#[test]
fn motions_restore_the_ctm_colour_and_stack() {
    let mut it = fresh(400, 200);
    it.run_str("matrix currentmatrix")
        .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    let before = it.operand_stack().last().map(|o| o.repr()).expect("ctm");
    it.run_str("clear 0.2 0.4 0.6 setrgbcolor")
        .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    it.run_str(
        "5 srand 0 0 200 100 screct << /Count 20 >> pkscumblein \
         0 0 200 100 screct << /Count 20 >> pkcrisscross \
         newpath 0 0 moveto 90 0 lineto << >> pkscumble \
         newpath 0 0 moveto 90 0 lineto << >> pktap \
         newpath 0 0 moveto 90 0 lineto << >> pkpull \
         newpath 0 0 moveto 90 0 lineto << >> pkzigzag \
         matrix currentmatrix currentrgbcolor",
    )
    .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    let st: Vec<String> = it.operand_stack().iter().map(|o| o.repr()).collect();
    assert_eq!(st.len(), 4, "motions leaked operands: {st:?}");
    assert_eq!(st[0], before, "a motion left the CTM changed");
    let rgb: Vec<f64> = st[1..].iter().map(|s| s.parse().expect("number")).collect();
    assert!(
        (rgb[0] - 0.2).abs() < 0.01 && (rgb[1] - 0.4).abs() < 0.01 && (rgb[2] - 0.6).abs() < 0.01
    );
    assert_eq!(ink_count(&it), 0, "a motion painted");
}

#[test]
fn scumble_loops_overlap_until_the_step_opens_them() {
    // The chain's x only ever advances when the step outruns the loop's
    // backward swing (Step > 2*pi*Radius); inside that it doubles back.
    let backtracks = |opts: &str| {
        let segs = motion_segs(&format!(
            "5 srand newpath 0 100 moveto 200 100 lineto << /RadiusJitter 0 {opts} >> pkscumble"
        ));
        segs.windows(2).filter(|w| w[1].1 < w[0].1 - 1e-9).count()
    };
    assert!(backtracks("/Radius 5 /Step 8") > 20, "tight loops overlap");
    assert_eq!(
        backtracks("/Radius 2 /Step 20"),
        0,
        "a wide step opens the loop"
    );
}

#[test]
fn scumble_chains_stay_within_a_radius_of_the_spine() {
    let segs = motion_segs(
        "5 srand newpath 0 100 moveto 200 100 lineto \
         << /Radius 6 /RadiusJitter 0.5 /Squash 0.5 >> pkscumble",
    );
    assert_eq!(segs.iter().filter(|s| s.0 == 0).count(), 1, "one chain");
    assert!(segs.iter().all(|s| s.0 == 0 || s.0 == 1));
    for s in &segs {
        // across the spine: Radius*(1+Jitter)*Squash at most
        assert!((s.2 - 100.0).abs() <= 6.0 * 1.5 * 0.5 + 1e-6, "y {}", s.2);
    }
}

#[test]
fn scumble_makes_a_circle_from_a_point_and_a_chain_per_subpath() {
    let circle = motion_segs(
        "5 srand newpath 50 50 moveto << /Radius 8 /RadiusJitter 0 >> pkscumble flattenpath",
    );
    assert_eq!(circle.iter().filter(|s| s.0 == 3).count(), 1, "closed");
    let xs: Vec<f64> = circle.iter().filter(|s| s.0 <= 1).map(|s| s.1).collect();
    let w = xs.iter().cloned().fold(-1e9, f64::max) - xs.iter().cloned().fold(1e9, f64::min);
    assert!((w - 16.0).abs() < 0.1, "diameter {w}");
    let two = motion_segs(
        "5 srand newpath 0 0 moveto 80 0 lineto 0 60 moveto 80 60 lineto << >> pkscumble",
    );
    assert_eq!(two.iter().filter(|s| s.0 == 0).count(), 2, "one chain each");
}

#[test]
fn tap_strokes_follow_direction_length_and_spacing() {
    let segs = motion_segs(&format!(
        "5 srand {SPINE} << /Spacing 20 /Length 8 /LengthJitter 0 /Direction 90 \
           /Lean 0 /Scatter 0 >> pktap"
    ));
    let s = strokes(&segs);
    // stops at 0 20 40 60 80 100: the end lands on a pitch multiple
    assert_eq!(s.len(), 6, "{s:?}");
    for (i, st) in s.iter().enumerate() {
        assert!((st.0.0 - 20.0 * i as f64).abs() < 1e-6 && (st.0.1 - 100.0).abs() < 1e-6);
        assert!((stroke_len(st) - 8.0).abs() < 1e-6);
        assert!((heading(st) - 90.0).abs() < 1e-6);
    }
}

#[test]
fn tap_skips_a_trailing_stop_that_would_double_up_the_last_tap() {
    // 90 long at spacing 40: stops 0, 40, 80, then the guaranteed end at
    // 90, 10 past the last: a tap there would be a pile-up.
    let n = |len: &str| {
        strokes(&motion_segs(&format!(
            "5 srand newpath 0 0 moveto {len} 0 lineto << /Spacing 40 >> pktap"
        )))
        .len()
    };
    assert_eq!(n("90"), 3);
    assert_eq!(n("80"), 3, "an exact multiple keeps its last tap");
    assert_eq!(n("100"), 4, "past halfway, the end is its own tap");
}

#[test]
fn tap_at_zero_length_makes_point_strokes_that_pkdab_reads_as_clumps() {
    let segs = motion_segs(&format!(
        "5 srand {SPINE} << /Spacing 25 /Length 0 /Scatter 0 >> pktap"
    ));
    let s = strokes(&segs);
    assert!(s.len() >= 4);
    assert!(s.iter().all(|st| stroke_len(st) == 0.0), "{s:?}");

    let mut it = fresh(400, 200);
    it.run_str(&format!(
        "0 0 0 setrgbcolor 5 srand 20 0 translate 0 -60 translate {SPINE} \
         << /Spacing 25 /Length 0 /Scatter 0 >> pktap << /Size 3 /Spread 9 /Clump 12 >> pkdab"
    ))
    .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    // five clumps of twelve dabs, not five specks
    assert!(ink_count(&it) > 5 * 150, "ink {}", ink_count(&it));
}

#[test]
fn tap_follows_every_subpath() {
    let s = strokes(&motion_segs(
        "5 srand newpath 0 0 moveto 40 0 lineto 0 50 moveto 40 50 lineto \
         << /Spacing 20 /Scatter 0 >> pktap",
    ));
    assert_eq!(s.iter().filter(|st| st.0.1 == 0.0).count(), 3);
    assert_eq!(s.iter().filter(|st| st.0.1 == 50.0).count(), 3);
}

#[test]
fn pull_strokes_start_on_the_spine_and_run_the_asked_way() {
    for (dir, sign) in [(-90.0, -1.0), (90.0, 1.0)] {
        let s = strokes(&motion_segs(&format!(
            "5 srand {SPINE} << /Spacing 10 /Wander 0 /Length 30 /LengthJitter 0 \
               /Direction {dir} /Lean 0 >> pkpull"
        )));
        assert_eq!(s.len(), 11);
        for (i, st) in s.iter().enumerate() {
            assert!((st.0.0 - 10.0 * i as f64).abs() < 1e-3, "starts on a stop");
            assert!((st.0.1 - 100.0).abs() < 1e-6, "starts on the spine");
            assert!((st.1.1 - (100.0 + sign * 30.0)).abs() < 1e-6, "dir {dir}");
            assert!((st.1.0 - st.0.0).abs() < 1e-6, "straight vertical");
        }
    }
}

#[test]
fn pull_wander_slides_starts_along_the_spine_only() {
    let s = strokes(&motion_segs(&format!(
        "5 srand {SPINE} << /Spacing 10 /Wander 1 >> pkpull"
    )));
    let mut moved = 0;
    for st in &s {
        assert!((st.0.1 - 100.0).abs() < 1e-6, "left the spine");
        assert!(
            (-1e-6..=100.0 + 1e-6).contains(&st.0.0),
            "start x {} is past an end of the spine",
            st.0.0
        );
        if (st.0.0 / 10.0 - (st.0.0 / 10.0).round()).abs() > 1e-6 {
            moved += 1;
        }
    }
    assert!(moved > 4, "wander moved only {moved} starts");

    // The first stroke of a spine starts on it however far Wander slides.
    for seed in 1..40 {
        let s = strokes(&motion_segs(&format!(
            "{seed} srand newpath 0 0 moveto 100 0 lineto << /Spacing 20 /Wander 1 >> pkpull"
        )));
        for st in &s {
            assert!(
                (-1e-6..=100.0 + 1e-6).contains(&st.0.0),
                "seed {seed}: start x {} is off the guide",
                st.0.0
            );
        }
    }
}

#[test]
fn pull_wander_follows_a_curved_or_cornered_spine() {
    // Quarter circle of radius 100: a start slid by the tangent would leave
    // it by about (slide^2 / 2r); sliding along the spine keeps it on it.
    let s = strokes(&motion_segs(
        "5 srand newpath 100 0 moveto 0 0 100 0 90 arc << /Spacing 10 /Wander 1 >> pkpull",
    ));
    assert!(s.len() > 10);
    for st in &s {
        let r = (st.0.0.powi(2) + st.0.1.powi(2)).sqrt();
        assert!((r - 100.0).abs() < 0.3, "start at radius {r}");
    }
    // A corner, both on a spacing multiple and *between* two stops (Codex
    // round 2: interpolating the stops cut across the corner, starting a
    // stroke near (88.8, 2.9), which is on neither leg).
    for corner in ["100", "95", "93.7"] {
        for seed in 1..25 {
            let s = strokes(&motion_segs(&format!(
                "{seed} srand newpath 0 0 moveto {corner} 0 lineto {corner} 100 lineto \
                 << /Spacing 20 /Wander 1 /LengthJitter 0 >> pkpull"
            )));
            assert!(s.len() > 8);
            let c: f64 = corner.parse().expect("number");
            for st in &s {
                let (x, y) = st.0;
                let on_leg1 = y.abs() < 1e-3 && (-1e-3..=c + 1e-3).contains(&x);
                let on_leg2 = (x - c).abs() < 1e-3 && (-1e-3..=100.0 + 1e-3).contains(&y);
                assert!(
                    on_leg1 || on_leg2,
                    "corner {corner} seed {seed}: start ({x}, {y}) is off the guide"
                );
            }
        }
    }
}

/// Every flattened point of a bowed stroke projects onto its heading within
/// the stroke's own length: the curve may bend sideways but never overshoots
/// its tip and turns back.
fn assert_bowed_strokes_stay_within_their_length(code: &str, length: f64) {
    let segs = motion_segs(&format!("{code} flattenpath"));
    let mut pts: Vec<(f64, f64)> = Vec::new();
    let mut checked = 0;
    let mut flush = |pts: &mut Vec<(f64, f64)>| {
        if pts.len() < 3 {
            pts.clear();
            return;
        }
        let (sx, sy) = pts[0];
        let (ex, ey) = pts[pts.len() - 1];
        let (dx, dy) = (ex - sx, ey - sy);
        let l = (dx * dx + dy * dy).sqrt();
        assert!((l - length).abs() < 1e-2, "chord {l}, wanted {length}");
        for &(x, y) in pts.iter() {
            let along = ((x - sx) * dx + (y - sy) * dy) / l;
            assert!(
                (-1e-3..=l + 1e-3).contains(&along),
                "point ({x}, {y}) projects to {along}, outside 0..{l}"
            );
        }
        checked += 1;
        pts.clear();
    };
    for s in &segs {
        match s.0 {
            0 => {
                flush(&mut pts);
                pts.push((s.1, s.2));
            }
            1 => pts.push((s.1, s.2)),
            _ => {}
        }
    }
    flush(&mut pts);
    assert!(checked > 3, "only {checked} curved strokes checked");
}

#[test]
fn bowed_pulls_do_not_overshoot_their_tip() {
    for dir in [0, 90, -90, 37] {
        assert_bowed_strokes_stay_within_their_length(
            &format!(
                "5 srand {SPINE} << /Spacing 10 /Wander 0 /Length 30 /LengthJitter 0 \
                   /Lean 0 /Direction {dir} /Bow 0.1 >> pkpull"
            ),
            30.0,
        );
    }
}

#[test]
fn bowed_crisscross_strokes_do_not_overshoot_their_tip() {
    assert_bowed_strokes_stay_within_their_length(
        "5 srand 0 0 200 100 screct \
         << /Count 20 /Length 20 /LengthJitter 0 /Spread 0 /Bow 0.2 >> pkcrisscross",
        20.0,
    );
}

#[test]
fn pull_bow_curves_and_lean_tilts() {
    let curved = motion_segs(&format!("5 srand {SPINE} << /Bow 0.3 >> pkpull"));
    assert!(curved.iter().filter(|s| s.0 == 2).count() > 5);
    let s = strokes(&motion_segs(&format!(
        "5 srand {SPINE} << /Lean 20 /Wander 0 >> pkpull"
    )));
    let max_dev = s
        .iter()
        .map(|st| (heading(st) + 90.0).abs())
        .fold(0.0, f64::max);
    assert!(
        max_dev > 5.0 && max_dev <= 20.0 + 1e-6,
        "lean spread {max_dev}"
    );
}

#[test]
fn zigzag_alternates_about_a_spine_it_starts_and_ends_on() {
    let segs = motion_segs(&format!(
        "5 srand {SPINE} << /Amplitude 5 /Wavelength 20 /AmpJitter 0 /WaveJitter 0 >> pkzigzag"
    ));
    assert_eq!(segs.iter().filter(|s| s.0 == 0).count(), 1, "one polyline");
    // stops every 10: x = 0..100; ends on the spine, interior at +-5
    assert_eq!(segs.len(), 11);
    assert!((segs[0].2 - 100.0).abs() < 1e-6 && (segs[10].2 - 100.0).abs() < 1e-6);
    for (i, s) in segs.iter().enumerate().take(10).skip(1) {
        assert!((s.1 - 10.0 * i as f64).abs() < 1e-6, "even slots");
        let want = if i % 2 == 1 { 105.0 } else { 95.0 };
        assert!((s.2 - want).abs() < 1e-6, "peak {i}: {}", s.2);
    }
}

#[test]
fn zigzag_follows_a_curved_spine_on_its_normal() {
    // A quarter circle of radius 100: every interior vertex sits Amplitude
    // off the circle, to alternating sides.
    let segs = motion_segs(
        "5 srand newpath 100 0 moveto 0 0 100 0 90 arc \
         << /Amplitude 4 /Wavelength 30 /AmpJitter 0 /WaveJitter 0 >> pkzigzag",
    );
    assert!(segs.len() > 8);
    let n = segs.len();
    for (i, s) in segs.iter().enumerate().take(n - 1).skip(1) {
        let r = (s.1 * s.1 + s.2 * s.2).sqrt();
        let off = r - 100.0;
        assert!((off.abs() - 4.0).abs() < 0.2, "vertex {i} off by {off}");
        if i > 1 {
            let prev = (segs[i - 1].1.powi(2) + segs[i - 1].2.powi(2)).sqrt() - 100.0;
            assert!(prev * off < 0.0, "vertex {i} did not alternate");
        }
    }
}

#[test]
fn zigzag_wave_jitter_moves_vertices_along_the_spine_without_reordering() {
    let segs = motion_segs(&format!(
        "5 srand {SPINE} << /WaveJitter 1 /Wavelength 20 >> pkzigzag"
    ));
    let xs: Vec<f64> = segs.iter().map(|s| s.1).collect();
    assert!(xs.windows(2).all(|w| w[1] > w[0]), "reordered: {xs:?}");
    assert!(
        xs.iter()
            .enumerate()
            .skip(1)
            .take(xs.len() - 2)
            .any(|(i, x)| (x - 10.0 * i as f64).abs() > 0.5)
    );
}

#[test]
fn zigzag_a_point_is_one_degenerate_stroke_and_subpaths_stay_apart() {
    let p = motion_segs("5 srand newpath 30 30 moveto << >> pkzigzag");
    assert_eq!(p, vec![(0, 30.0, 30.0), (1, 30.0, 30.0)]);
    let two = motion_segs(
        "5 srand newpath 0 0 moveto 50 0 lineto 0 40 moveto 50 40 lineto << >> pkzigzag",
    );
    assert_eq!(two.iter().filter(|s| s.0 == 0).count(), 2);
}

#[test]
fn motions_given_nothing_build_nothing() {
    for code in [
        "newpath << >> pkscumble",
        "newpath << >> pktap",
        "newpath << >> pkpull",
        "newpath << >> pkzigzag",
        // a region with no area (an empty path captured by scpath)
        "newpath scpath << >> pkcrisscross",
        "newpath scpath << >> pkscumblein",
    ] {
        assert!(motion_segs(code).is_empty(), "{code} built a path");
    }
}

#[test]
fn motions_compose_with_every_stroke_preset() {
    // Small on purpose: this is a 6 x 6 cross product in a debug build.
    let motions = [
        "0 0 360 120 screct << /Count 10 /Length 40 >> pkcrisscross",
        "0 0 360 120 screct << /Count 8 /Radius 6 >> pkscumblein",
        "newpath 20 60 moveto 120 70 lineto << /Radius 8 >> pkscumble",
        "newpath 20 60 moveto 340 80 lineto << /Spacing 40 /Length 8 >> pktap",
        "newpath 20 100 moveto 340 100 lineto << /Spacing 40 /Length 30 >> pkpull",
        "newpath 20 60 moveto 340 60 lineto << /Amplitude 8 /Wavelength 40 >> pkzigzag",
    ];
    for motion in motions {
        for brush in ["pkbroad", "pkfan", "pkliner", "pkoil", "pkdab", "pkribbon"] {
            let mut it = fresh(400, 200);
            it.run_str(&format!("0 0 0 setrgbcolor 5 srand {motion} << >> {brush}"))
                .unwrap_or_else(|e| panic!("{motion} -> {brush}: {}", it.error_report(&e)));
            assert!(ink_count(&it) > 20, "{motion} -> {brush} painted nothing");
            assert!(it.operand_stack().is_empty(), "{motion} -> {brush} leaked");
        }
    }
}

#[test]
fn motions_advertise_exactly_their_tested_parameters() {
    let src = std::fs::read_to_string("lib/paintkit.ps").expect("read paintkit");
    for (proc_name, expected) in [
        (
            "pkcrisscross",
            &["Angle", "Bow", "Count", "Length", "LengthJitter", "Spread"][..],
        ),
        (
            "pkscumblein",
            &["Count", "Radius", "RadiusJitter", "Squash"][..],
        ),
        (
            "pkscumble",
            &["Radius", "RadiusJitter", "Squash", "Step"][..],
        ),
        (
            "pktap",
            &[
                "Direction",
                "Lean",
                "Length",
                "LengthJitter",
                "Scatter",
                "Spacing",
            ][..],
        ),
        (
            "pkpull",
            &[
                "Bow",
                "Direction",
                "Lean",
                "Length",
                "LengthJitter",
                "Spacing",
                "Wander",
            ][..],
        ),
        (
            "pkzigzag",
            &["Amplitude", "AmpJitter", "Wavelength", "WaveJitter"][..],
        ),
    ] {
        let def = src
            .find(&format!("\n/{proc_name} {{"))
            .unwrap_or_else(|| panic!("{proc_name} def"));
        let head = &src[..def];
        let tags = &head[head.rfind("% @kind").expect("tags")..];
        let mut advertised: Vec<String> = tags
            .lines()
            .filter_map(|l| l.strip_prefix("% @param: /"))
            .map(|l| l.split_whitespace().next().unwrap_or("").to_string())
            .collect();
        advertised.sort();
        let mut expected: Vec<String> = expected.iter().map(|s| s.to_string()).collect();
        expected.sort();
        assert_eq!(
            advertised, expected,
            "{proc_name}'s advertised parameters drifted"
        );
        // ...and each one is actually read by the proc's body.
        let body_end = src[def + 1..].find("\n} def").expect("end") + def + 1;
        let body = &src[def..body_end];
        for p in &expected {
            assert!(
                body.contains(&format!("/{p} ")),
                "{proc_name} never reads /{p}"
            );
        }
    }
}

#[test]
fn ghostscript_accepts_the_motion_specimen() {
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
            "examples/paintkit_motion_demo.ps",
        ])
        .status()
        .expect("run gs");
    assert!(
        status.success(),
        "gs rejected examples/paintkit_motion_demo.ps"
    );
}
