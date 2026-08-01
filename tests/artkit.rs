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
fn ctext_preserves_the_callers_current_path() {
    // artkit's other path-touching brushes leave the caller's path
    // alone (alongpath's header: "The current path survives"); ctext
    // should honor the same contract rather than silently replacing
    // whatever path the caller had built with its own arc. Build a
    // large rectangle, call ctext, then stroke: if the rectangle
    // survived, the stroke's ink reflects its ~1500pt perimeter, not
    // just ctext's own small arc.
    let mut it = Interp::with_page(400, 400).expect("page");
    load(&mut it);
    it.run_str(
        "/Helvetica-Bold findfont 20 scalefont setfont \
         newpath 10 10 moveto 390 10 lineto 390 390 lineto 10 390 lineto closepath \
         200 200 60 0 (mark) ctext \
         2 setlinewidth stroke",
    )
    .unwrap_or_else(|e| panic!("ctext failed: {}", it.error_report(&e)));
    assert!(
        ink_count(&it) > 1000,
        "expected the surviving rectangle's stroke, not just ctext's own arc"
    );
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
fn hex_and_tri_fill() {
    for shape in [
        "100 100 60 hex",
        "100 100 60 true tri",
        "100 100 60 false tri",
    ] {
        let mut it = Interp::with_page(200, 200).expect("page");
        load(&mut it);
        it.run_str(&format!("newpath {shape} fill"))
            .unwrap_or_else(|e| panic!("{shape} failed: {}", it.error_report(&e)));
        assert!(ink_count(&it) > 500, "{shape} left no ink");
    }
}

#[test]
fn lattice_walks_the_expected_points() {
    // 2x2 lattice with an oblique second basis vector -- pin the exact
    // points visited (in order) so a future refactor can't silently
    // transpose i/j or swap v1/v2 without a test noticing. The proc
    // records each x,y it's called with into `hits` via two puts
    // (results array left on the stack via aload for the assertions).
    let got = eval(
        "/hits 8 array def /hi 0 def \
         0 0 10 0 3 5 2 2 { \
             /py exch def /px exch def \
             hits hi px put /hi hi 1 add def \
             hits hi py put /hi hi 1 add def \
         } lattice hits aload pop",
    );
    let nums: Vec<f64> = got.iter().map(|s| s.parse().unwrap()).collect();
    // points = (0,0) + i*(10,0) + j*(3,5), for i in 0..2, j in 0..2
    let expected = [(0.0, 0.0), (10.0, 0.0), (3.0, 5.0), (13.0, 5.0)];
    for (k, (ex, ey)) in expected.iter().enumerate() {
        assert_eq!(nums[k * 2], *ex, "point {k} x");
        assert_eq!(nums[k * 2 + 1], *ey, "point {k} y");
    }
}

#[test]
fn hexgrid_and_trigrid_tile_their_region_without_gaps() {
    // A correctly interlocking tiling should leave nearly as much ink
    // as a solid fill of the same bounding box -- a gap or overlap bug
    // (wrong row offset, wrong triangle orientation) would show up as
    // noticeably less coverage. The canvas is sized to exactly match
    // the box (200x200, no margin) so page clipping does the same job
    // clipping-to-the-box would -- ink outside the nominal region
    // can't hide on an off-screen margin and inflate the count.
    fn box_ink(body: &str) -> usize {
        let mut it = Interp::with_page(200, 200).expect("page");
        load(&mut it);
        it.run_str(body)
            .unwrap_or_else(|e| panic!("{body} failed: {}", it.error_report(&e)));
        ink_count(&it)
    }

    let baseline = box_ink("newpath 0 0 200 200 rectfill");

    let hex_ink = box_ink(
        "0 0 200 200 5 6 { 3 dict begin /r exch def /cy exch def /cx exch def \
            newpath cx cy r hex fill end } hexgrid",
    );
    assert!(
        hex_ink as f64 > baseline as f64 * 0.85,
        "hexgrid left gaps: hex_ink={hex_ink} baseline={baseline}"
    );

    let tri_ink = box_ink(
        "0 0 200 200 6 7 { 4 dict begin /up exch def /s exch def /cy exch def /cx exch def \
            newpath cx cy s up tri fill end } trigrid",
    );
    assert!(
        tri_ink as f64 > baseline as f64 * 0.85,
        "trigrid left gaps: tri_ink={tri_ink} baseline={baseline}"
    );
}

#[test]
fn truchet_calls_proc_once_per_cell_with_the_cell_size() {
    let got = eval(
        "/n 0 def \
         0 0 90 60 3 2 { /ch exch def /cw exch def \
             cw 30 eq { } { /n -1 def } ifelse \
             ch 30 eq { } { /n -1 def } ifelse \
             n 0 ge { /n n 1 add def } if \
         } truchet n",
    );
    assert_eq!(
        got.last().unwrap(),
        "6",
        "expected 6 cells, each sized 30x30"
    );
}

#[test]
fn truchet_randomizes_the_rotation_across_cells() {
    // Read back cos(rotation) from inside the stamp proc via
    // currentmatrix -- 0/90/180/270 degrees give distinct values
    // (1, 0, -1, 0). A real spread of buckets (not just one repeated
    // value) is the actual claim `truchet`'s doc comment makes.
    let got = eval(
        "3 srand /idx 0 def /hits 25 array def \
         0 0 100 100 5 5 { pop pop \
             hits idx matrix currentmatrix 0 get put \
             /idx idx 1 add def \
         } truchet hits aload pop",
    );
    let mut buckets = std::collections::HashSet::new();
    for v in &got {
        let f: f64 = v.parse().unwrap();
        let bucket = if f > 0.5 {
            1
        } else if f < -0.5 {
            -1
        } else {
            0
        };
        buckets.insert(bucket);
    }
    assert!(
        buckets.len() >= 2,
        "expected a mix of rotations across 25 cells, saw only {buckets:?}"
    );
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
        260 20 100 60 3 3 { 3 dict begin /r exch def /cy exch def /cx exch def \
            newpath cx cy r hex fill end } hexgrid \
        260 100 100 60 3 3 { 4 dict begin /up exch def /s exch def /cy exch def /cx exch def \
            newpath cx cy s up tri fill end } trigrid \
        0 0 60 60 3 3 { pop pop newpath 0 0 20 0 360 arc fill } truchet \
        300 300 15 0 0 15 3 3 { /y exch def /x exch def newpath x y 5 0 360 arc fill } lattice \
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
