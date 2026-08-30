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

// The next three tests are regressions from a Codex review of PR #121
// (this issue's own PR): all three were real bypasses of the
// documented safety limits, none caught by the 17 tests above.

#[test]
fn out_of_range_trim_is_rejected() {
    // /Trim [-100 0.1] can draw a *negative* trim fraction, which
    // lengthens a line's span instead of shortening it -- sampling
    // past what /MaxSamples's pre-flight estimate (bounded on the
    // assumption /Trim only ever shrinks) accounted for. Validating
    // the range up front closes that regardless of /MaxSamples.
    assert_eq!(
        hatch_err(
            "<< /BBox [0 0 200 200] /Spacing 20 /Trim [-100 0.1] \
              /Density { pop pop 1 } >> hatch"
        ),
        "hatch-trim-must-be-ordered-fractions-in-0-1"
    );
    assert_eq!(
        hatch_err("<< /BBox [0 0 200 200] /Spacing 20 /Trim [0.6 0.4] >> hatch"),
        "hatch-trim-must-be-ordered-fractions-in-0-1",
        "inverted range (lo > hi)"
    );
}

#[test]
fn mutating_the_angles_array_mid_call_does_not_bypass_max_lines() {
    // The exact shape a Codex review found: /Angles pointing at a
    // caller-owned array, mutated by a /Density callback (called
    // mid-drawing-pass) *after* the pre-flight budget was computed
    // from the array's original contents but *before* the mutated
    // angle would be swept -- with the static [0 45] equivalent
    // rejected by /MaxLines but the live-array version silently
    // sweeping the mutated 45 anyway, unbudgeted. `hatch` now takes a
    // private copy of /Angles up front, so the fix's observable
    // property is that the mutation has *no effect at all*: the call
    // succeeds (the un-mutated [0 0] budget was always within
    // /MaxLines) and draws pixel-identically to a plain, static
    // `/Angles [0 0]` call -- not the [0 45] cross-hatch a successful
    // mutation would have produced.
    let mut mutated = with_lib(100, 100);
    run(
        &mut mutated,
        "0 0 0 setrgbcolor 1 setlinecap \
         /A [0 0] def \
         newpath 0 0 moveto 1000 0 lineto 1000 100 lineto 0 100 lineto closepath clip \
         << /BBox [0 0 1000 100] /Angles A /Spacing 60 /MaxLines 5 \
            /Density { A 1 45 put pop pop 1 } >> hatch",
    );
    let mut baseline = with_lib(100, 100);
    run(
        &mut baseline,
        "0 0 0 setrgbcolor 1 setlinecap \
         newpath 0 0 moveto 1000 0 lineto 1000 100 lineto 0 100 lineto closepath clip \
         << /BBox [0 0 1000 100] /Angles [0 0] /Spacing 60 /MaxLines 5 \
            /Density { pop pop 1 } >> hatch",
    );
    assert_eq!(
        mutated.gfx().pixmap.data(),
        baseline.gfx().pixmap.data(),
        "mutating the caller's own /Angles array mid-call should have no effect on what's drawn"
    );

    // And the static equivalent of the *intended* bypass ([0 45], the
    // value the mutation tried to smuggle in) is still correctly
    // rejected -- confirming this isn't passing merely because
    // /MaxLines 5 is too loose to ever fire for this /BBox/Spacing.
    let err = hatch_err(
        "newpath 0 0 moveto 1000 0 lineto 1000 100 lineto 0 100 lineto closepath clip \
         << /BBox [0 0 1000 100] /Angles [0 45] /Spacing 60 /MaxLines 5 >> hatch",
    );
    assert_eq!(err, "hatch-line-count-exceeds-safety-limit");
}

#[test]
fn thin_axis_aligned_region_still_draws() {
    // A region thinner than /Spacing, at a plain axis-aligned angle,
    // used to compute its sweep normal via an independent
    // cos/sin(angle+90) trig call rather than deriving it from the
    // already-computed direction vector -- two separate floating-point
    // trig evaluations are not guaranteed exactly orthogonal, and for
    // a thin-enough box that sub-ulp slack placed the sole candidate
    // offset just outside the box's true projection, so hkclipseg
    // rejected it and the pass silently drew nothing.
    // A 1-unit-tall region only gets one hairline-thin stroke, whose
    // anti-aliased coverage per pixel can land right at (this build's
    // observed value: exactly 128) the ink_count threshold other
    // tests use -- so check for *any* deviation from pure white
    // instead of a "dark enough" threshold; the property under test
    // is whether hkclipseg accepted the candidate at all, not how
    // dark the resulting stroke looks.
    let mut it = with_lib(100, 100);
    run(
        &mut it,
        "0 0 0 setrgbcolor 1 setlinecap \
         << /BBox [0 0 100 1] /Angle 0 /Spacing 6 >> hatch",
    );
    let any_ink = it.gfx().pixmap.pixels().iter().any(|p| p.red() < 255);
    assert!(
        any_ink,
        "a thin axis-aligned region should still get at least one stroke"
    );
}

// The next two tests are round-2 regressions from a second Codex
// review of PR #121, after the first round's three fixes landed:
// `hstep`/`hspacing`/`htrimlo`/`htrimhi` were internal working state
// that gates a loop bound, but weren't actually `hk`-prefixed despite
// the library's own docs claiming that was the reserved contract --
// so a /Density callback (documented as forbidden from touching any
// `h`-prefixed name, but a caller relying on the *narrower*, actually
// enforced-by-naming "hk-" claim would reasonably believe otherwise)
// could clobber one and change a *later* line or angle's own budgeted
// work, bypassing /MaxLines or /MaxSamples for the rest of the call.
// Renamed to `hkspacing`/`hkstep`/`hktrimlo`/`hktrimhi` so the
// existing "don't touch hk- names" contract actually covers them.

#[test]
fn clobbering_hstep_from_density_does_not_change_the_real_sampling_step() {
    // `hkstep` (the real internal name after the fix) is still
    // documented as off-limits and a caller redefining it is still
    // undefined behavior -- but a caller touching the *unprefixed*
    // `hstep` name specifically (a name the pre-fix docs' "hk-"
    // contract never actually covered) must have no effect at all,
    // since it no longer aliases anything hatch reads.
    let mut it = with_lib(60, 60);
    it.run_str(
        "/n 0 def \
         << /BBox [0 0 50 50] /Angle 0 /Spacing 25 /MaxSamples 15 \
            /Density { /hstep 0.1 def /n n 1 add def pop pop 1 } >> hatch \
         n",
    )
    .unwrap_or_else(|e| panic!("hatch failed: {}", it.error_report(&e)));
    let stack = it.operand_stack();
    let n: i64 = stack
        .last()
        .expect("n left on the stack")
        .repr()
        .parse()
        .expect("n should be an integer");
    assert_eq!(
        n, 15,
        "redefining the unprefixed /hstep should not change the real budgeted sample count"
    );
}

#[test]
fn clobbering_trim_bounds_from_density_does_not_bypass_max_samples() {
    // Same shape as the /Trim input-validation test above, but via
    // global corruption mid-call rather than malformed initial input
    // -- /Trim's own validation only runs once, so if the bound
    // variables it validated aren't the same ones later code reads,
    // a /Density callback can reintroduce exactly the negative-trim
    // span-growth bug the validation was meant to close.
    // A plain successful `run()` (which also asserts an empty operand
    // stack afterward) is the assertion here: clobbering the
    // unprefixed names must have no effect, so this must behave
    // exactly like the equivalent call with no /Density at all --
    // not error, and not silently sample far more than /MaxSamples.
    let mut it = with_lib(60, 60);
    run(
        &mut it,
        "0 0 0 setrgbcolor 1 setlinecap \
         << /BBox [0 0 50 50] /Angle 0 /Spacing 25 /MaxSamples 15 \
            /Density { /htrimhi 999 def /htrimlo -999 def pop pop 1 } >> hatch",
    );
}

// The next three tests are round-3 regressions from a third Codex
// review of PR #121, after rounds 1-2's fixes landed. Two are
// non-adversarial bugs (an ordinary caller can hit either with no
// /Density callback at all); the third — bbox bounds living outside
// the reserved `hk-` prefix, the same shape as round 2's finding — is
// deliberately *not* fixed. Renaming cannot close that class: a
// callback that redefines the *already-hk-prefixed* `hkstep` directly
// still bypasses the cap (`clobbering_hstep_from_density_...` above,
// tested against the pre-fix unprefixed name, makes the same point).
// PostScript has no private namespace, so this stays a documented
// contract -- `lib/artkit.ps`'s `scatter` ships with the identical
// exposure for `/Mark`/`/Weight` against its own sc-/sq-/si- prefixes
// (see NOTES.md's issue #49 entry for the full disposition).

#[test]
fn preflight_line_count_matches_the_real_drawing_loop_exactly() {
    // A config where PostScript's own floating-point `for` (summing
    // /Spacing repeatedly) used to take one more trip than
    // cvi(span/spacing)+1 predicted -- an *ordinary* caller hits this,
    // no adversarial /Density needed. The fix makes the drawing loop
    // integer-indexed, deriving each line from `hkmin + i*hkspacing`
    // rather than accumulating, so the real trip count now equals the
    // pre-flight formula by construction. Asserted directly: exactly
    // /MaxSamples calls, not one more.
    let mut it = with_lib(60, 60);
    it.run_str(
        "/n 0 def \
         << /BBox [0 0 3.3 0.1] /Angle 0 /Spacing 0.55 /MaxSamples 12 \
            /Density { /n n 1 add def pop pop 1 } >> hatch \
         n",
    )
    .unwrap_or_else(|e| panic!("hatch failed: {}", it.error_report(&e)));
    let stack = it.operand_stack();
    let n: i64 = stack
        .last()
        .expect("n left on the stack")
        .repr()
        .parse()
        .expect("n should be an integer");
    assert_eq!(
        n, 12,
        "the real /Density call count should exactly match the pre-flight estimate"
    );
}

#[test]
fn dropout_of_exactly_1_drops_every_line() {
    // hkfrnd can return exactly 1.0, which a bare `hkfrnd hdropout lt`
    // turns into "never dropped" even at the documented-certain
    // /Dropout of 1 -- the exact trap lib/artkit.ps's scodds already
    // documents and guards against; hatchkit's own dropout roll now
    // mirrors that pattern. A seed exists (found by the review) where
    // the sole candidate line in a one-line-tall region survives a
    // /Dropout of 1 without this fix.
    let mut it = with_lib(20, 20);
    run(
        &mut it,
        "0 0 0 setrgbcolor 1 setlinecap \
         << /BBox [0 0 10 10] /Seed 230538014 /Dropout 1 >> hatch",
    );
    assert_eq!(
        ink_count(&it),
        0,
        "/Dropout 1 should drop every candidate line, drawing nothing"
    );
}

#[test]
fn bbox_must_have_exactly_four_elements() {
    // `aload pop` silently accepts an oversized array, reading its
    // *last* four elements as coordinates and leaving the rest on the
    // operand stack -- violating hatch's own `opts hatch -` contract.
    assert_eq!(
        hatch_err("<< /BBox [99 0 0 10 10] >> hatch"),
        "hatch-bbox-must-have-four-elements"
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
