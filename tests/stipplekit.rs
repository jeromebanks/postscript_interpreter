//! Density-driven stippling (issue #50, `lib/stipplekit.ps`): a thin
//! convenience layer over `lib/artkit.ps`'s `scatter` (issue #48) --
//! `stipple` places dots (or a caller-supplied point mark) across a
//! region according to a constant density or a per-point tone
//! callback. Since it's a wrapper, these tests lean on the sibling
//! `scatter` suite in tests/artkit.rs for the underlying placement
//! mechanics (reproducibility, min-spacing exactness, budget/tries
//! bounds) and instead focus on what `stipple` itself adds: the
//! constant/callback `/Density` split, the default dot `/Mark`, and
//! the option conflicts only `stipple` can catch.

use pscat::{Interp, PsError};

fn load(it: &mut Interp) {
    let artkit = std::fs::read("lib/artkit.ps").expect("artkit present");
    it.run_source(&artkit)
        .unwrap_or_else(|e| panic!("artkit.ps failed: {}", it.error_report(&e)));
    let stipplekit = std::fs::read("lib/stipplekit.ps").expect("stipplekit present");
    it.run_source(&stipplekit)
        .unwrap_or_else(|e| panic!("stipplekit.ps failed: {}", it.error_report(&e)));
}

fn eval(src: &str) -> Vec<String> {
    let mut it = Interp::new();
    load(&mut it);
    it.run_str(src)
        .unwrap_or_else(|e| panic!("eval of {src:?} failed: {}", it.error_report(&e)));
    it.operand_stack().iter().map(|o| o.repr()).collect()
}

fn placements(src: &str) -> Vec<(f64, f64)> {
    let got = eval(src);
    assert!(
        got.len().is_multiple_of(2),
        "a `/Mark {{ pop pop }}` stipple must leave pairs, got {got:?}"
    );
    got.chunks(2)
        .map(|c| {
            (
                c[0].parse().unwrap_or_else(|_| panic!("x: {:?}", c[0])),
                c[1].parse().unwrap_or_else(|_| panic!("y: {:?}", c[1])),
            )
        })
        .collect()
}

fn stipple_err(src: &str) -> String {
    let mut it = Interp::new();
    load(&mut it);
    match it.run_str(src).unwrap_err() {
        PsError::Undefined(name) => name,
        other => panic!("expected a self-documenting undefined name, got {other}"),
    }
}

fn ink_count(it: &Interp) -> usize {
    it.gfx()
        .pixmap
        .pixels()
        .iter()
        .filter(|p| p.red() < 128)
        .count()
}

fn with_lib(w: u32, h: u32) -> Interp {
    let mut it = Interp::with_page(w, h).expect("page");
    load(&mut it);
    it
}

fn run(it: &mut Interp, src: &str) {
    it.run_str(src)
        .unwrap_or_else(|e| panic!("eval of {src:?} failed: {}", it.error_report(&e)));
    assert!(
        it.operand_stack().is_empty(),
        "{src:?} left {:?} on the operand stack",
        it.operand_stack()
            .iter()
            .map(|o| o.repr())
            .collect::<Vec<_>>()
    );
}

#[test]
fn stipplekit_loads_without_drawing_anything() {
    let it = with_lib(100, 100);
    assert_eq!(ink_count(&it), 0, "loading stipplekit put ink on the page");
    assert!(!it.gfx().page_shown);
    assert!(
        it.operand_stack().is_empty(),
        "loading stipplekit left {:?} on the operand stack",
        it.operand_stack()
            .iter()
            .map(|o| o.repr())
            .collect::<Vec<_>>()
    );
}

#[test]
fn constant_density_forwards_straight_to_scatters_own_density() {
    // A plain-number /Density is documented to be scatter's own
    // /Density, unchanged -- Count = truncate(Density * Area), the
    // same property tests/artkit.rs's scatter_density_resolves_
    // against_the_region_area pins for the raw primitive.
    let got = eval(
        "0 0 100 200 screct << /Density 0.01 /Seed 2 /Mark { pop pop pop pop } >> stipple \
         scplaced \
         0 0 200 200 screct << /Density 0.01 /Seed 2 /Mark { pop pop pop pop } >> stipple \
         scplaced",
    );
    assert_eq!(got[0], "200", "0.01 * 20000");
    assert_eq!(got[1], "400", "0.01 * 40000");
}

#[test]
fn stipple_reproduces_a_seeded_arrangement_default_mark() {
    // With the default /Mark, placements land on the operand stack as
    // (radius-independent) x/y pairs only via an explicit override --
    // this test swaps in `{ pop pop }` (like every scatter/hatch
    // reproducibility test) purely to read positions back; the
    // default /Mark itself is exercised separately below via pixels.
    let call = "0 0 200 200 screct \
                << /Density 0.02 /Seed 91 /MinSpacing 5 /Mark { pop pop } >> stipple";
    let first = placements(call);
    let second = placements(call);
    assert_eq!(first, second, "same seed and options, same arrangement");
    assert!(!first.is_empty());
}

#[test]
fn callable_density_reproduces_a_seeded_arrangement() {
    let call = "0 0 200 200 screct \
                << /Density { pop 200 div } /MaxDensity 0.02 /Seed 7 /MinSpacing 4 \
                   /Mark { pop pop } >> stipple";
    let first = placements(call);
    let second = placements(call);
    assert_eq!(first, second, "same seed and callback, same arrangement");
    assert!(!first.is_empty());
}

#[test]
fn callable_density_shapes_where_marks_land() {
    // A tone of 0 on the left half, 1 on the right: not one mark may
    // land left of x=100, and the right half must still fill --
    // mirrors scatter_weight_biases_where_marks_land, since this *is*
    // that mechanism under a different option name.
    let pts = placements(
        "0 0 200 200 screct \
         << /Density { /y exch def /x exch def x 100 lt { 0 } { 1 } ifelse } \
            /MaxDensity 0.02 /Seed 4 /MinSpacing 4 /Mark { pop pop } >> stipple",
    );
    assert!(!pts.is_empty(), "nothing placed on the accepting half");
    for (x, _y) in &pts {
        assert!(*x >= 100.0, "a mark at x={x} landed on the zero-tone half");
    }
}

#[test]
fn callable_density_total_tracks_the_peak_not_the_average() {
    // Documented, deliberate behavior (this file's own header): with
    // scatter's default /Tries (20 retries per slot), a candidate
    // slot almost always finds *some* accepting position before
    // giving up, so the realized total tracks /MaxDensity * area, not
    // the field's own spatial average. The discriminating case is a
    // genuinely different average, not a thin sliver (where both
    // readings predict nearly the same number and the test would pass
    // under either mechanism): half the region at a relative tone of
    // 0.5, half at 1.0. Peak-driven Count is
    // truncate(0.01 * 40000) = 400; a naive
    // total-tracks-the-integral reading would instead predict roughly
    // 400 * (0.5 + 1.0)/2 =~ 300. Confirmed empirically (headless
    // pscat, this exact call) to land at 400, matching the peak
    // reading, not the integral one -- pinned here as a regression
    // guard for the design decision NOTES.md's issue #50 entry
    // documents at length.
    let got = eval(
        "0 0 200 200 screct \
         << /Density { /y exch def /x exch def x 100 lt { 0.5 } { 1 } ifelse } \
            /MaxDensity 0.01 /Seed 4 /Mark { pop pop pop pop } >> stipple \
         scplaced",
    );
    let placed: i64 = got[0].parse().expect("scplaced is a number");
    assert!(
        placed > 380,
        "expected the peak-driven count (~400) rather than the integral-driven one \
         (~300), got {placed}"
    );
}

#[test]
fn default_mark_draws_a_circle_of_the_expected_radius() {
    // DotRadius 5, Scale pinned to exactly 2 -> a circle of radius 10
    // centered wherever the single placed dot lands. Rather than
    // predict the (seed-dependent) center, place at a region small
    // enough, with /MinSpacing 0 and /Count 1, that inspecting ink
    // near the placed point's own bbox is enough: measure the ink
    // radius directly from the pixmap instead of trusting a guessed
    // center.
    let mut it = with_lib(200, 200);
    run(
        &mut it,
        "0 0 0 setrgbcolor \
         100 100 1 1 screct \
         << /Count 1 /Seed 1 /Tries 1 /DotRadius 5 /Scale [ 2 2 ] >> stipple",
    );
    let ink = ink_count(&it);
    // A filled circle of radius 10 covers pi*10^2 =~ 314 pixels.
    assert!(
        (250..=380).contains(&ink),
        "expected roughly a radius-10 filled circle (~314 px), got {ink} ink pixels"
    );
}

#[test]
fn custom_mark_overrides_the_default_dot() {
    let mut it = with_lib(200, 200);
    run(&mut it, "0 0 0 setrgbcolor");
    let before = ink_count(&it);
    run(
        &mut it,
        "0 0 200 200 screct \
         << /Count 30 /Seed 6 /MinSpacing 5 \
            /Mark { pop pop pop pop 5 5 moveto 10 10 lineto stroke } >> stipple",
    );
    assert!(
        ink_count(&it) > before,
        "a custom /Mark drew nothing (every mark drew a fixed line, not scaled by position)"
    );
}

#[test]
fn maxdensity_is_required_when_density_is_callable() {
    assert_eq!(
        stipple_err("0 0 100 100 screct << /Density { pop pop 1 } /Seed 1 >> stipple"),
        "stipple-maxdensity-required-when-density-is-callable"
    );
}

#[test]
fn maxdensity_must_be_a_non_negative_number() {
    assert_eq!(
        stipple_err(
            "0 0 100 100 screct \
             << /Density { pop pop 1 } /MaxDensity (nope) /Seed 1 >> stipple"
        ),
        "stipple-maxdensity-must-be-a-number"
    );
    assert_eq!(
        stipple_err(
            "0 0 100 100 screct \
             << /Density { pop pop 1 } /MaxDensity -1 /Seed 1 >> stipple"
        ),
        "stipple-maxdensity-must-be-non-negative"
    );
}

#[test]
fn weight_and_callable_density_are_mutually_exclusive() {
    assert_eq!(
        stipple_err(
            "0 0 100 100 screct \
             << /Density { pop pop 1 } /MaxDensity 0.01 /Weight { pop pop 1 } /Seed 1 >> stipple"
        ),
        "stipple-weight-and-callable-density-are-mutually-exclusive"
    );
}

#[test]
fn count_and_callable_density_conflict_propagates_from_scatter() {
    // stipple deliberately does not duplicate this check -- after it
    // substitutes its own peak /Density, a caller-supplied /Count
    // alongside it trips scatter's own mutual-exclusivity error.
    assert_eq!(
        stipple_err(
            "0 0 100 100 screct \
             << /Density { pop pop 1 } /MaxDensity 0.01 /Count 5 /Seed 1 >> stipple"
        ),
        "scatter-count-and-density-are-mutually-exclusive"
    );
}

#[test]
fn weight_forwards_through_unchanged_alongside_a_constant_density() {
    // Only a *callable* /Density reserves /Weight -- a plain-number
    // one still composes with an explicit /Weight, forwarded as-is.
    let pts = placements(
        "0 0 200 200 screct \
         << /Density 0.02 /Weight { /y exch def /x exch def x 100 lt { 0 } { 1 } ifelse } \
            /Seed 4 /Mark { pop pop } >> stipple",
    );
    assert!(!pts.is_empty());
    for (x, _y) in &pts {
        assert!(*x >= 100.0, "an explicit /Weight was not honored: x={x}");
    }
}

#[test]
fn density_that_is_neither_a_number_nor_a_procedure_fails_as_scatters_own_weight() {
    // Not re-validated by stipple -- it forwards a non-numeric
    // /Density straight to scatter as /Weight, and scatter's own
    // sccallable check (via its own /Weight validation) is what
    // actually rejects a bare string.
    assert_eq!(
        stipple_err("0 0 100 100 screct << /Density (nope) /MaxDensity 0.01 >> stipple"),
        "scatter-weight-must-be-a-procedure"
    );
}

#[test]
fn dotradius_must_be_a_positive_number() {
    assert_eq!(
        stipple_err("0 0 100 100 screct << /Count 1 /DotRadius (nope) >> stipple"),
        "stipple-dotradius-must-be-a-number"
    );
    assert_eq!(
        stipple_err("0 0 100 100 screct << /Count 1 /DotRadius 0 >> stipple"),
        "stipple-dotradius-must-be-positive"
    );
}

#[test]
fn opts_must_be_a_dict() {
    assert_eq!(
        stipple_err("0 0 100 100 screct [ ] stipple"),
        "stipple-opts-must-be-a-dict"
    );
}

#[test]
fn stipple_does_not_mutate_the_callers_options_dict() {
    // The callable branch rewrites /Density and /Weight internally --
    // it must do so on a private copy, never the dict the caller
    // still holds a reference to.
    let got = eval(
        "<< /Density { pop pop 1 } /MaxDensity 0.01 /Seed 1 >> /myopts exch def \
         0 0 100 100 screct myopts stipple \
         myopts /Density get xcheck myopts /Weight known myopts /MaxDensity get",
    );
    assert_eq!(
        got[0], "true",
        "caller's own /Density must still be the callback, not a number"
    );
    assert_eq!(
        got[1], "false",
        "caller's own dict must not have gained a /Weight key"
    );
    assert_eq!(got[2], "0.01", "caller's own /MaxDensity must be untouched");
}

#[test]
fn scplaced_reports_the_stipple_count_with_no_duplicate_readback() {
    let got = eval(
        "0 0 100 200 screct << /Density 0.01 /Seed 2 /Mark { pop pop pop pop } >> stipple \
         scplaced",
    );
    assert_eq!(got[0], "200");
}

#[test]
fn budget_is_still_checked_before_anything_is_drawn() {
    // Inherited from scatter, not re-implemented: a resolved count
    // above scatter's own /Budget default (20000) still rejects up
    // front. A callable /Density with an enormous /MaxDensity is the
    // path that exercises this through stipple's own substitution.
    let mut it = Interp::with_page(50, 50).expect("page");
    load(&mut it);
    let err = it
        .run_str(
            "0 0 500 500 screct \
             << /Density { pop pop 1 } /MaxDensity 1000 /Seed 1 >> stipple",
        )
        .unwrap_err();
    match err {
        PsError::Undefined(name) => {
            assert_eq!(name, "scatter-count-exceeds-safety-limit")
        }
        other => panic!("expected a self-documenting undefined name, got {other}"),
    }
    assert_eq!(ink_count(&it), 0, "a rejected budget must draw nothing");
}

#[test]
fn the_stippling_specimen_sheet_renders_ink_in_every_panel() {
    let source = std::fs::read("examples/stippling.ps").expect("read the specimen");
    let mut it = Interp::with_page(900, 360).expect("page");
    it.run_source(&source)
        .unwrap_or_else(|e| panic!("examples/stippling.ps failed: {}", it.error_report(&e)));
    assert!(it.gfx().page_shown, "showpage must have run");

    for (label, x0) in [("constant", 40), ("ramp", 340), ("point-shading", 640)] {
        let mut marked = 0;
        for dx in 0..240_u32 {
            for dy in 0..240_u32 {
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
            marked > 500,
            "the {label} panel has only {marked} marked pixels -- its stipple placed nothing"
        );
    }
}

#[test]
fn ghostscript_accepts_the_stippling_specimen_sheet() {
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
            "examples/stippling.ps",
        ])
        .status()
        .expect("run gs");
    assert!(status.success(), "gs rejected examples/stippling.ps");
}
