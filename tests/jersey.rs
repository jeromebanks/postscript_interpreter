//! Arched team name over a player number (issue #149, `lib/jersey.ps`):
//! exact text, ink-centred placement, containment inside the print
//! rectangle (checked against rendered pixels, not just the reported
//! boxes), clearance between the arch and the number, tilt limits,
//! named rejections, determinism, and that a call leaves the caller's
//! operand stack, font and path alone.

use pscat::{Interp, PsError};

const W: u32 = 600;
const H: u32 = 700;

fn with_lib(w: u32, h: u32) -> Interp {
    let mut it = Interp::with_page(w, h).expect("page");
    for f in ["lib/artkit.ps", "lib/jersey.ps"] {
        let src = std::fs::read(f).expect("lib present");
        it.run_source(&src)
            .unwrap_or_else(|e| panic!("{f} failed: {}", it.error_report(&e)));
    }
    it
}

fn stack_of(src: &str) -> Vec<String> {
    let mut it = with_lib(W, H);
    it.run_str(src)
        .unwrap_or_else(|e| panic!("eval of {src:?} failed: {}", it.error_report(&e)));
    it.operand_stack().iter().map(|o| o.repr()).collect()
}

fn nums(src: &str) -> Vec<f64> {
    stack_of(src)
        .iter()
        .map(|s| s.parse().unwrap_or_else(|_| panic!("not a number: {s:?}")))
        .collect()
}

fn err_of(src: &str) -> String {
    let mut it = with_lib(W, H);
    match it.run_str(src).unwrap_err() {
        PsError::Undefined(name) => name,
        other => panic!("expected a self-documenting undefined name, got {other}"),
    }
}

const RECT: &str = "[ 20 20 580 680 ]";

fn opts(name: &str, number: &str, extra: &str) -> String {
    format!("<< /Name ({name}) /Number ({number}) /Rect {RECT} /Margin 10 {extra} >>")
}

/// [x0 y0 x1 y1] of one of the layout's parts (`Name`, `Number`, `Caption`, or the union).
fn ink(o: &str, part: &str) -> [f64; 4] {
    let path = if part == "Ink" {
        "/Ink get".to_string()
    } else {
        format!("/{part} get /Ink get")
    };
    let v = nums(&format!("{o} jerseylayout {path} aload pop"));
    [v[0], v[1], v[2], v[3]]
}

/// Bounding box (PS coordinates, y up) of every non-white pixel drawn by `src`.
fn painted_bbox(src: &str) -> [f64; 4] {
    let it = render(src);
    let pm = &it.gfx().pixmap;
    let (w, h) = (pm.width() as usize, pm.height() as usize);
    let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0usize, 0usize);
    for (i, p) in pm.pixels().iter().enumerate() {
        if p.red() < 250 || p.green() < 250 || p.blue() < 250 {
            let (x, y) = (i % w, i / w);
            x0 = x0.min(x);
            x1 = x1.max(x + 1);
            y0 = y0.min(y);
            y1 = y1.max(y + 1);
        }
    }
    [x0 as f64, (h - y1) as f64, x1 as f64, (h - y0) as f64]
}

fn render(src: &str) -> Interp {
    let mut it = with_lib(W, H);
    it.run_str(&format!("1 setgray clippath fill 0 setgray {src}"))
        .unwrap_or_else(|e| panic!("render of {src:?} failed: {}", it.error_report(&e)));
    it
}

/// Per-column (lowest, highest) painted row, PS coordinates; None when a column is empty.
fn columns(it: &Interp) -> Vec<Option<(f64, f64)>> {
    let pm = &it.gfx().pixmap;
    let (w, h) = (pm.width() as usize, pm.height() as usize);
    let mut cols: Vec<Option<(f64, f64)>> = vec![None; w];
    for (i, p) in pm.pixels().iter().enumerate() {
        if p.red() < 128 {
            let (x, y) = (i % w, (h - 1 - i / w) as f64);
            cols[x] = Some(match cols[x] {
                None => (y, y),
                Some((lo, hi)) => (lo.min(y), hi.max(y)),
            });
        }
    }
    cols
}

#[test]
fn text_is_preserved_exactly() {
    let o = opts("Persona non grata", "00", "/Caption (Est. 1998  )");
    let s = stack_of(&format!(
        "{o} jerseylayout /L exch def \
         L /Name get /Text get L /Number get /Text get L /Caption get /Text get"
    ));
    assert_eq!(s, ["(Persona non grata)", "(00)", "(Est. 1998  )"]);
}

#[test]
fn ink_is_centred_and_inside_the_print_area() {
    // Different name lengths, number widths and print-area aspect ratios.
    let cases = [
        ("Cleveland Steamers", "69", "[ 20 20 580 680 ]"),
        ("Persona non grata", "86", "[ 20 20 580 680 ]"),
        ("Sox", "7", "[ 20 20 580 680 ]"),
        ("Metro Comets", "00", "[ 20 20 580 680 ]"),
        ("Riverside", "123", "[ 20 20 580 680 ]"),
        ("Northern Lights", "11", "[ 20 200 580 420 ]"),
        ("Cleveland Steamers", "69", "[ 150 20 450 680 ]"),
        ("A", "1", "[ 100 100 300 300 ]"),
        // a short, wide print area, a long name and a narrow number: the
        // arch's own drooping ends are the lowest ink
        (
            "Cleveland Steamers International",
            "1",
            "[ 20 300 580 380 ]",
        ),
    ];
    for (name, number, rect) in cases {
        let o = format!(
            "<< /Name ({name}) /Number ({number}) /Rect {rect} /Margin 12 /NameFont /Helvetica-Bold >>"
        );
        let r = nums(&format!("{rect} aload pop"));
        let v = nums(&format!(
            "{o} jerseylayout /L exch def L /Ink get aload pop \
             L /Name get /Ink get aload pop L /Number get /Ink get aload pop"
        ));
        let (u, n, m) = (
            [v[0], v[1], v[2], v[3]],
            [v[4], v[5], v[6], v[7]],
            [v[8], v[9], v[10], v[11]],
        );
        let cx = (r[0] + r[2]) / 2.0;
        // centred by ink on the vertical axis (name and number both)
        assert!(
            ((n[0] + n[2]) / 2.0 - cx).abs() < 0.5,
            "{name}: name centre"
        );
        assert!(
            ((m[0] + m[2]) / 2.0 - cx).abs() < 0.5,
            "{number}: number centre"
        );
        // inside rect - margin (outline included, so strictly inside)
        assert!(
            u[0] >= r[0] + 12.0 && u[2] <= r[2] - 12.0,
            "{name}/{number}: x"
        );
        assert!(
            u[1] >= r[1] + 12.0 && u[3] <= r[3] - 12.0,
            "{name}/{number}: y"
        );
        // ...and the rendered pixels agree with those boxes (2 px slack for antialiasing
        // and the outline growing the ink)
        let p = painted_bbox(&format!("{o} jersey"));
        assert!(
            p[0] >= r[0] + 12.0 - 2.0 && p[2] <= r[2] - 12.0 + 2.0,
            "{name}: pixel x {p:?}"
        );
        assert!(
            p[1] >= r[1] + 12.0 - 2.0 && p[3] <= r[3] - 12.0 + 2.0,
            "{name}: pixel y {p:?}"
        );
        assert!(
            (p[0] - u[0]).abs() < 8.0 && (p[2] - u[2]).abs() < 8.0,
            "{name}: {p:?} vs {u:?}"
        );
        assert!(
            (p[1] - u[1]).abs() < 8.0 && (p[3] - u[3]).abs() < 8.0,
            "{name}: {p:?} vs {u:?}"
        );
    }
}

#[test]
fn number_dominates_and_name_clears_it() {
    let o = opts(
        "Cleveland Steamers",
        "69",
        "/NameFont /Helvetica-Bold /Gap 10",
    );
    let s = nums(&format!(
        "{o} jerseylayout dup /Name get /Size get exch /Number get /Ink get aload pop"
    ));
    let (name_size, num_h) = (s[0], s[4] - s[2]);
    // default /Dominance 1.8 against the name's cap height (Helvetica-Bold ~ 0.72 em)
    assert!(
        num_h >= 1.8 * 0.70 * name_size,
        "number {num_h} vs name size {name_size}"
    );

    // Render the two parts apart and compare column by column: the arch
    // must clear the number by (about) the requested gap everywhere.
    let lay = format!("{o} jerseylayout /L exch def");
    let name = render(&format!(
        "{lay} L /Name get dup /FontDict get exch /Size get scalefont setfont \
         newpath L /Name get /Text get L /Center get aload pop L /Radius get jerseyarch fill"
    ));
    let num = render(&format!(
        "{lay} L /Number get dup /FontDict get exch /Size get scalefont setfont \
         newpath L /Number get /Origin get aload pop moveto L /Number get /Text get true charpath fill"
    ));
    let (nc, mc) = (columns(&name), columns(&num));
    let mut checked = 0;
    for (a, b) in nc.iter().zip(&mc) {
        if let (Some((name_lo, _)), Some((_, num_hi))) = (a, b) {
            // 2 px of antialiasing slack on a 10 pt gap
            assert!(
                name_lo - num_hi >= 10.0 - 2.0,
                "column clearance {}",
                name_lo - num_hi
            );
            checked += 1;
        }
    }
    assert!(
        checked > 50,
        "the number should sit under the name ({checked} shared columns)"
    );
}

#[test]
fn end_letters_respect_max_tilt() {
    let o = opts(
        "Cleveland Steamers International",
        "69",
        "/MaxTilt 20 /NameFont /Helvetica-Bold",
    );
    let t = nums(&format!("{o} jerseylayout /Name get /Tilt get"));
    assert!(t[0] <= 20.0 + 1e-6, "tilt {}", t[0]);
    // and it is not trivially zero: the arch really is curved
    assert!(t[0] > 5.0, "tilt {}", t[0]);
}

#[test]
fn a_tighter_arch_curves_more_and_shrinks_the_name() {
    let loose = nums(&format!(
        "{} jerseylayout /Name get /Size get",
        opts("Steamers", "9", "/Radius 900")
    ));
    let tight = nums(&format!(
        "{} jerseylayout /Name get /Size get",
        opts("Steamers", "9", "/Radius 200")
    ));
    assert!(tight[0] < loose[0], "{} vs {}", tight[0], loose[0]);
}

#[test]
fn impossible_geometry_is_rejected_by_name() {
    // a long name in a tiny rectangle
    assert_eq!(
        err_of("<< /Name (Cleveland Steamers) /Number (69) /Rect [ 0 0 40 400 ] >> jerseylayout"),
        "jersey-name-too-wide"
    );
    // a very tight arch: the end letters would be severely tilted
    assert_eq!(
        err_of(&format!(
            "{} jerseylayout",
            opts("Cleveland Steamers", "69", "/Radius 20 /MinNameSize 30")
        )),
        "jersey-name-too-tilted"
    );
    // no height left for any number under the name
    assert_eq!(
        err_of("<< /Name (Hi) /Number (9) /Rect [ 0 0 400 5 ] /MinNameSize 6 >> jerseylayout"),
        "jersey-does-not-fit"
    );
    // a name this large cannot leave the number 1.8x its cap height
    assert_eq!(
        err_of(&format!(
            "{} jerseylayout",
            opts("Hi", "9", "/MinNameSize 120 /Dominance 8 /NameMaxSize 200")
        )),
        "jersey-number-not-dominant"
    );
    assert_eq!(
        err_of("<< /Name (A) /Number (1) >> jerseylayout"),
        "jersey-missing-option"
    );
    assert_eq!(
        err_of(&format!(
            "{} jerseylayout",
            opts("A", "1", "/NameFont /NoSuchFace")
        )),
        "jersey-font-not-found"
    );
    assert_eq!(err_of("(nope) jerseydraw"), "jersey-not-a-layout");
}

#[test]
fn a_rejection_leaves_the_caller_untouched() {
    let mut it = with_lib(W, H);
    it.run_str("/Times-Roman findfont 14 scalefont setfont 1 2 3")
        .expect("setup");
    let bad = "<< /Name (Cleveland Steamers) /Number (69) /Rect [ 0 0 40 400 ] >> jerseylayout";
    assert!(it.run_str(bad).is_err());
    let ops: Vec<String> = it.operand_stack().iter().map(|o| o.repr()).collect();
    assert_eq!(ops, ["1", "2", "3"]);
    it.run_str("currentfont /FontName get 40 string cvs")
        .expect("font");
    let last = it.operand_stack().last().map(|o| o.repr());
    assert_eq!(last.as_deref(), Some("(Times-Roman)"));
}

#[test]
fn layout_and_draw_keep_font_path_and_stack() {
    let o = opts("Cleveland Steamers", "69", "");
    let s = stack_of(&format!(
        "/Times-Roman findfont 14 scalefont setfont newpath 7 8 moveto 9 10 lineto \
         {o} jerseylayout jerseydraw \
         currentfont /FontName get 40 string cvs \
         pathbbox"
    ));
    assert_eq!(s, ["(Times-Roman)", "7.0", "8.0", "9.0", "10.0"]);
}

#[test]
fn layouts_are_deterministic() {
    let o = opts("Persona non grata", "86", "/Caption (x)");
    let a = stack_of(&format!("{o} jerseylayout /Ink get aload pop"));
    let b = stack_of(&format!("{o} jerseylayout /Ink get aload pop"));
    assert_eq!(a, b);
}

#[test]
fn outline_off_draws_only_the_fill() {
    // /OutlineWidth 0 must be a pure fill: every painted pixel is the fill colour.
    let o = opts(
        "Metro Comets",
        "7",
        "/OutlineWidth 0 /Fill [ 1 0 0 ] /Outline [ 0 1 0 ]",
    );
    let it = render(&format!("{o} jersey"));
    let pm = &it.gfx().pixmap;
    assert!(
        pm.pixels()
            .iter()
            .all(|p| p.green() as i32 >= p.red() as i32 - 1 || p.green() == p.blue())
    );
    assert!(
        !pm.pixels().iter().any(|p| p.green() > 200 && p.red() < 100),
        "no green outline"
    );
}

#[test]
fn jerseyarch_centres_the_ink_on_the_axis() {
    // Non-symmetric advance: a lone "I" first letter and a "y" last letter.
    let b = painted_bbox(
        "/Helvetica-Bold findfont 60 scalefont setfont \
         newpath (Illy) 300 100 300 jerseyarch fill",
    );
    assert!(((b[0] + b[2]) / 2.0 - 300.0).abs() < 1.5, "{b:?}");
}

#[test]
fn the_caption_clears_the_arch_too() {
    // A wide caption under a narrow number: its ends sit right under the
    // drooping ends of the name.
    let o = opts(
        "Cleveland Steamers",
        "1",
        "/Caption (WORLD CHAMPIONS OF EVERYTHING) /NameFont /Helvetica-Bold /Gap 8",
    );
    let lay = format!("{o} jerseylayout /L exch def");
    let name = render(&format!(
        "{lay} L /Name get dup /FontDict get exch /Size get scalefont setfont \
         newpath L /Name get /Text get L /Center get aload pop L /Radius get jerseyarch fill"
    ));
    let cap = render(&format!(
        "{lay} L /Caption get dup /FontDict get exch /Size get scalefont setfont \
         newpath L /Caption get /Origin get aload pop moveto L /Caption get /Text get true charpath fill"
    ));
    let (nc, cc) = (columns(&name), columns(&cap));
    let mut shared = 0;
    for (a, b) in nc.iter().zip(&cc) {
        if let (Some((name_lo, _)), Some((_, cap_hi))) = (a, b) {
            assert!(
                name_lo - cap_hi >= 8.0 - 2.0,
                "caption clearance {}",
                name_lo - cap_hi
            );
            shared += 1;
        }
    }
    assert!(
        shared > 20,
        "caption should sit under the name ({shared} shared columns)"
    );
    // and the whole stack still fits
    let r = nums("[ 20 20 580 680 ] aload pop");
    let u = ink(&o, "Ink");
    assert!(
        u[1] >= r[1] + 10.0 && u[3] <= r[3] - 10.0 && u[0] >= r[0] + 10.0 && u[2] <= r[2] - 10.0
    );
}

#[test]
fn trailing_space_does_not_pull_the_number_off_centre() {
    // charpath leaves a moveto at the pen's final position; it must not count as ink.
    let o = opts("Sox", "1      ", "");
    let n = ink(&o, "Number");
    assert!(((n[0] + n[2]) / 2.0 - 300.0).abs() < 0.5, "{n:?}");
    // the arch's per-glyph advance movetos too: a name ending in a space
    let a = ink(&opts("Sox ", "1", ""), "Name");
    let b = ink(&opts("Sox", "1", ""), "Name");
    assert!(
        (a[0] - b[0]).abs() < 0.5 && (a[2] - b[2]).abs() < 0.5,
        "{a:?} vs {b:?}"
    );
}

#[test]
fn bad_input_is_rejected_without_corrupting_the_caller() {
    for bad in [
        "<< /Name [ 65 ] /Number (1) /Rect [ 20 20 580 680 ] >>",
        "<< /Name (A) /Number (1) /Rect [ 20 20 580 ] >>",
        "<< /Name (A) /Number (1) /Rect [ 20 20 580 680 ] /Margin (x) >>",
        "<< /Name (A) /Number (1) /Rect [ 20 20 580 680 ] /Fill [ 1 0 ] >>",
    ] {
        assert_eq!(
            err_of(&format!("{bad} jerseylayout")),
            "jersey-option-wrong-type"
        );
    }
    // the public arch primitive: no dict or path left behind on a rejection
    let mut it = with_lib(W, H);
    it.run_str(
        "/Times-Roman findfont 14 scalefont setfont newpath 7 8 moveto 9 10 lineto countdictstack",
    )
    .expect("setup");
    let depth = it.operand_stack().last().map(|o| o.repr()).unwrap();
    it.run_str("clear { () 300 100 300 jerseyarch } stopped pop clear countdictstack pathbbox")
        .expect("probe");
    let s: Vec<String> = it.operand_stack().iter().map(|o| o.repr()).collect();
    assert_eq!(
        s,
        [
            depth,
            "7.0".into(),
            "8.0".into(),
            "9.0".into(),
            "10.0".into()
        ]
    );
}

#[test]
fn a_name_size_ceiling_below_the_floor_is_rejected() {
    assert_eq!(
        err_of(&format!(
            "{} jerseylayout",
            opts("A", "1", "/MinNameSize 210")
        )),
        "jersey-does-not-fit"
    );
}
