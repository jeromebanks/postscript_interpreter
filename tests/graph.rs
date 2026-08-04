//! The graphing library (lib/graph.ps, issue #13): frame mapping,
//! the three 2D curve drivers, axes, and the 3D view/surface
//! machinery. Arithmetic is pinned exactly where the trig lands on
//! clean values (multiples of 90 degrees); plotsurface gets an
//! ink-coverage check the same way artkit's ldraw does, since a mesh
//! doesn't reduce to one clean number the way a single point does.

use pscat::Interp;

fn load(it: &mut Interp) {
    let src = std::fs::read("lib/graph.ps").expect("graph.ps present");
    it.run_source(&src)
        .unwrap_or_else(|e| panic!("graph.ps failed to load: {}", it.error_report(&e)));
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
        .filter(|p| p.red() < 128)
        .count()
}

#[test]
fn setframe_maps_domain_linearly_including_extrapolation() {
    // px=50,py=60,pw=400,ph=300 over domain [0,10]x[0,10].
    let got = eval_f64(
        "0 0 10 10 50 60 400 300 setframe \
         0 gmapx 10 gmapx 5 gmapx 0 gmapy 10 gmapy 5 gmapy \
         20 gmapx",
    );
    // gmapx/gmapy don't clamp -- a value outside the domain (20, last)
    // extrapolates along the same line rather than erroring.
    assert_eq!(got, [50.0, 450.0, 250.0, 60.0, 360.0, 210.0, 850.0]);
}

#[test]
fn gmoveto_glineto_build_a_mapped_path() {
    let got =
        eval_f64("0 0 10 20 50 50 400 400 setframe newpath 0 0 gmoveto 10 20 glineto currentpoint");
    assert_eq!(got, [450.0, 450.0]);
}

#[test]
fn plotfn_samples_n_plus_one_points_through_the_mapping() {
    // y = 2x over x in [0,10], mapped onto a domain that matches its
    // own range so the endpoint lands on a clean corner.
    let got = eval_f64(
        "0 0 10 20 50 50 400 400 setframe \
         newpath 0 10 4 { 2 mul } plotfn currentpoint",
    );
    assert_eq!(got, [450.0, 450.0]);
    // n=4 -> 5 points; pathbbox's lower-left is the start (0,0) mapped.
    let got = eval_f64(
        "0 0 10 20 50 50 400 400 setframe \
         newpath 0 10 4 { 2 mul } plotfn pathbbox",
    );
    assert_eq!(got, [50.0, 50.0, 450.0, 450.0]);
}

#[test]
fn plotparam_traces_a_unit_circle_at_clean_angles() {
    // x=cos t, y=sin t, domain/viewport chosen so gmapx/gmapy is
    // exactly 100*(v+1): t=0 and t=360 both land on cos=1,sin=0 ->
    // device (200,100), floating-point cos/sin of 360 included.
    let got = eval_f64(
        "-1 -1 1 1 0 0 200 200 setframe \
         newpath 0 360 4 { dup cos exch sin } plotparam currentpoint",
    );
    assert!((got[0] - 200.0).abs() < 1e-6, "x: {got:?}");
    assert!((got[1] - 100.0).abs() < 1e-6, "y: {got:?}");
}

#[test]
fn plotpolar_traces_a_circle_of_the_given_radius() {
    // r=5 constant, theta 0..360 in 4 steps -> last point back at
    // theta=360 (x=5,y=0), same mapping as above.
    let got = eval_f64(
        "-5 -5 5 5 0 0 200 200 setframe \
         newpath 0 360 4 { pop 5 } plotpolar currentpoint",
    );
    assert!((got[0] - 200.0).abs() < 1e-6, "x: {got:?}");
    assert!((got[1] - 100.0).abs() < 1e-6, "y: {got:?}");
}

#[test]
fn axes_draws_the_border_plus_outward_ticks() {
    // px=20,py=30,pw=400,ph=300, tlen=8: ticks project outward past
    // the bottom and left edges only (matplotlib-style), so the
    // bbox grows on those two sides and nowhere else.
    let got = eval_f64("0 0 10 10 20 30 400 300 setframe newpath 5 5 8 axes pathbbox");
    assert_eq!(got, [12.0, 22.0, 420.0, 330.0]);
}

#[test]
fn setview_and_project3_pin_exact_camera_angles() {
    // az=0,el=0,scale=1,cx=cy=0 is the identity view: (x,y,z) -> (x,y).
    let got = eval_f64("0 0 1 0 0 setview 3 4 5 project3");
    assert_eq!(got, [3.0, 4.0]);
    // az=90,el=90: rotating (1,0,0) 90 about Z then 90 about X sends it
    // to the origin exactly (hand-derived, see lib/graph.ps's project3).
    let got = eval_f64("90 90 1 0 0 setview 1 0 0 project3");
    assert!(got[0].abs() < 1e-9 && got[1].abs() < 1e-9, "got {got:?}");
    // scale and translate apply after projection.
    let got = eval_f64("0 0 10 100 200 setview 3 4 5 project3");
    assert_eq!(got, [130.0, 240.0]);
}

#[test]
fn axes3_draws_three_segments_from_the_projected_origin() {
    let got = eval_f64("0 0 1 0 0 setview newpath 5 6 7 axes3 pathbbox");
    // Identity view: origin (0,0,0)->(0,0); endpoints (5,0,0),(0,6,0),
    // (0,0,7) project to (5,0),(0,6),(0,0) respectively.
    assert_eq!(got, [0.0, 0.0, 5.0, 6.0]);
}

#[test]
fn surfacerow_and_surfacecol_render_a_mesh() {
    let mut it = Interp::with_page(200, 200).expect("page");
    load(&mut it);
    it.run_str(
        "0 0 30 100 100 setview \
         1.2 setlinewidth newpath \
         -3 3 0 8 { pop pop 0 } surfacerow stroke \
         newpath -3 3 0 8 { pop pop 0 } surfacecol stroke",
    )
    .unwrap_or_else(|e| panic!("surfacerow/col failed: {}", it.error_report(&e)));
    assert!(ink_count(&it) > 100, "expected two crossing lines' ink");
}

#[test]
fn plotsurface_renders_a_wireframe_mesh() {
    let mut it = Interp::with_page(200, 200).expect("page");
    load(&mut it);
    it.run_str(
        "20 20 18 100 100 setview \
         0.7 setlinewidth newpath \
         -4 4 -4 4 8 8 { dup mul exch dup mul add sqrt cos } plotsurface stroke",
    )
    .unwrap_or_else(|e| panic!("plotsurface failed: {}", it.error_report(&e)));
    assert!(ink_count(&it) > 500, "expected a mesh's worth of ink");
}

#[test]
fn plotsurface_forwards_the_sampling_proc_without_invoking_it_early() {
    // Regression guard: plotsurface hands its captured proc onward to
    // surfacerow/surfacecol wrapped as `{ gsffn }`. A bare `gsffn`
    // reference there would auto-execute mid-expression instead of
    // being pushed as surfacerow/surfacecol's operand, breaking the
    // call with a stackunderflow well before any count could be
    // taken. Rows and columns each resample every vertex on their own
    // pass (no z caching between the two), so the proc runs exactly
    // 2*(nx+1)*(ny+1) times -- pinning that count is really pinning
    // "the forwarding call chain still runs the proc at all."
    let got = eval_f64(
        "0 0 1 50 50 setview /calls 0 def \
         newpath -2 2 -2 2 3 4 { pop pop /calls calls 1 add def 0 } plotsurface \
         calls",
    );
    assert_eq!(got, [(2 * 4 * 5) as f64]);
}

#[test]
fn ghostscript_accepts_graph() {
    let gs_ok = std::process::Command::new("gs")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success());
    if !gs_ok {
        eprintln!("skipping gs compatibility check: gs not installed");
        return;
    }
    let lib = std::fs::read_to_string("lib/graph.ps").expect("graph.ps");
    let driver = "newpath 0 0 6.283185 2 40 40 300 300 setframe \
        0 6.283185 100 { sin } plotfn stroke \
        newpath 4 4 6 axes stroke \
        newpath -1 -1 1 1 380 40 200 200 setframe \
        0 360 200 { 3 mul cos } plotpolar stroke \
        30 20 20 500 300 setview \
        newpath -4 4 -4 4 16 16 { dup mul exch dup mul add sqrt 60 mul cos } plotsurface stroke \
        newpath 4 4 4 axes3 stroke \
        showpage\n";
    let dir = std::env::temp_dir().join(format!("pscat-graph-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let combined = dir.join("graph_gs.ps");
    std::fs::write(&combined, format!("{lib}\n{driver}")).expect("write");
    let status = std::process::Command::new("gs")
        .args([
            "-dNOPAUSE",
            "-dBATCH",
            "-q",
            "-sDEVICE=png16m",
            "-g600x400",
            "-r72",
            "-o/dev/null",
        ])
        .arg(&combined)
        .status()
        .expect("run gs");
    assert!(status.success(), "gs rejected graph.ps");
}
