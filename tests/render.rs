//! Headless rendering tests: run a program, check pixels on the canvas.
//! Device y is flipped relative to user space, so user (x, y) on a
//! 100-high page is pixel (x, 100 - y).

use pscat::{Interp, PsError};

fn render(src: &str) -> Interp {
    let mut it = Interp::with_page(100, 100).expect("test page");
    it.run_str(src)
        .unwrap_or_else(|e| panic!("render of {src:?} failed: {e}"));
    it
}

fn pixel(it: &Interp, x: u32, y: u32) -> (u8, u8, u8) {
    let p = it
        .gfx()
        .pixmap
        .pixel(x, y)
        .unwrap_or_else(|| panic!("pixel ({x},{y}) out of bounds"));
    (p.red(), p.green(), p.blue())
}

const BLACK: (u8, u8, u8) = (0, 0, 0);
const WHITE: (u8, u8, u8) = (255, 255, 255);

#[test]
fn fill_square() {
    let it = render("newpath 10 10 moveto 90 10 lineto 90 90 lineto 10 90 lineto closepath fill");
    assert_eq!(pixel(&it, 50, 50), BLACK);
    assert_eq!(pixel(&it, 5, 5), WHITE);
    // Fill consumed the path.
    assert!(it.gfx().state().path.is_empty());
}

#[test]
fn page_starts_white() {
    let it = render("newpath");
    assert_eq!(pixel(&it, 50, 50), WHITE);
}

#[test]
fn y_axis_points_up() {
    // A square in the *lower* half of user space must land in the
    // *bottom* half of the image.
    let it = render("newpath 40 10 moveto 60 10 lineto 60 30 lineto 40 30 lineto closepath fill");
    assert_eq!(pixel(&it, 50, 80), BLACK); // device y=80 == user y=20
    assert_eq!(pixel(&it, 50, 20), WHITE);
}

#[test]
fn stroke_line_with_width_and_color() {
    let it = render("0 0 1 setrgbcolor 6 setlinewidth newpath 10 50 moveto 90 50 lineto stroke");
    assert_eq!(pixel(&it, 50, 50), (0, 0, 255));
    assert_eq!(pixel(&it, 50, 40), WHITE); // 6 wide, not 20
}

#[test]
fn setgray_levels() {
    let it = render(
        "0.5 setgray newpath 10 10 moveto 90 10 lineto 90 90 lineto 10 90 lineto closepath fill",
    );
    let (r, g, b) = pixel(&it, 50, 50);
    assert_eq!((r, g), (g, b), "gray must have equal channels");
    assert!((125..=130).contains(&r), "0.5 gray ≈ 127-128, got {r}");
}

#[test]
fn arc_fills_a_circle() {
    let it = render("newpath 50 50 30 0 360 arc closepath fill");
    assert_eq!(pixel(&it, 50, 50), BLACK); // center
    assert_eq!(pixel(&it, 30, 50), BLACK); // inside (r=20 from center)
    assert_eq!(pixel(&it, 50, 15), WHITE); // outside (r=35)
    assert_eq!(pixel(&it, 10, 10), WHITE); // corner
}

#[test]
fn translate_moves_the_origin() {
    let it = render(
        "40 40 translate newpath 0 0 moveto 20 0 lineto 20 20 lineto 0 20 lineto closepath fill",
    );
    // User (0..20, 0..20) after translate = user (40..60, 40..60).
    assert_eq!(pixel(&it, 50, 50), BLACK);
    assert_eq!(pixel(&it, 20, 80), WHITE);
}

#[test]
fn rotate_and_scale_compose() {
    // Rotate 90° about the origin, then draw a square at (10..30, 10..30):
    // it lands at user x in -30..-10, y in 10..30 — off the left edge —
    // unless we translate to the center first.
    let it = render(
        "50 50 translate 90 rotate newpath 10 10 moveto 30 10 lineto 30 30 lineto 10 30 lineto closepath fill",
    );
    // (20,20) rotated 90° = (-20, 20); +center = user (30, 70) = device (30, 30).
    assert_eq!(pixel(&it, 30, 30), BLACK);
    assert_eq!(pixel(&it, 70, 30), WHITE);
}

#[test]
fn gsave_grestore_restores_color_and_ctm() {
    let it = render(
        "1 0 0 setrgbcolor \
         gsave 0 0 1 setrgbcolor 40 40 translate grestore \
         newpath 10 10 moveto 90 10 lineto 90 90 lineto 10 90 lineto closepath fill",
    );
    // Red, at the untranslated position: both saved attributes came back.
    assert_eq!(pixel(&it, 50, 50), (255, 0, 0));
}

#[test]
fn rlineto_uses_user_space_deltas() {
    let it = render("newpath 10 50 moveto 80 0 rlineto 0 4 rlineto -80 0 rlineto closepath fill");
    assert_eq!(pixel(&it, 50, 48), BLACK);
    assert_eq!(pixel(&it, 50, 40), WHITE);
}

#[test]
fn currentpoint_reports_user_space() {
    let mut it = Interp::with_page(100, 100).expect("test page");
    it.run_str("30 40 translate newpath 10 10 moveto currentpoint")
        .expect("currentpoint");
    let reprs: Vec<String> = it.operand_stack().iter().map(|o| o.repr()).collect();
    // currentpoint sees the point in *current* user space, not device.
    assert_eq!(reprs, ["10.0", "10.0"]);
}

#[test]
fn path_errors() {
    let mut it = Interp::with_page(100, 100).expect("test page");
    assert_eq!(
        it.run_str("newpath 10 10 lineto"),
        Err(PsError::NoCurrentPoint)
    );
    assert_eq!(
        it.run_str("newpath 5 5 rmoveto"),
        Err(PsError::NoCurrentPoint)
    );
    assert_eq!(it.run_str("currentpoint"), Err(PsError::NoCurrentPoint));
    assert_eq!(it.run_str("3 setlinecap"), Err(PsError::Rangecheck));
    assert_eq!(it.run_str("0.5 setmiterlimit"), Err(PsError::Rangecheck));
}

#[test]
fn showpage_marks_page_complete() {
    let it = render("showpage");
    assert!(it.gfx().page_shown);
}

#[test]
fn shfill_axial_gradient_basic() {
    let it = render(
        "<< /ShadingType 2 /ColorSpace /DeviceRGB /Coords [0 0 100 0] \
         /Function << /FunctionType 2 /Domain [0 1] /C0 [0 0 0] /C1 [1 1 1] /N 1 >> >> shfill",
    );
    let (r, _, _) = pixel(&it, 5, 50);
    assert!(r < 40, "near t=0 should be dark, got {r}");
    let (r, _, _) = pixel(&it, 95, 50);
    assert!(r > 215, "near t=1 should be light, got {r}");
    let (r, _, _) = pixel(&it, 50, 50);
    assert!((110..=145).contains(&r), "midpoint ~ mid gray, got {r}");
}

#[test]
fn shfill_honors_a_non_default_shading_domain() {
    // /Domain [0 0.5] sweeps only the first half of the function's own
    // ramp across the *whole* geometric axis: the shading's Domain
    // (not the function's own) is what maps gradient position onto
    // the function's input. At the far edge (pos≈0.95), t = 0.95*0.5
    // ≈ 0.475, so the color should sit near the function's *midpoint*
    // value, nowhere near its t=1 endpoint (pure white).
    let it = render(
        "<< /ShadingType 2 /ColorSpace /DeviceRGB /Coords [0 0 100 0] /Domain [0 0.5] \
         /Function << /FunctionType 2 /Domain [0 1] /C0 [0 0 0] /C1 [1 1 1] /N 1 >> >> shfill",
    );
    let (r, _, _) = pixel(&it, 95, 50);
    assert!(
        (95..150).contains(&r),
        "expected mid-range gray near the far edge (t≈0.475), got {r}"
    );
}

#[test]
fn shfill_does_not_consume_the_current_path() {
    // Unlike fill/stroke, shfill paints the clip region and leaves the
    // current path (and currentpoint) alone, per the PLRM.
    let mut it = Interp::with_page(100, 100).expect("test page");
    it.run_str(
        "newpath 10 10 moveto 20 20 lineto \
         << /ShadingType 2 /ColorSpace /DeviceRGB /Coords [0 0 100 0] \
         /Function << /FunctionType 2 /Domain [0 1] /C0 [0 0 0] /C1 [1 1 1] /N 1 >> >> shfill \
         currentpoint",
    )
    .expect("shfill");
    let reprs: Vec<String> = it.operand_stack().iter().map(|o| o.repr()).collect();
    assert_eq!(
        reprs,
        ["20.0", "20.0"],
        "shfill must leave the current path alone"
    );
}

#[test]
fn shfill_radial_gradient_burst_from_center() {
    let it = render(
        "<< /ShadingType 3 /ColorSpace /DeviceRGB /Coords [50 50 0 50 50 40] \
         /Function << /FunctionType 2 /Domain [0 1] /C0 [1 0 0] /C1 [0 0 1] /N 1 >> >> shfill",
    );
    let (r, g, b) = pixel(&it, 50, 50); // exact center: t=0
    assert!(
        r > 245 && g == 0 && b < 10,
        "center should be near-exact red, got ({r},{g},{b})"
    );
    let (r, _, b) = pixel(&it, 50, 10); // user (50,90): distance 40 from center
    assert!(
        b > 215 && r < 40,
        "edge of burst should be near-blue, got r={r} b={b}"
    );
    // Beyond the radius: Pad extend keeps the edge color (documented
    // deviation — shfill always extends both ends, see Gfx::shfill).
    let (r, _, b) = pixel(&it, 50, 2);
    assert!(
        b > 215 && r < 40,
        "beyond radius should stay pad-extended blue, got r={r} b={b}"
    );
}

#[test]
fn shfill_axial_gradient_respects_rotation_and_anisotropic_scale() {
    // Coords pass through in *user* space with the CTM handed straight
    // to the gradient shader as its own transform, rather than
    // pre-mapping just the two endpoints via user_to_device — the
    // latter would get the axis direction right but the perpendicular
    // banding wrong under anisotropic scale. Verified against
    // tiny-skia's actual Gradient::push_stages behavior (its transform
    // field is the local-to-device mapping, matching how the CTM
    // already works everywhere else in this file), not just inferred.
    //
    // Endpoints worked out by hand: `translate` is issued before
    // `rotate`/`scale`, so it applies last (outermost) — a local point
    // goes through `2 1 scale` first, then `45 rotate`, then the
    // translate to page center. t=0 (local (0,0)) lands exactly on the
    // center (100,100); t≈0.1 and t≈0.9 land at ≈(106,94) and
    // ≈(151,49).
    let mut it = Interp::with_page(200, 200).expect("test page");
    it.run_str(
        "100 100 translate 45 rotate 2 1 scale \
         << /ShadingType 2 /ColorSpace /DeviceRGB /Coords [0 0 40 0] \
         /Function << /FunctionType 2 /Domain [0 1] /C0 [1 0 0] /C1 [0 0 1] /N 1 >> >> shfill",
    )
    .expect("shfill");
    let (r, _, b) = pixel(&it, 106, 94);
    assert!(
        r > 190 && b < 65,
        "near t=0 should be red-dominant, got r={r} b={b}"
    );
    let (r, _, b) = pixel(&it, 151, 49);
    assert!(
        b > 190 && r < 65,
        "near t=1 should be blue-dominant, got r={r} b={b}"
    );
}

#[test]
fn shfill_rejects_malformed_shading_dicts() {
    let mut it = Interp::with_page(100, 100).expect("test page");
    assert_eq!(it.run_str("42 shfill"), Err(PsError::Typecheck));

    let mut it = Interp::with_page(100, 100).expect("test page");
    assert_eq!(
        it.run_str(
            "<< /ShadingType 9 /ColorSpace /DeviceGray /Coords [0 0 1 0] \
             /Function << /FunctionType 2 /C0 [0] /C1 [1] /N 1 >> >> shfill"
        ),
        Err(PsError::Rangecheck),
        "unsupported ShadingType"
    );

    let mut it = Interp::with_page(100, 100).expect("test page");
    assert_eq!(
        it.run_str(
            "<< /ShadingType 2 /ColorSpace /DeviceRGB /Coords [0 0 1 0] \
             /Function << /FunctionType 2 /C0 [0] /C1 [1] /N 1 >> >> shfill"
        ),
        Err(PsError::Rangecheck),
        "function outputs 1 component, DeviceRGB needs 3"
    );

    // The lexer rejects non-finite real *literals* outright (falls
    // back to a name token), but ordinary arithmetic overflow
    // (1e300*1e300) still produces a real inf at runtime -- must be
    // rejected here, not reach the domain-mapping arithmetic
    // downstream.
    let mut it = Interp::with_page(100, 100).expect("test page");
    assert_eq!(
        it.run_str(
            "<< /ShadingType 2 /ColorSpace /DeviceGray /Coords [0 0 1 0] \
             /Domain [1e300 1e300 mul 1] \
             /Function << /FunctionType 2 /Domain [0 1] /C0 [0] /C1 [1] /N 1 >> >> shfill"
        ),
        Err(PsError::Rangecheck),
        "non-finite Domain value"
    );
}

#[test]
fn shfill_accepts_extend_true_true_even_though_it_is_not_otherwise_honored() {
    // /Extend [true true] is near-universal in real-world shading
    // content (it's what most gradient-producing tools emit). It's
    // validated for shape but always behaves as if true regardless of
    // its actual value (Gfx::shfill's module doc) -- this pins that the
    // validator itself doesn't reject the common case.
    let mut it = Interp::with_page(100, 100).expect("test page");
    it.run_str(
        "<< /ShadingType 2 /ColorSpace /DeviceRGB /Coords [0 0 100 0] /Extend [true true] \
         /Function << /FunctionType 2 /Domain [0 1] /C0 [0 0 0] /C1 [1 1 1] /N 1 >> >> shfill",
    )
    .expect("Extend [true true] must be accepted");
}

#[test]
fn shfill_type2_function_defaults_c0_c1_but_requires_n() {
    // Confirmed against gs 10.07.1: a Type 2 function with C0/C1
    // omitted is accepted (they default to [0.0]/[1.0]); one with N
    // omitted is rejected with rangecheck (N has no default).
    let it = render(
        "<< /ShadingType 2 /ColorSpace /DeviceGray /Coords [0 0 100 0] \
         /Function << /FunctionType 2 /Domain [0 1] /N 1 >> >> shfill",
    );
    let (r, _, _) = pixel(&it, 5, 50);
    assert!(
        r < 40,
        "defaulted C0=[0] should read as near-black, got {r}"
    );
    let (r, _, _) = pixel(&it, 95, 50);
    assert!(
        r > 215,
        "defaulted C1=[1] should read as near-white, got {r}"
    );

    let mut it = Interp::with_page(100, 100).expect("test page");
    assert_eq!(
        it.run_str(
            "<< /ShadingType 2 /ColorSpace /DeviceGray /Coords [0 0 100 0] \
             /Function << /FunctionType 2 /Domain [0 1] /C0 [0] /C1 [1] >> >> shfill"
        ),
        Err(PsError::Rangecheck),
        "N has no default and must be required"
    );
}

#[test]
fn shfill_reversed_function_domain_is_rejected_not_a_panic() {
    // Confirmed against gs: a *function's* own /Domain [1 0] is a
    // rangecheck (unlike a *shading*'s top-level /Domain, which gs
    // accepts reversed to flip a gradient's direction). An empty
    // Bounds array sidesteps the bounds-monotonicity check alone, so
    // this needs its own guard -- reachable at parse time, not a
    // panic inside eval's clamp.
    let mut it = Interp::with_page(100, 100).expect("test page");
    assert_eq!(
        it.run_str(
            "<< /ShadingType 2 /ColorSpace /DeviceGray /Coords [0 0 100 0] \
             /Function << /FunctionType 3 /Domain [1 0] /Bounds [] /Encode [0 1] \
             /Functions [ << /FunctionType 2 /Domain [0 1] /C0 [0] /C1 [1] /N 1 >> ] >> >> \
             shfill"
        ),
        Err(PsError::Rangecheck)
    );
}

#[test]
fn shfill_discontinuous_stitching_bound_stays_a_hard_edge() {
    // A constant-red leg then a constant-blue leg: the whole left half
    // of the axis must stay pure red right up to the boundary, not
    // smear toward blue across the segment (the bug: a single sample
    // exactly at a stitching bound always resolves to the *right*
    // leg's color, per stitch_index's `x < b` test).
    let it = render(
        "<< /ShadingType 2 /ColorSpace /DeviceRGB /Coords [0 0 100 0] \
         /Function << /FunctionType 3 /Domain [0 1] /Bounds [0.5] /Encode [0 1 0 1] \
         /Functions [ \
           << /FunctionType 2 /Domain [0 1] /C0 [1 0 0] /C1 [1 0 0] /N 1 >> \
           << /FunctionType 2 /Domain [0 1] /C0 [0 0 1] /C1 [0 0 1] /N 1 >> \
         ] >> >> shfill",
    );
    // Just left of the midpoint (device x=49, close to the geometric
    // boundary but still in the red leg): must still be pure red, not
    // some red/blue blend.
    assert_eq!(pixel(&it, 25, 50), (255, 0, 0));
    assert_eq!(pixel(&it, 49, 50), (255, 0, 0));
    assert_eq!(pixel(&it, 51, 50), (0, 0, 255));
    assert_eq!(pixel(&it, 75, 50), (0, 0, 255));
}

#[test]
fn shfill_applies_function_range_after_evaluation() {
    // A ramp with C1 [1] but /Range [0 0.5] must cap output at 0.5
    // (mid-gray), never reaching white -- Range clips the *output*,
    // independently of Domain (which clips the input).
    let it = render(
        "<< /ShadingType 2 /ColorSpace /DeviceGray /Coords [0 0 100 0] \
         /Function << /FunctionType 2 /Domain [0 1] /Range [0 0.5] \
         /C0 [0] /C1 [1] /N 1 >> >> shfill",
    );
    let (r, _, _) = pixel(&it, 95, 50);
    assert!(
        r < 145,
        "Range [0 0.5] should cap near t=1 at ~half-gray, got {r}"
    );
}

#[test]
fn shfill_rejects_a_reversed_function_range() {
    // Confirmed against gs: /Range [1 0] is a rangecheck -- build_stops
    // feeds it straight into f64::clamp(lo, hi), which panics if
    // lo > hi.
    let mut it = Interp::with_page(100, 100).expect("test page");
    assert_eq!(
        it.run_str(
            "<< /ShadingType 2 /ColorSpace /DeviceGray /Coords [0 0 100 0] \
             /Function << /FunctionType 2 /Domain [0 1] /Range [1 0] \
             /C0 [0] /C1 [1] /N 1 >> >> shfill"
        ),
        Err(PsError::Rangecheck)
    );
}

#[test]
fn shfill_requires_a_function_domain() {
    // Confirmed against gs: a Type 2/3 function dict with /Domain
    // omitted is a rangecheck (unlike a shading's own top-level
    // /Domain, which is optional and defaults to [0 1]).
    let mut it = Interp::with_page(100, 100).expect("test page");
    assert_eq!(
        it.run_str(
            "<< /ShadingType 2 /ColorSpace /DeviceGray /Coords [0 0 100 0] \
             /Function << /FunctionType 2 /C0 [0] /C1 [1] /N 1 >> >> shfill"
        ),
        Err(PsError::Rangecheck)
    );
}

#[test]
fn shfill_rejects_deeply_nested_functions_instead_of_overflowing_the_stack() {
    // A few thousand levels of acyclic Type 3 nesting (or a
    // self-referential dict built via `put` after construction) would
    // overflow the Rust stack in parse_function's recursion without a
    // depth cap. Build one programmatically -- hand-writing this many
    // levels in a literal would be enormous -- and confirm it errors
    // cleanly rather than crashing the process. Constructing the
    // nested *dict* itself doesn't risk this: `<< >>` runs through the
    // ordinary PostScript machine, which is an explicit frame stack by
    // design, not native recursion.
    let mut func = "<< /FunctionType 2 /Domain [0 1] /C0 [0] /C1 [1] /N 1 >>".to_string();
    for _ in 0..200 {
        func = format!(
            "<< /FunctionType 3 /Domain [0 1] /Functions [{func}] /Bounds [] /Encode [0 1] >>"
        );
    }
    let mut it = Interp::with_page(100, 100).expect("test page");
    let src = format!(
        "<< /ShadingType 2 /ColorSpace /DeviceGray /Coords [0 0 100 0] /Function {func} >> shfill"
    );
    assert_eq!(it.run_str(&src), Err(PsError::Limitcheck));
}

#[test]
fn shfill_bbox_restricts_the_painted_region() {
    // Confirmed against gs: /BBox further clips the shading, in the
    // same user space Coords is defined in -- pixels outside it stay
    // untouched even though Coords/Extend would otherwise reach them.
    let it = render(
        "<< /ShadingType 2 /ColorSpace /DeviceGray /Coords [0 0 100 0] \
         /BBox [20 20 40 40] \
         /Function << /FunctionType 2 /Domain [0 1] /C0 [0] /C1 [1] /N 1 >> >> shfill",
    );
    assert_eq!(pixel(&it, 10, 50), WHITE, "outside BBox, left");
    assert_eq!(pixel(&it, 90, 50), WHITE, "outside BBox, right");
    // Inside the bbox (device x 20..40, y 60..80): some ink, not white.
    let (r, _, _) = pixel(&it, 30, 70);
    assert!(r < 250, "inside BBox should show the gradient, got r={r}");
}

#[test]
fn shfill_clamps_cmyk_components_before_converting_not_after() {
    // Confirmed against gs: `-1 0 0 0.5 setcmykcolor` reads back as
    // 0.5 0.5 0.5 (C clamped to 0 *before* the (1-c)(1-k) product),
    // not the naive (1-(-1))*(1-0.5) = 1.0 clamping only the result.
    // C0 here is out of range on purpose (C=-1); a Type 2 function is
    // happy to produce it, since C0/C1 aren't themselves range-checked.
    let it = render(
        "<< /ShadingType 2 /ColorSpace /DeviceCMYK /Coords [0 0 100 0] \
         /Function << /FunctionType 2 /Domain [0 1] \
         /C0 [-1 0 0 0.5] /C1 [-1 0 0 0.5] /N 1 >> >> shfill",
    );
    let (r, g, b) = pixel(&it, 50, 50);
    assert_eq!((r, g, b), (128, 128, 128), "expected clamped 0.5 gray");
}

#[test]
fn shfill_extend_wrong_length_is_rangecheck_not_typecheck() {
    // Confirmed against gs: /Extend with other than 2 elements is a
    // rangecheck, distinct from a typecheck for wrong element types.
    let mut it = Interp::with_page(100, 100).expect("test page");
    assert_eq!(
        it.run_str(
            "<< /ShadingType 2 /ColorSpace /DeviceGray /Coords [0 0 100 0] /Extend [true] \
             /Function << /FunctionType 2 /Domain [0 1] /C0 [0] /C1 [1] /N 1 >> >> shfill"
        ),
        Err(PsError::Rangecheck)
    );
    let mut it = Interp::with_page(100, 100).expect("test page");
    assert_eq!(
        it.run_str(
            "<< /ShadingType 2 /ColorSpace /DeviceGray /Coords [0 0 100 0] /Extend [1 2] \
             /Function << /FunctionType 2 /Domain [0 1] /C0 [0] /C1 [1] /N 1 >> >> shfill"
        ),
        Err(PsError::Typecheck)
    );
}

#[test]
fn shfill_coincident_axial_endpoints_paint_nothing() {
    // Confirmed against gs: Coords [x y x y] (coincident start/end) is
    // a no-op, even with Extend [true true] -- tiny-skia's own
    // LinearGradient::new does *not* return None for this in Pad mode
    // (it returns a solid fill of the last stop), so this needs an
    // explicit check rather than relying on that return value.
    let it = render(
        "1 1 1 setrgbcolor 0 0 100 100 rectfill \
         0 0 0 setrgbcolor \
         << /ShadingType 2 /ColorSpace /DeviceGray /Coords [50 50 50 50] \
         /Extend [true true] \
         /Function << /FunctionType 2 /Domain [0 1] /C0 [0] /C1 [1] /N 1 >> >> shfill",
    );
    assert_eq!(pixel(&it, 50, 50), WHITE, "coincident axial paints nothing");
}
