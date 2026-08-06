//! The data-visualization library (lib/dataviz.ps, issue #14): the
//! categorical frame mapping, bar/line/area/scatter/pie drivers, axes,
//! and the default color cycle. Frame arithmetic is pinned exactly
//! (it's all linear); the drawing procs get ink-coverage checks where
//! a single number can't capture the shape, plus two tests aimed
//! specifically at the bug class that bit lib/graph.ps twice before:
//! a donut filled solid to the center, and a pie wedge sweeping the
//! wrong direction. Both are invisible to a plain ink-count assertion
//! (same amount of ink either way), which is exactly why they're
//! pinned to specific pixels instead.

use pscat::Interp;

fn load(it: &mut Interp) {
    let src = std::fs::read("lib/dataviz.ps").expect("dataviz.ps present");
    it.run_source(&src)
        .unwrap_or_else(|e| panic!("dataviz.ps failed to load: {}", it.error_report(&e)));
}

fn eval(src: &str) -> Vec<String> {
    let mut it = Interp::new();
    load(&mut it);
    it.run_str(src)
        .unwrap_or_else(|e| panic!("eval of {src:?} failed: {}", it.error_report(&e)));
    it.operand_stack().iter().map(|o| o.repr()).collect()
}

fn eval_f64(src: &str) -> Vec<f64> {
    eval(src)
        .iter()
        .map(|s| s.parse().unwrap_or_else(|_| panic!("not a number: {s}")))
        .collect()
}

fn ink_count(it: &Interp) -> usize {
    it.gfx()
        .pixmap
        .pixels()
        .iter()
        .filter(|p| p.red() < 250 || p.green() < 250 || p.blue() < 250)
        .count()
}

// Pixmap rows run top-down; PostScript's device space is bottom-up.
// (See tests/handwriting.rs's `792 - 420 + dy` for the same convention.)
fn pixel_at(it: &Interp, px: f64, py: f64, height: u32) -> (u8, u8, u8) {
    let p = it
        .gfx()
        .pixmap
        .pixel(px.round() as u32, height - py.round() as u32)
        .expect("pixel in bounds");
    (p.red(), p.green(), p.blue())
}

#[test]
fn setdvframe_maps_values_and_categories_linearly() {
    let got = eval_f64(
        "0 10 50 60 400 300 setdvframe \
         0 dvmapy 10 dvmapy 5 dvmapy 2 5 dvcatx 5 0.2 dvbarw",
    );
    assert_eq!(got, [60.0, 360.0, 210.0, 250.0, 64.0]);
}

#[test]
fn dvbounds_finds_raw_min_and_max() {
    let got = eval_f64("[3 7 2 9 5] dvbounds");
    assert_eq!(got, [2.0, 9.0]);
}

#[test]
fn dvsum_totals_the_array() {
    let got = eval_f64("[3 7 2 9 5] dvsum");
    assert_eq!(got, [26.0]);
}

#[test]
fn barchart_draws_one_rect_from_baseline_to_value() {
    // Painting consumes the path in this interpreter (fill/stroke are
    // an implicit newpath — a deliberate deviation, see HANDOFF.md),
    // and barchart fills each bar internally, so pathbbox can't see
    // it afterward; check pixels instead. n=1, gap=0.2, frame 0..10
    // over a 100x100 viewport at the origin: center=50, width=80, so
    // x spans [10,90]; y spans [0, dvmapy(7)=70].
    let mut it = Interp::with_page(100, 100).expect("page");
    load(&mut it);
    it.run_str(
        "0 10 0 0 100 100 setdvframe \
         newpath [7] 0.2 { pop pop 0.2 0.4 0.8 } barchart",
    )
    .unwrap_or_else(|e| panic!("barchart failed: {}", it.error_report(&e)));
    assert_ne!(
        pixel_at(&it, 50.0, 35.0, 100),
        (255, 255, 255),
        "expected fill inside the bar"
    );
    assert_eq!(
        pixel_at(&it, 50.0, 85.0, 100),
        (255, 255, 255),
        "expected background above the bar's top"
    );
    assert_eq!(
        pixel_at(&it, 5.0, 35.0, 100),
        (255, 255, 255),
        "expected background left of the bar"
    );
}

#[test]
fn barchart_draws_negative_values_below_the_baseline() {
    // Baseline (value 0) at py+ph/2=100; bar for -3 spans devy 85..100.
    let mut it = Interp::with_page(100, 200).expect("page");
    load(&mut it);
    it.run_str(
        "-10 10 0 50 100 100 setdvframe \
         newpath [-3] 0.2 { pop pop 0.2 0.4 0.8 } barchart",
    )
    .unwrap_or_else(|e| panic!("barchart failed: {}", it.error_report(&e)));
    assert_ne!(
        pixel_at(&it, 50.0, 92.0, 200),
        (255, 255, 255),
        "expected fill below the baseline"
    );
    assert_eq!(
        pixel_at(&it, 50.0, 110.0, 200),
        (255, 255, 255),
        "expected background above the baseline"
    );
}

#[test]
fn barchart_survives_a_color_proc_that_touches_the_path() {
    // Direct check of the header's ordering promise: the color proc
    // runs before each bar's path is built, specifically so a
    // misbehaving proc (here, a stray newpath -- the exact hazard the
    // header names) can't drop the element. n=4, gap=0.2 over a
    // 100x100 viewport: centers 12.5/37.5/62.5/87.5, widths 20;
    // values 1..4 map to tops 10/20/30/40.
    let mut it = Interp::with_page(100, 100).expect("page");
    load(&mut it);
    it.run_str(
        "0 10 0 0 100 100 setdvframe \
         newpath [1 2 3 4] 0.2 { pop pop newpath 0.2 0.4 0.8 } barchart",
    )
    .unwrap_or_else(|e| panic!("barchart failed: {}", it.error_report(&e)));
    for (cx, top) in [12.5, 37.5, 62.5, 87.5]
        .iter()
        .zip([10.0, 20.0, 30.0, 40.0].iter())
    {
        assert_ne!(
            pixel_at(&it, *cx, top / 2.0, 100),
            (255, 255, 255),
            "expected the bar at x={cx} to render despite the proc's stray newpath"
        );
    }
    assert_eq!(
        pixel_at(&it, 50.0, 90.0, 100),
        (255, 255, 255),
        "expected background well above every bar"
    );
}

#[test]
fn linechart_samples_at_category_centers() {
    // n=3 over [0,100]x[0,100]: centers at 100*(0.5/3), 100*(1.5/3),
    // 100*(2.5/3) = 16.667, 50, 83.333; values 5,10,8 -> y 50,100,80.
    let got = eval_f64(
        "0 10 0 0 100 100 setdvframe \
         newpath [5 10 8] linechart pathbbox",
    );
    assert!((got[0] - 16.667).abs() < 1e-2, "x0: {got:?}");
    assert_eq!(got[1], 50.0);
    assert!((got[2] - 83.333).abs() < 1e-2, "x1: {got:?}");
    assert_eq!(got[3], 100.0);
}

#[test]
fn areachart_closes_down_to_the_baseline() {
    // Same data as the linechart test, but the closed baseline pulls
    // the bbox's y0 down to 0 even though every value is above zero.
    let got = eval_f64(
        "0 10 0 0 100 100 setdvframe \
         newpath [5 10 8] areachart pathbbox",
    );
    assert!((got[0] - 16.667).abs() < 1e-2, "x0: {got:?}");
    assert_eq!(got[1], 0.0);
    assert!((got[2] - 83.333).abs() < 1e-2, "x1: {got:?}");
    assert_eq!(got[3], 100.0);
}

#[test]
fn areachart_is_a_no_op_on_an_empty_series() {
    // Regression guard (found by cross-model review): the baseline-
    // closing code ran unconditionally, calling dvcatx with n=0 and
    // dividing by zero, even though the loop above it already treats
    // an empty series as a no-op -- same as every other driver here.
    let mut it = Interp::with_page(100, 100).expect("page");
    load(&mut it);
    it.run_str("0 10 0 0 100 100 setdvframe newpath [] areachart")
        .unwrap_or_else(|e| panic!("areachart on [] failed: {}", it.error_report(&e)));
}

#[test]
fn setscatterframe_maps_continuous_x_and_y() {
    let got = eval_f64(
        "0 0 10 10 50 50 400 300 setscatterframe \
         0 dvsmapx 10 dvsmapx 5 dvsmapy",
    );
    assert_eq!(got, [50.0, 450.0, 200.0]);
}

#[test]
fn scatterchart_draws_a_dot_per_point() {
    let mut it = Interp::with_page(300, 300).expect("page");
    load(&mut it);
    it.run_str(
        "0 0 10 10 20 20 260 260 setscatterframe \
         newpath [[1 1] [5 5] [9 9] [1 9] [9 1]] 8 { pop pop dvcolor } scatterchart",
    )
    .unwrap_or_else(|e| panic!("scatterchart failed: {}", it.error_report(&e)));
    assert!(ink_count(&it) > 200, "expected five dots' worth of ink");
}

#[test]
fn piechart_sweeps_clockwise_from_twelve_oclock() {
    // Four equal wedges (index 0..3), colors from DVColors: wedge 0 is
    // blue-dominant ([0.20 0.47 0.75]), wedge 3 is red-dominant
    // ([0.90 0.65 0.15]). Clockwise from 12 puts wedge 0 in the NE
    // quadrant (0..90 degrees) and wedge 3 in NW (180..270). A sweep
    // direction bug (arc instead of arcn) would swap them -- wedge 0
    // would land in NW instead, invisible to any ink-count check since
    // the same total ink is painted either way.
    let mut it = Interp::with_page(200, 200).expect("page");
    load(&mut it);
    it.run_str("newpath [1 1 1 1] 100 100 80 0 { pop dvcolor } piechart")
        .unwrap_or_else(|e| panic!("piechart failed: {}", it.error_report(&e)));
    // 45 degrees from center, radius 40 (well inside r=80): NE quadrant.
    let ne = pixel_at(
        &it,
        100.0 + 40.0 * 45f64.to_radians().cos(),
        100.0 + 40.0 * 45f64.to_radians().sin(),
        200,
    );
    assert!(
        ne.2 > ne.0 && ne.2 > ne.1,
        "NE point should be wedge 0 ([0.20 0.47 0.75], blue highest), got {ne:?}"
    );
    // 135 degrees from center: NW quadrant.
    let nw = pixel_at(
        &it,
        100.0 + 40.0 * 135f64.to_radians().cos(),
        100.0 + 40.0 * 135f64.to_radians().sin(),
        200,
    );
    assert!(
        nw.0 > nw.2,
        "NW point should be wedge 3 (red-dominant), got {nw:?}"
    );
}

#[test]
fn piechart_with_zero_inner_radius_fills_the_center() {
    let mut it = Interp::with_page(200, 200).expect("page");
    load(&mut it);
    it.run_str("newpath [1 1 1 1] 100 100 80 0 { pop dvcolor } piechart")
        .unwrap_or_else(|e| panic!("piechart failed: {}", it.error_report(&e)));
    let (r, g, b) = pixel_at(&it, 100.0, 100.0, 200);
    assert!(
        r < 250 || g < 250 || b < 250,
        "pie center should be filled, got ({r},{g},{b})"
    );
}

#[test]
fn piechart_with_inner_radius_leaves_the_center_empty() {
    // Regression guard: the donut branch builds two arcs walked in
    // opposite directions (outer clockwise, inner counterclockwise
    // back) to trace an annular sector. Get the inner arc's direction
    // or the lineto endpoints wrong and it silently fills solid to the
    // center instead -- an ink-count check can't tell the difference
    // (same ink either way), so this checks the actual center pixel.
    let mut it = Interp::with_page(200, 200).expect("page");
    load(&mut it);
    it.run_str("newpath [1 1 1 1] 100 100 80 30 { pop dvcolor } piechart")
        .unwrap_or_else(|e| panic!("piechart failed: {}", it.error_report(&e)));
    let (r, g, b) = pixel_at(&it, 100.0, 100.0, 200);
    assert_eq!(
        (r, g, b),
        (255, 255, 255),
        "donut center must stay background"
    );
}

#[test]
fn piechart_is_a_no_op_on_a_zero_total_series() {
    // Regression guard (found by cross-model review): a nonempty
    // series summing to zero (every category filtered to nothing)
    // divided by dptotal=0 and aborted with undefinedresult, even
    // though this is exactly the "nothing to draw" case every other
    // driver in this file already treats as a no-op.
    let mut it = Interp::with_page(200, 200).expect("page");
    load(&mut it);
    it.run_str("newpath [0 0] 100 100 80 0 { pop dvcolor } piechart")
        .unwrap_or_else(|e| panic!("piechart on [0 0] failed: {}", it.error_report(&e)));
}

#[test]
fn dvaxes_draws_the_border_plus_outward_ticks() {
    // Same px/py/pw/ph/tlen as graph.rs's own axes test, for an easy
    // cross-check: the border sets [20,30,420,330]; x-ticks (category
    // centers, unlike graph.ps's edge ticks) still only project
    // downward by tlen, so they don't move the x extent; y-ticks
    // (edge-based, 0..ny inclusive) project leftward by tlen. Same
    // resulting bbox as graph.ps's axes despite the different x-tick
    // placement convention.
    let got = eval_f64("0 10 20 30 400 300 setdvframe newpath 5 5 8 dvaxes pathbbox");
    assert_eq!(got, [12.0, 22.0, 420.0, 330.0]);
}

#[test]
fn dvaxes_with_zero_y_ticks_draws_the_border_only() {
    // Regression guard (found by cross-model review): `0 1 davny for`
    // with davny=0 still runs once (0<=0), and the tick-position
    // formula divides by davny -- a caller asking for a bordered axis
    // with no value gridlines used to hit undefinedresult instead.
    // With no y-ticks, only the x-ticks push the bbox below py=30
    // (to 22); the left edge stays at px=20 since no y-tick projects
    // leftward.
    let got = eval_f64("0 10 20 30 400 300 setdvframe newpath 5 0 8 dvaxes pathbbox");
    assert_eq!(got, [20.0, 22.0, 420.0, 330.0]);
}

#[test]
fn dvaxes_x_ticks_land_under_category_centers() {
    // Direct check for the alignment issue: a bar chart's dvcatx(i,n)
    // and dvaxes's own x-tick placement must agree exactly, or a
    // bar+line combo (like the gallery piece) would show ticks
    // between bars instead of under them.
    let mut it = Interp::with_page(500, 400).expect("page");
    load(&mut it);
    it.run_str(
        "0 10 20 30 400 300 setdvframe newpath 5 5 8 dvaxes 0 setgray stroke \
         2 5 dvcatx",
    )
    .unwrap_or_else(|e| panic!("dvaxes failed: {}", it.error_report(&e)));
    let tick_x: f64 = it
        .pop()
        .expect("dvcatx result")
        .repr()
        .parse()
        .expect("number");
    // pathbbox of just the last x-tick segment isn't isolated, so
    // instead confirm ink exists directly under dvcatx(2,5) at the
    // tick's y-range (py down to py-tlen = 30 down to 22).
    let has_ink_at = |it: &Interp, x: u32| {
        (22u32..30).any(|y| {
            it.gfx()
                .pixmap
                .pixel(x, 400 - y)
                .is_some_and(|p| p.red() < 128)
        })
    };
    assert!(
        has_ink_at(&it, tick_x.round() as u32),
        "expected a tick directly under dvcatx(2,5)={tick_x}"
    );
    // The pair of assertions is what actually pins "centers, not
    // edges": an edge-based tick (graph.ps's own axes convention,
    // px + pw*i/nx) for nx=5 would land at 20,100,180,260,340,420 --
    // 180 is the edge nearest category 2's center (220) and is not
    // itself a valid center for any i, so ink there would mean the
    // edge-tick convention crept back in.
    assert!(
        !has_ink_at(&it, 180),
        "expected no tick at the edge position 180 (only at category centers)"
    );
}

#[test]
fn dvcolor_cycles_through_the_default_palette() {
    let got = eval_f64("0 dvcolor 8 dvcolor");
    // Index 8 wraps to index 0 (DVColors has 8 entries) -> same color.
    assert_eq!(got, [got[0], got[1], got[2], got[0], got[1], got[2]]);
}

#[test]
fn ghostscript_accepts_dataviz() {
    let gs_ok = std::process::Command::new("gs")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success());
    if !gs_ok {
        eprintln!("skipping gs compatibility check: gs not installed");
        return;
    }
    let lib = std::fs::read_to_string("lib/dataviz.ps").expect("dataviz.ps");
    let driver = "newpath 0 10 40 40 400 250 setdvframe \
        [3 7 2 9 5] 0.2 { pop dvcolor } barchart \
        newpath [3 7 2 9 5] linechart 0 setgray stroke \
        newpath 5 5 8 dvaxes stroke \
        0 0 10 10 470 40 200 200 setscatterframe \
        newpath [[1 1] [5 5] [9 9]] 6 { pop pop dvcolor } scatterchart \
        newpath [3 7 2 9 5] 700 150 60 0 { pop dvcolor } piechart \
        newpath [3 7 2 9 5] 700 350 60 25 { pop dvcolor } piechart \
        showpage\n";
    let dir = std::env::temp_dir().join(format!("pscat-dataviz-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let combined = dir.join("dataviz_gs.ps");
    std::fs::write(&combined, format!("{lib}\n{driver}")).expect("write");
    let status = std::process::Command::new("gs")
        .args([
            "-dNOPAUSE",
            "-dBATCH",
            "-q",
            "-sDEVICE=png16m",
            "-g900x450",
            "-r72",
            "-o/dev/null",
        ])
        .arg(&combined)
        .status()
        .expect("run gs");
    assert!(status.success(), "gs rejected dataviz.ps");
}
