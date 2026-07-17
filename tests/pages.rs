//! Multi-page semantics — Stage 9 task 1. showpage snapshots the
//! page, resets the graphics state (full initgraphics, pinned against
//! gs), and erases *lazily*: the canvas keeps the finished page until
//! the program paints again, so single-page programs — and the live
//! window — keep their picture.

use pscat::Interp;

fn run(src: &str) -> Interp {
    let mut it = Interp::with_page(100, 100).expect("test page");
    it.run_str(src)
        .unwrap_or_else(|e| panic!("run failed: {}", it.error_report(&e)));
    it
}

fn top_repr(it: &mut Interp) -> String {
    it.pop().expect("operand").repr()
}

fn pixel(it: &Interp, x: u32, y: u32) -> (u8, u8, u8) {
    let p = it.gfx().pixmap.pixel(x, y).expect("pixel in bounds");
    (p.red(), p.green(), p.blue())
}

const SQUARE: &str = "newpath 20 20 moveto 80 20 lineto 80 80 lineto 20 80 lineto closepath fill";

#[test]
fn single_page_keeps_its_picture() {
    let it = run(&format!("{SQUARE} showpage"));
    assert_eq!(pixel(&it, 50, 50), (0, 0, 0), "canvas not erased");
    assert_eq!(it.gfx().pages().len(), 1);
    assert!(!it.gfx().has_trailing_art());
}

#[test]
fn second_page_erases_first_lazily() {
    // Page 2's first mark triggers the erase; page 1's snapshot keeps
    // the square.
    let it = run(&format!(
        "{SQUARE} showpage
         newpath 5 90 moveto 10 90 lineto 10 95 lineto closepath fill"
    ));
    assert_eq!(pixel(&it, 50, 50), (255, 255, 255), "old square erased");
    let p1 = &it.gfx().pages()[0];
    let px = p1.pixel(50, 50).expect("in bounds");
    assert_eq!(
        (px.red(), px.green(), px.blue()),
        (0, 0, 0),
        "snapshot kept"
    );
    assert!(it.gfx().has_trailing_art(), "page 2 has art");
}

#[test]
fn showpage_resets_the_graphics_state() {
    // Pinned against gs: full initgraphics — CTM, color, width.
    let mut it = run("72 72 translate 1 0 0 setrgbcolor 5 setlinewidth showpage
         matrix currentmatrix 4 get currentlinewidth currentrgbcolor");
    assert_eq!(top_repr(&mut it), "0.0", "b");
    assert_eq!(top_repr(&mut it), "0.0", "g");
    assert_eq!(top_repr(&mut it), "0.0", "r: black again");
    assert_eq!(top_repr(&mut it), "1.0", "line width");
    assert_eq!(top_repr(&mut it), "0.0", "tx: translation gone");
}

#[test]
fn copypage_keeps_canvas_and_state() {
    let mut it = run(&format!(
        "72 0 translate {SQUARE} copypage matrix currentmatrix 4 get"
    ));
    assert_eq!(top_repr(&mut it), "72.0", "CTM survives copypage");
    assert_eq!(it.gfx().pages().len(), 1, "page emitted");
    assert!(it.gfx().has_trailing_art(), "canvas still counts as live");
}

#[test]
fn initgraphics_resets_but_keeps_the_font() {
    // Pinned against gs: initgraphics leaves the current font alone.
    let mut it = run("/Helvetica findfont 10 scalefont setfont
         72 72 translate 3 setlinewidth initgraphics
         currentfont /FontName get currentlinewidth");
    assert_eq!(top_repr(&mut it), "1.0");
    assert_eq!(top_repr(&mut it), "/Helvetica");
}

#[test]
fn three_pages_snapshot_in_order() {
    let it = run(
        "0 setgray newpath 0 0 moveto 100 0 lineto 100 100 lineto 0 100 lineto closepath fill showpage
         0.5 setgray newpath 0 0 moveto 100 0 lineto 100 100 lineto 0 100 lineto closepath fill showpage
         1 0 0 setrgbcolor newpath 0 0 moveto 100 0 lineto 100 100 lineto 0 100 lineto closepath fill showpage",
    );
    let pages = it.gfx().pages();
    assert_eq!(pages.len(), 3);
    let shades: Vec<u8> = pages
        .iter()
        .map(|p| p.pixel(50, 50).expect("px").red())
        .collect();
    assert_eq!(shades[0], 0, "page 1 black");
    assert!((i32::from(shades[1]) - 128).abs() <= 1, "page 2 gray");
    assert_eq!(shades[2], 255, "page 3 red channel");
    assert!(!it.gfx().has_trailing_art(), "nothing after the last page");
}

#[test]
fn erasepage_counts_as_painting() {
    // erasepage after showpage consumes the pending erase and marks
    // the new page as touched (a deliberately-blank page is a page).
    let it = run(&format!("{SQUARE} showpage erasepage"));
    assert!(it.gfx().has_trailing_art());
    assert_eq!(pixel(&it, 50, 50), (255, 255, 255));
}
