//! Stage 5 graphics: clipping, dashes, color spaces, matrix operators.

use pscat::{Interp, PsError};

fn render(src: &str) -> Interp {
    let mut it = Interp::with_page(100, 100).expect("test page");
    it.run_str(src)
        .unwrap_or_else(|e| panic!("render of {src:?} failed: {e}"));
    it
}

fn eval(src: &str) -> Vec<String> {
    let it = render(src);
    it.operand_stack().iter().map(|o| o.repr()).collect()
}

fn pixel(it: &Interp, x: u32, y: u32) -> (u8, u8, u8) {
    let p = it.gfx().pixmap.pixel(x, y).expect("pixel in bounds");
    (p.red(), p.green(), p.blue())
}

const FULL_PAGE: &str = "newpath 0 0 moveto 100 0 lineto 100 100 lineto 0 100 lineto closepath";
const BLACK: (u8, u8, u8) = (0, 0, 0);
const WHITE: (u8, u8, u8) = (255, 255, 255);

#[test]
fn clip_restricts_painting() {
    let it = render(&format!(
        "newpath 20 20 moveto 80 20 lineto 80 80 lineto 20 80 lineto closepath clip newpath {FULL_PAGE} fill"
    ));
    assert_eq!(pixel(&it, 50, 50), BLACK);
    assert_eq!(pixel(&it, 10, 10), WHITE);
    assert_eq!(pixel(&it, 90, 90), WHITE);
}

#[test]
fn nested_clips_intersect() {
    let it = render(
        "newpath 10 10 moveto 60 10 lineto 60 60 lineto 10 60 lineto closepath clip \
         newpath 40 40 moveto 90 40 lineto 90 90 lineto 40 90 lineto closepath clip \
         newpath 0 0 moveto 100 0 lineto 100 100 lineto 0 100 lineto closepath fill",
    );
    assert_eq!(pixel(&it, 50, 50), BLACK); // inside both
    assert_eq!(pixel(&it, 20, 20), WHITE); // first only
    assert_eq!(pixel(&it, 80, 80), WHITE); // second only
}

#[test]
fn gsave_restores_clip_and_initclip_drops_it() {
    let it = render(&format!(
        "gsave newpath 40 40 moveto 60 40 lineto 60 60 lineto 40 60 lineto closepath clip grestore newpath {FULL_PAGE} fill"
    ));
    assert_eq!(pixel(&it, 10, 10), BLACK, "grestore must drop the clip");

    let it = render(&format!(
        "newpath 40 40 moveto 60 40 lineto 60 60 lineto 40 60 lineto closepath clip initclip newpath {FULL_PAGE} fill"
    ));
    assert_eq!(pixel(&it, 10, 10), BLACK, "initclip must drop the clip");
}

#[test]
fn clip_leaves_current_path() {
    // Per the PLRM, clip does not clear the path — fill right after.
    let it =
        render("newpath 20 20 moveto 80 20 lineto 80 80 lineto 20 80 lineto closepath clip fill");
    assert_eq!(pixel(&it, 50, 50), BLACK);
}

#[test]
fn setdash_makes_gaps() {
    let it = render("[10 10] 0 setdash 4 setlinewidth newpath 0 50 moveto 100 50 lineto stroke");
    assert_eq!(pixel(&it, 5, 50), BLACK); // first on-segment
    assert_eq!(pixel(&it, 15, 50), WHITE); // first gap
    assert_eq!(pixel(&it, 25, 50), BLACK); // second on-segment
    assert_eq!(eval("[4 2] 1 setdash currentdash"), ["[4.0 2.0]", "1.0"]);
    assert_eq!(eval_err("[-1] 0 setdash"), PsError::Rangecheck);
    assert_eq!(eval_err("[0 0] 0 setdash"), PsError::Rangecheck);
}

fn eval_err(src: &str) -> PsError {
    let mut it = Interp::with_page(100, 100).expect("page");
    it.run_str(src).expect_err("expected failure")
}

#[test]
fn color_spaces() {
    let it = render(&format!("0 1 1 sethsbcolor {FULL_PAGE} fill"));
    assert_eq!(pixel(&it, 50, 50), (255, 0, 0), "hsb hue 0 is red");
    let it = render(&format!("1 0 0 0 setcmykcolor {FULL_PAGE} fill"));
    assert_eq!(pixel(&it, 50, 50), (0, 255, 255), "pure cyan");
    assert_eq!(eval("1 0 0 setrgbcolor currentgray"), ["0.3"]);
    assert_eq!(eval("0.5 setgray currentrgbcolor pop pop"), ["0.5"]);
    // HSB round-trips through RGB.
    let hsb = eval("0.25 0.5 0.75 sethsbcolor currenthsbcolor");
    let vals: Vec<f64> = hsb.iter().map(|s| s.parse().unwrap()).collect();
    assert!((vals[0] - 0.25).abs() < 0.01, "hue {hsb:?}");
    assert!((vals[1] - 0.5).abs() < 0.01, "sat {hsb:?}");
    assert!((vals[2] - 0.75).abs() < 0.01, "val {hsb:?}");
}

#[test]
fn matrix_operators() {
    assert_eq!(
        eval("matrix aload pop"),
        ["1.0", "0.0", "0.0", "1.0", "0.0", "0.0"]
    );
    assert_eq!(
        eval("matrix currentmatrix aload pop"),
        ["1.0", "0.0", "0.0", "-1.0", "0.0", "100.0"]
    );
    assert_eq!(
        eval("10 20 matrix translate aload pop"),
        ["1.0", "0.0", "0.0", "1.0", "10.0", "20.0"]
    );
    assert_eq!(
        eval("2 3 matrix scale aload pop"),
        ["2.0", "0.0", "0.0", "3.0", "0.0", "0.0"]
    );
    // Through the default CTM: user (30,40) is device (30,60) on a
    // 100-high page, and back again.
    assert_eq!(eval("30 40 transform"), ["30.0", "60.0"]);
    assert_eq!(eval("30 60 itransform"), ["30.0", "40.0"]);
    assert_eq!(eval("0 1 dtransform"), ["0.0", "-1.0"]);
    // Explicit matrix operand, no CTM involvement.
    assert_eq!(eval("5 7 [2 0 0 2 0 0] transform"), ["10.0", "14.0"]);
    assert_eq!(eval("5 7 [2 0 0 2 0 0] itransform"), ["2.5", "3.5"]);
    // concat then draw lands where translate would have put it.
    let it = render(
        "[1 0 0 1 40 40] concat newpath 0 0 moveto 20 0 lineto 20 20 lineto 0 20 lineto closepath fill",
    );
    assert_eq!(pixel(&it, 50, 50), BLACK);
    // setmatrix restores a captured CTM exactly.
    assert_eq!(
        eval(
            "matrix currentmatrix /m exch def 5 5 translate 90 rotate m setmatrix 30 40 transform"
        ),
        ["30.0", "60.0"]
    );
    // concatmatrix: translate-then-scale doubles the offset.
    assert_eq!(
        eval("[1 0 0 1 10 0] [2 0 0 2 0 0] matrix concatmatrix aload pop"),
        ["2.0", "0.0", "0.0", "2.0", "20.0", "0.0"]
    );
    assert_eq!(eval_err("[1 2 3] setmatrix"), PsError::Rangecheck);
}

#[test]
fn pathbbox_and_clippath() {
    assert_eq!(
        eval("newpath 10 20 moveto 50 60 lineto pathbbox"),
        ["10.0", "20.0", "50.0", "60.0"]
    );
    // Unclipped clippath is the whole page.
    assert_eq!(eval("clippath pathbbox"), ["0.0", "0.0", "100.0", "100.0"]);
    // With a clip, clippath reports the clip region's box.
    assert_eq!(
        eval(
            "newpath 20 30 moveto 70 30 lineto 70 90 lineto 20 90 lineto closepath clip newpath clippath pathbbox"
        ),
        ["20.0", "30.0", "70.0", "90.0"]
    );
    let mut it = Interp::with_page(100, 100).expect("page");
    assert_eq!(it.run_str("newpath pathbbox"), Err(PsError::NoCurrentPoint));
}

#[test]
fn clippath_reports_the_intersection_of_nested_clips() {
    // Issue #120: clippath used to report only the newest of several
    // nested clips, not their true intersection (confirmed against
    // Ghostscript, which reports the same 30 30 60 60 as below).
    assert_eq!(
        eval("newpath 0 0 60 60 rectclip newpath 30 30 60 60 rectclip clippath pathbbox"),
        ["30.0", "30.0", "60.0", "60.0"]
    );
}

#[test]
fn clippath_traces_a_non_rectangular_nested_clip() {
    // A rect nested with a curved clip can't take the exact-rectangle
    // fast path (chain_rect_intersection); the pixel-mask trace still
    // reports the true (tighter) intersection instead of just the
    // newest clip in the chain. Matches Ghostscript's 30 30 60 60.
    assert_eq!(
        eval(
            "newpath 0 0 60 60 rectclip \
             newpath 50 50 20 0 360 arc closepath clip newpath \
             clippath pathbbox"
        ),
        ["30.0", "30.0", "60.0", "60.0"]
    );
}

#[test]
fn clippath_traces_a_diagonal_nested_clip() {
    // A 45°-rotated diamond nested with an axis-aligned rect exercises
    // the boundary tracer's diagonal-touch ("checkerboard") handling —
    // its edges cross the pixel grid at a shallow angle, unlike the
    // straight-on rect/circle cases above. Matches Ghostscript's own
    // 30 30 80 80 exactly.
    assert_eq!(
        eval(
            "newpath 20 20 60 60 rectclip \
             newpath 60 30 moveto 90 60 lineto 60 90 lineto 30 60 lineto closepath clip newpath \
             clippath pathbbox"
        ),
        ["30.0", "30.0", "80.0", "80.0"]
    );
}

#[test]
fn clippath_rect_fast_path_is_fill_rule_agnostic() {
    // as_axis_aligned_rect doesn't consult ClipNode::rule: for one
    // non-self-intersecting closed loop, nonzero and even-odd always
    // agree, so an eoclip rectangle still takes the exact rect fast
    // path. Matches Ghostscript's 20 20 80 80.
    assert_eq!(
        eval(
            "newpath 0 0 100 100 rectclip \
             newpath 20 20 moveto 20 80 lineto 80 80 lineto 80 20 lineto closepath eoclip \
             clippath pathbbox"
        ),
        ["20.0", "20.0", "80.0", "80.0"]
    );
}

#[test]
fn currentlinewidth_reports() {
    assert_eq!(eval("3 setlinewidth currentlinewidth"), ["3.0"]);
}

// --- the Level 2 rectangle conveniences (gs-pinned semantics) --------

#[test]
fn rectfill_paints_and_preserves_state() {
    let it = render("20 20 60 60 rectfill");
    assert_eq!(pixel(&it, 50, 50), BLACK);
    assert_eq!(pixel(&it, 10, 10), WHITE);
    // Implicit gsave: the current path (and point) survive.
    assert_eq!(
        eval("newpath 5 6 moveto 20 20 10 10 rectfill currentpoint"),
        ["5.0", "6.0"]
    );
    // Negative width/height: the rect is corner-defined.
    let it = render("70 70 -40 -40 rectfill");
    assert_eq!(pixel(&it, 50, 50), BLACK);
}

#[test]
fn rect_array_forms() {
    // Two quads in one flat array; empty array is a no-op.
    let it = render("[10 10 20 20 60 60 20 20] rectfill");
    assert_eq!(pixel(&it, 20, 80), BLACK); // device y flips: (20,20) area
    assert_eq!(pixel(&it, 70, 30), BLACK);
    assert_eq!(pixel(&it, 50, 50), WHITE); // between them
    let it = render("[] rectfill");
    assert_eq!(pixel(&it, 50, 50), WHITE);
    assert_eq!(eval_err("[1 2 3] rectfill"), PsError::Typecheck);
    assert_eq!(eval_err("(str) rectfill"), PsError::Typecheck);
}

#[test]
fn rectstroke_draws_an_outline() {
    let it = render("4 setlinewidth 20 20 60 60 rectstroke");
    assert_eq!(pixel(&it, 20, 50), BLACK, "left edge inked");
    assert_eq!(pixel(&it, 50, 50), WHITE, "interior open");
    // Matrix form: a 6-array on top is always the matrix (gs pin) and
    // shapes the pen, not the rectangles.
    let it = render("2 setlinewidth 20 20 60 60 [4 0 0 4 0 0] rectstroke");
    assert_eq!(pixel(&it, 20, 50), BLACK);
    assert_eq!(pixel(&it, 50, 50), WHITE);
    // The scaled pen spans 16..24 around the edge at x=20; an unscaled
    // 2-unit pen would only cover 19..21, leaving x=23 white.
    assert_eq!(
        pixel(&it, 23, 50),
        BLACK,
        "matrix-scaled pen is 8 units wide"
    );
    // ...so a 6-element rect list under a matrix typechecks, as in gs.
    assert_eq!(
        eval_err("[1 2 3 4 5 6] [1 0 0 1 0 0] rectstroke"),
        PsError::Typecheck
    );
    // And the state survives, matrix and all.
    assert_eq!(
        eval("10 10 5 5 [2 0 0 2 0 0] rectstroke 30 40 transform"),
        ["30.0", "60.0"]
    );
}

#[test]
fn rectclip_clips_and_clears_the_path() {
    let it = render(&format!("20 20 60 60 rectclip {FULL_PAGE} fill"));
    assert_eq!(pixel(&it, 50, 50), BLACK);
    assert_eq!(pixel(&it, 10, 10), WHITE);
    // The current path is left empty (gs pin).
    let mut it = Interp::with_page(100, 100).expect("page");
    assert_eq!(
        it.run_str("0 0 moveto 20 20 60 60 rectclip currentpoint"),
        Err(PsError::NoCurrentPoint)
    );
    // An empty rect list clips everything away (gs pin).
    let it = render(&format!("[] rectclip {FULL_PAGE} fill"));
    assert_eq!(pixel(&it, 50, 50), WHITE);
}

// --- issue #47: setalpha/setblendmode, pscat's own extensions -------
//
// These are deliberately *not* PLRM operators (real Ghostscript has no
// PostScript-callable alpha operator at all — issue #46's spike tested
// that directly), so there's no gs behavior to pin them against the way
// the rest of this file pins its operators. What's asserted here is the
// contract `docs/WATERCOLOR.md` recorded for them: clamping, graphics-
// state semantics, and a hard error on an unknown mode name.

#[test]
fn setalpha_clamps_and_round_trips() {
    assert_eq!(eval("0.25 setalpha currentalpha"), ["0.25"]);
    assert_eq!(eval("2 setalpha currentalpha"), ["1.0"]);
    assert_eq!(eval("-3 setalpha currentalpha"), ["0.0"]);
    // Nothing set it: the default is opaque.
    assert_eq!(eval("currentalpha"), ["1.0"]);
}

#[test]
fn setblendmode_accepts_only_the_two_modes() {
    assert_eq!(
        eval("/Multiply setblendmode currentblendmode"),
        ["/Multiply"]
    );
    assert_eq!(eval("/Normal setblendmode currentblendmode"), ["/Normal"]);
    assert_eq!(eval("currentblendmode"), ["/Normal"]);

    let mut it = Interp::with_page(100, 100).expect("test page");
    // An unknown mode is a rangecheck, not a silent fall-through to
    // Normal — a typo has to be loud or it diverges from what the
    // program asked for with no signal at all.
    assert!(matches!(
        it.run_str("/Screen setblendmode"),
        Err(PsError::Rangecheck)
    ));
    let mut it = Interp::with_page(100, 100).expect("test page");
    assert!(matches!(
        it.run_str("(Multiply) setblendmode"),
        Err(PsError::Typecheck)
    ));
}

#[test]
fn alpha_and_blend_are_graphics_state() {
    // gsave/grestore snapshot them like any other paint attribute...
    assert_eq!(
        eval(
            "0.5 setalpha /Multiply setblendmode gsave 0.1 setalpha /Normal setblendmode grestore currentalpha currentblendmode"
        ),
        ["0.5", "/Multiply"]
    );
    // ...and initgraphics resets them.
    assert_eq!(
        eval("0.5 setalpha /Multiply setblendmode initgraphics currentalpha currentblendmode"),
        ["1.0", "/Normal"]
    );
}

#[test]
fn alpha_paints_translucently_and_stacks() {
    // One 50% black wash over white paper is mid grey; a second one
    // over the first is darker still — the build-up `pkwash`'s /Layers
    // key is made of.
    let it = render(&format!("0.5 setalpha 0 setgray newpath {FULL_PAGE} fill"));
    let (r, _, _) = pixel(&it, 50, 50);
    assert!(r.abs_diff(128) <= 2, "one wash: {r}");

    let it = render(&format!(
        "0.5 setalpha 0 setgray newpath {FULL_PAGE} fill newpath {FULL_PAGE} fill"
    ));
    let (r2, _, _) = pixel(&it, 50, 50);
    assert!(r2.abs_diff(64) <= 3, "two washes: {r2}");
}

#[test]
fn zero_alpha_paints_nothing() {
    let it = render(&format!("0 setalpha 0 setgray newpath {FULL_PAGE} fill"));
    assert_eq!(pixel(&it, 50, 50), WHITE);
}

#[test]
fn multiply_darkens_where_normal_replaces() {
    // Yellow then blue. Under Normal the blue simply wins; under
    // Multiply the overlap goes to the component-wise product, which
    // for these two is near-black — the pigment-overlap look.
    let program = |mode: &str| {
        format!(
            "/{mode} setblendmode 1 1 0 setrgbcolor newpath {FULL_PAGE} fill
             0 0 1 setrgbcolor newpath {FULL_PAGE} fill"
        )
    };
    assert_eq!(pixel(&render(&program("Normal")), 50, 50), (0, 0, 255));
    assert_eq!(pixel(&render(&program("Multiply")), 50, 50), BLACK);
}
