//! The art toolkit (lib/artkit.ps, Stage 19): every subsystem loads
//! and computes what its header promises, deterministically under
//! srand, and the file is accepted by Ghostscript. Geometry is pinned
//! by arithmetic (turtle positions, L-system growth), rendering by
//! ink counts — the corpus policy for rand-driven art.

use pscat::Interp;

fn load(it: &mut Interp) {
    let src = std::fs::read("lib/artkit.ps").expect("artkit present");
    it.run_source(&src)
        .unwrap_or_else(|e| panic!("artkit failed to load: {}", it.error_report(&e)));
}

fn eval(src: &str) -> Vec<String> {
    let mut it = Interp::new();
    load(&mut it);
    it.run_str(src)
        .unwrap_or_else(|e| panic!("eval of {src:?} failed: {}", it.error_report(&e)));
    it.operand_stack().iter().map(|o| o.repr()).collect()
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
fn random_helpers_are_seeded_and_in_range() {
    let got = eval("3 srand 100 chance 3 srand 100 chance");
    assert_eq!(got[0], got[1], "same seed, same draw");
    let n: i64 = got[0].parse().unwrap();
    assert!((0..100).contains(&n), "chance out of range: {n}");
    let got = eval("1 srand 50 { 10 jit } repeat");
    for v in &got {
        let n: i64 = v.parse().unwrap();
        assert!((-10..=10).contains(&n), "jit out of range: {n}");
    }
    let got = eval("2 srand frnd frnd");
    for v in &got {
        let f: f64 = v.parse().unwrap();
        assert!((0.0..=1.0).contains(&f), "frnd out of range: {f}");
    }
}

#[test]
fn color_helpers_compute() {
    assert_eq!(eval("[1 0 0] [0 0 1] 0.5 mix3"), ["0.5", "0.0", "0.5"]);
    assert_eq!(eval("[0.8 0.6 0.4] 0.5 shade"), ["0.4", "0.3", "0.2"]);
    // k>1 mixes toward white
    assert_eq!(eval("[0.8 0.6 0.4] 1.5 shade"), ["0.9", "0.8", "0.7"]);
    // every palette holds five [r g b] triples
    let got = eval(
        "0 Palettes { exch pop dup length 5 eq { { length 3 eq { 1 add } if } forall } { pop } ifelse }
         forall",
    );
    assert_eq!(got, ["40"], "8 palettes x 5 well-formed colors");
}

#[test]
fn turtle_walks_and_nests() {
    assert_eq!(
        eval("newpath 0 0 0 thome 10 fd 90 tl 10 fd currentpoint"),
        ["10.0", "10.0"]
    );
    // tpush/tpop restore pose exactly
    assert_eq!(
        eval("newpath 5 5 45 thome tpush 30 fd 90 tr tpush 10 hop tpop tpop currentpoint"),
        ["5.0", "5.0"]
    );
}

#[test]
fn lsystem_grows_and_caps() {
    // F[+F]F[-F]F is 11 chars with 5 F's: depth 2 = 5*11 + 6.
    assert_eq!(
        eval("(F) << (F) 0 get (F[+F]F[-F]F) >> 2 lsys length"),
        ["61"]
    );
    // The 60000-char cap stops expansion instead of erroring.
    let got = eval("(F) << (F) 0 get (FFFFFFFFFF) >> 9 lsys length");
    let n: i64 = got[0].parse().unwrap();
    assert!(n <= 60000, "lsys ran past the cap: {n}");
}

#[test]
fn ldraw_renders_a_plant() {
    let mut it = Interp::with_page(200, 200).expect("page");
    load(&mut it);
    it.run_str(
        "1 setlinewidth newpath 100 10 90 thome \
         (F) << (F) 0 get (F[+F]F[-F]F) >> 3 lsys \
         2.2 25 ldraw stroke",
    )
    .unwrap_or_else(|e| panic!("ldraw failed: {}", it.error_report(&e)));
    assert!(ink_count(&it) > 200, "expected a plant's ink");
}

#[test]
fn alongpath_stamps_at_pitch() {
    // A 100-unit line at pitch 10 gets stamps at 0,10,...,100.
    assert_eq!(
        eval(
            "newpath 0 0 moveto 100 0 lineto /n 0 def \
             10 { pop pop pop /n n 1 add def } alongpath n"
        ),
        ["11"]
    );
    // The stamp receives the local direction in degrees.
    assert_eq!(
        eval("newpath 0 0 moveto 0 50 lineto /a null def 60 { /a exch def pop pop } alongpath a"),
        ["90.0"]
    );
}

#[test]
fn pathtext_sets_text_along_a_path() {
    let mut it = Interp::with_page(300, 100).expect("page");
    load(&mut it);
    it.run_str(
        "/Helvetica findfont 30 scalefont setfont \
         newpath 10 40 moveto 290 40 lineto (WALKING) pathtext",
    )
    .unwrap_or_else(|e| panic!("pathtext failed: {}", it.error_report(&e)));
    assert!(ink_count(&it) > 300, "expected glyph ink along the path");
}

fn ink_of(body: &str) -> usize {
    let mut it = Interp::with_page(400, 400).expect("page");
    load(&mut it);
    it.run_str(body)
        .unwrap_or_else(|e| panic!("{body} failed: {}", it.error_report(&e)));
    ink_count(&it)
}

#[test]
fn ctext_stamps_every_glyph_including_the_last() {
    // Regression guard for the classic path-following-text bug: if the
    // swept arc is even slightly shorter than the string's measured
    // width, the trailing glyph is silently never stamped -- no error,
    // and a whole-canvas ink_count threshold wouldn't notice. Compare
    // ink with and without the final character: dropping it should
    // visibly reduce ink; if ctext already silently drops it, the two
    // counts come out nearly equal instead.
    let setup = "/Helvetica-Bold findfont 24 scalefont setfont ";
    let full = ink_of(&format!("{setup} 200 200 90 0 (CIRCULARTEXT) ctext"));
    let truncated = ink_of(&format!("{setup} 200 200 90 0 (CIRCULARTEX) ctext"));
    assert!(
        full > truncated + 50,
        "last glyph doesn't appear to contribute ink: full={full} truncated={truncated}"
    );
}

#[test]
fn ctext_and_ctextctr_leave_ink_at_small_and_large_radius() {
    // Small radius (with a font sized to actually fit the circle)
    // exercises tight curvature -- more flattened-segment faceting per
    // glyph; large radius exercises a long sweep. Both should place
    // visible glyph ink.
    // The small-radius glyphs are individually tiny (12pt), so
    // antialiasing leaves far fewer fully-dark pixels than the large
    // case's 24pt glyphs -- the two cases get separate, individually
    // calibrated thresholds rather than one shared number.
    let small = "/Helvetica-Bold findfont 12 scalefont setfont ";
    let large = "/Helvetica-Bold findfont 24 scalefont setfont ";
    for body in [
        format!("{small} 150 150 40 0 (hi) ctext"),
        format!("{small} 150 150 40 (hi) ctextctr"),
    ] {
        assert!(ink_of(&body) > 20, "{body} left too little ink");
    }
    for body in [
        format!("{large} 200 200 180 0 (going the distance) ctext"),
        format!("{large} 200 200 180 (going the distance) ctextctr"),
    ] {
        assert!(ink_of(&body) > 200, "{body} left too little ink");
    }
}

#[test]
fn shapes_fill() {
    for shape in [
        "100 100 60 6 ngon",
        "100 100 70 30 5 star",
        "30 30 140 100 20 rrect",
    ] {
        let mut it = Interp::with_page(200, 200).expect("page");
        load(&mut it);
        it.run_str(&format!("newpath {shape} fill"))
            .unwrap_or_else(|e| panic!("{shape} failed: {}", it.error_report(&e)));
        assert!(ink_count(&it) > 500, "{shape} left no ink");
    }
}

#[test]
fn ghostscript_accepts_artkit() {
    let gs_ok = std::process::Command::new("gs")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success());
    if !gs_ok {
        eprintln!("skipping gs compatibility check: gs not installed");
        return;
    }
    let lib = std::fs::read_to_string("lib/artkit.ps").expect("artkit");
    let driver = "3 srand \
        newpath 100 100 0 thome 1 1 60 { dup fd 89 tr pop } for stroke \
        newpath 200 50 90 thome (F) << (F) 0 get (F[+F]F) >> 3 lsys 3 20 ldraw stroke \
        newpath 50 300 40 6 ngon fill \
        newpath 20 200 moveto 380 200 lineto 25 { pop pop pop } alongpath \
        /Helvetica findfont 20 scalefont setfont \
        newpath 20 350 moveto 380 380 lineto (gs runs artkit) pathtext \
        150 250 40 (ring of type) ctextctr \
        showpage\n";
    let dir = std::env::temp_dir().join(format!("pscat-artkit-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let combined = dir.join("artkit_gs.ps");
    std::fs::write(&combined, format!("{lib}\n{driver}")).expect("write");
    let status = std::process::Command::new("gs")
        .args([
            "-dNOPAUSE",
            "-dBATCH",
            "-q",
            "-sDEVICE=png16m",
            "-g400x400",
            "-r72",
            "-o/dev/null",
        ])
        .arg(&combined)
        .status()
        .expect("run gs");
    assert!(status.success(), "gs rejected artkit");
}
