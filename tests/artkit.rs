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
fn hexgrid_stamp_calling_hex_needs_its_own_dict_and_that_actually_works() {
    // hexgrid and hex share the tg- prefix (tgx/tgy/tgr): a stamp that
    // calls hex *without* opening a fresh dict first clobbers hexgrid's
    // own loop state the instant the first cell runs (proven while
    // building this feature -- and true of the pre-existing grid+ngon
    // pair too, same tk- prefix, same mechanism; not new here). The
    // section header documents the fix: wrap the call to hex/tri/
    // another tiling driver in its own small dict so its defs land
    // there instead of shadowing the outer driver's. This test pins
    // that the documented fix actually produces the right 9 centers,
    // not just that it doesn't error.
    let got = eval(
        "/hits 18 array def /hi 0 def \
         0 0 90 90 3 3 { \
             /r exch def /cy exch def /cx exch def \
             hits hi cx put /hi hi 1 add def \
             hits hi cy put /hi hi 1 add def \
             3 dict begin newpath cx cy r hex end \
         } hexgrid hits aload pop",
    );
    let nums: Vec<f64> = got.iter().map(|s| s.parse().unwrap()).collect();
    // r = (90/3)/sqrt(3) = 17.32..; dx = r*sqrt(3) = 30; dy = r*1.5 = 25.98..
    let expected = [
        (0.0, 0.0),
        (30.0, 0.0),
        (60.0, 0.0),
        (15.0, 25.980759227066642),
        (45.0, 25.980759227066642),
        (75.0, 25.980759227066642),
        (0.0, 51.961518454133284),
        (30.0, 51.961518454133284),
        (60.0, 51.961518454133284),
    ];
    for (k, (ex, ey)) in expected.iter().enumerate() {
        assert!(
            (nums[k * 2] - ex).abs() < 1e-6 && (nums[k * 2 + 1] - ey).abs() < 1e-6,
            "cell {k}: expected ({ex}, {ey}), got ({}, {})",
            nums[k * 2],
            nums[k * 2 + 1]
        );
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
    // The httile call near the end runs at depth 4 (not a shallower,
    // faster depth): the near-collinear-with-origin edge case in
    // horthocircle that produces an oversized arc radius -- caught once
    // already as a gs `limitcheck`, since it's gs's own `arc` that has
    // the tighter limit -- only actually arises a few reflection
    // generations deep. A shallower depth would pass this check without
    // ever exercising that path.
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
        340 350 35 7 3 4 { pop 0.4 setgray fill } httile \
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

#[test]
fn trigrid_stamp_calling_tri_with_a_whole_stamp_dict_wrap() {
    // Companion to the hexgrid+hex regression above, but for the
    // pattern the shipped code actually uses: examples/tiling.ps,
    // gallery/woven_labyrinth.ps, and the gs-compat driver all wrap
    // the *whole* stamp proc in one dict rather than just the inner
    // hex/tri call. `tri` is the worse case to get wrong: it clobbers
    // tgs/tga/tgR too, not just tgx/tgy -- an unwrapped composition
    // corrupts tile *size*, which the ink-coverage test above wouldn't
    // necessarily catch (a few undersized triangles barely move the
    // total). Pins both the exact centers and that `s` stays exactly
    // 30.0 across every triangle. The counter here can't be a plain
    // `/i i 1 add def` (that trick would itself be swallowed by the
    // wrap, per the header's second gotcha) -- it's a 1-element array
    // mutated with `put` instead, immune to dict nesting.
    let got = eval(
        "/hits 24 array def /idxbox 1 array def idxbox 0 0 put \
         0 0 90 60 3 1 { 5 dict begin \
             /up exch def /s exch def /cy exch def /cx exch def \
             /i idxbox 0 get def \
             hits i cx put \
             hits i 1 add cy put \
             hits i 2 add s put \
             hits i 3 add up { 1 } { 0 } ifelse put \
             idxbox 0 i 4 add put \
             newpath cx cy s up tri fill \
         end } trigrid hits aload pop",
    );
    let nums: Vec<f64> = got.iter().map(|s| s.parse().unwrap()).collect();
    // s = 90/3 = 30 exactly; a = s*0.288675, R = s*0.577350 (up/down
    // centroid offsets) -- values below taken from the interpreter's
    // own arithmetic, not hand-rounded.
    let expected = [
        (7.5, 8.660250000000001, true),
        (22.5, 17.320500000000003, false),
        (37.5, 8.660250000000001, true),
        (52.5, 17.320500000000003, false),
        (67.5, 8.660250000000001, true),
        (82.5, 17.320500000000003, false),
    ];
    for (k, (ex, ey, eup)) in expected.iter().enumerate() {
        let (cx, cy, s, up) = (
            nums[k * 4],
            nums[k * 4 + 1],
            nums[k * 4 + 2],
            nums[k * 4 + 3],
        );
        assert!((cx - ex).abs() < 1e-9, "triangle {k}: cx {cx} != {ex}");
        assert!((cy - ey).abs() < 1e-9, "triangle {k}: cy {cy} != {ey}");
        assert_eq!(s, 30.0, "triangle {k}: edge length drifted to {s}");
        assert_eq!(
            up,
            if *eup { 1.0 } else { 0.0 },
            "triangle {k}: wrong orientation"
        );
    }
}

#[test]
fn horthocircle_produces_a_circle_orthogonal_to_the_unit_circle() {
    // A circle (center C, radius r) is orthogonal to the unit circle iff
    // |C|^2 = r^2 + 1 -- the defining property a hyperbolic geodesic's
    // support circle must have. Checked on several non-collinear-with-
    // origin point pairs (isline must come back false for each).
    for (x1, y1, x2, y2) in [
        (0.2, 0.1, 0.5, -0.3),
        (-0.4, 0.6, 0.1, -0.5),
        (0.7, 0.2, -0.3, 0.6),
        (-0.6, -0.6, 0.5, -0.2),
    ] {
        let got = eval(&format!("{x1} {y1} {x2} {y2} horthocircle"));
        assert_eq!(got[3], "false", "({x1},{y1})-({x2},{y2}) came back isline");
        let a: f64 = got[0].parse().unwrap();
        let b: f64 = got[1].parse().unwrap();
        let r: f64 = got[2].parse().unwrap();
        assert!(
            (a * a + b * b - (r * r + 1.0)).abs() < 1e-9,
            "({x1},{y1})-({x2},{y2}): center {a},{b} radius {r} not orthogonal to unit circle"
        );
    }
}

#[test]
fn hreflect_is_an_involution_and_fixes_its_own_geodesic_points() {
    // Reflecting a point across a geodesic twice must return it exactly
    // (hreflect is its own inverse), and reflecting either point that
    // *defines* the geodesic must fix it (it's already on the line/arc
    // being reflected across). Covers both branches: a proper arc and
    // the degenerate diameter (points collinear with the origin).
    for (x1, y1, x2, y2, px, py) in [
        (0.2, 0.1, 0.5, -0.3, 0.35, 0.42), // arc case
        (0.3, 0.3, -0.2, -0.2, -0.1, 0.6), // diameter case
    ] {
        let fixed = eval(&format!(
            "{x1} {y1} {x2} {y2} horthocircle {x1} {y1} hreflect"
        ));
        let fx: f64 = fixed[0].parse().unwrap();
        let fy: f64 = fixed[1].parse().unwrap();
        assert!(
            (fx - x1).abs() < 1e-6 && (fy - y1).abs() < 1e-6,
            "defining point ({x1},{y1}) not fixed: got ({fx},{fy})"
        );

        let twice = eval(&format!(
            "/hc [{x1} {y1} {x2} {y2} horthocircle] def \
             hc aload pop {px} {py} hreflect \
             /ry exch def /rx exch def \
             hc aload pop rx ry hreflect"
        ));
        let tx: f64 = twice[0].parse().unwrap();
        let ty: f64 = twice[1].parse().unwrap();
        assert!(
            (tx - px).abs() < 1e-6 && (ty - py).abs() < 1e-6,
            "double reflection of ({px},{py}) didn't return: got ({tx},{ty})"
        );
    }
}

#[test]
fn httile_circumradius_formula_matches_independently_verified_values() {
    // httile's fundamental-polygon radius: cosh(rh) = cot(pi/p)*cot(pi/q),
    // R = tanh(rh/2), simplified to sqrt((C-1)/(C+1)) to avoid needing
    // acosh (C = cosh(rh)). This re-runs that exact expression (mirrored
    // from lib/artkit.ps's httile, not calling it -- httile has no public
    // way to report R on its own) against values independently verified
    // by building each {p,q}'s fundamental polygon at the candidate R and
    // measuring the *Euclidean* tangent angle between adjacent edges at a
    // shared vertex (equal to the hyperbolic angle there, since the disk
    // model is conformal) -- confirmed to reproduce 360/q exactly for
    // both pairs below (and three others) in a standalone prototype
    // before this formula went into artkit.ps (see NOTES.md).
    for (p, q, expected_r) in [(7, 3, 0.300742618746379), (6, 4, 0.5176380902050418)] {
        let got = eval(&format!(
            "180 {p} div cos 180 {p} div sin div \
             180 {q} div cos 180 {q} div sin div mul \
             dup 1 sub exch 1 add div sqrt"
        ));
        let r: f64 = got[0].parse().unwrap();
        assert!(
            (r - expected_r).abs() < 1e-12,
            "{{{p},{q}}}: R = {r}, expected {expected_r}"
        );
    }
}

#[test]
fn httile_and_hpoly_never_paint_outside_the_disk() {
    // The single most likely place for a subtle bug: hgeo/hpoly's
    // arc-direction picker (which of the two arcs on the geodesic's
    // support circle stays inside the disk). A wrong branch produces an
    // arc that bulges *outside* the unit circle -- fill the whole
    // tessellation (hpoly's path) and stroke a handful of standalone
    // near-boundary geodesics (hgeo -- exercised nowhere else in this
    // suite, and it duplicates hpoly's arc-direction logic rather than
    // sharing it, so a bug fixed in one and not the other needs its own
    // coverage) and assert not one pixel of ink lands outside the
    // disk's own radius from its center (canvas sized to exactly match,
    // so there's nowhere for stray ink to hide off-screen).
    let mut it = Interp::with_page(300, 300).expect("page");
    load(&mut it);
    it.run_str(
        "1 setgray newpath 0 0 300 300 rectfill \
         150 150 145 7 3 4 { pop 0 setgray fill } httile \
         0 setgray 1.5 setlinewidth \
         newpath 150 150 145 0.85 0.1 -0.75 0.6 hgeo stroke \
         newpath 150 150 145 -0.8 -0.2 0.3 -0.85 hgeo stroke \
         newpath 150 150 145 0.1 0.9 -0.9 -0.15 hgeo stroke \
         newpath 150 150 145 0.05 0.05 0.95 0.1 hgeo stroke",
    )
    .unwrap_or_else(|e| panic!("httile failed: {}", it.error_report(&e)));
    let pixmap = &it.gfx().pixmap;
    let (cx, cy, r) = (150.0_f64, 150.0_f64, 145.0_f64);
    let margin = 2.0; // stroke/AA slack, not geometry slack
    for y in 0..pixmap.height() {
        for x in 0..pixmap.width() {
            let p = pixmap.pixel(x, y).expect("in bounds");
            if p.red() < 128 {
                let dx = x as f64 + 0.5 - cx;
                let dy = y as f64 + 0.5 - cy;
                let dist = (dx * dx + dy * dy).sqrt();
                assert!(
                    dist <= r + margin,
                    "ink at ({x},{y}), distance {dist:.2} from center exceeds disk radius {r}"
                );
            }
        }
    }
}

#[test]
fn httile_generates_the_expected_tile_count_and_calls_proc_that_many_times() {
    // Pins the whole BFS-plus-dedup pipeline's output size for one fixed
    // {p,q,depth,frame} -- a regression net on the reflection generator
    // and its edge-length-scaled dedup tolerance together, the way
    // lattice_walks_the_expected_points pins lattice's exact points.
    //
    // This pin sits closer to a floating-point cliff than most: the
    // dedup check is `distance < 0.3 * edge_length`, and at least one
    // candidate tile in this {7,3} depth-2 configuration was observed to
    // land within noise of that boundary (pscat and an independent
    // Python prototype of the same algorithm agreed on every {p,q,depth}
    // tried except this class of case, off by exactly one tile — see
    // NOTES.md). If this test starts failing after an unrelated change
    // (a trig implementation detail, a number-representation change),
    // check whether the new count is still a plausible dense {7,3}
    // tiling (render it) before assuming the reflection/dedup logic
    // itself regressed -- it may just be a duplicate near the boundary
    // resolving the other way.
    let got = eval("/n 0 def 100 100 90 7 3 2 { pop /n n 1 add def } httile n");
    assert_eq!(
        got,
        ["30"],
        "httile tile count drifted from its pinned value"
    );
}

#[test]
fn fundamental_polygon_edges_meet_at_the_expected_interior_angle() {
    // Complements the circumradius arithmetic pin above with the
    // geometric property the formula is *for*: q polygons meeting at
    // every vertex means each interior angle is 360/q degrees. Measures
    // it directly (not re-deriving the angle algebraically) off the
    // fundamental polygon's own two edges at vertex 0, via each edge's
    // true Euclidean tangent direction there (equal to the hyperbolic
    // angle, since the disk model is conformal) -- horthocircle gives
    // the supporting circle, and the tangent at a point on it is
    // perpendicular to (point - center); the isline case's `a,b` is
    // already a unit direction, no perpendicular needed. Each tangent's
    // sign is disambiguated by which way points back toward the other
    // vertex on that edge (the arcs in play here are always < 180 deg,
    // so "closer to the straight chord" reliably picks the right one).
    // Reproduces this repo's own construction, so this is checking the
    // same claim the Python prototype (see NOTES.md) checked
    // independently before this formula went into artkit.ps -- not a
    // duplicate of the circumradius test above, since a bug in vertex
    // ordering or the horthocircle/hreflect math itself (as opposed to
    // just the R formula) would pass that test but fail this one.
    const BODY: &str = "\
             /C 180 p div cos 180 p div sin div \
                180 q div cos 180 q div sin div mul def \
             /R C 1 sub C 1 add div sqrt def \
             /v0x R def /v0y 0 def \
             /v1a 360 1 mul p div def \
             /v1x R v1a cos mul def /v1y R v1a sin mul def \
             /vpa 360 p 1 sub mul p div def \
             /vpx R vpa cos mul def /vpy R vpa sin mul def \
             /tangent { \
                 /twy exch def /twx exch def \
                 /pty exch def /ptx exch def \
                 /isl exch def /r exch def /b exch def /a exch def \
                 isl { /tx a def /ty b def } { \
                     /tx pty b sub neg def /ty ptx a sub def \
                     /tn tx dup mul ty dup mul add sqrt def \
                     /tx tx tn div def /ty ty tn div def \
                 } ifelse \
                 tx twx ptx sub mul ty twy pty sub mul add 0 lt { \
                     /tx tx neg def /ty ty neg def \
                 } if \
                 tx ty \
             } def \
             vpx vpy v0x v0y horthocircle v0x v0y vpx vpy tangent \
             /t1y exch def /t1x exch def \
             v0x v0y v1x v1y horthocircle v0x v0y v1x v1y tangent \
             /t2y exch def /t2x exch def \
             /dotp t1x t2x mul t1y t2y mul add def \
             /crossp t1x t2y mul t1y t2x mul sub def \
             /ang crossp dotp atan def \
             ang 180 gt { 360 ang sub } { ang } ifelse";
    for (p, q) in [(7, 3), (6, 4), (5, 4), (8, 3)] {
        let got = eval(&format!("/p {p} def /q {q} def {BODY}"));
        let angle: f64 = got[0].parse().unwrap();
        let expected = 360.0 / q as f64;
        assert!(
            (angle - expected).abs() < 1e-6,
            "{{{p},{q}}}: measured interior angle {angle}, expected {expected}"
        );
    }
}
