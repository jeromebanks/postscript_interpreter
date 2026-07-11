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
