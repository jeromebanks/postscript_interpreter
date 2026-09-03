//! Paper/canvas/print-surface textures (issue #51, `lib/surfacekit.ps`):
//! `grain`/`fiber`/`scuff`/`misreg` are thin wrappers over
//! `lib/artkit.ps`'s `scatter` (issue #48), so their reproducibility,
//! budget, and containment mechanics are already pinned by
//! tests/artkit.rs and tests/stipplekit.rs (the closest sibling) --
//! these tests focus on what surfacekit itself adds: the shared
//! `/Color`/`/Strength` vocabulary, each preset's own option
//! vocabulary, the mandatory `gsave`/`grestore` isolation, and
//! `weave`'s own grid/budget logic (not scatter-based at all).

use pscat::{Interp, PsError};

fn load(it: &mut Interp) {
    let artkit = std::fs::read("lib/artkit.ps").expect("artkit present");
    it.run_source(&artkit)
        .unwrap_or_else(|e| panic!("artkit.ps failed: {}", it.error_report(&e)));
    let surfacekit = std::fs::read("lib/surfacekit.ps").expect("surfacekit present");
    it.run_source(&surfacekit)
        .unwrap_or_else(|e| panic!("surfacekit.ps failed: {}", it.error_report(&e)));
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

fn ink_count(it: &Interp) -> usize {
    it.gfx()
        .pixmap
        .pixels()
        .iter()
        .filter(|p| p.red() < 250)
        .count()
}

/// The self-documenting-undefined-name error idiom this codebase uses
/// throughout -- same helper shape as tests/hatchkit.rs's `hatch_err`.
fn surfacekit_err(src: &str) -> String {
    let mut it = with_lib(100, 100);
    match it.run_str(src).unwrap_err() {
        PsError::Undefined(name) => name,
        other => panic!("expected a self-documenting undefined name, got {other}"),
    }
}

#[test]
fn surfacekit_loads_without_drawing_anything() {
    let it = with_lib(100, 100);
    assert_eq!(ink_count(&it), 0, "loading surfacekit put ink on the page");
    assert!(!it.gfx().page_shown);
    assert!(
        it.operand_stack().is_empty(),
        "loading surfacekit left {:?} on the operand stack",
        it.operand_stack()
            .iter()
            .map(|o| o.repr())
            .collect::<Vec<_>>()
    );
}

// --- grain/fiber/scuff/misreg: reproducibility ------------------------

#[test]
fn each_scatter_based_preset_reproduces_a_seeded_arrangement() {
    let calls = [
        "0 0 100 100 screct << /Count 60 /Scale [0.4 1.6] /Seed 3 >> grain",
        "0 0 100 100 screct << /Count 40 /Rotate [0 360] /Seed 5 >> fiber",
        "0 0 100 100 screct << /Count 10 /Rotate [0 360] /Seed 9 >> scuff",
        "0 0 100 100 screct << /Count 8 /Scale [0.8 1.8] /Seed 11 >> misreg",
    ];
    for src in calls {
        let mut a = with_lib(100, 100);
        run(&mut a, src);
        let mut b = with_lib(100, 100);
        run(&mut b, src);
        assert_eq!(
            a.gfx().pixmap.data(),
            b.gfx().pixmap.data(),
            "{src:?}: same seed and options should reproduce pixel-for-pixel"
        );
        assert!(ink_count(&a) > 0, "{src:?} drew nothing");
    }
}

#[test]
fn different_seeds_produce_different_arrangements() {
    let mut a = with_lib(100, 100);
    run(
        &mut a,
        "0 0 100 100 screct << /Count 40 /Rotate [0 360] /Seed 1 >> fiber",
    );
    let mut b = with_lib(100, 100);
    run(
        &mut b,
        "0 0 100 100 screct << /Count 40 /Rotate [0 360] /Seed 2 >> fiber",
    );
    assert_ne!(
        a.gfx().pixmap.data(),
        b.gfx().pixmap.data(),
        "different seeds should draw different arrangements"
    );
}

// --- shared /Mark-override escape hatch --------------------------------

#[test]
fn a_caller_supplied_mark_skips_default_option_validation() {
    // Mirrors stipplekit's own /DotRadius precedent: an unrelated,
    // even malformed, default-mark-only option must not be penalized
    // when the caller overrides /Mark entirely.
    let mut it = with_lib(100, 100);
    run(
        &mut it,
        "0 0 100 100 screct \
         << /Count 5 /Radius (not-a-number) /Mark { pop pop pop pop } >> grain",
    );
}

#[test]
fn a_caller_supplied_mark_receives_scatters_own_arguments() {
    // If `grain` forwarded anything other than exactly `scatter`'s own
    // (x y scale angle) per mark, a `/Mark` that pops exactly four
    // values would either underflow or leave a leftover -- neither of
    // which this would reach cleanly.
    let mut it = with_lib(100, 100);
    it.run_str(
        "0 0 100 100 screct << /Count 3 /Seed 2 /Mark { pop pop pop pop } >> grain scplaced",
    )
    .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    let stack = it.operand_stack();
    assert_eq!(stack.len(), 1);
    assert_eq!(stack[0].repr(), "3");
}

// --- gsave/grestore isolation ------------------------------------------

#[test]
fn a_preset_call_does_not_leak_color_or_linewidth() {
    let mut it = with_lib(60, 60);
    it.run_str(
        "0.2 0.4 0.6 setrgbcolor 3 setlinewidth \
         0 0 60 60 screct << /Count 50 /Rotate [0 360] /Seed 7 >> fiber \
         currentrgbcolor currentlinewidth",
    )
    .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    let stack = it.operand_stack();
    let got: Vec<f64> = stack
        .iter()
        .map(|o| o.repr().parse().expect("number"))
        .collect();
    for (g, want) in got.iter().zip([0.2, 0.4, 0.6, 3.0]) {
        assert!(
            (g - want).abs() < 1e-4,
            "fiber must not leak its own internal color/linewidth past its own \
             gsave/grestore: got {got:?}, want approximately [0.2, 0.4, 0.6, 3.0]"
        );
    }
}

#[test]
fn weave_does_not_leak_color() {
    let mut it = with_lib(60, 60);
    it.run_str(
        "0.1 0.2 0.3 setrgbcolor \
         0 0 60 60 screct << /Pitch 6 /Seed 1 >> weave \
         currentrgbcolor",
    )
    .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    let stack = it.operand_stack();
    let got: Vec<f64> = stack
        .iter()
        .map(|o| o.repr().parse().expect("number"))
        .collect();
    for (g, want) in got.iter().zip([0.1, 0.2, 0.3]) {
        assert!(
            (g - want).abs() < 1e-4,
            "weave must not leak its own internal color past its own gsave/grestore: \
             got {got:?}, want approximately [0.1, 0.2, 0.3]"
        );
    }
}

// --- a leftover scpath path must not get drawn on the first mark ------

#[test]
fn a_leftover_scpath_path_is_not_filled_by_the_first_mark() {
    // scpath deliberately leaves the flattened region path behind
    // (artkit.ps's own docs) -- a default mark that skipped `newpath`
    // would fill that leftover outline on its very first call.
    // Building the region from a *triangle* and checking a point well
    // outside any plausible single fleck, but inside the triangle,
    // stays dark only if the leftover outline itself got filled.
    let mut it = with_lib(120, 120);
    run(
        &mut it,
        "0 0 0 setrgbcolor \
         newpath 10 10 moveto 110 10 lineto 60 110 lineto closepath \
         scpath << /Count 1 /Seed 42 /MinSpacing 0 >> grain",
    );
    // Center of the triangle, far from its edges -- a single tiny
    // fleck (radius well under 2) cannot reach here; only an
    // accidental fill of the whole leftover path would.
    let pm = &it.gfx().pixmap;
    let center_device_y = 120 - 45;
    assert!(
        pm.pixel(60, center_device_y).unwrap().red() >= 250,
        "the triangle's interior got filled -- the leftover scpath path was drawn"
    );
}

// --- weave: grid geometry and its own budget ---------------------------

#[test]
fn weave_reproduces_a_seeded_jitter_pixel_for_pixel() {
    let src = "0 0 100 100 screct << /Pitch 6 /Seed 4 >> weave";
    let mut a = with_lib(100, 100);
    run(&mut a, src);
    let mut b = with_lib(100, 100);
    run(&mut b, src);
    assert_eq!(a.gfx().pixmap.data(), b.gfx().pixmap.data());
    assert!(ink_count(&a) > 0, "weave drew nothing");
}

#[test]
fn weave_is_deterministic_even_without_a_seed() {
    let src = "0 0 100 100 screct << /Pitch 6 >> weave";
    let mut a = with_lib(100, 100);
    run(&mut a, src);
    let mut b = with_lib(100, 100);
    run(&mut b, src);
    assert_eq!(
        a.gfx().pixmap.data(),
        b.gfx().pixmap.data(),
        "weave's grid geometry is always exactly regular, seed or not"
    );
}

#[test]
fn weave_leaves_a_single_cells_corners_unpainted() {
    // One cell exactly the size of the canvas: whichever orientation
    // (horizontal or vertical thread bump) is drawn, it is a strip
    // through the center, not a fill -- all four corners stay paper.
    let mut it = with_lib(30, 30);
    run(
        &mut it,
        "0 0 0 setrgbcolor \
         0 0 30 30 screct << /Pitch 30 /Width 6 >> weave",
    );
    assert!(ink_count(&it) > 0, "weave drew nothing");
    let pm = &it.gfx().pixmap;
    for (x, y) in [(0, 0), (29, 0), (0, 29), (29, 29)] {
        assert!(
            pm.pixel(x, y).unwrap().red() >= 250,
            "corner ({x},{y}) should be unpainted paper, not part of the thread bump"
        );
    }
}

#[test]
fn weave_rejects_a_pitch_that_exceeds_the_cell_budget() {
    assert_eq!(
        surfacekit_err("0 0 100 100 screct << /Pitch 0.5 >> weave"),
        "weave-cell-count-exceeds-safety-limit"
    );
}

#[test]
fn weave_rejects_a_complex_path_region_that_exceeds_the_edge_budget() {
    // A ~300-edge zigzag path region, sized so the grid's own cell
    // count (10,000) stays under /MaxThreads's default (20,000) --
    // only the cells-times-edges cap should be what trips here,
    // proving the two budgets are independently enforced the way
    // hatchkit's own /MaxLines and /MaxSamples are.
    let mut it = with_lib(10, 10);
    it.run_str(
        "newpath 0 0 moveto \
         1 1 300 { \
           /sfti exch def \
           sfti 0.333 mul sfti 2 mod 10 mul lineto \
         } for \
         closepath scpath /sftregion exch def",
    )
    .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    // Sanity: the region really is a many-edge path region, not a rect.
    it.run_str("sftregion /Kind get sftregion /Edges get length 4 idiv")
        .unwrap_or_else(|e| panic!("{}", it.error_report(&e)));
    let stack = it.operand_stack();
    assert_eq!(stack[0].repr(), "/path");
    let edges: usize = stack[1].repr().parse().expect("edge count");
    assert!(
        edges > 100,
        "expected a many-edge region, got {edges} edges"
    );

    match it.run_str("sftregion << /Pitch 0.32 >> weave").unwrap_err() {
        PsError::Undefined(name) => {
            assert_eq!(name, "weave-region-too-complex-for-this-pitch")
        }
        other => panic!("expected a self-documenting undefined name, got {other}"),
    }
}

#[test]
fn weave_rejects_a_non_positive_pitch_or_width() {
    assert_eq!(
        surfacekit_err("0 0 100 100 screct << /Pitch 0 >> weave"),
        "weave-pitch-must-be-positive"
    );
    assert_eq!(
        surfacekit_err("0 0 100 100 screct << /Width 0 >> weave"),
        "weave-width-must-be-positive"
    );
}

#[test]
fn weave_covers_the_far_edge_when_pitch_does_not_evenly_divide_the_bbox() {
    // A ceiling-based grid at a fixed /Pitch would center its last
    // column outside the bbox whenever the width isn't an exact
    // multiple of /Pitch, and `scin`'s own bbox test would then
    // silently drop that whole column -- 100 wide at pitch 7 leaves a
    // clear remainder (100/7 = 14.28...). Checking ink near the very
    // right/top edge (not just the middle) catches that regression.
    let mut it = with_lib(100, 100);
    run(
        &mut it,
        "0 0 0 setrgbcolor 0 0 100 100 screct << /Pitch 7 /Width 4 >> weave",
    );
    let pm = &it.gfx().pixmap;
    let right_edge_inked = (0..100u32).any(|y| pm.pixel(97, y).unwrap().red() < 250);
    let top_edge_inked = (0..100u32).any(|x| pm.pixel(x, 2).unwrap().red() < 250);
    assert!(
        right_edge_inked && top_edge_inked,
        "weave left an edge of a non-evenly-divisible bbox completely bare"
    );
}

#[test]
fn weave_draws_at_least_one_cell_on_a_region_narrower_than_pitch() {
    // A ceiling-based grid whose sole cell's center fell outside a
    // narrow bbox drew *nothing at all* for a region narrower than
    // /Pitch/2 -- rounding to fit (rather than a fixed /Pitch) must
    // still guarantee at least one cell for any positive-area region.
    let mut it = with_lib(20, 20);
    run(
        &mut it,
        "0 0 0 setrgbcolor 0 0 3 20 screct << /Pitch 20 /Width 2 >> weave",
    );
    assert!(ink_count(&it) > 0, "a narrow region drew nothing at all");
}

#[test]
fn weave_rejects_an_extreme_pitch_cleanly_instead_of_overflowing() {
    // A pitch this small divided into an ordinary bbox would overflow
    // this interpreter's i64 int type on a bare `cvi` -- the pre-flight
    // check must reject it as a real-number comparison before any
    // conversion, the same overflow class tests/sweep.rs's own
    // extreme-range tests guard against elsewhere in this codebase.
    assert_eq!(
        surfacekit_err("0 0 100 100 screct << /Pitch 1e-300 >> weave"),
        "weave-cell-count-exceeds-safety-limit"
    );
}

// --- shared /Color and /Strength validation (checked once via grain) --

#[test]
fn color_must_be_a_three_number_rgb_array() {
    assert_eq!(
        surfacekit_err("0 0 100 100 screct << /Count 1 /Color (nope) >> grain"),
        "surfacekit-color-must-be-an-rgb-array"
    );
    assert_eq!(
        surfacekit_err("0 0 100 100 screct << /Count 1 /Color [1 2] >> grain"),
        "surfacekit-color-must-be-an-rgb-array"
    );
}

#[test]
fn color_components_must_be_0_to_1() {
    assert_eq!(
        surfacekit_err("0 0 100 100 screct << /Count 1 /Color [1.5 0 0] >> grain"),
        "surfacekit-color-components-must-be-0-to-1"
    );
    assert_eq!(
        surfacekit_err("0 0 100 100 screct << /Count 1 /Color [-0.1 0 0] >> grain"),
        "surfacekit-color-components-must-be-0-to-1"
    );
}

#[test]
fn strength_must_be_a_number_in_0_to_1() {
    assert_eq!(
        surfacekit_err("0 0 100 100 screct << /Count 1 /Strength (nope) >> grain"),
        "surfacekit-strength-must-be-a-number"
    );
    assert_eq!(
        surfacekit_err("0 0 100 100 screct << /Count 1 /Strength 1.5 >> grain"),
        "surfacekit-strength-must-be-0-to-1"
    );
}

// --- per-preset option validation ---------------------------------------

#[test]
fn grain_radius_must_be_positive() {
    assert_eq!(
        surfacekit_err("0 0 100 100 screct << /Count 1 /Radius 0 >> grain"),
        "grain-radius-must-be-positive"
    );
}

#[test]
fn fiber_length_must_be_a_two_number_range() {
    // scokrange (artkit.ps) checks "array of exactly 2 numbers," not
    // ordering -- surfacekit doesn't add an ordering check of its own
    // (a descending [hi lo] is tolerated, matching scatter's own
    // /Scale contract), so the error name says exactly what's
    // checked rather than overclaiming "ascending."
    assert_eq!(
        surfacekit_err("0 0 100 100 screct << /Count 1 /Length [1 2 3] >> fiber"),
        "fiber-length-must-be-a-two-number-range"
    );
}

#[test]
fn fiber_tolerates_a_descending_length_range() {
    let mut it = with_lib(100, 100);
    run(
        &mut it,
        "0 0 100 100 screct << /Count 5 /Length [9 3] /Seed 1 >> fiber",
    );
}

#[test]
fn fiber_width_must_be_positive() {
    assert_eq!(
        surfacekit_err("0 0 100 100 screct << /Count 1 /Width -1 >> fiber"),
        "fiber-width-must-be-positive"
    );
}

#[test]
fn scuff_kink_must_be_a_number() {
    assert_eq!(
        surfacekit_err("0 0 100 100 screct << /Count 1 /Kink (nope) >> scuff"),
        "scuff-kink-must-be-a-number"
    );
}

#[test]
fn misreg_rings_must_be_at_least_one_and_an_integer() {
    assert_eq!(
        surfacekit_err("0 0 100 100 screct << /Count 1 /Rings 0 >> misreg"),
        "misreg-rings-must-be-at-least-1"
    );
    assert_eq!(
        surfacekit_err("0 0 100 100 screct << /Count 1 /Rings 1.5 >> misreg"),
        "misreg-rings-must-be-an-integer"
    );
}

#[test]
fn misreg_rings_has_its_own_cap_independent_of_budget() {
    // /Budget bounds scatter's own mark count, not the per-mark work
    // /Rings multiplies -- a huge /Rings with a tiny /Count must still
    // be rejected pre-flight rather than looping unboundedly per mark.
    assert_eq!(
        surfacekit_err("0 0 100 100 screct << /Count 1 /Rings 100000 >> misreg"),
        "misreg-rings-must-be-100-or-fewer"
    );
}

#[test]
fn misreg_rejects_an_extreme_rings_cleanly_instead_of_a_raw_rangecheck() {
    // The integer check (`cvi`) originally ran *before* the 100 cap --
    // a finite but extreme /Rings (1e300) is well outside this
    // interpreter's i64 range, so `cvi` itself raised a raw
    // `rangecheck` instead of this file's own named error (Codex
    // review, PR #125, round 3). The cap must be a plain real-number
    // comparison checked first, since that never needs a conversion.
    assert_eq!(
        surfacekit_err("0 0 100 100 screct << /Count 1 /Rings 1e300 >> misreg"),
        "misreg-rings-must-be-100-or-fewer"
    );
}

#[test]
fn every_preset_rejects_a_non_dict_opts() {
    for call in ["grain", "fiber", "scuff", "misreg", "weave"] {
        assert_eq!(
            surfacekit_err(&format!("0 0 100 100 screct (nope) {call}")),
            format!("{call}-opts-must-be-a-dict")
        );
    }
}

// --- Codex review (PR #125): auto-execution hazards on unvalidated ----
// --- operands, closed by sfregiondef and by wrapping /Color//Length --
// --- before any bare reference (see lib/surfacekit.ps's own headers) -

#[test]
fn every_preset_rejects_a_procedure_passed_as_the_region_and_never_runs_it() {
    // Before sfregiondef existed, `sfregion` was bound bare straight
    // from the caller's first operand with no type check at all --
    // `{ boom } opts grain` would bind `boom`'s procedure to
    // `sfregion` and auto-execute it on the very next bare reference.
    // A `PsError::Undefined("boom")` here would mean it ran.
    for call in ["grain", "fiber", "scuff", "misreg", "weave"] {
        assert_eq!(
            surfacekit_err(&format!("{{ boom }} <<>> {call}")),
            format!("{call}-region-must-be-a-region"),
            "{call} either ran the procedure or reported the wrong error"
        );
    }
}

#[test]
fn every_preset_rejects_a_non_region_dict_passed_as_the_region() {
    for call in ["grain", "fiber", "scuff", "misreg", "weave"] {
        assert_eq!(
            surfacekit_err(&format!("<< /Kind /bogus >> <<>> {call}")),
            format!("{call}-region-must-be-a-region")
        );
    }
}

#[test]
fn color_rejects_an_executable_array_without_running_it() {
    // A procedure is itself an `arraytype` object -- `type /arraytype
    // eq` alone doesn't rule it out. Before the fix, `/Color { boom
    // }` bound the procedure to `sfcdval` and executed it on the next
    // bare reference instead of reporting a color error.
    assert_eq!(
        surfacekit_err("0 0 100 100 screct << /Count 1 /Color { boom } >> grain"),
        "surfacekit-color-must-be-an-rgb-array"
    );
}

#[test]
fn length_rejects_an_executable_array_without_running_it() {
    assert_eq!(
        surfacekit_err("0 0 100 100 screct << /Count 1 /Length { boom } >> fiber"),
        "fiber-length-must-be-a-two-number-range"
    );
    assert_eq!(
        surfacekit_err("0 0 100 100 screct << /Count 1 /Length { boom } >> scuff"),
        "scuff-length-must-be-a-two-number-range"
    );
}

#[test]
fn a_weight_callback_mutating_the_shared_scale_array_does_not_corrupt_shading() {
    // grain/fiber/scuff re-read /Scale fresh from their own private
    // options dict on every mark call (each default mark's own
    // comment explains why), but a shallow dict `copy` still shares
    // the /Scale *array itself* with whatever the caller passed in --
    // a /Weight callback that closes over that same array and mutates
    // it in place corrupted what a later mark read, after scatter's
    // own validation of the *original* values had already run and
    // couldn't re-run (Codex review, PR #125, round 2: this raised a
    // raw `typecheck` deep in sflerpfrac before the fix, rather than
    // anything self-documenting or even running to completion).
    // sfscalesnapshot closes it by copying /Scale into a private array
    // before scatter ever runs, so no /Weight callback can reach it.
    let mut it = with_lib(100, 100);
    it.run_str(
        "/myscale [ 0.5 1.5 ] def \
         0 0 100 100 screct \
         << /Count 20 /Seed 3 /Scale myscale \
            /Weight { pop pop myscale 0 (bogus) put 1 } >> grain",
    )
    .unwrap_or_else(|e| panic!("mutated /Scale corrupted the run: {}", it.error_report(&e)));
    assert!(
        ink_count(&it) > 0,
        "grain drew nothing once /Scale was mutated mid-run"
    );
}

#[test]
fn every_example_tag_runs_clean() {
    // The `% @example:` lines are what `--capabilities`/`pscat-mcp`
    // hand an agent to try first (issue #39's catalog); parsing them
    // straight out of the file (rather than retyping them here) means
    // a future preset's example that trips its own /Budget fails this
    // test instead of shipping a registered example that raises.
    let src = std::fs::read_to_string("lib/surfacekit.ps").expect("surfacekit present");
    let examples: Vec<&str> = src
        .lines()
        .filter_map(|l| l.trim().strip_prefix("% @example:"))
        .map(str::trim)
        .collect();
    assert!(
        examples.len() >= 5,
        "expected at least one @example: per preset, found {}",
        examples.len()
    );
    for example in examples {
        let mut it = with_lib(200, 200);
        it.run_str(example)
            .unwrap_or_else(|e| panic!("@example {example:?} failed: {}", it.error_report(&e)));
    }
}

#[test]
fn ghostscript_accepts_the_surfacekit_specimen_sheet() {
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
            "-g900x640",
            "-r72",
            "-o/dev/null",
            "examples/surfacekit.ps",
        ])
        .status()
        .expect("run gs");
    assert!(status.success(), "gs rejected examples/surfacekit.ps");
}
