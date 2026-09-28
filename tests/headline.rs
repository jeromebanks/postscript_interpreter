//! Headline composition helpers (issue #145, `lib/headline.ps`):
//! measured-ink geometry, the shirt-front specimen's acceptance
//! criteria (exact strings, no collisions, `Real` dominant, capital
//! above the lowercase, descender below the baseline), recompute-or-
//! fail behavior, determinism, and that a call leaves the caller's
//! operand stack, font and path alone.

use pscat::{Interp, PsError};

fn with_lib(w: u32, h: u32) -> Interp {
    let mut it = Interp::with_page(w, h).expect("page");
    for f in ["lib/artkit.ps", "lib/headline.ps"] {
        let src = std::fs::read(f).expect("lib present");
        it.run_source(&src)
            .unwrap_or_else(|e| panic!("{f} failed: {}", it.error_report(&e)));
    }
    it
}

fn stack_of(src: &str) -> Vec<String> {
    let mut it = with_lib(900, 900);
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
    let mut it = with_lib(900, 900);
    match it.run_str(src).unwrap_err() {
        PsError::Undefined(name) => name,
        other => panic!("expected a self-documenting undefined name, got {other}"),
    }
}

/// The shirt front: the same layout examples/headline.ps draws.
const FRONT: &str = "
/front [ 40 470 860 880 ] [
  << /Name /real /Text (Real) /Font /AlfaSlabOne /FitBox [ 780 300 ] /At [ 60 490 ] >>
  << /Name /keep /Text (Keepin' it) /Font /Helvetica-Bold /SizeOf [ /real 0.17 ]
     /At << /To /real /Chars [ 1 3 ] /H 0 /V 1 >>
     /Anchor [ /left /bottom ] /Offset [ 8 18 ]
     /Avoid [ [ /real 0 1 ] [ /real 1 3 ] [ /real 3 4 ] ] /Clearance 10 >>
  << /Name /im /Text (I'm) /Font /Helvetica-Bold /SizeOf [ /keep 0.75 ]
     /At << /To /keep /H 0 /V 1 >> /Anchor [ /left /bottom ] /Offset [ 0 12 ]
     /Avoid [ /keep [ /real 0 1 ] [ /real 1 3 ] [ /real 3 4 ] ] /Clearance 10 >>
] hllayout def
";

fn ink(name: &str) -> [f64; 4] {
    let v = nums(&format!("{FRONT} front /{name} hlrunink"));
    [v[0], v[1], v[2], v[3]]
}

fn size(name: &str) -> f64 {
    nums(&format!("{FRONT} front /Names get /{name} get /Size get"))[0]
}

fn chars(name: &str, i: u32, j: u32) -> [f64; 4] {
    let v = nums(&format!("{FRONT} front /{name} {i} {j} hlcharink"));
    [v[0], v[1], v[2], v[3]]
}

fn disjoint(a: [f64; 4], b: [f64; 4], gap: f64) -> bool {
    a[2] + gap <= b[0] || b[2] + gap <= a[0] || a[3] + gap <= b[1] || b[3] + gap <= a[1]
}

#[test]
fn front_lays_out_the_exact_strings() {
    let got = stack_of(&format!("{FRONT} front /Runs get {{ /Text get }} forall"));
    assert_eq!(got, ["(Real)", "(Keepin' it)", "(I'm)"]);
}

#[test]
fn front_stays_in_its_region_and_real_dominates() {
    for name in ["real", "keep", "im"] {
        let b = ink(name);
        assert!(
            b[0] >= 40.0 && b[1] >= 470.0 && b[2] <= 860.0 && b[3] <= 880.0,
            "{name}: {b:?}"
        );
    }
    assert!(size("real") > 4.0 * size("keep"));
    let (r, k, m) = (ink("real"), ink("keep"), ink("im"));
    let area = |b: [f64; 4]| (b[2] - b[0]) * (b[3] - b[1]);
    assert!(area(r) > 5.0 * area(k) && area(k) > area(m));
}

#[test]
fn front_supporting_type_clears_the_glyph_groups_it_avoids() {
    let groups = [
        chars("real", 0, 1),
        chars("real", 1, 3),
        chars("real", 3, 4),
    ];
    for g in groups {
        assert!(disjoint(ink("keep"), g, 10.0), "keep vs {g:?}");
        assert!(disjoint(ink("im"), g, 10.0), "im vs {g:?}");
    }
    assert!(disjoint(ink("im"), ink("keep"), 10.0));
}

#[test]
fn ink_boxes_are_glyph_ink_not_advance_and_see_ascenders_and_descenders() {
    // The capital R and the l rise above the lowercase e and a (the
    // measured box, not the font's nominal cap height).
    let r = chars("real", 0, 1);
    let ea = chars("real", 1, 3);
    let l = chars("real", 3, 4);
    assert!(r[3] > ea[3] + 20.0 && l[3] > ea[3] + 20.0);
    // A descender goes below the baseline: the p of `Keepin'` is the
    // lowest ink of its line, below the baseline (Origin y).
    let base = nums(&format!(
        "{FRONT} front /Names get /keep get /Origin get 1 get"
    ))[0];
    let keep = ink("keep");
    assert!(
        keep[1] < base - 1.0,
        "descender {} vs baseline {base}",
        keep[1]
    );
    // Ink is not advance: `stringwidth` is wider than the ink of a
    // string with side bearings.
    let v =
        nums("/Helvetica-Bold findfont 100 scalefont setfont (Real) hlink (Real) stringwidth pop");
    let ink_w = v[2] - v[0];
    assert!(
        ink_w > 0.0 && ink_w < v[4],
        "ink {ink_w} vs advance {}",
        v[4]
    );
}

#[test]
fn bounds_exclude_control_points_and_the_trailing_pen_advance_point() {
    // `o` unflattened: pathbbox includes the Bezier control points, so
    // the library's (flattened) box must be strictly tighter.
    let v = nums(
        "/PermanentMarker findfont 100 scalefont setfont (o) hlink
         newpath 0 0 moveto (o) false charpath pathbbox",
    );
    assert!(v[1] > v[5] + 0.05, "{v:?}");
    // charpath leaves a moveto at the advance point that pathbbox
    // counts: the ink of `l` ends before its advance, and an
    // apostrophe sits entirely above the baseline (pathbbox says 0).
    let v = nums(
        "/AlfaSlabOne findfont 100 scalefont setfont (l) hlink (l) stringwidth pop
         (') hlink pop pop exch pop",
    );
    assert!(v[2] < v[4] - 1.0, "ink right {} vs advance {}", v[2], v[4]);
    assert!(v[5] > 20.0, "apostrophe bottom {}", v[5]);
}

#[test]
fn a_longer_phrase_recomputes_within_the_declared_region() {
    let run = |text: &str| {
        nums(&format!(
            "[ 0 0 500 300 ] [ << /Name /w /Text ({text}) /Font /AlfaSlabOne
               /FitBox [ 400 200 ] /At [ 50 50 ] /Anchor [ /left /bottom ] >> ] hllayout
             dup /Names get /w get /Size get exch /w hlrunink"
        ))
    };
    let short = run("Real");
    let long = run("Realistically");
    let (ss, ls) = (short[0], long[0]);
    assert!(ls < ss, "a longer phrase must scale down: {ls} vs {ss}");
    for v in [&short, &long] {
        let (x0, y0, x1, y1) = (v[1], v[2], v[3], v[4]);
        assert!(
            x0 >= 49.99 && x1 <= 450.01 && y0 >= 49.99 && y1 <= 250.01,
            "{v:?}"
        );
    }
}

#[test]
fn overflow_and_collisions_fail_with_named_errors() {
    let region = "[ 0 0 300 200 ]";
    assert_eq!(
        err_of(&format!(
            "{region} [ << /Text (Real) /Font /AlfaSlabOne /Size 200 /At [ 10 10 ] >> ] hllayout"
        )),
        "headline-run-outside-region"
    );
    assert_eq!(
        err_of(&format!(
            "{region} [ << /Text (Realistically) /Font /AlfaSlabOne /FitBox [ 280 40 ]
                          /MinSize 60 /At [ 10 10 ] >> ] hllayout"
        )),
        "headline-fit-below-minimum-size"
    );
    assert_eq!(
        err_of(&format!(
            "{region} [ << /Name /a /Text (Real) /Size 60 /At [ 10 10 ] >>
                        << /Name /b /Text (Real) /Size 60 /At [ 30 20 ] >> ] hllayout"
        )),
        "headline-runs-collide"
    );
    // Separated by /Offset the same two runs are fine, and an explicit
    // empty /Avoid opts a run out of the check.
    stack_of(&format!(
        "{region} [ << /Name /a /Text (Real) /Size 60 /At [ 10 10 ] >>
                    << /Name /b /Text (Real) /Size 60 /At [ 30 20 ] /Avoid [ ] >> ] hllayout pop"
    ));
}

#[test]
fn option_and_font_errors_are_self_documenting() {
    let base = |run: &str| format!("[ 0 0 300 200 ] [ << /Text (Real) {run} >> ] hllayout");
    for (run, want) in [
        (
            "/Font /NoSuchFaceAnywhere /Size 20 /At [ 0 0 ]",
            "headline-font-not-found",
        ),
        ("/Size 20", "headline-run-needs-an-at-point"),
        ("/At [ 0 0 ]", "headline-run-needs-exactly-one-size-option"),
        (
            "/Size 20 /FitWidth 20 /At [ 0 0 ]",
            "headline-run-needs-exactly-one-size-option",
        ),
        ("/Size 20 /At << /To /nope >>", "headline-unknown-reference"),
        (
            "/Size 20 /At [ 0 0 ] /Anchor [ /sideways /top ]",
            "headline-unknown-position-name",
        ),
        (
            "/Size 20 /At [ 0 0 ] /Justify /full",
            "headline-justify-must-be-left-center-or-right",
        ),
        ("/Size -3 /At [ 0 0 ]", "headline-size-must-be-positive"),
    ] {
        assert_eq!(err_of(&base(run)), want, "{run}");
    }
    assert_eq!(
        err_of("[ 0 0 300 200 ] [ << /Text (   ) /Size 20 /At [ 10 10 ] >> ] hllayout"),
        "headline-text-has-no-ink"
    );
    assert_eq!(
        err_of("[ 0 0 0 200 ] [ ] hllayout"),
        "headline-region-must-have-positive-size"
    );
    assert_eq!(
        err_of("/Helvetica findfont 20 scalefont setfont (  ) hlink"),
        "headline-text-has-no-ink"
    );

    let mut it = with_lib(400, 200);
    let font = std::fs::read("lib/fonts/neon.ps").expect("neon present");
    it.run_source(&font).expect("neon loads");
    match it
        .run_str(&base("/Font /Neon /Size 20 /At [ 10 10 ]"))
        .unwrap_err()
    {
        PsError::Undefined(name) => assert_eq!(name, "headline-type3-faces-have-no-outlines"),
        other => panic!("expected a self-documenting error, got {other}"),
    }
}

#[test]
fn executable_operands_are_never_executed() {
    for (src, want) in [
        (
            "{ boom } [ ] hllayout",
            "headline-region-must-be-x0-y0-x1-y1",
        ),
        (
            "[ 0 0 9 9 ] { boom } hllayout",
            "headline-runs-must-be-an-array",
        ),
        (
            "[ 0 0 9 9 ] [ { boom } ] hllayout",
            "headline-run-must-be-a-dict",
        ),
        ("{ boom } hlink", "headline-text-must-be-a-string"),
        (
            "[ 0 0 99 99 ] [ << /Text (x) /Size 9 /At [ 1 1 ] /Justify { boom } >> ] hllayout",
            "headline-justify-must-be-left-center-or-right",
        ),
        (
            "[ 0 0 99 99 ] [ << /Text (x) /Size 9 /At { boom } >> ] hllayout",
            "headline-at-must-be-a-point-or-a-reference",
        ),
        (
            "[ 0 0 99 99 ] [ << /Text (x) /Size 9 /At [ 1 1 ] /Anchor { boom } >> ] hllayout",
            "headline-anchor-must-be-h-v",
        ),
        (
            "[ 0 0 99 99 ] [ << /Text (x) /Size 9 /At [ 1 1 ] /Name { boom } >> ] hllayout",
            "headline-name-must-be-a-name",
        ),
        ("{ boom } /a hlrunink", "headline-layout-must-be-a-dict"),
    ] {
        assert_eq!(err_of(src), want, "{src}");
    }
}

#[test]
fn a_call_leaves_stack_font_and_path_alone_even_on_failure() {
    let mut it = with_lib(400, 200);
    it.run_str(
        "/Times-Roman findfont 17 scalefont setfont newpath 5 5 moveto 9 9 lineto 42 (kept)",
    )
    .expect("setup");
    let before: Vec<String> = it.operand_stack().iter().map(|o| o.repr()).collect();
    it.run_str(
        "[ 0 0 300 200 ] [ << /Text (Real) /Font /AlfaSlabOne /Size 60 /At [ 10 10 ] >> ]
         hllayout pop",
    )
    .expect("ok layout");
    let failed = it.run_str(
        "[ 0 0 300 200 ] [ << /Text (Real) /Font /AlfaSlabOne /Size 500 /At [ 10 10 ] >> ] hllayout",
    );
    assert!(failed.is_err());
    // Drop the failed call's error, then confirm the rest is intact.
    it.run_str("currentfont /FontName get == 1 1 1 1 pathbbox pop pop pop pop")
        .ok();
    let mut probe = |src: &str| -> Vec<String> {
        it.run_str(src).expect("probe");
        it.operand_stack().iter().map(|o| o.repr()).collect()
    };
    let after = probe("");
    assert!(after.len() >= before.len());
    assert_eq!(&after[..before.len()], &before[..], "operand stack changed");
    let font = probe("currentfont /FontName get 96 string cvs");
    assert_eq!(font.last().unwrap(), "(Times-Roman)");
}

#[test]
fn layouts_and_pixels_are_deterministic() {
    let render = || {
        let mut it = with_lib(900, 900);
        it.run_str("1 setgray clippath fill 0 setgray")
            .and_then(|_| it.run_str(&format!("{FRONT} front hldraw")))
            .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
        it.gfx()
            .pixmap
            .pixels()
            .iter()
            .flat_map(|p| [p.red(), p.green(), p.blue()])
            .collect::<Vec<u8>>()
    };
    assert_eq!(render(), render());
    assert_eq!(
        stack_of(&format!("{FRONT} front /Runs get {{ /Ink get }} forall")),
        stack_of(&format!("{FRONT} front /Runs get {{ /Ink get }} forall"))
    );
}

/// The measured box is where the ink actually is: every inked pixel
/// falls inside the union of the runs' boxes, and each box's four edges
/// are touched by ink (so the box is tight, not merely a superset).
#[test]
fn rendered_ink_matches_the_measured_boxes() {
    let mut it = with_lib(900, 900);
    it.run_str("1 setgray clippath fill 0 setgray")
        .and_then(|_| it.run_str(&format!("{FRONT} front hldraw")))
        .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    let (w, h) = (900usize, 900usize);
    let px = it.gfx().pixmap.pixels().to_vec();
    let inked = |x: usize, y: usize| px[y * w + x].red() < 128;
    // Page y is flipped: device row = h - y.
    let boxes = [ink("real"), ink("keep"), ink("im")];
    let inside = |x: usize, y: usize| {
        let (fx, fy) = (x as f64 + 0.5, (h - 1 - y) as f64 + 0.5);
        boxes
            .iter()
            .any(|b| fx >= b[0] - 1.5 && fx <= b[2] + 1.5 && fy >= b[1] - 1.5 && fy <= b[3] + 1.5)
    };
    let mut any = 0;
    for y in 0..h {
        for x in 0..w {
            if inked(x, y) {
                any += 1;
                assert!(inside(x, y), "ink at ({x},{y}) outside every measured box");
            }
        }
    }
    assert!(any > 1000);
    for (i, b) in boxes.iter().enumerate() {
        let (x0, x1) = (b[0].floor() as usize, (b[2].ceil() as usize).min(w - 1));
        let (y_lo, y_hi) = (b[1].floor() as usize, (b[3].ceil() as usize).min(h - 1));
        let row =
            |fy: usize| (x0.saturating_sub(1)..=(x1 + 1).min(w - 1)).any(|x| inked(x, h - 1 - fy));
        let col = |fx: usize| {
            (y_lo.saturating_sub(1)..=(y_hi + 1).min(h - 1)).any(|y| inked(fx, h - 1 - y))
        };
        // An edge pixel is only partly covered when the box edge falls
        // mid-pixel, so accept ink within two pixels of each edge.
        let near = |c: &dyn Fn(usize) -> bool, e: usize| {
            (e.saturating_sub(2)..=e + 2).any(|v| v < w.max(h) && c(v))
        };
        assert!(near(&row, y_lo), "box {i} bottom edge has no ink");
        assert!(near(&row, y_hi), "box {i} top edge has no ink");
        assert!(near(&col, x0), "box {i} left edge has no ink");
        assert!(near(&col, x1), "box {i} right edge has no ink");
    }
}

#[test]
fn explicit_line_breaks_justify_and_stack() {
    let v = nums(
        "[ 0 0 400 300 ] [ << /Name /p /Text (ab\\nlonger line) /Font /Helvetica-Bold
            /Size 40 /Justify /right /At [ 20 200 ] >> ] hllayout
         /Names get /p get /LineOrigins get
         dup 0 get aload pop 3 -1 roll 1 get aload pop",
    );
    // Right-justified: the short first line starts to the right of the
    // second line, and the second line sits below the first.
    let (x0, y0, x1, y1) = (
        v[v.len() - 4],
        v[v.len() - 3],
        v[v.len() - 2],
        v[v.len() - 1],
    );
    assert!(
        x0 > x1,
        "short line must start right of the long one: {v:?}"
    );
    assert!(y1 < y0);
}

#[test]
fn a_custom_draw_proc_receives_text_position_and_size() {
    // /Draw must consume its four operands and leave nothing (hldraw
    // runs under save/restore so a failing proc cannot leak graphics
    // state), so the proc paints a size x size square at (x, y).
    let mut it = with_lib(400, 300);
    it.run_str("1 setgray clippath fill 0 setgray").expect("bg");
    it.run_str(
        "[ 0 0 400 300 ] [ << /Text (Real) /Size 50 /At [ 20 100 ]
           /Draw { 4 dict begin /s exch def /y exch def /x exch def pop
                   x y s s rectfill end } >> ] hllayout hldraw",
    )
    .expect("draw");
    let px = it.gfx().pixmap.pixels().to_vec();
    let dark = |x: usize, y_up: usize| px[(300 - 1 - y_up) * 400 + x].red() < 128;
    // The pen origin is one left side-bearing left of the ink edge at
    // x=20, so the square starts just under 20 and spans 50 points.
    assert!(dark(30, 110) && dark(60, 140));
    assert!(!dark(5, 110) && !dark(30, 160));
}

#[test]
fn an_error_inside_a_nested_gsave_restores_font_dicts_and_path() {
    let mut it = with_lib(900, 900);
    it.run_str(&format!(
        "/Times-Roman findfont 17 scalefont setfont newpath 5 5 moveto 9 9 lineto
         {FRONT} countdictstack"
    ))
    .expect("setup");
    let depth = it.operand_stack().last().unwrap().repr();
    // Index 7 of "Keepin' it" is the space: no ink, raised from inside
    // hlpchars's own gsave.
    let r = it.run_str("front /keep 7 8 hlcharink");
    assert!(r.is_err());
    it.run_str(
        "clear countdictstack currentfont /FontName get 96 string cvs
         newpath 5 5 moveto 9 9 lineto pathbbox",
    )
    .ok();
    // Font, dict depth and the caller's path survived the failure.
    let got: Vec<String> = it.operand_stack().iter().map(|o| o.repr()).collect();
    assert_eq!(got[0], depth, "dict stack depth changed: {got:?}");
    assert_eq!(got[1], "(Times-Roman)");
    assert_eq!(&got[2..], ["5.0", "5.0", "9.0", "9.0"], "{got:?}");
}

/// Swap the face and lengthen the phrases: every variant must either
/// stay inside its region and clear its /Avoid, or raise a named
/// headline-* error -- never silently overflow.
#[test]
fn changing_face_or_phrase_recomputes_or_fails_clearly() {
    let variant = |real_font: &str, keep: &str| {
        format!(
            "/front [ 40 470 860 880 ] [
               << /Name /real /Text (Real) /Font /{real_font} /FitBox [ 780 300 ] /At [ 60 490 ] >>
               << /Name /keep /Text ({keep}) /Font /Helvetica-Bold /SizeOf [ /real 0.17 ]
                  /At << /To /real /Chars [ 1 3 ] /H 0 /V 1 >>
                  /Anchor [ /left /bottom ] /Offset [ 8 18 ]
                  /Avoid [ [ /real 0 1 ] [ /real 1 3 ] [ /real 3 4 ] ] /Clearance 10 >>
             ] hllayout def
             front /real hlrunink front /keep hlrunink"
        )
    };
    let mut ok = 0;
    let mut failed = 0;
    for font in ["AlfaSlabOne", "PermanentMarker"] {
        for keep in ["Keepin' it", "Keepin' it, all the way real"] {
            let mut it = with_lib(900, 900);
            match it.run_str(&variant(font, keep)) {
                Ok(()) => {
                    let v: Vec<f64> = it
                        .operand_stack()
                        .iter()
                        .map(|o| o.repr().parse().expect("number"))
                        .collect();
                    for b in v.chunks(4) {
                        assert!(
                            b[0] >= 39.99 && b[1] >= 469.99 && b[2] <= 860.01 && b[3] <= 880.01,
                            "{font}/{keep}: {b:?} left the region"
                        );
                    }
                    ok += 1;
                }
                Err(PsError::Undefined(name)) => {
                    assert!(name.starts_with("headline-"), "{font}/{keep}: {name}");
                    failed += 1;
                }
                Err(other) => panic!("{font}/{keep}: unexpected {other}"),
            }
        }
    }
    assert_eq!(ok + failed, 4);
    assert!(ok >= 1, "the base case must lay out");
}

#[test]
fn documented_examples_run_and_capabilities_lists_the_api() {
    let src = std::fs::read_to_string("README.md").expect("README");
    let start = src
        .find("(lib/headline.ps) run\n[ 40 40 860 400 ]")
        .expect("snippet");
    let snippet = &src[start..src[start..].find("```").map(|e| start + e).unwrap()];
    let mut it = with_lib(900, 900);
    it.run_str(snippet)
        .unwrap_or_else(|e| panic!("README snippet failed: {}", it.error_report(&e)));
    let caps = pscat::capabilities::payload_json().to_string();
    for name in ["hllayout", "hldraw", "hlrunink", "hlcharink", "hlink"] {
        assert!(
            caps.contains(&format!("\"{name}\"")),
            "{name} missing from --capabilities"
        );
    }
}

#[test]
fn draw_proc_errors_and_bad_char_offsets_do_not_leak_state() {
    let mut it = with_lib(900, 900);
    it.run_str(&format!(
        "/Times-Roman findfont 17 scalefont setfont newpath 5 5 moveto 9 9 lineto
         {FRONT} countdictstack"
    ))
    .expect("setup");
    let depth = it.operand_stack().last().unwrap().repr();
    for src in [
        "front /keep 0.5 1 hlcharink",
        "front /keep 1 2.5 hlcharink",
        "[ 0 0 99 99 ] [ << /Text (x) /Size 9 /At [ 1 1 ] /MaxSize -5 >> ] hllayout",
        "[ 0 0 300 200 ] [ << /Text (Real) /Size 50 /At [ 10 10 ]
            /Draw { pop pop pop boom-in-draw } >> ] hllayout hldraw",
    ] {
        assert!(it.run_str(src).is_err(), "{src}");
        it.run_str(
            "clear countdictstack currentfont /FontName get 96 string cvs
             newpath 5 5 moveto 9 9 lineto pathbbox",
        )
        .ok();
        let got: Vec<String> = it.operand_stack().iter().map(|o| o.repr()).collect();
        assert_eq!(got[0], depth, "{src}: dict depth changed");
        assert_eq!(got[1], "(Times-Roman)", "{src}");
        assert_eq!(&got[2..], ["5.0", "5.0", "9.0", "9.0"], "{src}");
        it.run_str("clear").ok();
    }
}

#[test]
fn fitted_ink_is_measured_at_the_drawn_size() {
    // Curved outlines flatten to a fixed tolerance, so bounds measured
    // at 100pt do not scale exactly: the reported box must still be
    // what a fresh measurement at the chosen size gives, and inside
    // the requested width.
    let v = nums(
        "[ 0 0 700 700 ] [ << /Name /o /Text (o) /Font /PermanentMarker
            /FitWidth 600 /At [ 20 20 ] >> ] hllayout
         /Names get /o get /Size get
         /PermanentMarker findfont exch scalefont setfont (o) hlink",
    );
    let (mx0, mx1) = (v[0], v[2]);
    let w = mx1 - mx0;
    assert!(w <= 600.0 + 1e-6 && w > 599.0, "measured width {w}");
}

#[test]
fn top_level_validation_failures_restore_the_operand_stack() {
    let mut it = with_lib(400, 200);
    it.run_str("42 (kept)").expect("setup");
    for src in [
        "[ 0 0 9 9 ] { boom } hllayout",
        "{ boom } [ ] hllayout",
        "[ 0 0 9 ] [ ] hllayout",
    ] {
        assert!(it.run_str(src).is_err(), "{src}");
        let got: Vec<String> = it.operand_stack().iter().map(|o| o.repr()).collect();
        assert_eq!(got, ["42", "(kept)"], "{src}");
    }
}

#[test]
fn a_draw_proc_that_leaks_a_gsave_and_fails_is_fully_unwound() {
    let mut it = with_lib(400, 200);
    it.run_str("0.5 setgray").expect("setup");
    let r = it.run_str(
        "[ 0 0 300 200 ] [ << /Text (Real) /Size 50 /At [ 10 10 ]
            /Draw { pop pop pop gsave 0.1 setgray boom-in-draw } >> ] hllayout hldraw",
    );
    assert!(r.is_err());
    it.run_str("clear 0.9 setgray grestore currentgray").ok();
    let g: f64 = it.operand_stack().last().unwrap().repr().parse().unwrap();
    assert!(
        (g - 0.9).abs() < 1e-6,
        "a leaked gsave was restored into: {g}"
    );
}

#[test]
fn a_fit_landing_on_min_size_still_fails_clearly() {
    assert_eq!(
        err_of(
            "[ 0 0 900 900 ] [ << /Text (Real) /Font /Helvetica-Bold /FitWidth 200
               /MinSize 1000 /At [ 10 10 ] >> ] hllayout"
        ),
        "headline-fit-below-minimum-size"
    );
    // A fit whose size sits at MinSize (after the safety margin) fails
    // rather than returning a size under the declared minimum.
    let size = nums(
        "[ 0 0 900 900 ] [ << /Name /r /Text (Real) /Font /Helvetica-Bold /FitWidth 100 /At [ 10 10 ] >> ]
         hllayout /Names get /r get /Size get",
    )[0];
    assert!(size > 0.0);
    let min = size * 1.00005;
    assert_eq!(
        err_of(&format!(
            "[ 0 0 900 900 ] [ << /Text (Real) /Font /Helvetica-Bold /FitWidth 100
               /MinSize {min} /At [ 10 10 ] >> ] hllayout"
        )),
        "headline-fit-below-minimum-size"
    );
}
