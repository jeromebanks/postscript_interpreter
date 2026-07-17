//! The dynamic handwriting font — Stage 12. Structure tests only:
//! the page is rand-driven art, so pixel comparison with gs is
//! excluded (same policy as snowflak/vasarely in the corpus); gs
//! compatibility is asserted by running the file there when gs is
//! present (it must not error — the font is plain Type 3 PostScript).

use pscat::Interp;

fn run_example() -> Interp {
    let src = std::fs::read("examples/handwriting.ps").expect("example present");
    let mut it = Interp::with_page(612, 792).expect("page");
    it.run_source(&src)
        .unwrap_or_else(|e| panic!("handwriting.ps failed: {}", it.error_report(&e)));
    it
}

#[test]
fn the_letter_writes_itself() {
    let it = run_example();
    assert_eq!(it.gfx().pages().len(), 1, "one page shown");
    // Ink where the text lives (device y ~100..620, x ~110..550):
    // count clearly-dark pixels on the final page.
    let page = &it.gfx().pages()[0];
    let mut dark = 0;
    for y in (100..620).step_by(3) {
        for x in (110..550).step_by(3) {
            let p = page.pixel(x, y).expect("in bounds");
            if p.red() < 120 && p.blue() < 160 {
                dark += 1;
            }
        }
    }
    assert!(
        dark > 300,
        "handwriting ink present, got {dark} dark samples"
    );
}

#[test]
fn no_two_glyph_instances_match() {
    // Draw the same character twice in one interpreter (the rand
    // stream advances between them) and compare equal-sized windows:
    // the jitter must make them differ.
    let mut it = run_example();
    it.run_str(
        "erasepage initgraphics 0 setgray
         /HandScript findfont 100 scalefont setfont
         60 300 moveto (o) show
         360 300 moveto (o) show",
    )
    .unwrap_or_else(|e| panic!("draw failed: {}", it.error_report(&e)));
    let pm = &it.gfx().pixmap;
    let mut differing = 0;
    let mut inked = 0;
    for dy in 0..160u32 {
        for dx in 0..140u32 {
            let a = pm.pixel(50 + dx, 792 - 420 + dy).expect("a");
            let b = pm.pixel(350 + dx, 792 - 420 + dy).expect("b");
            if a.red() < 128 || b.red() < 128 {
                inked += 1;
            }
            if a.red() != b.red() {
                differing += 1;
            }
        }
    }
    assert!(inked > 100, "both glyphs drew something, got {inked}");
    assert!(
        differing > 50,
        "two 'o's should never match ({differing} differing pixels)"
    );
}

#[test]
fn deterministic_across_runs() {
    // rand is a seeded LCG: the same program produces the same page
    // twice — reproducible art, per the project doctrine.
    let a = run_example();
    let b = run_example();
    assert_eq!(
        a.gfx().pages()[0].data(),
        b.gfx().pages()[0].data(),
        "same seed, same letter"
    );
}

#[test]
fn ghostscript_accepts_the_font() {
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
            "-dNOPAUSE",
            "-dBATCH",
            "-q",
            "-sDEVICE=png16m",
            "-g612x792",
            "-r72",
            "-o/dev/null",
            "examples/handwriting.ps",
        ])
        .status()
        .expect("run gs");
    assert!(status.success(), "gs rejected the handwriting font");
}
