//! The reusable hatching library (issue #49, `lib/hatchkit.ps`): a
//! single operator, `hatch`, that fills whatever region is currently
//! clipped with parallel line strokes. Geometry (candidate line and
//! sample counts) is fully deterministic from /BBox/Spacing/Angles, so
//! the pre-flight safety limits are asserted on directly; anything
//! that actually draws is asserted on ink coverage, this codebase's
//! usual corpus policy for rand-driven output (see tests/artkit.rs's
//! scatter tests, the closest sibling to this one).

use pscat::{Interp, PsError};

fn with_lib(w: u32, h: u32) -> Interp {
    let lib = std::fs::read("lib/hatchkit.ps").expect("library present");
    let mut it = Interp::with_page(w, h).expect("page");
    it.run_source(&lib)
        .unwrap_or_else(|e| panic!("hatchkit.ps failed: {}", it.error_report(&e)));
    it
}

fn run(it: &mut Interp, src: &str) {
    it.run_str(src)
        .unwrap_or_else(|e| panic!("eval of {src:?} failed: {}", it.error_report(&e)));
    // `hatch`'s own contract is `opts hatch -`; a leftover operand
    // here would be exactly the kind of leak (a /Density proc that
    // doesn't consume both its operands, say) HANDOFF.md records
    // `--lint` catching in `et-hatch` and `tfdrawline` -- assert it
    // directly rather than relying on `--lint` being run separately.
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
/// throughout (`hatch-spacing-must-be-positive` and friends) -- same
/// helper shape as tests/artkit.rs's `scatter_err`.
fn hatch_err(src: &str) -> String {
    let mut it = with_lib(100, 100);
    match it.run_str(src).unwrap_err() {
        PsError::Undefined(name) => name,
        other => panic!("expected a self-documenting undefined name, got {other}"),
    }
}

#[test]
fn hatchkit_loads_without_drawing_anything() {
    let it = with_lib(100, 100);
    assert_eq!(ink_count(&it), 0, "loading hatchkit put ink on the page");
    assert!(!it.gfx().page_shown);
    assert!(
        it.operand_stack().is_empty(),
        "loading hatchkit left {:?} on the operand stack",
        it.operand_stack()
            .iter()
            .map(|o| o.repr())
            .collect::<Vec<_>>()
    );
}

#[test]
fn flat_hatch_fills_the_clip_and_nothing_outside_it() {
    let mut it = with_lib(200, 200);
    run(
        &mut it,
        "0 0 0 setrgbcolor 1 setlinecap \
         newpath 40 40 moveto 160 40 lineto 160 160 lineto 40 160 lineto closepath clip \
         << /Angle 30 /Spacing 4 /Seed 1 >> hatch",
    );
    assert!(ink_count(&it) > 0, "hatch drew nothing inside the clip");

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
fn hatch_clips_to_a_concave_region() {
    // An arrow/chevron: concave, and its bounding box's own corners
    // (near (0,0) and (100,100)) sit outside the shape entirely --
    // exactly the case a bbox-only region would get wrong, and the
    // one `hatch` never attempts on its own (it leans on the real
    // `clip`, see the library's own header).
    let mut it = with_lib(120, 120);
    run(
        &mut it,
        "0 0 0 setrgbcolor 1 setlinecap \
         newpath 10 10 moveto 50 90 lineto 90 10 lineto 50 40 lineto closepath clip \
         << /Angle 10 /Spacing 2 /Seed 4 >> hatch",
    );
    assert!(ink_count(&it) > 0, "nothing drawn inside the chevron");

    let pm = &it.gfx().pixmap;
    // (5,5) and (115,115), in device pixels, sit outside the bbox
    // entirely -- a trivial check the clip is active at all.
    assert!(pm.pixel(5, 5).unwrap().red() >= 250);
    assert!(pm.pixel(115, 115).unwrap().red() >= 250);
    // The bbox corners near (10,90) [top-left-ish] and (90,90)
    // [top-right-ish] in user space sit in the bbox but outside the
    // chevron's own notch -- the concave case a bbox-only region
    // would wrongly ink. User (12,88) -> device (12, 120-88=32).
    assert!(
        pm.pixel(12, 32).unwrap().red() >= 250,
        "ink landed outside the concave chevron, inside its bbox"
    );
}

#[test]
fn seeded_hatch_is_reproducible() {
    let src = "0 0 0 setrgbcolor 1 setlinecap \
        newpath 10 10 moveto 90 10 lineto 90 90 lineto 10 90 lineto closepath clip \
        << /Angle 25 /Spacing 5 /Seed 7 /Wobble 1.2 /Dropout 0.2 /Width [0.5 2] /Trim [0.1 0.3] >> hatch";
    let mut a = with_lib(100, 100);
    run(&mut a, src);
    let mut b = with_lib(100, 100);
    run(&mut b, src);
    assert_eq!(
        a.gfx().pixmap.data(),
        b.gfx().pixmap.data(),
        "same seed and options should reproduce pixel-for-pixel"
    );
}

#[test]
fn different_seeds_produce_different_arrangements() {
    let src = |seed: u32| {
        format!(
            "0 0 0 setrgbcolor 1 setlinecap \
             newpath 10 10 moveto 90 10 lineto 90 90 lineto 10 90 lineto closepath clip \
             << /Angle 25 /Spacing 5 /Seed {seed} /Wobble 1.5 /Dropout 0.3 >> hatch"
        )
    };
    let mut a = with_lib(100, 100);
    run(&mut a, &src(1));
    let mut b = with_lib(100, 100);
    run(&mut b, &src(2));
    assert_ne!(
        a.gfx().pixmap.data(),
        b.gfx().pixmap.data(),
        "different seeds should draw different arrangements"
    );
}

#[test]
fn bbox_default_matches_an_explicit_bbox_of_the_same_path() {
    let mut a = with_lib(100, 100);
    run(
        &mut a,
        "0 0 0 setrgbcolor 1 setlinecap \
         newpath 10 10 moveto 90 10 lineto 90 90 lineto 10 90 lineto closepath clip \
         << /Angle 25 /Spacing 5 /Seed 7 >> hatch",
    );
    let mut b = with_lib(100, 100);
    run(
        &mut b,
        "0 0 0 setrgbcolor 1 setlinecap \
         newpath 10 10 moveto 90 10 lineto 90 90 lineto 10 90 lineto closepath clip \
         << /BBox [10 10 90 90] /Angle 25 /Spacing 5 /Seed 7 >> hatch",
    );
    assert_eq!(
        a.gfx().pixmap.data(),
        b.gfx().pixmap.data(),
        "defaulting /BBox from pathbbox should match passing the same box explicitly"
    );
}

#[test]
fn dropout_reduces_ink() {
    let src = |dropout: f64| {
        format!(
            "0 0 0 setrgbcolor 1 setlinecap \
             newpath 10 10 moveto 90 10 lineto 90 90 lineto 10 90 lineto closepath clip \
             << /Angle 30 /Spacing 3 /Seed 5 /Dropout {dropout} >> hatch"
        )
    };
    let mut none = with_lib(100, 100);
    run(&mut none, &src(0.0));
    let mut most = with_lib(100, 100);
    run(&mut most, &src(0.85));
    assert!(
        ink_count(&most) < ink_count(&none) / 2,
        "high dropout ({}) should draw noticeably less ink than none ({})",
        ink_count(&most),
        ink_count(&none)
    );
}

#[test]
fn angles_layers_each_pass() {
    let mut single = with_lib(100, 100);
    run(
        &mut single,
        "0 0 0 setrgbcolor 1 setlinecap \
         newpath 10 10 moveto 90 10 lineto 90 90 lineto 10 90 lineto closepath clip \
         << /Angle 0 /Spacing 6 /Seed 9 >> hatch",
    );
    let mut cross = with_lib(100, 100);
    run(
        &mut cross,
        "0 0 0 setrgbcolor 1 setlinecap \
         newpath 10 10 moveto 90 10 lineto 90 90 lineto 10 90 lineto closepath clip \
         << /Angles [0 90] /Spacing 6 /Seed 9 >> hatch",
    );
    assert!(
        ink_count(&cross) > ink_count(&single),
        "a two-angle /Angles pass should ink more than a single angle"
    );
}

#[test]
fn density_at_or_below_threshold_draws_no_ink() {
    // A hard left/right split: x < 50 gets density 0 (at the default
    // /DensityThreshold, no ink at all), x >= 50 gets density 1.
    let mut it = with_lib(100, 100);
    run(
        &mut it,
        "0 0 0 setrgbcolor 1 setlinecap \
         newpath 10 10 moveto 90 10 lineto 90 90 lineto 10 90 lineto closepath clip \
         << /Angle 20 /Spacing 2 /Seed 3 /Width [0.5 2] \
            /Density { pop 50 lt { 0.0 } { 1.0 } ifelse } >> hatch",
    );
    let pm = &it.gfx().pixmap;
    let mut left = 0usize;
    let mut right = 0usize;
    for y in 0..100u32 {
        for x in 0..100u32 {
            let dark = pm.pixel(x, y).unwrap().red() < 250;
            if x < 45 {
                if dark {
                    left += 1;
                }
            } else if x >= 55 && dark {
                right += 1;
            }
        }
    }
    assert_eq!(left, 0, "the below-threshold half should have no ink");
    assert!(right > 0, "the above-threshold half should be inked");
}

#[test]
fn density_return_value_is_clamped_not_rejected() {
    // A callback returning wildly out-of-range numbers must not error
    // or hang -- only ever change tone, never the amount of geometry
    // attempted (the library's own documented bound on /Density).
    let mut high = with_lib(60, 60);
    run(
        &mut high,
        "0 0 0 setrgbcolor 1 setlinecap \
         newpath 5 5 moveto 55 5 lineto 55 55 lineto 5 55 lineto closepath clip \
         << /Angle 15 /Spacing 3 /Seed 2 /Width [0.5 2] /DensityThreshold 0.9 \
            /Density { pop pop 999 } >> hatch",
    );
    assert!(
        ink_count(&high) > 0,
        "a callback returning far above 1 should clamp to full ink, not vanish"
    );

    let mut low = with_lib(60, 60);
    run(
        &mut low,
        "0 0 0 setrgbcolor 1 setlinecap \
         newpath 5 5 moveto 55 5 lineto 55 55 lineto 5 55 lineto closepath clip \
         << /Angle 15 /Spacing 3 /Seed 2 /Width [0.5 2] \
            /Density { pop pop -999 } >> hatch",
    );
    assert_eq!(
        ink_count(&low),
        0,
        "a callback returning far below 0 should clamp to no ink, not error"
    );
}

#[test]
fn density_proc_that_leaks_an_operand_is_visible_on_the_stack() {
    // /Density's contract (the library's own docs) is the same one
    // scatter's /Mark and /Weight carry: it must consume both of its
    // operands. A proc that only pops one leaks the other per sample
    // -- not silently swallowed by `hatch`, which is what makes
    // `--lint` able to catch it (HANDOFF.md records real leaks of
    // exactly this shape it found in et-hatch and tfdrawline).
    let mut it = with_lib(60, 60);
    it.run_str(
        "newpath 5 5 moveto 55 5 lineto 55 55 lineto 5 55 lineto closepath clip \
         << /Angle 15 /Spacing 3 /Seed 2 /Density { pop 0.5 } >> hatch",
    )
    .expect("a leaking Density proc should not itself error");
    assert!(
        !it.operand_stack().is_empty(),
        "a Density proc that only pops one operand should leave the other behind"
    );
}

#[test]
fn spacing_must_be_positive() {
    assert_eq!(
        hatch_err(
            "newpath 0 0 moveto 10 0 lineto 10 10 lineto closepath clip << /Spacing 0 >> hatch"
        ),
        "hatch-spacing-must-be-positive"
    );
    assert_eq!(
        hatch_err(
            "newpath 0 0 moveto 10 0 lineto 10 10 lineto closepath clip << /Spacing -2 >> hatch"
        ),
        "hatch-spacing-must-be-positive"
    );
}

#[test]
fn degenerate_bbox_is_rejected() {
    assert_eq!(
        hatch_err("<< /BBox [10 10 10 50] >> hatch"),
        "hatch-bbox-degenerate",
        "zero width"
    );
    assert_eq!(
        hatch_err("<< /BBox [10 50 50 10] >> hatch"),
        "hatch-bbox-degenerate",
        "inverted y"
    );
}

#[test]
fn density_must_be_a_callable_proc() {
    assert_eq!(
        hatch_err("<< /BBox [0 0 10 10] /Density 3 >> hatch"),
        "hatch-density-must-be-callable"
    );
}

#[test]
fn max_lines_rejects_before_drawing_anything() {
    let mut it = with_lib(100, 100);
    let err = it
        .run_str("<< /BBox [0 0 1000 1000] /Spacing 1 /MaxLines 10 >> hatch")
        .unwrap_err();
    match err {
        PsError::Undefined(name) => assert_eq!(name, "hatch-line-count-exceeds-safety-limit"),
        other => panic!("expected a self-documenting undefined name, got {other}"),
    }
    assert_eq!(
        ink_count(&it),
        0,
        "a rejected /MaxLines call must not have drawn anything first"
    );
}

#[test]
fn max_samples_rejects_before_drawing_anything() {
    let mut it = with_lib(100, 100);
    let err = it
        .run_str(
            "<< /BBox [0 0 1000 1000] /Spacing 20 /MaxSamples 5 \
              /Density { pop pop 1 } >> hatch",
        )
        .unwrap_err();
    match err {
        PsError::Undefined(name) => assert_eq!(name, "hatch-sample-count-exceeds-safety-limit"),
        other => panic!("expected a self-documenting undefined name, got {other}"),
    }
    assert_eq!(
        ink_count(&it),
        0,
        "a rejected /MaxSamples call must not have drawn anything first"
    );
}

#[test]
fn ghostscript_accepts_the_hatching_specimen_sheet() {
    // The acceptance criterion itself -- the specimen page runs
    // unchanged in both interpreters. `-dNOSAFER` is needed because
    // the file does `(lib/hatchkit.ps) run` from disk, which gs's
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
            "-g900x360",
            "-r72",
            "-o/dev/null",
            "examples/hatching.ps",
        ])
        .status()
        .expect("run gs");
    assert!(status.success(), "gs rejected examples/hatching.ps");
}
