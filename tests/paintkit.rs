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
    let driver = "3 srand \
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
