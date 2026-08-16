//! The art toolkit (lib/artkit.ps, Stage 19): every subsystem loads
//! and computes what its header promises, deterministically under
//! srand, and the file is accepted by Ghostscript. Geometry is pinned
//! by arithmetic (turtle positions, L-system growth), rendering by
//! ink counts — the corpus policy for rand-driven art.

use pscat::{Interp, PsError};

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

fn pixel(it: &Interp, x: u32, y: u32) -> (u8, u8, u8) {
    let p = it
        .gfx()
        .pixmap
        .pixel(x, y)
        .unwrap_or_else(|| panic!("pixel ({x},{y}) out of bounds"));
    (p.red(), p.green(), p.blue())
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
fn walkpath_regular_stops_match_alongpath_exactly() {
    // The compatibility claim this file's header and NOTES.md make:
    // walkpath's regular (non-end-flagged) stops land exactly where
    // alongpath's would, on the same path. Mixes line and closepath
    // segments across a closed subpath so the walk crosses several
    // segment boundaries, not just one straight run.
    let path = "newpath 0 0 moveto 40 0 lineto 40 40 lineto 0 40 lineto closepath ";
    let along = eval(&format!(
        "{path} 7 {{ /a exch def /y exch def /x exch def x y }} alongpath"
    ));
    let walk = eval(&format!(
        "{path} 7 {{ /at exch def /sp exch def /t exch def /ang exch def \
              /y exch def /x exch def at 2 and 0 eq {{ x y }} if }} walkpath"
    ));
    assert_eq!(along, walk);
}

#[test]
fn walkpath_on_an_empty_path_is_a_silent_no_op() {
    let got = eval("newpath /n 0 def 10 { pop pop pop pop pop pop /n n 1 add def } walkpath n");
    assert_eq!(got, ["0"]);
}

#[test]
fn walkpath_rejects_non_positive_pitch_instead_of_hanging() {
    // A zero or negative pitch would otherwise never advance wkt2 past
    // wkseglen, looping forever (Codex review, round 1) -- caught up
    // front with the file's existing malformed-input idiom (a guarded
    // call to a self-documenting undefined name, see
    // et-spacing-must-be-positive in lib/etching.ps).
    let mut it = Interp::new();
    load(&mut it);
    for pitch in ["0", "-5"] {
        let err = it
            .run_str(&format!(
                "newpath 0 0 moveto 100 0 lineto {pitch} {{ }} walkpath"
            ))
            .unwrap_err();
        assert!(
            matches!(err, PsError::Undefined(ref name) if name == "walkpath-pitch-must-be-positive"),
            "pitch {pitch}: got {err}"
        );
    }
}

#[test]
fn walkpath_short_nonzero_subpath_gets_distinct_start_and_end_stops() {
    // A subpath shorter than one pitch step but with nonzero length
    // (unlike a true single-point subpath) still gets two distinct
    // calls -- start and guaranteed end -- not coalesced into one
    // atend=3 call (Codex review, round 1: the header previously
    // claimed otherwise).
    let got = eval(
        "newpath 0 0 moveto 1 0 lineto /n 0 def /firstat null def /lastat null def \
         10 { /at exch def pop pop pop pop pop \
              n 0 eq { /firstat at def } if /lastat at def \
              /n n 1 add def } walkpath \
         n firstat lastat",
    );
    assert_eq!(got, ["2", "1", "2"]);
}

#[test]
fn walkpath_adds_a_guaranteed_end_stop_alongpath_cannot_promise() {
    // Same 100-unit line, pitch 30: interior stops land at 0,30,60,90
    // (4, matching alongpath exactly), plus one guaranteed extra call
    // at the literal end (100) that isn't a pitch multiple -- flagged
    // atend=2 (end bit), with sp reporting the 10-unit leftover.
    let got = eval(
        "newpath 0 0 moveto 100 0 lineto /n 0 def /lastatend 0 def /lastsp 0 def /lastt 0 def \
         30 { /at exch def /sp exch def /t exch def pop pop pop \
              /lastatend at def /lastsp sp def /lastt t def \
              /n n 1 add def } walkpath \
         n lastatend lastsp lastt",
    );
    assert_eq!(got, ["5", "2", "10.0", "1.0"]);
}

#[test]
fn walkpath_first_stop_is_the_start_with_its_tangent() {
    // Unlike a bare pitch-stepper, the very first call always carries
    // the true start point's tangent (not an undefined/zero angle).
    let got = eval(
        "newpath 0 0 moveto 0 50 lineto \
         /firstang null def /firstt null def /firstatend null def /n 0 def \
         60 { /at exch def /sp exch def /t exch def /ang exch def pop pop \
              n 0 eq { /firstang ang def /firstt t def /firstatend at def } if \
              /n n 1 add def } walkpath \
         firstang firstt firstatend",
    );
    assert_eq!(
        got,
        ["90.0", "0.0", "1"],
        "start bit set, t=0, correct tangent"
    );
}

#[test]
fn walkpath_closed_subpath_start_and_guaranteed_end_coincide() {
    // A 20x20 square's 80-unit perimeter at pitch 10: the guaranteed
    // end stop lands exactly back on the literal start point.
    let got = eval(
        "newpath 0 0 moveto 20 0 lineto 20 20 lineto 0 20 lineto closepath \
         /firstx null def /firsty null def /lastx null def /lasty null def /n 0 def \
         10 { /at exch def /sp exch def /t exch def /ang exch def /y exch def /x exch def \
              n 0 eq { /firstx x def /firsty y def } if \
              /lastx x def /lasty y def /n n 1 add def } walkpath \
         firstx firsty lastx lasty",
    );
    assert_eq!(got, ["0.0", "0.0", "0.0", "0.0"]);
}

#[test]
fn walkpath_degenerate_point_subpath_gets_one_call() {
    // A moveto with no following segment: sublen is 0, so walkpath
    // fires exactly once, both start and end bits set (atend=3).
    let got = eval(
        "newpath 5 5 moveto /n 0 def /a null def /t null def /sp null def \
         10 { /at exch def /sp1 exch def /t1 exch def pop pop pop \
              /a at def /t t1 def /sp sp1 def /n n 1 add def } walkpath \
         n a t sp",
    );
    assert_eq!(got, ["1", "3", "0", "0"]);
}

#[test]
fn walkpath_resets_progress_per_subpath() {
    // Two disjoint 10-unit lines walked at pitch 5: each is its own
    // centerline -- t restarts at 0 for the second one instead of
    // continuing from the first's cumulative length.
    let got = eval(
        "newpath 0 0 moveto 10 0 lineto 100 100 moveto 100 110 lineto \
         /n 0 def /secondfirstt null def /seen 0 def \
         5 { /at exch def /sp exch def /t exch def /ang exch def /y exch def /x exch def \
             x 100 ge seen 0 eq and { /secondfirstt t def /seen 1 def } if \
             /n n 1 add def } walkpath \
         n secondfirstt",
    );
    assert_eq!(
        got,
        ["8", "0.0"],
        "4 stops per subpath (0,5,10 + guaranteed end); second starts at t=0"
    );
}

#[test]
fn walkpath_handles_a_curve() {
    // Flattened like alongpath's path: curveto segments become chords
    // pathforall walks like any other, so t still runs 0..1 smoothly.
    let got = eval(
        "newpath 10 100 moveto 10 20 190 20 190 100 curveto \
         /n 0 def /lastt 0 def \
         5 { /at exch def /sp exch def /t exch def pop pop pop \
             /lastt t def /n n 1 add def } walkpath \
         n lastt",
    );
    assert_eq!(got[1], "1.0", "progress reaches 1.0 at the curve's end");
    let n: i64 = got[0].parse().unwrap();
    assert!(n > 5, "expected several stops along a flattened curve: {n}");
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
    // The httile calls near the end run at depth 4 (not a shallower,
    // faster depth): the near-collinear-with-origin edge case in
    // horthocircle that produces an oversized arc radius -- caught once
    // already as a gs `limitcheck`, since it's gs's own `arc` that has
    // the tighter limit -- only actually arises a few reflection
    // generations deep. A shallower depth would pass this check without
    // ever exercising that path. The second httile ({10,10}, the highest
    // p,q pair this suite exercises) is the exact configuration that once
    // made gs raise `undefinedresult` dividing by a catastrophically
    // cancelled `hod` -- see httile_survives_catastrophic_cancellation_at_high_p_q.
    //
    // The {7,3} depth-4 call also counts its own tiles (`nbox`) rather
    // than just painting, and that count is checked below against a
    // *band*, not pscat's own exact pinned value (232, see
    // httile_generates_the_expected_tile_count_and_calls_proc_that_many_times).
    // A cross-model review (round 8) found gs computes PostScript reals
    // in 32-bit float (`1 3 div` prints `0.333333343` under gs, vs this
    // interpreter's `0.3333333333333333`) -- a full order of magnitude
    // less precise than the f64 arithmetic horthocircle's degeneracy
    // threshold is tuned against, so gs's own tile count diverges from
    // this interpreter's at *every* depth this suite has checked (29 vs
    // 30 at depth 2, 232 vs 233 at this depth 4, 1711 vs 1653 at {6,4}
    // depth 5) -- a real, inherent precision difference between the two
    // interpreters' arithmetic, not a defect (see NOTES.md). The band
    // below exists to catch what actually matters under gs: a crash, a
    // collapse back toward the round-8 bug's 323, or growth toward
    // htmax -- not bit-for-bit parity with this interpreter, which
    // recursive floating-point BFS reflection can't guarantee across
    // different float widths.
    let driver = "3 srand \
        newpath 100 100 0 thome 1 1 60 { dup fd 89 tr pop } for stroke \
        newpath 200 50 90 thome (F) << (F) 0 get (F[+F]F) >> 3 lsys 3 20 ldraw stroke \
        newpath 50 300 40 6 ngon fill \
        newpath 20 200 moveto 380 200 lineto 25 { pop pop pop } alongpath \
        newpath 20 220 moveto 380 220 lineto \
            25 { pop pop pop pop pop pop } walkpath \
        newpath 100 5 moveto 100 55 300 55 300 5 curveto \
            20 { pop pop pop pop pop pop } walkpath \
        newpath 340 5 moveto 380 5 lineto 380 45 lineto 340 45 lineto closepath \
            10 { pop pop pop pop pop pop } walkpath \
        /Helvetica findfont 20 scalefont setfont \
        newpath 20 350 moveto 380 380 lineto (gs runs artkit) pathtext \
        150 250 40 (ring of type) ctextctr \
        260 20 100 60 3 3 { 3 dict begin /r exch def /cy exch def /cx exch def \
            newpath cx cy r hex fill end } hexgrid \
        260 100 100 60 3 3 { 4 dict begin /up exch def /s exch def /cy exch def /cx exch def \
            newpath cx cy s up tri fill end } trigrid \
        0 0 60 60 3 3 { pop pop newpath 0 0 20 0 360 arc fill } truchet \
        300 300 15 0 0 15 3 3 { /y exch def /x exch def newpath x y 5 0 360 arc fill } lattice \
        /nbox 1 array def nbox 0 0 put \
        340 350 35 7 3 4 { pop nbox 0 nbox 0 get 1 add put 0.4 setgray fill } httile \
        (GSTILES ) print nbox 0 get == \
        30 30 15 10 10 4 { pop } httile \
        newpath [150 20 180 71.96 210 20] /koch fgen 3 edgepoly fill \
        150 100 210 100 180 151.96 3 { \
            6 dict begin /gy3 exch def /gx3 exch def /gy2 exch def /gx2 exch def \
                /gy1 exch def /gx1 exch def \
                newpath gx1 gy1 moveto gx2 gy2 lineto gx3 gy3 lineto closepath \
                0.5 setgray fill \
            end \
        } gasket \
        230 20 60 60 2 { newpath 0.3 setgray rectfill } carpet \
        /Helvetica findfont 6 scalefont setfont \
        10 390 100 8 5 /justify (gs runs the paragraph flow section too) tfblock pop \
        10 5 45 5 18 2 6 /left (a short run of copy split across two narrow columns) tfcols pop \
        { pop 350 395 } 393 385 5 /center (curve) tfflow pop \
        noiseinit \
        100 100 noise2 pop \
        100 100 0.5 { pop pop 42 } curl2 pop pop \
        newpath 50 50 5 2 { pop pop 1 0 } advect stroke \
        showpage\n";
    let dir = std::env::temp_dir().join(format!("pscat-artkit-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let combined = dir.join("artkit_gs.ps");
    std::fs::write(&combined, format!("{lib}\n{driver}")).expect("write");
    let output = std::process::Command::new("gs")
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
        .output()
        .expect("run gs");
    assert!(
        output.status.success(),
        "gs rejected artkit: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let tiles: i64 = stdout
        .lines()
        .find_map(|l| l.strip_prefix("GSTILES "))
        .unwrap_or_else(|| panic!("gs didn't print a tile count: {stdout:?}"))
        .trim()
        .parse()
        .expect("tile count parses as an integer");
    assert!(
        (200..=260).contains(&tiles),
        "gs's {{7,3}} depth-4 tile count ({tiles}) is outside the sanity band \
         [200, 260] -- either far tighter dedup collapse or the round-8-style \
         inflation this band exists to catch (pscat's own exact count is 232, \
         gs's own precision means somewhere nearby, not identical, is expected)"
    );
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
    // Cross-checked against an independent Python prototype of the same
    // algorithm, which agrees exactly at this and several other
    // {p,q,depth} combinations (NOTES.md) -- this is a real invariant of
    // the geometry, not an incidental snapshot of whatever the code
    // happened to produce.
    let got = eval("/n 0 def 100 100 90 7 3 2 { pop /n n 1 add def } httile n");
    assert_eq!(
        got,
        ["29"],
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

#[test]
fn hreflect_stays_exact_near_the_old_radius_cap_boundary() {
    // Regression test for a real bug caught by cross-model review:
    // horthocircle used to fall back to treating *any* large-radius
    // support circle as a diameter -- a reasonable approximation for
    // *drawing* (an enormous arc and a straight line are visually
    // identical), but wrong for hreflect's transformation math, since
    // the diameter reflection formula only agrees with true circle
    // inversion when the two points really are collinear with the
    // origin. This exact pair -- collinear-with-origin *except* for a
    // small x offset -- has a real orthogonal-circle radius just above
    // the old cap (~51) while being nowhere near an actual diameter: the
    // old code reflected p1 to (-0.010195, 0.2) instead of fixing it.
    // horthocircle must still report the true circle (isline=false), and
    // hreflect must still fix p1 exactly. The >50 approximation now
    // lives only in hgeo/hpoly's drawing code, not here.
    let got = eval("0.010195 0.2 0.010195 -0.2 horthocircle");
    assert_eq!(got[3], "false", "should report a true circle, not isline");
    let r: f64 = got[2].parse().unwrap();
    assert!(r > 50.0, "expected a radius past the old cap, got {r}");

    let fixed = eval("0.010195 0.2 0.010195 -0.2 horthocircle 0.010195 0.2 hreflect");
    let fx: f64 = fixed[0].parse().unwrap();
    let fy: f64 = fixed[1].parse().unwrap();
    assert!(
        (fx - 0.010195).abs() < 1e-9 && (fy - 0.2).abs() < 1e-9,
        "p1 should be a fixed point of its own geodesic's reflection: got ({fx}, {fy})"
    );
}

#[test]
fn horthocircle_collinearity_test_is_scale_invariant() {
    // Regression test for a real bug caught by a third round of
    // cross-model review: the collinearity check used the raw
    // circumcircle determinant against an absolute threshold, whose
    // magnitude scales with how far apart p1/p2 happen to be, not with
    // how collinear they are with the origin. Two points a hair's width
    // apart but clearly *not* collinear with the origin -- this exact
    // pair, verified non-collinear since (0.99, 0) and
    // (0.98999999995, 0.0000099) have a nonzero angle between them as
    // seen from the origin -- used to false-positive as isline=true
    // (the same failure mode as the radius-cap bug above: hreflect then
    // mirrors across the wrong line and moves p1 instead of fixing it).
    // Exactly the kind of pair `httile` produces a few reflection
    // generations toward the rim, where edges shrink fast. Fixed by
    // testing collinearity angularly (sin of the angle between p1 and
    // p2 from the origin, via the cross product, scale-invariant by
    // construction) instead of via the raw determinant's magnitude.
    let got = eval("0.99 0 0.98999999995 0.0000099 horthocircle");
    assert_eq!(got[3], "false", "should report a true circle, not isline");

    let fixed = eval("0.99 0 0.98999999995 0.0000099 horthocircle 0.99 0 hreflect");
    let fx: f64 = fixed[0].parse().unwrap();
    let fy: f64 = fixed[1].parse().unwrap();
    assert!(
        (fx - 0.99).abs() < 1e-6 && fy.abs() < 1e-6,
        "p1 should be a fixed point of its own geodesic's reflection: got ({fx}, {fy})"
    );
}

#[test]
fn horthocircle_does_not_special_case_points_merely_near_the_origin() {
    // Regression test for a real bug caught by a fifth round of
    // cross-model review, in the same failure family as the radius-cap
    // and collinearity-scale bugs above: an earlier version of
    // horthocircle short-circuited to isline=true whenever *either*
    // point's squared norm was below an absolute 0.000001 (norm below
    // 0.001) -- meant to keep a point genuinely at the origin (a true
    // geometric special case: a geodesic through the exact center really
    // is a diameter) from dividing by its own zero norm downstream, but
    // wrong for any point merely *close* to the origin whose partner
    // isn't also along that same direction. This exact pair -- p1 a
    // hair's width from center, p2 nowhere near collinear with it and
    // the origin -- used to report isline=true and move p1 under
    // hreflect instead of fixing it. The rewritten horthocircle (see
    // httile_survives_catastrophic_cancellation_at_high_p_q below) solves
    // the orthogonal circle directly rather than special-casing either
    // point's magnitude, so a near-origin point degenerates correctly
    // only when it's actually collinear with its partner and the origin.
    let got = eval("0.0005 0 0 0.5 horthocircle");
    assert_eq!(got[3], "false", "should report a true circle, not isline");

    let fixed = eval("0.0005 0 0 0.5 horthocircle 0.0005 0 hreflect");
    let fx: f64 = fixed[0].parse().unwrap();
    let fy: f64 = fixed[1].parse().unwrap();
    assert!(
        (fx - 0.0005).abs() < 1e-9 && fy.abs() < 1e-9,
        "p1 should be a fixed point of its own geodesic's reflection: got ({fx}, {fy})"
    );
}

#[test]
fn horthocircle_isline_fallback_uses_the_radial_not_chord_direction() {
    // Regression test for a real bug caught by a sixth round of
    // cross-model review, in the same near-collinear-pair family as the
    // tests above. The isline branch used to derive its diameter
    // direction from the chord p2-p1 -- correct for *exactly* collinear
    // points (where the chord and the radius agree, since any two
    // distinct points on the same line through the origin have a
    // difference vector parallel to that line), but wrong for a
    // near-collinear pair let in by this branch's tolerance: when the
    // displacement from p1 to p2 has a tangential component, the chord
    // is no longer purely radial.
    //
    // Constructing a pair that actually exercises this took retuning
    // after round seven's fix below changed how tight that tolerance is
    // (see horthocircle_does_not_treat_close_together_points_as_collinear):
    // p1=(0.7,0.3) with p2 displaced by a *purely tangential* nudge
    // (perpendicular to p1, direction (-0.3,0.7)) small enough that D's
    // subtraction genuinely cancels relative to its own noise floor. The
    // old chord-direction formula's direction here is (-0.394, 0.919) --
    // essentially perpendicular to the true radial direction
    // (0.919, 0.394) -- so reflecting p1 across it would mirror across
    // entirely the wrong line through the origin. Fixed by taking the
    // direction from whichever of p1/p2 is farther from the origin
    // instead (better-conditioned, and exact whenever the pair really is
    // collinear, since both points' own directions and the chord's then
    // agree).
    let got = eval("0.7 0.3 0.6999999999997899 0.30000000000049 horthocircle");
    assert_eq!(got[3], "true", "should take the isline fallback");

    let fixed = eval("0.7 0.3 0.6999999999997899 0.30000000000049 horthocircle 0.7 0.3 hreflect");
    let fx: f64 = fixed[0].parse().unwrap();
    let fy: f64 = fixed[1].parse().unwrap();
    assert!(
        (fx - 0.7).abs() < 1e-6 && (fy - 0.3).abs() < 1e-6,
        "p1 should be a fixed point of its own geodesic's reflection: got ({fx}, {fy})"
    );
}

#[test]
fn horthocircle_does_not_treat_close_together_points_as_collinear() {
    // Regression test for a real bug caught by a seventh round of
    // cross-model review. The collinearity test used to compare
    // sin(angle between p1 and p2 as seen from the origin) against a
    // fixed relative threshold -- scale-invariant in |p1||p2| (fixing
    // round three's bug), but that isn't actually the right criterion:
    // sin(angle) is also small whenever p1 and p2 are simply close
    // together, regardless of whether the geodesic through them is
    // anywhere near a diameter. This exact pair -- two vertices from a
    // real {10,10} depth-4 httile BFS, ~9e-7 apart near the disk
    // boundary -- has sin(angle) tiny purely from that proximity: the
    // old test reported isline=true, but the true orthogonal circle here
    // has radius ~9.17e-6, a small, perfectly well-conditioned circle,
    // not anything diameter-like. Treating it as isline discarded real
    // geometry for a wrong approximation. Fixed by testing D = x1*y2 -
    // y1*x2 (the un-normalized cross product) against its own
    // subtraction's noise floor instead -- eps * (|x1*y2| + |y1*x2|) --
    // which is small only when there's genuine floating-point
    // cancellation in computing D itself, not merely when p1/p2 happen
    // to be nearby.
    let got = eval("0.9371500703 0.3489261063 0.9371497472 0.3489269739 horthocircle");
    assert_eq!(
        got[3], "false",
        "should report a true (small) circle, not isline"
    );
    let r: f64 = got[2].parse().unwrap();
    assert!(
        (r - 9.165692e-6).abs() < 1e-9,
        "expected the true small-radius circle, got r={r}"
    );
}

#[test]
#[ignore = "exercises an unrealistic {10,10} depth-4 case (~8,200 tiles); \
            takes ~30s release / minutes debug from the O(n^2) dedup scan \
            alone, so it's excluded from the default `cargo test` run -- \
            use `cargo test -- --ignored` to run it. The specific bugs it \
            found now have cheap, direct regression tests above; this one's \
            job is pinning the dedup mechanism's exact combinatorial output \
            at an extreme case, which genuinely needs the full BFS."]
fn httile_survives_catastrophic_cancellation_at_high_p_q() {
    // Regression test for a real bug caught by a fourth round of
    // cross-model review, and two follow-up rounds to get the fix right.
    // `horthocircle` originally found the orthogonal circle by inverting
    // p2 in the unit circle and solving a three-point circumcircle -- a
    // formula whose determinant is a *different*, non-scale-invariant
    // quantity from the angular collinearity test above, and can
    // catastrophically cancel to near (or exactly) zero even when p1/p2
    // are genuinely non-collinear. Round four caught this as a crash (gs
    // raised `undefinedresult` dividing by an `hod` that rounded to
    // exactly 0.0 four generations into a {10,10} tiling); rounds five
    // and six (see the tests above) found the fixes still had two more
    // real bugs in the same failure family. `horthocircle` now solves
    // the orthogonal circle directly: a circle's orthogonality to the
    // unit circle (|c|^2 = r^2+1) combined with passing through p_i
    // (|c-p_i|^2 = r^2) collapses to one linear equation per point,
    // c.p_i = (|p_i|^2+1)/2 -- a 2x2 system in c whose determinant is
    // exactly the angular test's own cross product, so one
    // scale-invariant quantity now serves as both the sole degeneracy
    // test and the sole divisor.
    //
    // Separately, round six also showed the dedup tolerance itself (see
    // httile's own comment on `httol`) was too loose at this extreme:
    // the {10,10} tile-adjacency graph has girth q=10 (the shortest
    // cycle comes from the 10 tiles meeting at a vertex), so no two
    // walks of length <=4 from the root can reach the same tile without
    // closing a cycle shorter than the girth allows -- the true count is
    // the plain reflection-tree growth 1+10+90+810+7290=8201, and the
    // old 0.3 factor gave 8191, merging ten genuinely distinct tiles.
    // Tightened to 0.2, which recovers 8201 here at no extra runtime
    // cost and leaves every other pinned {p,q,depth} in this file
    // unchanged. This also confirms {10,10} depth 4 stays well under
    // `htmax` (20000), so it's not a silent-truncation artifact either.
    let got = eval("/n 0 def 100 100 90 10 10 4 { pop /n n 1 add def } httile n");
    assert_eq!(
        got,
        ["8201"],
        "httile tile count for the {{10,10}} depth-4 cancellation case drifted"
    );
}

#[test]
fn httile_does_not_leak_a_callers_pre_existing_path_into_the_first_tile() {
    // Regression test for a real bug caught by cross-model review:
    // httile used to build each tile's path with hpoly alone, which only
    // *appends* to whatever path already existed -- so a caller who
    // hadn't just called newpath (or a page with leftover path state)
    // would have its first tile's fill/stroke drag in unrelated ink.
    // httile now calls newpath itself before each tile.
    let mut it = Interp::with_page(300, 300).expect("page");
    load(&mut it);
    it.run_str(
        "newpath 5 5 moveto 6 6 lineto \
         150 150 140 7 3 0 { pop } httile",
    )
    .unwrap_or_else(|e| panic!("httile failed: {}", it.error_report(&e)));
    let (lx, ly, ux, uy) = it.gfx().path_bbox().expect("path exists");
    // The stray (5,5)-(6,6) segment would pull the bbox's low corner
    // down near the origin if it leaked into the tile's path; the
    // fundamental heptagon at cx=cy=150, r=140 should have a bbox
    // comfortably inside the page and nowhere near (5,5).
    assert!(
        lx > 50.0 && ly > 50.0 && ux < 300.0 && uy < 300.0,
        "bbox ({lx}, {ly})-({ux}, {uy}) suggests the stray path leaked in"
    );
}

#[test]
fn hpolar_stays_finite_for_a_large_hyperbolic_radius() {
    // Regression test for a real bug caught by cross-model review:
    // hpolar computed tanh(hrad/2) as (e^hrad - 1)/(e^hrad + 1), which
    // overflows to NaN once e^hrad exceeds f64 range (hrad a few hundred
    // is already enough). Rewritten using e^-hrad, which underflows
    // harmlessly to 0 instead, giving the correct limit of 1.
    let got = eval("710 0 hpolar");
    let x: f64 = got[0].parse().unwrap();
    let y: f64 = got[1].parse().unwrap();
    assert!(
        x.is_finite() && y.is_finite(),
        "got ({x}, {y}), expected finite"
    );
    assert!(
        (x - 1.0).abs() < 1e-9,
        "expected x -> 1 at this radius, got {x}"
    );
    assert!(y.abs() < 1e-9, "expected y -> 0 at angle 0, got {y}");
}

#[test]
fn edgefractal_presets_close_exactly_confirming_the_scale_divisor() {
    // Regression test for the actual bug this feature caught during
    // development: a first draft divided segment length by the turn
    // array's own length (4 for koch, 8 for quadkoch) instead of by a
    // separately-tracked "scale" -- the number of base-edge-lengths the
    // generator spans end to end, which for koch is 3 (not 4: the
    // bump's two slanted sides fold back across each other) and for
    // quadkoch is 4 (not 8). Get the divisor wrong and edgefractal
    // silently draws a curve at the wrong total length instead of
    // erroring. Checked at depth 1 (the divisor's direct effect) and
    // depth 3 (confirming the scale holds recursively, not just once).
    for (name, len, depth) in [
        ("koch", 90.0, 1),
        ("koch", 90.0, 3),
        ("quadkoch", 80.0, 1),
        ("quadkoch", 80.0, 3),
    ] {
        let got = eval(&format!(
            "newpath 0 0 moveto {len} 0 /{name} fgen {depth} edgefractal currentpoint"
        ));
        let x: f64 = got[0].parse().unwrap();
        let y: f64 = got[1].parse().unwrap();
        assert!(
            (x - len).abs() < 1e-6 && y.abs() < 1e-6,
            "{name} depth {depth}: expected ({len}, 0), got ({x}, {y})"
        );
    }
}

#[test]
fn fractal_gens_presets_have_zero_net_turn_and_the_documented_scale() {
    // Independent of edgefractal itself: walks each preset's turn array
    // as plain unit-length turtle steps (the exact by-hand check that
    // caught the scale-divisor bug above) and confirms it closes -- net
    // turn a multiple of 360, net displacement (scale, 0) -- the
    // defining property of a valid Koch-family generator. A caller
    // extending FractalGens with a new preset that doesn't satisfy this
    // draws a curve that silently drifts off axis or the wrong length;
    // this pins the two shipped presets so a future edit can't quietly
    // break the property this feature depends on.
    for (name, expected_scale) in [("koch", 3.0), ("quadkoch", 4.0)] {
        let got = eval(&format!(
            "/turns /{name} fgen pop def
             /h 0 def /x 0 def /y 0 def
             turns {{
                 h add /h exch def
                 x h cos add /x exch def
                 y h sin add /y exch def
             }} forall
             h 360 mod x y"
        ));
        let h_mod: f64 = got[0].parse().unwrap();
        let x: f64 = got[1].parse().unwrap();
        let y: f64 = got[2].parse().unwrap();
        assert!(
            h_mod.abs() < 1e-9 || (h_mod.abs() - 360.0).abs() < 1e-9,
            "{name}: net turn {h_mod} is not a multiple of 360"
        );
        assert!(
            (x - expected_scale).abs() < 1e-9,
            "{name}: net x displacement {x}, expected scale {expected_scale}"
        );
        assert!(y.abs() < 1e-9, "{name}: net y displacement {y}, expected 0");
    }
}

#[test]
fn edgepoly_builds_a_closed_snowflake_that_bulges_outward_with_depth() {
    // Winding matters for edgepoly (documented in its header): clockwise
    // vertices put the koch/quadkoch bumps (whose first nonzero turn is
    // positive, i.e. a left turn) on the outside. Confirmed by
    // rendering both orderings during development -- counterclockwise
    // folds the bumps inward instead. This pins the outward case two
    // ways: ink strictly increases with depth (each generation adds
    // area, the classic snowflake growth) and the path actually closes
    // (currentpoint lands back on the first vertex).
    let verts = "[0 0 75 129.9038 150 0]"; // clockwise equilateral triangle
    let base_ink = ink_of("newpath 0 0 moveto 75 129.9038 lineto 150 0 lineto closepath fill");
    let mut prev_ink = base_ink;
    for depth in 1..=3 {
        let ink = ink_of(&format!("newpath {verts} /koch fgen {depth} edgepoly fill"));
        assert!(
            ink > prev_ink,
            "depth {depth} ink ({ink}) did not grow past the previous depth's ({prev_ink}) -- \
             bumps should be adding outward area, not folding inward"
        );
        prev_ink = ink;
    }

    let got = eval(&format!(
        "newpath {verts} /koch fgen 3 edgepoly currentpoint"
    ));
    let x: f64 = got[0].parse().unwrap();
    let y: f64 = got[1].parse().unwrap();
    assert!(
        x.abs() < 1e-3 && y.abs() < 1e-3,
        "edgepoly should close back at the first vertex (0,0), got ({x}, {y})"
    );
}

#[test]
fn gasket_visits_exactly_3_to_the_depth_leaves() {
    // Regression test for the real bug this feature caught during
    // development: a first draft implemented gasket as a recursive
    // PostScript proc wrapping every level in its own `dict begin/end`
    // (mirroring koch/edgefractal). But unlike those, gasket also
    // invokes a *caller-supplied* proc at the leaves -- and since a
    // recursive call happens before its own `end`, every ancestor
    // level's dict is still open at that moment, so a stamp as simple
    // as `{ /n n 1 add def }` (the exact idiom
    // truchet_calls_proc_once_per_cell_with_the_cell_size below already
    // relies on for a non-recursive driver) silently rebound `/n` into
    // an ancestor's throwaway frame instead of the caller's own dict --
    // confirmed empirically (the counter always read back 0). Rewritten
    // to drive the walk with an explicit stack array (same reason
    // httile below doesn't recurse either), so gkproc always runs with
    // no gasket-owned frame open, exactly like every other driver in
    // this file. This test is the regression: a bare `def` counter,
    // unlike the array-based counters other tests in this file
    // sometimes need for a *different* reason (composition, not this
    // one).
    for depth in 0..=4 {
        let got = eval(&format!(
            "/n 0 def \
             0 0 100 0 0 100 {depth} {{ pop pop pop pop pop pop /n n 1 add def }} gasket n"
        ));
        let expected = 3i64.pow(depth);
        assert_eq!(
            got.last().unwrap(),
            &expected.to_string(),
            "depth {depth}: expected {expected} leaves"
        );
    }
}

#[test]
fn gasket_depth1_leaves_match_the_expected_midpoint_split() {
    // Pins the exact three sub-triangles for a simple right triangle
    // (0,0)-(100,0)-(0,100), verified by hand against the documented
    // midpoint split (mx12,my12)=(50,0), (mx23,my23)=(50,50),
    // (mx31,my31)=(0,50) -- and the actual visiting order, C then B
    // then A (the reverse of the push order), since the walk is an
    // explicit LIFO stack, not a queue.
    // The leaf closure wraps in its own dict (to name x1..y3 for
    // readability), so the running index can't be a plain `/hi hi 6 add
    // def` -- that `def` would land in the closure's own wrapper dict
    // and be discarded at `end`, same failure mode as the scoping bug
    // gasket itself just got fixed for. Same fix the tiling section's
    // own tests already use: a 1-element array mutated with `put`.
    let got = eval(
        "/hits 18 array def /idxbox 1 array def idxbox 0 0 put \
         0 0 100 0 0 100 1 { \
             6 dict begin \
                 /gy3 exch def /gx3 exch def /gy2 exch def /gx2 exch def \
                 /gy1 exch def /gx1 exch def \
                 /hi idxbox 0 get def \
                 hits hi gx1 put hits hi 1 add gy1 put \
                 hits hi 2 add gx2 put hits hi 3 add gy2 put \
                 hits hi 4 add gx3 put hits hi 5 add gy3 put \
                 idxbox 0 hi 6 add put \
             end \
         } gasket hits aload pop",
    );
    let nums: Vec<f64> = got.iter().map(|s| s.parse().unwrap()).collect();
    let expected = [
        // C: (mx31,my31)-(mx23,my23)-(x3,y3)
        (0.0, 50.0, 50.0, 50.0, 0.0, 100.0),
        // B: (mx12,my12)-(x2,y2)-(mx23,my23)
        (50.0, 0.0, 100.0, 0.0, 50.0, 50.0),
        // A: (x1,y1)-(mx12,my12)-(mx31,my31)
        (0.0, 0.0, 50.0, 0.0, 0.0, 50.0),
    ];
    for (k, exp) in expected.iter().enumerate() {
        let base = k * 6;
        let got6 = (
            nums[base],
            nums[base + 1],
            nums[base + 2],
            nums[base + 3],
            nums[base + 4],
            nums[base + 5],
        );
        assert_eq!(got6, *exp, "leaf {k} mismatch");
    }
}

#[test]
fn carpet_visits_exactly_8_to_the_depth_leaves() {
    // Companion to gasket's leaf-count regression test above -- same
    // bug class, same fix (an explicit stack array instead of
    // recursion), pinned here with carpet's own 8-ary branching.
    for depth in 0..=3 {
        let got = eval(&format!(
            "/m 0 def \
             0 0 90 90 {depth} {{ pop pop pop pop /m m 1 add def }} carpet m"
        ));
        let expected = 8i64.pow(depth);
        assert_eq!(
            got.last().unwrap(),
            &expected.to_string(),
            "depth {depth}: expected {expected} leaves"
        );
    }
}

#[test]
fn carpet_depth1_leaves_are_the_eight_outer_cells_with_the_center_missing() {
    // Pins the exact 8 surviving cells of a 90x90 box split into a 3x3
    // grid of 30x30 cells, and separately confirms the center cell
    // (30,30) -- the one Sierpinski's carpet always removes -- is not
    // among them. Visiting order is the reverse of the nested-loop push
    // order (LIFO stack, same reason as gasket's own test above).
    // Same array-based-counter fix as gasket's own leaf-coordinate test
    // above, same reason (the closure's own dict wrap would swallow a
    // plain `/hi hi 4 add def`).
    let got = eval(
        "/hits 32 array def /idxbox 1 array def idxbox 0 0 put \
         0 0 90 90 1 { \
             4 dict begin \
                 /ch exch def /cw exch def /cy exch def /cx exch def \
                 /hi idxbox 0 get def \
                 hits hi cx put hits hi 1 add cy put \
                 hits hi 2 add cw put hits hi 3 add ch put \
                 idxbox 0 hi 4 add put \
             end \
         } carpet hits aload pop",
    );
    let nums: Vec<f64> = got.iter().map(|s| s.parse().unwrap()).collect();
    let expected_origins = [
        (60.0, 60.0),
        (60.0, 30.0),
        (60.0, 0.0),
        (30.0, 60.0),
        (30.0, 0.0),
        (0.0, 60.0),
        (0.0, 30.0),
        (0.0, 0.0),
    ];
    for (k, (ex, ey)) in expected_origins.iter().enumerate() {
        let base = k * 4;
        assert_eq!(nums[base], *ex, "leaf {k} cx");
        assert_eq!(nums[base + 1], *ey, "leaf {k} cy");
        assert_eq!(nums[base + 2], 30.0, "leaf {k} cw");
        assert_eq!(nums[base + 3], 30.0, "leaf {k} ch");
    }
    assert!(
        !expected_origins.contains(&(30.0, 30.0)),
        "sanity: the center cell must not be one of the 8 surviving cells"
    );
}

#[test]
fn gasket_and_carpet_paint_via_the_callers_proc_at_depth_zero() {
    // Smoke test that the basic fill path works end to end: at depth 0
    // each driver should call its proc exactly once, on the whole
    // shape, and a caller building+filling that shape should see ink
    // roughly matching a plain fill of the same region.
    let gasket_ink = ink_of(
        "newpath 20 20 moveto 380 20 lineto 200 340 lineto closepath \
         20 20 380 20 200 340 0 { \
             6 dict begin \
                 /gy3 exch def /gx3 exch def /gy2 exch def /gx2 exch def \
                 /gy1 exch def /gx1 exch def \
                 newpath gx1 gy1 moveto gx2 gy2 lineto gx3 gy3 lineto closepath fill \
             end \
         } gasket",
    );
    assert!(
        gasket_ink > 10000,
        "expected a filled triangle's ink, got {gasket_ink}"
    );

    let carpet_ink = ink_of("20 20 360 360 0 { newpath rectfill } carpet");
    assert!(
        carpet_ink > 100000,
        "expected a filled box's ink, got {carpet_ink}"
    );
}

#[test]
fn gasket_nested_in_its_own_leaf_needs_the_inner_call_wrapped_in_a_dict() {
    // Regression test for a real bug caught by cross-model (Codex)
    // review: gasket/carpet's own traversal state (gksp/gkstack/gkproc)
    // is a set of plain globals, not dict-scoped -- fine for the
    // ancestor-frame problem their own header explains, but it means a
    // leaf proc that calls `gasket` again itself (nested fractals, a
    // gasket of gaskets) clobbers the *outer* traversal's state the
    // moment the inner call starts, since both bind the same names.
    // Same shape, same fix, as the tiling section's tg-/tk- gotcha:
    // wrap just the inner call in its own dict so its `def`s shadow
    // there instead of landing on the outer's own bindings. This pins
    // both halves -- the unwrapped case genuinely breaks (visits only
    // 1 of the outer's 3 leaves, not 3) and the documented fix restores
    // it (all 3, with the inner traversal's own count read via an
    // array, since a plain `def` counter would itself be swallowed by
    // that same wrapper -- the tiling section's *other* gotcha).
    let got = eval(
        "/outerN 0 def \
         0 0 100 0 0 100 1 { \
             pop pop pop pop pop pop \
             /outerN outerN 1 add def \
             outerN 1 eq { \
                 0 0 10 0 0 10 1 { pop pop pop pop pop pop } gasket \
             } if \
         } gasket outerN",
    );
    assert_eq!(
        got.last().unwrap(),
        "1",
        "expected the unwrapped nested call to break the outer traversal (visits only 1 leaf)"
    );

    let got = eval(
        "/outerN 0 def /innerbox 1 array def innerbox 0 0 put \
         0 0 100 0 0 100 1 { \
             pop pop pop pop pop pop \
             /outerN outerN 1 add def \
             outerN 1 eq { \
                 2 dict begin \
                     0 0 10 0 0 10 1 { \
                         pop pop pop pop pop pop \
                         innerbox 0 innerbox 0 get 1 add put \
                     } gasket \
                 end \
             } if \
         } gasket outerN innerbox 0 get",
    );
    assert_eq!(
        got[0], "3",
        "wrapped: outer traversal should visit all 3 leaves"
    );
    assert_eq!(
        got[1], "3",
        "wrapped: inner traversal should also visit all 3 leaves"
    );
}

#[test]
fn carpet_nested_in_its_own_leaf_needs_the_inner_call_wrapped_in_a_dict() {
    // Companion to gasket's own version of this regression -- same bug
    // class (cpsp/cpstack/cpproc are the equivalent plain globals),
    // same fix, pinned here with carpet's 8-ary branching (depth 1 = 8
    // leaves instead of gasket's 3).
    let got = eval(
        "/outerN 0 def \
         0 0 90 90 1 { pop pop pop pop /outerN outerN 1 add def } carpet outerN",
    );
    assert_eq!(got[0], "8", "sanity: unnested carpet visits all 8 leaves");

    let got = eval(
        "/outerN 0 def \
         0 0 90 90 1 { \
             pop pop pop pop \
             /outerN outerN 1 add def \
             outerN 1 eq { \
                 0 0 9 9 1 { pop pop pop pop } carpet \
             } if \
         } carpet outerN",
    );
    assert!(
        got[0].parse::<i64>().unwrap() < 8,
        "expected the unwrapped nested call to break the outer traversal (fewer than 8 leaves), got {}",
        got[0]
    );

    let got = eval(
        "/outerN 0 def /innerbox 1 array def innerbox 0 0 put \
         0 0 90 90 1 { \
             pop pop pop pop \
             /outerN outerN 1 add def \
             outerN 1 eq { \
                 2 dict begin \
                     0 0 9 9 1 { \
                         pop pop pop pop \
                         innerbox 0 innerbox 0 get 1 add put \
                     } carpet \
                 end \
             } if \
         } carpet outerN innerbox 0 get",
    );
    assert_eq!(
        got[0], "8",
        "wrapped: outer traversal should visit all 8 leaves"
    );
    assert_eq!(
        got[1], "8",
        "wrapped: inner traversal should also visit all 8 leaves"
    );
}

// --- paragraph / flowing text -----------------------------------------

fn with_helvetica(size: f64, body: &str) -> String {
    format!("/Helvetica findfont {size} scalefont setfont {body}")
}

#[test]
fn tfwrap_breaks_on_words_that_fit() {
    // A width wide enough for two words but not three should wrap
    // "aa bb cc" into ["aa bb", "cc"].
    let got = eval(&with_helvetica(14.0, "(aa bb cc) 45 tfwrap aload pop"));
    assert_eq!(got.len(), 2, "expected two lines, got {got:?}");
    assert_eq!(got[0], "(aa bb)");
    assert_eq!(got[1], "(cc)");
}

#[test]
fn tfwrap_does_not_drop_the_final_word_of_the_last_line() {
    // Regression test for the exact bug the advisor caught in review:
    // tflinebreak's end-of-string branch used to unconditionally take
    // the whole remainder as one line without checking whether it
    // actually fit, so the last word of every non-terminal paragraph
    // silently overflowed the wrap width instead of moving to its own
    // line. "aa bb cc" at a width sized for two words must still come
    // back as two lines, not one overflowing line.
    let got = eval(&with_helvetica(14.0, "(aa bb cc) 45 tfwrap length"));
    assert_eq!(
        got,
        ["2"],
        "expected 2 wrapped lines, not 1 overflowing line"
    );

    // And confirm neither line individually overflows the width.
    let got = eval(&with_helvetica(
        14.0,
        "(aa bb cc) 45 tfwrap { stringwidth pop } forall",
    ));
    for (i, w) in got.iter().enumerate() {
        let w: f64 = w.parse().unwrap();
        assert!(w <= 45.0, "line {i} width {w} exceeds wrap width 45");
    }
}

#[test]
fn tfwrap_forces_a_break_on_embedded_newline() {
    // An embedded newline always breaks the line, even with plenty of
    // width left -- that's what lets a caller pass multiple paragraphs
    // as one string.
    let got = eval(&with_helvetica(14.0, "(hi\\nthere) 400 tfwrap aload pop"));
    assert_eq!(got, ["(hi)", "(there)"]);
}

#[test]
fn tfwrap_places_an_oversized_single_word_alone_rather_than_splitting_it() {
    // No hyphenation: a word wider than the wrap width still gets its
    // own line instead of erroring or being silently dropped.
    let got = eval(&with_helvetica(
        14.0,
        "(ab pneumonoultramicroscopicsilicovolcanoconiosis cd) 40 tfwrap aload pop",
    ));
    assert_eq!(got.len(), 3, "expected 3 lines, got {got:?}");
    assert_eq!(got[1], "(pneumonoultramicroscopicsilicovolcanoconiosis)");
}

#[test]
fn tfwrap_of_empty_string_is_an_empty_array_not_one_blank_line() {
    // Contract check flagged in review: tfwrap on "" should agree with
    // tfflow's own reading of "nothing to do" (tfflow on an empty
    // string draws zero lines and returns immediately), not silently
    // report one line. A caller counting lines via `tfwrap length`
    // should get 0 for empty input, not 1.
    let got = eval(&with_helvetica(14.0, "() 100 tfwrap length"));
    assert_eq!(got, ["0"]);
}

#[test]
fn tfwrap_scratch_array_bound_holds_on_a_run_of_unfittable_separators() {
    // Regression/invariant check for the array-sizing argument in
    // tfwrap's header comment: three spaces at a width narrower than a
    // single space forces the worst case for tflinebreak's "at least
    // one char consumed per call" guarantee -- each call peels off
    // exactly one blank line and one separator, so this exercises
    // exactly `length` calls against a `length + 1`-slot array. This
    // must neither error (rangecheck on an out-of-bounds `put`) nor
    // silently truncate.
    let got = eval(&with_helvetica(14.0, "(   ) 1 tfwrap length"));
    assert_eq!(got, ["3"], "expected one blank line per space, got {got:?}");
}

#[test]
fn tfblock_left_aligns_flush_to_the_box_edge() {
    let mut it = Interp::with_page(300, 100).expect("page");
    load(&mut it);
    it.run_str(&with_helvetica(
        16.0,
        "10 10 280 80 18 /left (hi) tfblock pop",
    ))
    .unwrap_or_else(|e| panic!("tfblock failed: {}", it.error_report(&e)));
    let (lx, _ly, _ux, _uy) = it.gfx().path_bbox().unwrap_or((0.0, 0.0, 0.0, 0.0));
    // path_bbox tracks the current path, not painted glyph ink, so
    // measure ink position directly instead: leftmost dark pixel
    // should sit close to x=10, not drifted toward the box's center
    // or right edge.
    let pixmap = &it.gfx().pixmap;
    let mut min_x = None;
    for y in 0..pixmap.height() {
        for x in 0..pixmap.width() {
            if pixmap.pixel(x, y).expect("in bounds").red() < 128 {
                min_x = Some(min_x.map_or(x, |m: u32| m.min(x)));
            }
        }
    }
    let min_x = min_x.expect("expected some ink") as f64;
    assert!(
        min_x < 30.0,
        "left-aligned text's leftmost ink at x={min_x} is not close to the box edge (10); lx hint={lx}"
    );
}

#[test]
fn tfdrawline_right_and_center_shift_ink_relative_to_left() {
    // Compare the rightmost ink pixel across left/right/center for the
    // same short string in the same box -- right should push ink
    // furthest right, center in between, left least.
    fn rightmost_ink(just: &str) -> u32 {
        let mut it = Interp::with_page(300, 100).expect("page");
        load(&mut it);
        it.run_str(&with_helvetica(
            16.0,
            &format!("10 10 280 80 18 /{just} (hi) tfblock pop"),
        ))
        .unwrap_or_else(|e| panic!("tfblock failed: {}", it.error_report(&e)));
        let pixmap = &it.gfx().pixmap;
        let mut max_x = 0u32;
        for y in 0..pixmap.height() {
            for x in 0..pixmap.width() {
                if pixmap.pixel(x, y).expect("in bounds").red() < 128 {
                    max_x = max_x.max(x);
                }
            }
        }
        max_x
    }

    let left = rightmost_ink("left");
    let center = rightmost_ink("center");
    let right = rightmost_ink("right");
    assert!(
        left < center && center < right,
        "expected left < center < right, got left={left} center={center} right={right}"
    );
}

#[test]
fn tfdrawline_justify_stretches_gaps_to_fill_the_line_but_not_the_last_line() {
    // A justified non-last line's words should span from x0 all the
    // way to x1 (its rightmost ink close to the box's right edge);
    // the paragraph's actual last line should NOT be stretched (falls
    // back to /left, so its rightmost ink stays well short of x1).
    fn ink_bbox_x(body: &str) -> (u32, u32) {
        let mut it = Interp::with_page(300, 200).expect("page");
        load(&mut it);
        it.run_str(body)
            .unwrap_or_else(|e| panic!("{body} failed: {}", it.error_report(&e)));
        let pixmap = &it.gfx().pixmap;
        let (mut min_x, mut max_x) = (u32::MAX, 0u32);
        for y in 0..pixmap.height() {
            for x in 0..pixmap.width() {
                if pixmap.pixel(x, y).expect("in bounds").red() < 128 {
                    min_x = min_x.min(x);
                    max_x = max_x.max(x);
                }
            }
        }
        (min_x, max_x)
    }

    // Box narrow enough that "one two three four five six" wraps to
    // two lines ("one two three" / "four five six", confirmed against
    // tfwrap directly), so the first line is genuinely non-last.
    let (_min, max) = ink_bbox_x(&with_helvetica(
        14.0,
        "10 10 100 150 18 /justify (one two three four five six) tfblock pop",
    ));
    assert!(
        max > 100,
        "justified non-last line should stretch close to the box's right edge (10+100=110), got rightmost ink at {max}"
    );
}

#[test]
fn tfdrawline_justify_does_not_leak_the_operand_stack() {
    // Regression test for a real bug found by issue #17's lint mode
    // against a real example file (examples/paragraph_layout.ps):
    // `search`'s "not found" return is `string false` -- the searched
    // string comes back unchanged, still under the bool `ifelse` just
    // consumed -- and the justify loop's last-word branch fell through
    // without popping it, leaking one stray string per justified line.
    let mut it = Interp::with_page(300, 200).expect("page");
    load(&mut it);
    it.run_str(&with_helvetica(
        14.0,
        "10 10 100 150 18 /justify (one two three four five six) tfblock pop",
    ))
    .unwrap_or_else(|e| panic!("tfblock failed: {}", it.error_report(&e)));
    assert!(
        it.operand_stack().is_empty(),
        "leftover on operand stack: {:?}",
        it.operand_stack()
            .iter()
            .map(|o| o.repr())
            .collect::<Vec<_>>()
    );
}

#[test]
fn tfdrawline_justify_keeps_the_natural_space_width_not_just_the_stretch() {
    // Regression test for a real bug caught by rendering the example
    // specimen sheet: the justify branch advanced each word by
    // stringwidth(word) + tdextra alone, dropping the line's own
    // natural inter-word space entirely -- every word landed roughly
    // one space-width short of where it belonged, and with several
    // gaps in a row the shortfall compounded into words visibly
    // overlapping ("flowsabrush:wordbyword..." instead of "flows a
    // brush: word by word..."). Calls tfdrawline directly (lastline
    // forced false) so this pins the arithmetic in isolation, without
    // depending on tfwrap's own line-break choices. "aa bb cc dd ee
    // ff" at 20pt Helvetica is 147.88pt natural width with 5.556pt
    // spaces (5 gaps); stretched to fill a 160pt span, the dropped-
    // space bug falls 5*5.556=27.78pt short of the right edge (lands
    // near x=142), while the fix reaches it (near x=170).
    let mut it = Interp::with_page(200, 100).expect("page");
    load(&mut it);
    it.run_str(&with_helvetica(
        20.0,
        "10 170 50 /justify false (aa bb cc dd ee ff) tfdrawline",
    ))
    .unwrap_or_else(|e| panic!("tfdrawline failed: {}", it.error_report(&e)));
    let pixmap = &it.gfx().pixmap;
    let mut max_x = 0u32;
    for y in 0..pixmap.height() {
        for x in 0..pixmap.width() {
            if pixmap.pixel(x, y).expect("in bounds").red() < 128 {
                max_x = max_x.max(x);
            }
        }
    }
    assert!(
        max_x > 160,
        "justified line should stretch its rightmost ink close to x1=170; \
         got {max_x}, which matches the dropped-natural-space bug's shortfall (expected ~142)"
    );
}

#[test]
fn tfwrap_can_leave_a_trailing_space_on_a_wrapped_line() {
    // Sets up the scenario tfdrawline_justify_ignores_a_trailing_space
    // below depends on: a run of two source spaces at a wrap point
    // leaves one attached to the end of the emitted line rather than
    // being collapsed -- confirmed directly against tfwrap so that
    // test's premise isn't just assumed.
    let got = eval(&with_helvetica(14.0, "(aa bb  cc dd) 45 tfwrap aload pop"));
    assert_eq!(got, ["(aa bb )", "(cc dd)"]);
}

#[test]
fn tfdrawline_justify_ignores_a_trailing_space_left_by_the_wrap() {
    // Regression test for a real bug caught by cross-model (Codex)
    // review: tfwordgaps/the justify loop counted every literal space
    // in the line, including a trailing one left by tflinebreak at a
    // double-space wrap point (see the test above) -- so /justify
    // treated it as a real gap with an invisible "word" after it,
    // spending stretch on nothing and leaving the actual last word
    // short of x1. "aa bb" (trimmed) is 50.05pt at 20pt Helvetica,
    // "aa bb " (untrimmed) is 55.61pt; stretched into a 90pt span, the
    // untrimmed bug spends half its stretch on the phantom trailing
    // gap and reaches only ~77 (10 + natural width + one gap's worth
    // of stretch), while the fix (trim first, one real gap) reaches
    // the full span, close to x1=100.
    let mut it = Interp::with_page(150, 100).expect("page");
    load(&mut it);
    it.run_str(&with_helvetica(
        20.0,
        "10 100 50 /justify false (aa bb ) tfdrawline",
    ))
    .unwrap_or_else(|e| panic!("tfdrawline failed: {}", it.error_report(&e)));
    let pixmap = &it.gfx().pixmap;
    let mut max_x = 0u32;
    for y in 0..pixmap.height() {
        for x in 0..pixmap.width() {
            if pixmap.pixel(x, y).expect("in bounds").red() < 128 {
                max_x = max_x.max(x);
            }
        }
    }
    assert!(
        max_x > 90,
        "justified line should stretch its rightmost ink close to x1=100 \
         once the trailing space is excluded from gap-counting; \
         got {max_x}, which matches the phantom-trailing-gap bug's shortfall (expected ~77)"
    );
}

#[test]
fn tfflow_returns_leftover_text_that_did_not_fit_vertically() {
    // A box too short to hold every line should return the unflowed
    // remainder rather than silently dropping or erroring on it.
    let got = eval(&with_helvetica(
        14.0,
        "10 10 100 20 16 /left (one two three four five six seven eight nine) tfblock",
    ));
    assert_eq!(got.len(), 1, "expected one leftover string on the stack");
    assert!(
        !got[0].trim_matches(|c| c == '(' || c == ')').is_empty(),
        "expected non-empty leftover from a too-short box, got {got:?}"
    );
}

#[test]
fn tfflow_returns_empty_leftover_when_everything_fits() {
    let got = eval(&with_helvetica(
        14.0,
        "10 10 280 200 16 /left (short text) tfblock",
    ));
    assert_eq!(got, ["()"], "expected empty leftover when text fully fits");
}

#[test]
fn tfflow_honors_a_custom_boundsproc_for_a_non_rectangular_region() {
    // The whole point of tfflow taking a boundsproc instead of a fixed
    // width: a region whose available width varies by line (not just
    // a plain rectangle) should actually get different per-line
    // widths. Use a boundsproc that halves the available width for
    // any line below y=100, and confirm a wrapped line placed below
    // that threshold is measurably narrower (as ink) than one above
    // it, for the same font and text.
    let mut it = Interp::with_page(300, 200).expect("page");
    load(&mut it);
    it.run_str(&with_helvetica(
        14.0,
        "{ /y exch def y 100 gt { 20 220 } { 20 100 } ifelse } \
         116 20 20 /left \
         (aaaa bbbb cccc dddd eeee ffff gggg hhhh iiii jjjj) tfflow pop",
    ))
    .unwrap_or_else(|e| panic!("tfflow failed: {}", it.error_report(&e)));
    // Sanity: at least some ink was placed both above and below y=100.
    let pixmap = &it.gfx().pixmap;
    let mut ink_above = 0usize;
    let mut ink_below = 0usize;
    for y in 0..pixmap.height() {
        for x in 0..pixmap.width() {
            if pixmap.pixel(x, y).expect("in bounds").red() < 128 {
                // pixmap y=0 is the top of the canvas; device y = height-1-y in PS coords.
                let ps_y = pixmap.height() - 1 - y;
                if ps_y > 100 {
                    ink_above += 1;
                } else {
                    ink_below += 1;
                }
            }
        }
    }
    assert!(ink_above > 0, "expected ink above y=100");
    assert!(ink_below > 0, "expected ink below y=100");
}

#[test]
fn tfflow_nested_in_its_own_boundsproc_needs_the_inner_call_wrapped_in_a_dict() {
    // Regression test for a real bug caught by cross-model (Codex)
    // review: tfflow's own traversal state (tfstr/tfy/tflast/tfrest/...)
    // is a set of plain globals, not dict-scoped -- same failure family
    // as the tiling section's tg-/tk- gotcha and gasket/carpet's own
    // nesting bug, but for tfflow itself, not just tfblock/tfcols. A
    // boundsproc that calls tfflow again, unwrapped, clobbers the
    // outer call's state the instant the inner one runs -- confirmed
    // to actually discard real text, not just draw the wrong thing:
    // the outer call's own leftover comes back empty even though a box
    // too short to hold it all should leave a real remainder.
    //
    // Box: y0=100, ybot=90, leading=16 -- capacity for exactly one
    // line, so the long outer sentence below must leave substantial
    // leftover text under correct behavior.
    let unwrapped = eval(&with_helvetica(
        12.0,
        "{ /y exch def 10 190 \
          { pop 10 190 } 300 10 14 /left (inner words fill this space nicely) tfflow pop } \
         100 90 16 /left \
         (outer words must remain visible after the inner call runs and this text is long) \
         tfflow",
    ));
    assert_eq!(
        unwrapped,
        ["()"],
        "expected the unwrapped nested call to lose the outer leftover (comes back empty)"
    );

    let wrapped = eval(&with_helvetica(
        12.0,
        "{ /y exch def 10 190 \
          8 dict begin \
              { pop 10 190 } 300 10 14 /left (inner words fill this space nicely) tfflow pop \
          end } \
         100 90 16 /left \
         (outer words must remain visible after the inner call runs and this text is long) \
         tfflow",
    ));
    assert_ne!(
        wrapped,
        ["()"],
        "wrapped: outer leftover should be the real unflowed remainder, not empty"
    );
    assert!(
        wrapped[0].contains("text is long"),
        "wrapped: expected the outer's own true leftover text, got {wrapped:?}"
    );
}

#[test]
fn tfcols_flows_across_columns_and_leaves_correct_leftover() {
    // Three narrow columns should each take a chunk of the text; with
    // enough columns/height to hold it all, the leftover should be
    // empty, and different columns should each end up with ink (i.e.
    // the whole run isn't dumped into the first column alone).
    let mut it = Interp::with_page(300, 200).expect("page");
    load(&mut it);
    // Column height 30 with 14pt leading holds exactly 2 lines per
    // column (first baseline at y+h-leading=26, second at 12, a third
    // would land at -2 < ybot=10) -- 6 line-slots across 3 columns for
    // the 5 lines "one two three four five six seven eight nine ten
    // eleven twelve" wraps to at width 80 (confirmed via tfwrap
    // directly), so it spills as far as the third column but not past
    // it.
    it.run_str(&with_helvetica(
        12.0,
        "10 10 80 10 30 3 14 /left \
         (one two three four five six seven eight nine ten eleven twelve) tfcols pop",
    ))
    .unwrap_or_else(|e| panic!("tfcols failed: {}", it.error_report(&e)));
    let pixmap = &it.gfx().pixmap;
    let mut ink_col1 = 0usize; // x in [10,90)
    let mut ink_col3 = 0usize; // x in [190,270)
    for y in 0..pixmap.height() {
        for x in 0..pixmap.width() {
            if pixmap.pixel(x, y).expect("in bounds").red() < 128 {
                if (10..90).contains(&x) {
                    ink_col1 += 1;
                } else if (190..270).contains(&x) {
                    ink_col3 += 1;
                }
            }
        }
    }
    assert!(ink_col1 > 0, "expected ink in the first column");
    assert!(
        ink_col3 > 0,
        "expected ink to have flowed as far as the third column"
    );
}

#[test]
fn tfcols_returns_leftover_when_text_exceeds_all_columns() {
    let got = eval(&with_helvetica(
        14.0,
        "10 10 40 5 20 2 16 /left \
         (one two three four five six seven eight nine ten eleven twelve) tfcols",
    ));
    assert_eq!(got.len(), 1);
    assert!(
        !got[0].trim_matches(|c| c == '(' || c == ')').is_empty(),
        "expected non-empty leftover once both narrow columns fill up, got {got:?}"
    );
}

// --- noise / flow fields (issue #19) -------------------------------

#[test]
fn noise2_is_deterministic_under_the_same_seed() {
    let got = eval("9 srand noiseinit 1.37 -4.21 noise2 9 srand noiseinit 1.37 -4.21 noise2");
    assert_eq!(got[0], got[1], "same seed, same noise2 sample");
}

#[test]
fn noise2_is_exactly_zero_at_every_integer_lattice_point() {
    // A real Perlin invariant, not a coincidence of the implementation:
    // fade(0)=0 on both axes collapses the double lerp to the corner's
    // own gradient dotted with the zero displacement vector, which is
    // 0 regardless of which gradient the hash picked. Checked at
    // positive, negative, and mixed-sign lattice points -- negative
    // coordinates are exactly where a `mod`-instead-of-`and` bug (see
    // the noiseinit header) would misindex Perm and this would stop
    // holding.
    for (x, y) in [(0, 0), (3, 4), (-3, -4), (-3, 4), (17, -255), (256, 256)] {
        let got = eval(&format!("5 srand noiseinit {x} {y} noise2"));
        let n: f64 = got[0].parse().unwrap();
        assert!(n.abs() < 1e-9, "noise2({x},{y}) = {n}, expected exactly 0");
    }
}

#[test]
fn noise2_is_continuous_across_lattice_boundaries_positive_and_negative() {
    // Straddle x=1 and x=-1 by 1e-4 on each side. A wrong `+1` wrap or
    // a `mod` sneaking in where `and 255` is needed shows up here as a
    // large jump right at the boundary, not as an out-of-range crash --
    // exactly the bug class direct-arithmetic-only testing would miss.
    for boundary in [1.0, -1.0] {
        let got = eval(&format!(
            "6 srand noiseinit {} 0.3 noise2 {} 0.3 noise2",
            boundary - 1e-4,
            boundary + 1e-4
        ));
        let a: f64 = got[0].parse().unwrap();
        let b: f64 = got[1].parse().unwrap();
        assert!(
            (a - b).abs() < 1e-3,
            "noise2 jumps from {a} to {b} across x={boundary}"
        );
    }
}

#[test]
fn noise2_is_coherent_and_stays_within_its_empirical_range() {
    // Coherence (the actual point of "coherent noise"): nearby samples
    // should differ much less than distant ones. A field that's just
    // flat, or just bounded random noise with no spatial correlation,
    // would pass a range-only check but fail this. Range bound is
    // empirical (~[-0.66, 0.66] measured over a 300x300 grid during
    // development, see NOTES.md) with headroom, not a derived textbook
    // constant -- this repo's convention for gradient-table-dependent
    // bounds.
    let got = eval(
        "8 srand noiseinit
         1.234 5.678 noise2
         1.254 5.678 noise2
         1.234 5.678 noise2
         4.234 8.678 noise2",
    );
    let a1: f64 = got[0].parse().unwrap();
    let a2: f64 = got[1].parse().unwrap();
    let b1: f64 = got[2].parse().unwrap();
    let b2: f64 = got[3].parse().unwrap();
    let near_delta = (a1 - a2).abs();
    let far_delta = (b1 - b2).abs();
    assert!(
        near_delta < far_delta,
        "adjacent samples (delta {near_delta}) should differ less than distant ones (delta {far_delta})"
    );

    let mut min = f64::MAX;
    let mut max = f64::MIN;
    let mut it = Interp::new();
    load(&mut it);
    it.run_str("3 srand noiseinit").unwrap();
    for ix in 0..60 {
        for iy in 0..60 {
            let x = ix as f64 * 0.37;
            let y = iy as f64 * 0.29;
            let got = it
                .run_str(&format!("{x} {y} noise2"))
                .map(|_| it.operand_stack()[0].repr())
                .unwrap();
            it.run_str("pop").unwrap();
            let n: f64 = got.parse().unwrap();
            min = min.min(n);
            max = max.max(n);
        }
    }
    assert!(
        (-0.8..=0.8).contains(&min) && (-0.8..=0.8).contains(&max),
        "noise2 range [{min}, {max}] outside the expected envelope"
    );
}

#[test]
fn noiseinit_produces_a_valid_permutation() {
    // Fisher-Yates over 0..255 must yield every value exactly once --
    // a shuffle bug (off-by-one range, or a swap that drops a value)
    // would silently bias which gradients get picked instead of
    // erroring.
    let got = eval(
        "9 srand noiseinit
         /seen 256 array def
         0 1 255 { seen exch 0 put } for
         0 1 255 {
             /pi exch def
             /pv Perm pi get def
             seen pv seen pv get 1 add put
         } for
         /allonce true def
         0 1 255 { seen exch get 1 ne { /allonce false def } if } for
         allonce",
    );
    assert_eq!(got[0], "true", "Perm is not a valid permutation of 0..255");
}

#[test]
fn curl2_is_unit_length_and_orthogonal_to_the_gradient_at_the_gallery_eps() {
    // eps=0.5 against a 0.02 frequency (the exact combo the gallery
    // piece uses) is the realistic cancellation-risk case documented
    // in curl2's header -- same shape as
    // httile_survives_catastrophic_cancellation_at_high_p_q. Dotting
    // the result against the field's own central-difference gradient
    // at the same point/eps catches a swapped-component bug
    // ((dx,-dy) instead of (dy,-dx)); a whole-vector sign flip would
    // still pass this (it stays orthogonal, and only reverses flow
    // direction, which is aesthetically irrelevant) so this is the
    // right and sufficient check, not "unit length" alone.
    let got = eval(
        "9 srand noiseinit
         /flow { 0.02 mul exch 0.02 mul exch noise2 } def
         /cx 137.0 def /cy 219.0 def /ceps 0.5 def
         cx cy ceps /flow load curl2
         /cdy exch def /cdx exch def
         cx cy ceps add flow
         cx cy ceps sub flow
         sub /gdy exch def
         cx ceps add cy flow
         cx ceps sub cy flow
         sub /gdx exch def
         cdx gdx mul cdy gdy mul add
         cdx dup mul cdy dup mul add sqrt",
    );
    let dot: f64 = got[0].parse().unwrap();
    let len: f64 = got[1].parse().unwrap();
    assert!(
        dot.abs() < 1e-6,
        "curl2 not orthogonal to gradient: dot={dot}"
    );
    assert!((len - 1.0).abs() < 1e-6, "curl2 not unit length: len={len}");
}

#[test]
fn curl2_output_is_not_exactly_divergence_free_after_normalization() {
    // A cross-model (Codex) review found the original docstring
    // overclaimed "divergence-free by construction, so particles
    // neither pool nor source": that's true of the *unnormalized*
    // perpendicular gradient (an exact vector-calculus identity), but
    // curl2 returns a *unit* vector, and normalizing by a position-
    // dependent magnitude does not preserve the identity in general.
    // Pinned here for `psi(x,y) = x*y` (a field whose gradient
    // magnitude genuinely varies) at (2,1): estimate div(curl2's
    // output) by central-differencing curl2's own dx and dy components
    // directly (not the underlying math) -- matches the review's
    // measurement of about -0.27, confirming this is real, measured
    // behavior of the shipped code, not a theoretical footnote. See
    // the section header for why curl2 normalizes anyway (advect needs
    // a uniform step size) and why this is an accepted, standard
    // curl-noise tradeoff rather than a bug to fix.
    let got = eval(
        "/psixy { /py exch def /px exch def px py mul } def
         /h 0.01 def
         2 h add 1 0.3 /psixy load curl2 pop
         2 h sub 1 0.3 /psixy load curl2 pop
         sub h 2 mul div
         2 1 h add 0.3 /psixy load curl2 exch pop
         2 1 h sub 0.3 /psixy load curl2 exch pop
         sub h 2 mul div",
    );
    let dvx_dx: f64 = got[0].parse().unwrap();
    let dvy_dy: f64 = got[1].parse().unwrap();
    let divergence = dvx_dx + dvy_dy;
    assert!(
        (divergence - (-0.268)).abs() < 0.01,
        "expected divergence around -0.268 at (2,1) for psi=x*y (matching the review's \
         measurement), got {divergence} -- if this is now ~0, curl2's normalization or \
         the field it's tested against changed and the docstring needs re-checking"
    );
}

#[test]
fn curl2_returns_zero_vector_for_a_perfectly_flat_field() {
    let got = eval("100 100 0.5 { pop pop 42 } curl2");
    let dx: f64 = got[0].parse().unwrap();
    let dy: f64 = got[1].parse().unwrap();
    assert_eq!((dx, dy), (0.0, 0.0), "flat field should curl to 0 0");
}

#[test]
fn curl2_normalizes_a_low_amplitude_but_genuinely_nonzero_gradient() {
    // Regression for a real bug a cross-model (Codex) review found: the
    // flatness guard originally used an arbitrary absolute threshold
    // (`c2len 1e-9 gt`), which doesn't match what curl2's own docstring
    // already promised ("0 0 if the gradient is *exactly* flat") --
    // `(x+y)*1e-10` has a real, uniform, nonzero gradient everywhere
    // (dPsi/dx = dPsi/dy = 1e-10), but its finite-differenced magnitude
    // at eps=1 falls under 1e-9, so the old threshold misclassified it
    // as flat and `advect` would stop immediately on a field that
    // never actually goes flat. Fixed to `c2len 0 gt`, matching a
    // genuinely flat field's finite difference (identical repeated
    // evaluations subtract to exactly 0.0, not just something small --
    // see the test above). The exact curl of `x+y` is analytically
    // (1/sqrt2, -1/sqrt2) regardless of the field's amplitude, since
    // normalizing removes any constant scale factor.
    let got = eval("2 3 1 { add 1e-10 mul } curl2");
    let dx: f64 = got[0].parse().unwrap();
    let dy: f64 = got[1].parse().unwrap();
    let sqrt_half = std::f64::consts::FRAC_1_SQRT_2;
    assert!(
        (dx - sqrt_half).abs() < 1e-6 && (dy + sqrt_half).abs() < 1e-6,
        "expected ({sqrt_half}, {}) for a tiny-but-nonzero uniform gradient, got ({dx}, {dy})",
        -sqrt_half
    );
}

#[test]
fn curl2_uses_plain_globals_so_an_ordinary_field_proc_can_hold_state() {
    // The whole point of curl2 *not* wrapping its own body in a
    // private dict (see the section header, and the cross-model
    // review that caught the original dict-wrapped draft breaking
    // this): a field proc is ordinary caller code and should be able
    // to hold plain `def`-based mutable state across calls, same as
    // any other artkit callback (grid/hexgrid/gasket's own stamp
    // procs all can). A dict-wrapped curl2 would silently swallow
    // this counter's updates at its own `end`, leaving it at 0.
    let got = eval(
        "/calls 0 def
         /countingfield { /calls calls 1 add def pop pop 5 } def
         100 100 0.5 /countingfield load curl2 pop pop
         calls",
    );
    assert_eq!(
        got[0], "4",
        "expected the counter to see all 4 of curl2's field evaluations, got {got:?}"
    );
}

#[test]
fn curl2_nested_in_its_own_field_proc_needs_the_inner_call_wrapped_in_a_dict() {
    // Regression for the flip side of the fix above: c2* being plain
    // globals (not dict-scoped) means a field proc that itself calls
    // curl2 again (composing two flow fields) clobbers the *outer*
    // call's own in-flight c2x/c2y/c2e/c2p the instant the inner call
    // starts, since both bind the same names -- same shape, same fix,
    // as gasket/carpet/hexgrid's own nested-composition gotcha: wrap
    // just the inner call in its own dict so its `def`s shadow there
    // instead. This pins both halves with a field, `ox+oy`, whose
    // curl is analytically known (constant gradient (1,1), so the
    // correct unit output is (1/sqrt(2), -1/sqrt(2)) everywhere):
    // outerfield corrupts c2p on its very first call, so if
    // unwrapped, the outer's *remaining* three field evaluations
    // silently invoke the inner call's own proc (`{ pop pop 7 }`,
    // constant) instead of outerfield -- collapsing c2dx to exactly 0
    // and leaving the result (1, 0) instead of the true diagonal, a
    // corruption caught by *value*, not just "it still runs".
    let unwrapped = eval(
        "/cx 50.0 def /cy 60.0 def /ceps 2.0 def
         /triggered false def
         /outerfield {
             /oy exch def /ox exch def
             triggered not {
                 /triggered true def
                 999 999 5.0 { pop pop 7 } curl2 pop pop
             } if
             ox oy add
         } def
         cx cy ceps /outerfield load curl2",
    );
    let ux: f64 = unwrapped[0].parse().unwrap();
    let uy: f64 = unwrapped[1].parse().unwrap();
    assert!(
        (ux - 1.0).abs() < 1e-9 && uy.abs() < 1e-9,
        "expected the unwrapped nested call to collapse the outer's result to exactly \
         (1, 0) (c2dx zeroed out by the hijacked proc), got ({ux}, {uy})"
    );

    let wrapped = eval(
        "/cx 50.0 def /cy 60.0 def /ceps 2.0 def
         /triggered false def
         /outerfield {
             /oy exch def /ox exch def
             triggered not {
                 /triggered true def
                 2 dict begin
                     999 999 5.0 { pop pop 7 } curl2 pop pop
                 end
             } if
             ox oy add
         } def
         cx cy ceps /outerfield load curl2",
    );
    let wx: f64 = wrapped[0].parse().unwrap();
    let wy: f64 = wrapped[1].parse().unwrap();
    let sqrt_half = std::f64::consts::FRAC_1_SQRT_2;
    assert!(
        (wx - sqrt_half).abs() < 1e-9 && (wy + sqrt_half).abs() < 1e-9,
        "wrapped: expected the true curl of ox+oy, (1/sqrt2, -1/sqrt2) = \
         ({sqrt_half}, {}), got ({wx}, {wy})",
        -sqrt_half
    );
}

#[test]
fn advect_does_not_normalize_the_fields_return_value() {
    // advect trusts the field's own magnitude and just scales by
    // stepsize -- curl2's output happens to be unit length, but a
    // hand-written field need not be, and advect must not silently
    // renormalize it out from under the caller.
    let got = eval("newpath 100 100 3 1 { pop pop 2 0 } advect currentpoint");
    let x: f64 = got[0].parse().unwrap();
    let y: f64 = got[1].parse().unwrap();
    assert!(
        (x - 106.0).abs() < 1e-9 && (y - 100.0).abs() < 1e-9,
        "expected (106, 100) from 3 steps of (2,0)*1, got ({x}, {y})"
    );
}

#[test]
fn advect_stops_early_and_leaves_no_linetos_on_a_zero_field() {
    let got = eval(
        "newpath 100 100 5 1 { pop pop 0 0 } advect
         /n 0 def
         { pop pop } { pop pop /n n 1 add def } { pop pop pop pop pop pop } { } pathforall
         n currentpoint",
    );
    let n: f64 = got[0].parse().unwrap();
    let x: f64 = got[1].parse().unwrap();
    let y: f64 = got[2].parse().unwrap();
    assert_eq!(n, 0.0, "a zero field should stop before any lineto");
    assert!(
        (x - 100.0).abs() < 1e-9 && (y - 100.0).abs() < 1e-9,
        "particle should not have moved from its start, got ({x}, {y})"
    );
}

#[test]
fn advect_uses_plain_globals_so_an_ordinary_field_proc_can_hold_state() {
    // Same rationale as curl2's equivalent test: advect not wrapping
    // its own body in a private dict is what lets an ordinary field
    // proc keep plain `def`-based state across calls, matching every
    // other artkit callback convention. A dict-wrapped advect would
    // silently swallow this counter's updates at its own `end`.
    let got = eval(
        "/calls 0 def
         /countingfield { /calls calls 1 add def pop pop 1 0 } def
         newpath 100 100 5 1 /countingfield load advect
         calls",
    );
    assert_eq!(
        got[0], "5",
        "expected the counter to see all 5 of advect's field evaluations, got {got:?}"
    );
}

#[test]
fn advect_nested_in_its_own_field_proc_needs_the_inner_call_wrapped_in_a_dict() {
    // Regression for the flip side of the fix above: ad* being plain
    // globals means a field proc that spawns a child trail (a nested
    // advect call) clobbers the *outer* particle's own in-flight
    // position and stepsize the instant the inner call starts. Same
    // fix as curl2's equivalent test and gasket/carpet's own
    // precedent: wrap just the inner call in its own dict. The child
    // draws on its own isolated subpath (gsave/newpath/...grestore) so
    // this isolates scratch-corruption from ordinary shared-current-
    // path interaction. Values pinned by direct execution: an outer
    // particle walking 12 steps of (1,0)*1 from the origin should end
    // at exactly (12, 0); confirmed the unwrapped draft instead lands
    // at (3.3, 0) -- the child's stepsize (0.3) and final x-position
    // leak into the outer's own subsequent steps.
    let unwrapped = eval(
        "/childcount 0 def
         /spawnfield {
             pop pop
             /childcount childcount 1 add def
             childcount 5 mod 0 eq {
                 gsave newpath 0 0 3 0.3 { pop pop 1 0 } advect grestore
             } if
             1 0
         } def
         newpath 0 0 12 1.0 /spawnfield load advect currentpoint",
    );
    let ux: f64 = unwrapped[0].parse().unwrap();
    let uy: f64 = unwrapped[1].parse().unwrap();
    assert!(
        (ux - 12.0).abs() > 1.0 && uy.abs() < 1e-6,
        "expected the unwrapped nested call to corrupt the outer's stepsize/position \
         (drifting well short of x=12), got ({ux}, {uy})"
    );

    let wrapped = eval(
        "/childcount2 0 def
         /spawnfield {
             pop pop
             /childcount2 childcount2 1 add def
             childcount2 5 mod 0 eq {
                 2 dict begin
                     gsave newpath 0 0 3 0.3 { pop pop 1 0 } advect grestore
                 end
             } if
             1 0
         } def
         newpath 0 0 12 1.0 /spawnfield load advect currentpoint",
    );
    assert_eq!(
        wrapped.len(),
        2,
        "field proc should leave exactly 2 values (dx,dy) per call, not leak -- got {wrapped:?}"
    );
    let wx: f64 = wrapped[0].parse().unwrap();
    let wy: f64 = wrapped[1].parse().unwrap();
    assert!(
        (wx - 12.0).abs() < 1e-9 && wy.abs() < 1e-9,
        "wrapped: outer particle (12 steps of (1,0)) should end at exactly (12, 0), got ({wx}, {wy})"
    );
}

#[test]
fn gradfn_builds_a_stitching_function_shaped_by_the_color_count() {
    // 3 colors -> 2 legs -> 1 bound -> 4 encode entries.
    assert_eq!(
        eval(
            "[[1 0 0] [0 1 0] [0 0 1]] gradfn \
             dup /FunctionType get exch \
             dup /Functions get length exch \
             dup /Bounds get length exch \
             /Encode get length"
        ),
        ["3", "2", "1", "4"]
    );
    // 2 colors -> 1 leg -> 0 bounds -> 2 encode entries.
    assert_eq!(
        eval(
            "[[1 0 0] [0 0 1]] gradfn \
             dup /Functions get length exch \
             /Bounds get length"
        ),
        ["1", "0"]
    );
}

#[test]
fn gradfn_places_bounds_evenly_across_the_domain() {
    let got = eval("[[1 0 0] [0 1 0] [0 0 1] [1 1 0]] gradfn /Bounds get aload pop");
    let b0: f64 = got[0].parse().unwrap();
    let b1: f64 = got[1].parse().unwrap();
    assert!((b0 - 1.0 / 3.0).abs() < 1e-9, "b0={b0}");
    assert!((b1 - 2.0 / 3.0).abs() < 1e-9, "b1={b1}");
}

#[test]
fn gradfn_legs_carry_the_adjacent_input_colors() {
    // First leg's C0 is the first color, last leg's C1 is the last.
    assert_eq!(
        eval("[[1 0 0] [0 1 0] [0 0 1]] gradfn /Functions get 0 get /C0 get aload pop"),
        ["1", "0", "0"]
    );
    assert_eq!(
        eval("[[1 0 0] [0 1 0] [0 0 1]] gradfn /Functions get 1 get /C1 get aload pop"),
        ["0", "0", "1"]
    );
}

#[test]
fn axialsh_and_radialsh_build_shading_dicts_with_correct_shape() {
    assert_eq!(
        eval(
            "10 20 30 40 [[1 0 0] [0 0 1]] axialsh \
             dup /ShadingType get exch \
             dup /ColorSpace get exch \
             /Coords get aload pop"
        ),
        ["2", "/DeviceRGB", "10", "20", "30", "40"]
    );
    assert_eq!(
        eval(
            "1 2 3 4 5 6 [[1 0 0] [0 0 1]] radialsh dup /ShadingType get exch /Coords get aload pop"
        ),
        ["3", "1", "2", "3", "4", "5", "6"]
    );
}

#[test]
fn gradfill_paints_an_axial_gradient_clipped_to_the_path() {
    let mut it = Interp::with_page(100, 100).expect("page");
    load(&mut it);
    it.run_str(
        "newpath 0 0 moveto 100 0 lineto 100 100 lineto 0 100 lineto closepath \
         0 0 100 0 [[0 0 0] [1 1 1]] axialsh gradfill",
    )
    .unwrap_or_else(|e| panic!("gradfill failed: {}", it.error_report(&e)));
    let (r, _, _) = pixel(&it, 5, 50);
    assert!(r < 40, "near t=0 should be dark, got {r}");
    let (r, _, _) = pixel(&it, 95, 50);
    assert!(r > 215, "near t=1 should be light, got {r}");
}

#[test]
fn gradfill_clips_to_the_current_path_not_the_whole_page() {
    let mut it = Interp::with_page(100, 100).expect("page");
    load(&mut it);
    it.run_str(
        "newpath 10 10 moveto 40 10 lineto 40 40 lineto 10 40 lineto closepath \
         0 0 100 0 [[0 0 0] [1 1 1]] axialsh gradfill",
    )
    .unwrap_or_else(|e| panic!("gradfill failed: {}", it.error_report(&e)));
    assert_eq!(
        pixel(&it, 80, 80),
        (255, 255, 255),
        "gradfill must not paint outside the clipped path"
    );
}
