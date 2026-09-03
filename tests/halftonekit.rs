//! The reusable halftone library (issue #53, `lib/halftonekit.ps`): a
//! single operator, `halftone`, that fills whatever region is currently
//! clipped with a regular dot, line, or cross-line screen, sized per
//! cell by a tone and shiftable per layer by a registration offset.
//! Lattice geometry is fully deterministic from /BBox/Frequency/Angle,
//! so the pre-flight cell budget is asserted on directly; anything
//! that actually draws is asserted on ink coverage (the corpus policy
//! for sibling-library tests, see tests/hatchkit.rs).

use pscat::{Interp, PsError};

fn with_lib(w: u32, h: u32) -> Interp {
    let lib = std::fs::read("lib/halftonekit.ps").expect("library present");
    let mut it = Interp::with_page(w, h).expect("page");
    it.run_source(&lib)
        .unwrap_or_else(|e| panic!("halftonekit.ps failed: {}", it.error_report(&e)));
    it
}

fn run(it: &mut Interp, src: &str) {
    it.run_str(src)
        .unwrap_or_else(|e| panic!("eval of {src:?} failed: {}", it.error_report(&e)));
    // `halftone`'s own contract is `opts halftone -`; a leftover
    // operand here would be exactly the kind of leak (a /Tone proc
    // that doesn't consume both its operands) `--lint` catches
    // elsewhere -- assert it directly rather than relying on `--lint`
    // being run separately.
    assert!(
        it.operand_stack().is_empty(),
        "{src:?} left {:?} on the operand stack",
        it.operand_stack()
            .iter()
            .map(|o| o.repr())
            .collect::<Vec<_>>()
    );
}

fn ink_count(it: &Interp) -> usize {
    it.gfx()
        .pixmap
        .pixels()
        .iter()
        .filter(|p| p.red() < 128)
        .count()
}

/// The self-documenting-undefined-name error idiom this codebase uses
/// throughout (`halftone-frequency-must-be-positive` and friends) --
/// same helper shape as tests/hatchkit.rs's `hatch_err`.
fn halftone_err(src: &str) -> String {
    let mut it = with_lib(100, 100);
    match it.run_str(src).unwrap_err() {
        PsError::Undefined(name) => name,
        other => panic!("expected a self-documenting undefined name, got {other}"),
    }
}

fn pixbuf(it: &Interp) -> Vec<u8> {
    it.gfx().pixmap.data().to_vec()
}

#[test]
fn halftonekit_loads_without_drawing_anything() {
    let it = with_lib(100, 100);
    assert_eq!(ink_count(&it), 0, "loading halftonekit put ink on the page");
    assert!(!it.gfx().page_shown);
    assert!(
        it.operand_stack().is_empty(),
        "loading halftonekit left {:?} on the operand stack",
        it.operand_stack()
            .iter()
            .map(|o| o.repr())
            .collect::<Vec<_>>()
    );
}

#[test]
fn flat_dot_fills_the_clip_and_nothing_outside_it() {
    let mut it = with_lib(200, 200);
    run(
        &mut it,
        "0 0 0 setrgbcolor \
         newpath 40 40 moveto 160 40 lineto 160 160 lineto 40 160 lineto closepath clip \
         << /Screen /dot /Frequency 12 /Angle 0 /Tone 0.5 >> halftone",
    );
    assert!(ink_count(&it) > 0, "halftone drew nothing inside the clip");

    let pm = &it.gfx().pixmap;
    let w = pm.width();
    for (i, p) in pm.pixels().iter().enumerate() {
        let x = (i as u32) % w;
        let y = (i as u32) / w;
        let outside = !(40..160).contains(&x) || !(40..160).contains(&y);
        if outside {
            assert!(
                p.red() >= 250,
                "ink at ({x},{y}), outside the [40,160)x[40,160) clip"
            );
        }
    }
}

#[test]
fn halftone_clips_to_a_concave_region() {
    // An arrow/chevron: concave, and its bounding box's own corners
    // sit outside the shape entirely -- exactly the case a bbox-only
    // region would get wrong, and the one `halftone` never attempts
    // on its own (it leans on the real `clip`, see the library's own
    // header).
    let mut it = with_lib(120, 120);
    run(
        &mut it,
        "0 0 0 setrgbcolor \
         newpath 10 10 moveto 50 90 lineto 90 10 lineto 50 40 lineto closepath clip \
         << /Screen /dot /Frequency 12 /Angle 0 /Tone 1 >> halftone",
    );
    assert!(ink_count(&it) > 0, "nothing drawn inside the chevron");

    let pm = &it.gfx().pixmap;
    assert!(pm.pixel(5, 5).unwrap().red() >= 250);
    assert!(pm.pixel(115, 115).unwrap().red() >= 250);
}

#[test]
fn same_options_reproduce_identical_pixels() {
    // No random draws anywhere in the operator -- not even a /Seed
    // option -- so this must hold trivially, including with a
    // caller-supplied /Tone callback in the loop.
    let src = "0 0 0 setrgbcolor \
        newpath 10 10 moveto 90 10 lineto 90 90 lineto 10 90 lineto closepath clip \
        << /Screen /cross /Frequency 8 /Angle 25 /Tone { add 200 div } >> halftone";
    let mut a = with_lib(100, 100);
    run(&mut a, src);
    let mut b = with_lib(100, 100);
    run(&mut b, src);
    assert_eq!(
        pixbuf(&a),
        pixbuf(&b),
        "same options should reproduce pixel-for-pixel"
    );
}

#[test]
fn the_three_screens_are_visually_distinct() {
    // Same region, same flat mid tone: a dot screen covers ~39% of
    // the area (pi/4 times the tone), a line screen ~50% plus its
    // round-cap seams, a cross screen roughly both directions at
    // once. Wide bands, not exact counts -- the point is three
    // clearly separated ink levels, the issue's first acceptance
    // criterion.
    let ink = |screen: &str| {
        let mut it = with_lib(200, 200);
        run(
            &mut it,
            &format!(
                "0 0 0 setrgbcolor \
                 newpath 20 20 moveto 180 20 lineto 180 180 lineto 20 180 lineto closepath clip \
                 << /Screen /{screen} /Frequency 12 /Angle 0 /Tone 0.5 >> halftone"
            ),
        );
        ink_count(&it)
    };
    let (dot, line, cross) = (ink("dot"), ink("line"), ink("cross"));
    assert!(
        (5000..11000).contains(&dot),
        "dot screen ink {dot} outside its band"
    );
    assert!(
        (11000..17500).contains(&line),
        "line screen ink {line} outside its band"
    );
    assert!(cross > 19000, "cross screen ink {cross} too low");
    assert!(
        dot < line && line < cross,
        "screens not ordered dot ({dot}) < line ({line}) < cross ({cross})"
    );
}

#[test]
fn zero_offset_matches_an_omitted_offset() {
    // "Misregistration can be disabled exactly": the default path and
    // an explicit [0 0] are the same `translate`, not two branches.
    let clip = "0 0 0 setrgbcolor \
        newpath 10 10 moveto 90 10 lineto 90 90 lineto 10 90 lineto closepath clip";
    let mut a = with_lib(100, 100);
    run(
        &mut a,
        &format!("{clip} << /Screen /line /Frequency 10 /Tone 0.6 >> halftone"),
    );
    let mut b = with_lib(100, 100);
    run(
        &mut b,
        &format!("{clip} << /Screen /line /Frequency 10 /Tone 0.6 /Offset [0 0] >> halftone"),
    );
    assert_eq!(
        pixbuf(&a),
        pixbuf(&b),
        "omitted /Offset must match an explicit [0 0]"
    );
}

#[test]
fn a_nonzero_offset_visibly_shifts_the_screen() {
    let clip = "0 0 0 setrgbcolor \
        newpath 10 10 moveto 90 10 lineto 90 90 lineto 10 90 lineto closepath clip";
    let mut a = with_lib(100, 100);
    run(
        &mut a,
        &format!("{clip} << /Screen /dot /Frequency 12 /Tone 0.6 /Offset [0 0] >> halftone"),
    );
    let mut b = with_lib(100, 100);
    run(
        &mut b,
        &format!("{clip} << /Screen /dot /Frequency 12 /Tone 0.6 /Offset [4 0] >> halftone"),
    );
    let (pa, pb) = (pixbuf(&a), pixbuf(&b));
    assert_eq!(pa.len(), pb.len());
    let diff = pa.iter().zip(pb.iter()).filter(|(x, y)| x != y).count();
    assert!(
        diff > 200,
        "a 4-unit /Offset should shift marks visibly, only {diff} bytes differ"
    );
}

#[test]
fn maxcells_rejects_before_any_ink_lands() {
    let mut it = with_lib(200, 200);
    let err = it
        .run_str(
            "0 0 0 setrgbcolor \
             newpath 20 20 moveto 180 20 lineto 180 180 lineto 20 180 lineto closepath clip \
             << /Screen /dot /Frequency 400 /MaxCells 10 >> halftone",
        )
        .unwrap_err();
    assert!(
        matches!(err, PsError::Undefined(ref n) if n == "halftone-maxcells-exceeded"),
        "wrong error: {err:?}"
    );
    assert_eq!(ink_count(&it), 0, "a rejected call must draw nothing first");
}

#[test]
fn tone_clamps_out_of_range_returns() {
    // A constant above 1 behaves as full tone, pixel-for-pixel; a
    // constant below 0 behaves as zero tone (blank).
    let clip = "0 0 0 setrgbcolor \
        newpath 10 10 moveto 90 10 lineto 90 90 lineto 10 90 lineto closepath clip";
    let mut a = with_lib(100, 100);
    run(
        &mut a,
        &format!("{clip} << /Screen /dot /Frequency 12 /Tone 1 >> halftone"),
    );
    let mut b = with_lib(100, 100);
    run(
        &mut b,
        &format!("{clip} << /Screen /dot /Frequency 12 /Tone {{ exch pop pop 5 }} >> halftone"),
    );
    assert_eq!(
        pixbuf(&a),
        pixbuf(&b),
        "a /Tone callback returning 5 must clamp to full tone"
    );

    let mut c = with_lib(100, 100);
    run(
        &mut c,
        &format!("{clip} << /Screen /dot /Frequency 12 /Tone {{ exch pop pop -1 }} >> halftone"),
    );
    assert_eq!(ink_count(&c), 0, "a fully negative tone must draw nothing");
}

#[test]
fn bbox_defaults_to_pathbbox() {
    let clip = "0 0 0 setrgbcolor \
        newpath 10 10 moveto 90 10 lineto 90 90 lineto 10 90 lineto closepath clip";
    let mut a = with_lib(100, 100);
    run(
        &mut a,
        &format!("{clip} << /Screen /line /Frequency 10 /Angle 20 /Tone 0.5 >> halftone"),
    );
    let mut b = with_lib(100, 100);
    run(
        &mut b,
        &format!(
            "{clip} << /Screen /line /Frequency 10 /Angle 20 /Tone 0.5 /BBox [10 10 90 90] >> halftone"
        ),
    );
    assert_eq!(
        pixbuf(&a),
        pixbuf(&b),
        "default /BBox must match an explicit pathbbox"
    );
}

#[test]
fn a_string_screen_name_selects_by_content() {
    // `/Screen (dot)` works exactly like `/Screen /dot`: screen
    // selection uses the language's own `eq`, which compares a
    // string and a name by content (Ghostscript agrees).
    let clip = "0 0 0 setrgbcolor \
        newpath 10 10 moveto 90 10 lineto 90 90 lineto 10 90 lineto closepath clip";
    let mut a = with_lib(100, 100);
    run(
        &mut a,
        &format!("{clip} << /Screen /dot /Frequency 12 /Tone 0.5 >> halftone"),
    );
    let mut b = with_lib(100, 100);
    run(
        &mut b,
        &format!("{clip} << /Screen (dot) /Frequency 12 /Tone 0.5 >> halftone"),
    );
    assert_eq!(
        pixbuf(&a),
        pixbuf(&b),
        "`(dot)` must select the same screen as `/dot`"
    );
}

#[test]
fn malformed_bboxes_are_rejected() {
    assert_eq!(
        halftone_err("<< /BBox [10 10 10] /Tone 1 >> halftone"),
        "halftone-bbox-must-have-four-elements"
    );
    assert_eq!(
        halftone_err("<< /BBox [10 10 90 90 100] /Tone 1 >> halftone"),
        "halftone-bbox-must-have-four-elements"
    );
    assert_eq!(
        halftone_err("<< /BBox [10 10 10 20] /Tone 1 >> halftone"),
        "halftone-bbox-degenerate"
    );
    assert_eq!(
        halftone_err("<< /BBox [20 20 10 10] /Tone 1 >> halftone"),
        "halftone-bbox-degenerate"
    );
}

#[test]
fn option_validation_reports_self_documenting_errors() {
    let bad = |opts: &str| {
        halftone_err(&format!(
            "newpath 0 0 moveto 50 0 lineto 50 50 lineto closepath clip << /BBox [0 0 50 50] {opts} >> halftone"
        ))
    };
    assert_eq!(
        bad("/Screen /stipple"),
        "halftone-screen-must-be-dot-line-or-cross"
    );
    // `eq` compares a string and a name by content (Ghostscript
    // agrees), so only genuinely unequal values are rejected.
    assert_eq!(
        bad("/Screen (dots)"),
        "halftone-screen-must-be-dot-line-or-cross"
    );
    assert_eq!(
        bad("/Screen 5"),
        "halftone-screen-must-be-dot-line-or-cross"
    );
    assert_eq!(bad("/Frequency 0"), "halftone-frequency-must-be-positive");
    assert_eq!(bad("/Frequency -3"), "halftone-frequency-must-be-positive");
    assert_eq!(
        bad("/Frequency (fast)"),
        "halftone-frequency-must-be-a-number"
    );
    assert_eq!(bad("/Angle (steep)"), "halftone-angle-must-be-a-number");
    assert_eq!(
        bad("/Tone (loud)"),
        "halftone-tone-must-be-a-number-or-procedure"
    );
    assert_eq!(
        bad("/Screen /dot /MaxRadius 0"),
        "halftone-maxradius-must-be-positive"
    );
    assert_eq!(
        bad("/Screen /dot /MaxRadius (huge)"),
        "halftone-maxradius-must-be-a-number"
    );
    assert_eq!(
        bad("/Screen /line /MaxWidth 0"),
        "halftone-maxwidth-must-be-positive"
    );
    assert_eq!(
        bad("/Offset [1 2 3]"),
        "halftone-offset-must-be-a-two-element-array"
    );
    assert_eq!(
        bad("/Offset [1]"),
        "halftone-offset-must-be-a-two-element-array"
    );
    assert_eq!(
        bad("/Offset (flat)"),
        "halftone-offset-must-be-a-two-element-array"
    );
    assert_eq!(
        bad("/Offset [0 (up)]"),
        "halftone-offset-must-be-a-two-element-array"
    );
    assert_eq!(bad("/MaxCells 0"), "halftone-maxcells-must-be-positive");
    assert_eq!(
        bad("/MaxCells (many)"),
        "halftone-maxcells-must-be-a-number"
    );
    assert_eq!(
        halftone_err("/hfbogus { pop } def /hfbogus load halftone"),
        "halftone-opts-must-be-a-dict"
    );
}

/// A caller-supplied procedure in a spot that must hold a dict must be
/// rejected, never executed: `get` doesn't execute, but a bare name
/// bound to a proc does, so the options operand travels boxed until
/// it has proven to be a dict.
#[test]
fn a_procedure_for_opts_is_rejected_never_executed() {
    let mut it = with_lib(100, 100);
    let err = it
        .run_str("/hfevilopts { /hfeviloptran true def } def /hfevilopts load halftone")
        .unwrap_err();
    assert!(
        matches!(err, PsError::Undefined(ref n) if n == "halftone-opts-must-be-a-dict"),
        "wrong error: {err:?}"
    );
    // `run` asserts an empty stack, so the boolean probe below goes
    // through bare `run_str` and reads the stack directly instead.
    // (Boolean reprs are bare `true`/`false`.)
    it.run_str("/hfeviloptran where { pop true } { false } ifelse")
        .expect("probe");
    let last = it.operand_stack().last().expect("probe result").repr();
    assert_eq!(last, "false", "the rejected opts proc must never have run");
    it.run_str("clear").expect("clear");
}

/// Same shape one level down: an executable *name* is not a callable
/// /Tone (only an executable array is), and checking must not run it.
#[test]
fn an_executable_name_for_tone_is_rejected_never_executed() {
    let mut it = with_lib(100, 100);
    let err = it
        .run_str(
            "newpath 0 0 moveto 50 0 lineto 50 50 lineto closepath clip \
             /hfevil { /hfevilran true def } def \
             << /BBox [0 0 50 50] /Tone /hfevil cvx >> halftone",
        )
        .unwrap_err();
    assert!(
        matches!(err, PsError::Undefined(ref n) if n == "halftone-tone-must-be-a-number-or-procedure"),
        "wrong error: {err:?}"
    );
    it.run_str("/hfevilran where { pop true } { false } ifelse")
        .expect("probe");
    let last = it.operand_stack().last().expect("probe result").repr();
    assert_eq!(
        last, "false",
        "the rejected /Tone value must never have run"
    );
    it.run_str("clear").expect("clear");
}

/// And once more for /Screen: a procedure compares, never executes.
#[test]
fn a_procedure_for_screen_is_rejected_never_executed() {
    let mut it = with_lib(100, 100);
    let err = it
        .run_str(
            "newpath 0 0 moveto 50 0 lineto 50 50 lineto closepath clip \
             /hfevils { /hfevilsran true def } def \
             << /BBox [0 0 50 50] /Screen /hfevils load >> halftone",
        )
        .unwrap_err();
    assert!(
        matches!(err, PsError::Undefined(ref n) if n == "halftone-screen-must-be-dot-line-or-cross"),
        "wrong error: {err:?}"
    );
    it.run_str("/hfevilsran where { pop true } { false } ifelse")
        .expect("probe");
    let last = it.operand_stack().last().expect("probe result").repr();
    assert_eq!(
        last, "false",
        "the rejected /Screen proc must never have run"
    );
    it.run_str("clear").expect("clear");
}

#[test]
fn tone_proc_that_leaks_an_operand_is_visible_on_the_stack() {
    // /Tone's contract (the library's own docs) is the same one
    // scatter's /Mark and /Weight carry: it must consume both of its
    // operands. A proc that only pops one leaks the other per cell --
    // not silently swallowed by `halftone`, which is what makes
    // `--lint` able to catch it.
    let mut it = with_lib(60, 60);
    it.run_str(
        "newpath 5 5 moveto 55 5 lineto 55 55 lineto 5 55 lineto closepath clip \
         << /Screen /dot /Frequency 12 /Tone { pop 0.5 } >> halftone",
    )
    .expect("a leaking Tone proc should not itself error");
    assert!(
        !it.operand_stack().is_empty(),
        "a Tone proc that only pops one operand should leave the other behind"
    );
}

#[test]
fn zero_tone_draws_nothing_even_for_line_screens() {
    // Load-bearing, not an optimization: a zero-width stroke is a
    // hairline in PostScript, not nothing, so the line/cross branches
    // must skip zero-tone cells rather than draw through them.
    let mut it = with_lib(200, 200);
    run(
        &mut it,
        "0 0 0 setrgbcolor \
         newpath 20 20 moveto 180 20 lineto 180 180 lineto 20 180 lineto closepath clip \
         << /Screen /line /Frequency 12 /Tone 0 >> halftone",
    );
    assert_eq!(ink_count(&it), 0, "zero tone must draw no marks");
}

#[test]
fn an_unused_malformed_size_option_is_harmless() {
    // /MaxRadius only feeds the /dot branch; a line call carrying a
    // malformed one in a shared dict must not fail for an option it
    // never touches (stipplekit's /DotRadius lesson). Mirror image
    // for /MaxWidth under /dot.
    let mut it = with_lib(100, 100);
    run(
        &mut it,
        "0 0 0 setrgbcolor \
         newpath 10 10 moveto 90 10 lineto 90 90 lineto 10 90 lineto closepath clip \
         << /Screen /line /Frequency 12 /Tone 0.5 /MaxRadius (huge) >> halftone",
    );
    assert!(ink_count(&it) > 0);
    let mut jt = with_lib(100, 100);
    run(
        &mut jt,
        "0 0 0 setrgbcolor \
         newpath 10 10 moveto 90 10 lineto 90 90 lineto 10 90 lineto closepath clip \
         << /Screen /dot /Frequency 12 /Tone 0.5 /MaxWidth -4 >> halftone",
    );
    assert!(ink_count(&jt) > 0);
}

#[test]
fn a_second_layered_call_reuses_the_surviving_path() {
    // The misregistration layering the issue asks for: two calls over
    // one clip, the second relying on the default /BBox from the path
    // the first call's gsave/grestore preserved -- and landing
    // visibly offset from the first plate.
    let clip = "newpath 10 10 moveto 90 10 lineto 90 90 lineto 10 90 lineto closepath clip";
    let mut one = with_lib(100, 100);
    run(
        &mut one,
        &format!(
            "0 0 0 setrgbcolor {clip} \
             << /Screen /dot /Frequency 12 /Tone 0.6 /Offset [0 0] >> halftone"
        ),
    );
    let ink_one = ink_count(&one);
    let mut two = with_lib(100, 100);
    run(
        &mut two,
        &format!(
            "0 0 0 setrgbcolor {clip} \
             << /Screen /dot /Frequency 12 /Tone 0.6 /Offset [0 0] >> halftone \
             0 0 1 setrgbcolor \
             << /Screen /dot /Frequency 12 /Tone 0.6 /Offset [3 2] >> halftone"
        ),
    );
    assert!(
        ink_count(&two) > ink_one,
        "a second offset plate should add ink ({0} vs {ink_one})",
        ink_count(&two)
    );
}

#[test]
fn the_halftone_specimen_sheet_renders_ink_in_every_panel() {
    let source = std::fs::read("examples/halftone.ps").expect("read the specimen");
    let mut it = Interp::with_page(1100, 360).expect("page");
    it.run_source(&source)
        .unwrap_or_else(|e| panic!("examples/halftone.ps failed: {}", it.error_report(&e)));
    assert!(it.gfx().page_shown, "showpage must have run");

    // Inset well clear of each panel's own 0.75pt border stroke --
    // the full 240x240 box includes that frame, which alone
    // contributes marked pixels regardless of whether `halftone`
    // screened anything, so counting it would let a silently broken
    // panel still pass (the false-positive-coverage lesson from
    // tests/stipplekit.rs).
    const INSET: u32 = 10;
    for (label, x0) in [
        ("ramp", 40),
        ("line", 300),
        ("cross", 560),
        ("two-plate", 820),
    ] {
        let mut marked = 0;
        for dx in INSET..(240 - INSET) {
            for dy in INSET..(240 - INSET) {
                let px = x0 + dx;
                let py = 360 - (40 + dy) - 1;
                let p = it
                    .gfx()
                    .pixmap
                    .pixel(px, py)
                    .unwrap_or_else(|| panic!("pixel ({px},{py}) out of bounds"));
                if (p.red(), p.green(), p.blue()) != (255, 255, 255) {
                    marked += 1;
                }
            }
        }
        assert!(
            marked > 200,
            "{label} panel looks unscreened ({marked} non-white interior pixels)"
        );
    }
}

#[test]
fn ghostscript_accepts_the_halftone_specimen_sheet() {
    // The acceptance criterion itself -- the specimen page runs
    // unchanged in both interpreters. `-dNOSAFER` is needed because
    // the file does `(lib/halftonekit.ps) run` from disk, which gs's
    // default sandbox blocks (same reasoning as every other sibling
    // library's own version of this test).
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
            "-dNOSAFER",
            "-dNOPAUSE",
            "-dBATCH",
            "-q",
            "-sDEVICE=png16m",
            "-g1100x360",
            "-r72",
            "-o/dev/null",
            "examples/halftone.ps",
        ])
        .status()
        .expect("run gs");
    assert!(status.success(), "gs rejected examples/halftone.ps");
}
