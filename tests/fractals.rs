//! The payoff tests: the real hand-written fractal programs in
//! `examples/` run end-to-end and put the right ink in the right places.

use pscat::Interp;

fn render_example(name: &str, w: u32, h: u32) -> Interp {
    let source =
        std::fs::read(format!("examples/{name}")).unwrap_or_else(|e| panic!("read {name}: {e}"));
    let mut it = Interp::with_page(w, h).expect("page");
    it.run_source(&source)
        .unwrap_or_else(|e| panic!("{name} failed: {}", it.error_report(&e)));
    it
}

fn pixel(it: &Interp, x: u32, y: u32) -> (u8, u8, u8) {
    let p = it.gfx().pixmap.pixel(x, y).expect("pixel in bounds");
    (p.red(), p.green(), p.blue())
}

fn ink_count(it: &Interp) -> usize {
    it.gfx()
        .pixmap
        .pixels()
        .iter()
        .filter(|p| (p.red(), p.green(), p.blue()) != (255, 255, 255))
        .count()
}

#[test]
fn sierpinski_triangle() {
    let it = render_example("sierpinski.ps", 500, 500);
    assert!(it.gfx().page_shown, "showpage must have run");
    // Bottom-left sub-triangle is solid black near its base
    // (user (56,51) -> device (56, 449)).
    assert_eq!(pixel(&it, 56, 449), (0, 0, 0));
    // The big central hole is empty (user (250,180) -> device (250,320)).
    assert_eq!(pixel(&it, 250, 320), (255, 255, 255));
    // Outside the triangle entirely.
    assert_eq!(pixel(&it, 10, 100), (255, 255, 255));
    // Depth 5 = 243 filled triangles; a sanity floor on total ink.
    assert!(ink_count(&it) > 20_000, "got {}", ink_count(&it));
}

#[test]
fn koch_snowflake() {
    let it = render_example("koch_snowflake.ps", 500, 500);
    assert!(it.gfx().page_shown);
    // Stroked in 0 0 0.6 setrgbcolor: some pixel must be distinctly blue.
    let blue = it
        .gfx()
        .pixmap
        .pixels()
        .iter()
        .any(|p| p.blue() > 100 && p.red() < 60);
    assert!(blue, "expected blue snowflake strokes");
    assert!(ink_count(&it) > 3_000, "got {}", ink_count(&it));
}

#[test]
fn golden_spiral() {
    let it = render_example("golden_spiral.ps", 500, 500);
    assert!(it.gfx().page_shown);
    // The radial burst is centered at user (380,380) = device (380,120);
    // 72 spokes overlap near the hub, so just off-center is solidly dark
    // regardless of per-spoke antialiasing.
    let (r, g, b) = pixel(&it, 382, 120);
    assert_eq!((r, g), (g, b), "burst spokes are gray");
    assert!(r < 120, "burst hub should be dark, got {r}");
    // The spiral is stroked in 0.85 0.6 0.1 setrgbcolor; its 1.5-wide
    // core has full-coverage pixels of exactly that color.
    let orange = it
        .gfx()
        .pixmap
        .pixels()
        .iter()
        .any(|p| (p.red(), p.green(), p.blue()) == (217, 153, 26));
    assert!(orange, "expected full-coverage spiral pixels");
    assert!(ink_count(&it) > 5_000, "got {}", ink_count(&it));
}
