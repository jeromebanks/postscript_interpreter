//! Woodcut/linocut/engraving printmaking presets (issue #52,
//! `lib/printkit.ps`), composed from `lib/hatchkit.ps`'s `hatch`
//! (issue #49), `lib/artkit.ps`'s `scatter` (issue #48), and
//! `lib/surfacekit.ps`'s `grain` (issue #51). `hatch`'s own
//! reproducibility/budget mechanics and `scatter`'s own placement
//! mechanics are already pinned by tests/hatchkit.rs and
//! tests/artkit.rs; these tests focus on what this file itself adds:
//! the path-based (not `region`-based) calling convention, the shared
//! option vocabulary and its printkit-owned validation, `/Budget`
//! forwarding, the optional `/Paper` pass, and that the three presets
//! actually read as different techniques.

use pscat::gfx::PathElement;
use pscat::{Interp, PsError};

fn load(it: &mut Interp) {
    for lib in [
        "lib/artkit.ps",
        "lib/hatchkit.ps",
        "lib/surfacekit.ps",
        "lib/printkit.ps",
    ] {
        let src = std::fs::read(lib).unwrap_or_else(|e| panic!("{lib} present: {e}"));
        it.run_source(&src)
            .unwrap_or_else(|e| panic!("{lib} failed: {}", it.error_report(&e)));
    }
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
fn printkit_err(src: &str) -> String {
    let mut it = with_lib(150, 150);
    match it.run_str(src).unwrap_err() {
        PsError::Undefined(name) => name,
        other => panic!("expected a self-documenting undefined name, got {other}"),
    }
}

const RECT: &str = "newpath 20 20 moveto 130 20 lineto 130 130 lineto 20 130 lineto closepath";

#[test]
fn printkit_loads_without_drawing_anything() {
    let it = with_lib(100, 100);
    assert_eq!(ink_count(&it), 0, "loading printkit put ink on the page");
    assert!(!it.gfx().page_shown);
    assert!(
        it.operand_stack().is_empty(),
        "loading printkit left {:?} on the operand stack",
        it.operand_stack()
            .iter()
            .map(|o| o.repr())
            .collect::<Vec<_>>()
    );
}

// --- each preset draws, stays inside its clip -------------------------

#[test]
fn each_preset_draws_ink_from_a_bare_path_no_pre_clip_needed() {
    for preset in ["woodcut", "linocut", "engraving"] {
        let mut it = with_lib(150, 150);
        run(&mut it, &format!("{RECT} << /Seed 3 >> {preset}"));
        assert!(ink_count(&it) > 0, "{preset} drew nothing");
    }
}

#[test]
fn each_preset_stays_inside_a_concave_path() {
    // A chevron/arrow, same shape hatchkit's own concave-clip test
    // uses: its bounding box's own corners sit outside the shape
    // entirely, exactly what a bbox-only region would get wrong.
    for preset in ["woodcut", "linocut", "engraving"] {
        let mut it = with_lib(120, 120);
        run(
            &mut it,
            &format!(
                "newpath 10 10 moveto 50 90 lineto 90 10 lineto 50 40 lineto closepath \
                 << /Seed 4 /Density 0.8 >> {preset}"
            ),
        );
        let pm = &it.gfx().pixmap;
        assert!(
            pm.pixel(5, 5).unwrap().red() >= 250,
            "{preset}: ink outside the chevron's bbox entirely"
        );
        assert!(
            pm.pixel(115, 115).unwrap().red() >= 250,
            "{preset}: ink outside the chevron's bbox entirely"
        );
        // User (12,88) sits inside the bbox but outside the chevron's
        // own notch -- device (12, 120-88=32).
        assert!(
            pm.pixel(12, 32).unwrap().red() >= 250,
            "{preset}: ink landed in the bbox's concave notch"
        );
    }
}

// --- reproducibility ----------------------------------------------------

#[test]
fn each_seeded_preset_reproduces_identically() {
    for preset in ["woodcut", "linocut", "engraving"] {
        let src = format!("{RECT} << /Seed 11 /Density 0.7 /Roughness 0.8 /Scale 1.3 >> {preset}");
        let mut a = with_lib(150, 150);
        run(&mut a, &src);
        let mut b = with_lib(150, 150);
        run(&mut b, &src);
        assert_eq!(
            a.gfx().pixmap.data(),
            b.gfx().pixmap.data(),
            "{preset}: same seed, same options, different pixels"
        );
    }
}

// --- the three presets actually read as different techniques ----------

#[test]
fn the_three_presets_are_visually_distinguishable() {
    let mut woodcut = with_lib(150, 150);
    run(&mut woodcut, &format!("{RECT} << /Seed 9 >> woodcut"));
    let mut linocut = with_lib(150, 150);
    run(&mut linocut, &format!("{RECT} << /Seed 9 >> linocut"));
    let mut engraving = with_lib(150, 150);
    run(&mut engraving, &format!("{RECT} << /Seed 9 >> engraving"));

    let wp = woodcut.gfx().pixmap.data();
    let lp = linocut.gfx().pixmap.data();
    let ep = engraving.gfx().pixmap.data();
    assert_ne!(wp, lp, "woodcut and linocut rendered identically");
    assert_ne!(wp, ep, "woodcut and engraving rendered identically");
    assert_ne!(lp, ep, "linocut and engraving rendered identically");

    // engraving's three-angle crosshatch at fine spacing should cover
    // noticeably more of the region than either single-pass preset at
    // matched default options -- a concrete, preset-specific claim
    // (not just "the bytes differ") that pins the intended character
    // of each technique against a silent formula regression.
    let wi = ink_count(&woodcut);
    let li = ink_count(&linocut);
    let ei = ink_count(&engraving);
    assert!(
        ei > wi && ei > li,
        "engraving ({ei}) should out-cover woodcut ({wi}) and linocut ({li})"
    );
}

// --- /Paper --------------------------------------------------------------

#[test]
fn paper_true_adds_ink_under_the_same_clip() {
    let mut without_paper = with_lib(150, 150);
    run(&mut without_paper, &format!("{RECT} << /Seed 6 >> linocut"));
    let mut with_paper = with_lib(150, 150);
    run(
        &mut with_paper,
        &format!("{RECT} << /Seed 6 /Paper true >> linocut"),
    );
    assert!(
        ink_count(&with_paper) > ink_count(&without_paper),
        "/Paper true should add ink over the same call"
    );

    // The paper pass is scattered inside the same clip established
    // from the caller's path, so it stays exactly bounded too, not
    // merely scin's own approximate overhang.
    let pm = &with_paper.gfx().pixmap;
    assert!(
        pm.pixel(5, 5).unwrap().red() >= 250,
        "paper ink escaped the clip"
    );
}

#[test]
fn paper_true_works_for_engraving_which_has_no_chip_marks() {
    // engraving never calls scpath for chip marks (it has none) --
    // /Paper true is the one path that still needs a region there.
    let mut it = with_lib(150, 150);
    run(
        &mut it,
        &format!("{RECT} << /Seed 6 /Paper true >> engraving"),
    );
    assert!(ink_count(&it) > 0);
}

#[test]
fn no_preset_builds_an_unused_region() {
    // Codex review, PR #128, rounds 2 and 3: an earlier draft called
    // `scpath` unconditionally in every preset, including whenever
    // its result goes unused -- `engraving` with `/Paper false` (it
    // has no chip marks at all), and `woodcut`/`linocut` whenever
    // `/Paper false` *and* `/Density` rounds their own chip count to
    // 0. `scpath` enforces its own 20000-edge ceiling regardless of
    // whether anything downstream needs a region that large, so a
    // caller with a genuinely complex path -- one `hatch`'s own
    // `clip` would happily draw -- got a spurious
    // `scpath-too-many-edges` rejection from a region nothing was
    // going to use. Every preset must skip `scpath` entirely when
    // nothing needs it; `/Paper true` still needs the region and
    // should still reject the same path for all three, proving the
    // fix is "skip the unused call," not "never call scpath."
    let mut complex = String::from("newpath 0 0 moveto ");
    for i in 0..21_000 {
        complex.push_str(&format!("{} {} lineto ", i % 500, i as f64 * 0.001));
    }
    complex.push_str("closepath");

    // /Density 0 keeps woodcut/linocut's own chip count at 0 (round
    // 0.5 -> 0 for both formulas), same as engraving having none at
    // all -- the shared "region never used" precondition.
    for preset in ["woodcut", "linocut", "engraving"] {
        let mut it = with_lib(600, 600);
        it.run_str(&format!("{complex} << /Seed 1 /Density 0 >> {preset}"))
            .unwrap_or_else(|e| {
                panic!(
                    "{preset} with /Paper false rejected a complex path hatch alone would handle: {}",
                    it.error_report(&e)
                )
            });

        let got = printkit_err(&format!(
            "{complex} << /Seed 1 /Density 0 /Paper true >> {preset}"
        ));
        assert_eq!(got, "scpath-too-many-edges", "{preset}");
    }
}

// --- gsave/grestore isolation --------------------------------------------

#[test]
fn a_preset_call_restores_color_linewidth_and_the_callers_path() {
    let mut it = with_lib(150, 150);
    it.run_str(&format!(
        "0.25 0.5 0.75 setrgbcolor 2.0 setlinewidth \
         {RECT} << /Seed 2 >> woodcut \
         currentrgbcolor currentlinewidth"
    ))
    .unwrap_or_else(|e| panic!("eval failed: {}", it.error_report(&e)));
    // currentlinewidth, then currentrgbcolor's b g r, in pop order.
    // 0.25/0.5/0.75/2.0 all round-trip exactly through the graphics
    // state's internal f32 color storage, unlike e.g. 0.6.
    assert_eq!(it.pop().expect("linewidth").repr(), "2.0");
    assert_eq!(it.pop().expect("b").repr(), "0.75");
    assert_eq!(it.pop().expect("g").repr(), "0.5");
    assert_eq!(it.pop().expect("r").repr(), "0.25");
}

#[test]
fn a_preset_call_leaves_the_callers_path_current() {
    // After the preset call, the caller's own path (the same RECT)
    // should still be current -- pathbbox on it should still report
    // the caller's own rectangle, not an empty/cleared path.
    let mut it = with_lib(150, 150);
    it.run_str(&format!("{RECT} << /Seed 2 >> woodcut pathbbox"))
        .unwrap_or_else(|e| panic!("eval failed: {}", it.error_report(&e)));
    assert_eq!(it.pop().expect("y1").repr(), "130.0");
    assert_eq!(it.pop().expect("x1").repr(), "130.0");
    assert_eq!(it.pop().expect("y0").repr(), "20.0");
    assert_eq!(it.pop().expect("x0").repr(), "20.0");
}

#[test]
fn every_preset_leaves_a_curved_callers_path_genuinely_unflattened() {
    // `scpath` (called unconditionally by every preset -- to build a
    // region for chip marks / the optional /Paper pass, or, for
    // `engraving` with neither, purely for uniformity per this file's
    // own header) flattens the current path -- but only for the
    // duration of the preset's own internal `gsave`/`grestore`. `path`
    // lives on this interpreter's saved graphics state, so the
    // closing `grestore` restores the caller's path exactly as it was
    // at entry, real curves included -- `scpath`'s flattening is local
    // to the call, never a caller-visible cost. (An earlier draft of
    // this file's header claimed the caller's path comes back
    // flattened; this test is what caught that the claim was wrong --
    // `gsave`/`grestore` in this interpreter already isolates it.)
    // Pin it against the path's own segment list, not just pathbbox
    // (whose four corners a flattened polygon and the original curve
    // can still coincidentally match), and check all three presets
    // uniformly, since all three now call `scpath` unconditionally.
    let curved = "newpath 20 20 moveto 20 120 20 120 120 120 curveto \
                  120 120 120 20 20 20 curveto closepath";
    for preset in ["woodcut", "linocut", "engraving"] {
        let mut it = with_lib(150, 150);
        it.run_str(&format!("{curved} << /Seed 2 >> {preset}"))
            .unwrap_or_else(|e| panic!("{preset}: eval failed: {}", it.error_report(&e)));
        let has_curve = it
            .gfx()
            .state()
            .path
            .elements()
            .iter()
            .any(|e| matches!(e, PathElement::Curve(..)));
        assert!(
            has_curve,
            "{preset}: the caller's curves did not survive the call"
        );
    }
}

#[test]
fn unrecognized_option_keys_are_silently_ignored_not_forwarded() {
    // printkit builds every hatch/scatter/grain options dict fresh
    // from its own eight documented keys -- unlike surfacekit, it
    // never forwards an unowned key from the caller's dict. A
    // hatchkit-shaped /Spacing here must not change spacing (and must
    // not error either): the call with it present renders pixel-for-
    // pixel identical to the same call without it.
    let mut without = with_lib(150, 150);
    run(&mut without, &format!("{RECT} << /Seed 8 >> woodcut"));
    let mut with_spacing = with_lib(150, 150);
    run(
        &mut with_spacing,
        &format!("{RECT} << /Seed 8 /Spacing 1 >> woodcut"),
    );
    assert_eq!(
        without.gfx().pixmap.data(),
        with_spacing.gfx().pixmap.data(),
        "/Spacing changed woodcut's output -- it should be silently ignored"
    );
}

// --- /Budget forwarding ---------------------------------------------------

#[test]
fn omitting_budget_never_lowers_hatchs_own_ceiling() {
    // A modestly sized region at engraving's default (fine) spacing
    // would trip a low /Budget, but must succeed under printkit's own
    // default (hatch's own default, 20000) -- printkit must never
    // silently lower the ceiling a bare `hatch` call would get.
    let mut it = with_lib(400, 400);
    it.run_str(
        "newpath 20 20 moveto 380 20 lineto 380 380 lineto 20 380 lineto closepath \
         << /Seed 1 /Scale 0.6 /Density 0.9 >> engraving",
    )
    .unwrap_or_else(|e| {
        panic!(
            "default /Budget rejected a normal call: {}",
            it.error_report(&e)
        )
    });
}

#[test]
fn an_explicit_small_budget_rejects_via_hatchs_own_name() {
    let got = printkit_err(&format!("{RECT} << /Seed 1 /Budget 2 >> engraving"));
    assert_eq!(got, "hatch-line-count-exceeds-safety-limit");
}

#[test]
fn an_explicit_small_budget_rejects_chip_marks_via_scatters_own_name() {
    // printkit reads only its own eight documented option keys --
    // /Spacing (a hatchkit key) would be silently ignored here, not
    // forwarded, so this deliberately leaves it out and instead uses
    // a low /Density: at /Density 0.05, woodcut's own hatch spacing
    // (16 / (0.3 + 1.4*Density)) is wide enough that the single hatch
    // pass over this small RECT stays under a /Budget of 5 on its
    // own, so the chip-mark scatter pass (count = round(140*Density)
    // = 7) is what actually trips the cap -- proving /Budget really
    // is forwarded as scatter's own /Budget too, not just hatch's
    // /MaxLines. Pinned empirically (both sub-calls forward the same
    // /Budget, so this is a statement about the current formulas'
    // relative candidate counts, not a structural guarantee); if a
    // future formula change makes hatch trip first instead, that
    // sub-call's own name would surface and this assertion would need
    // updating, not the wiring itself.
    let got = printkit_err(&format!(
        "{RECT} << /Seed 1 /Budget 5 /Density 0.05 >> woodcut"
    ));
    assert_eq!(got, "scatter-count-exceeds-safety-limit");
}

// --- validation error table ---------------------------------------------

#[test]
fn opts_must_be_a_dict_per_preset() {
    assert_eq!(
        printkit_err(&format!("{RECT} 5 woodcut")),
        "woodcut-opts-must-be-a-dict"
    );
    assert_eq!(
        printkit_err(&format!("{RECT} 5 linocut")),
        "linocut-opts-must-be-a-dict"
    );
    assert_eq!(
        printkit_err(&format!("{RECT} 5 engraving")),
        "engraving-opts-must-be-a-dict"
    );
}

#[test]
fn shared_option_validation_error_table() {
    let cases: &[(&str, &str)] = &[
        ("<< /Scale (nope) >>", "printkit-scale-must-be-a-number"),
        ("<< /Scale -1 >>", "printkit-scale-must-be-positive"),
        ("<< /Scale 0 >>", "printkit-scale-must-be-positive"),
        ("<< /Scale 1e9 >>", "printkit-scale-must-be-1000-or-less"),
        ("<< /Scale 1001 >>", "printkit-scale-must-be-1000-or-less"),
        (
            "<< /Scale 1e-20 >>",
            "printkit-scale-must-be-at-least-0.001",
        ),
        (
            "<< /Scale 0.0001 >>",
            "printkit-scale-must-be-at-least-0.001",
        ),
        ("<< /Density (nope) >>", "printkit-density-must-be-a-number"),
        (
            "<< /Density 1.5 >>",
            "printkit-density-must-be-a-fraction-in-0-1",
        ),
        (
            "<< /Density -0.1 >>",
            "printkit-density-must-be-a-fraction-in-0-1",
        ),
        (
            "<< /Roughness (nope) >>",
            "printkit-roughness-must-be-a-number",
        ),
        (
            "<< /Roughness 1.5 >>",
            "printkit-roughness-must-be-a-fraction-in-0-1",
        ),
        ("<< /Angle (nope) >>", "printkit-angle-must-be-a-number"),
        ("<< /Seed (nope) >>", "printkit-seed-must-be-a-number"),
        ("<< /Seed 2147483647 >>", "printkit-seed-out-of-range"),
        ("<< /Seed -2147483647 >>", "printkit-seed-out-of-range"),
        ("<< /Budget (nope) >>", "printkit-budget-must-be-a-number"),
        ("<< /Budget 0 >>", "printkit-budget-must-be-positive"),
        ("<< /Budget -5 >>", "printkit-budget-must-be-positive"),
        (
            "<< /Budget 300000 >>",
            "printkit-budget-must-be-200000-or-fewer",
        ),
        ("<< /Paper (nope) >>", "printkit-paper-must-be-a-boolean"),
        ("<< /Paper 1 >>", "printkit-paper-must-be-a-boolean"),
        ("<< /Color (nope) >>", "printkit-color-must-be-an-rgb-array"),
        (
            "<< /Color [ 1 2 ] >>",
            "printkit-color-must-be-an-rgb-array",
        ),
        (
            "<< /Color [ 1 2 (x) ] >>",
            "printkit-color-must-be-an-rgb-array",
        ),
        (
            "<< /Color [ 1.5 0 0 ] >>",
            "printkit-color-components-must-be-0-to-1",
        ),
        (
            "<< /Color [ -0.1 0 0 ] >>",
            "printkit-color-components-must-be-0-to-1",
        ),
    ];
    for preset in ["woodcut", "linocut", "engraving"] {
        for (opts, expected) in cases {
            let got = printkit_err(&format!("{RECT} {opts} {preset}"));
            assert_eq!(got, *expected, "{preset} {opts}");
        }
    }
}

#[test]
fn a_seed_at_the_documented_boundary_succeeds_uniformly_across_presets() {
    // Codex review, PR #128: `/Seed` was previously unvalidated, so a
    // value near scatter's own +/-2147483647 bound succeeded for
    // `engraving` (max sub-call offset 1) but failed for
    // `woodcut`/`linocut` (max sub-call offset 2) with *scatter's*
    // own error name, not printkit's -- an inconsistency between
    // presets a caller had no way to predict. `propts` now rejects
    // anything past 2147483645 for every preset uniformly (this
    // file's header documents the number), so the boundary itself
    // must still work for all three.
    for preset in ["woodcut", "linocut", "engraving"] {
        let mut it = with_lib(150, 150);
        it.run_str(&format!("{RECT} << /Seed 2147483645 >> {preset}"))
            .unwrap_or_else(|e| panic!("{preset}: boundary seed failed: {}", it.error_report(&e)));
    }
}

#[test]
fn a_sibling_shaped_scale_value_is_rejected_under_printkits_own_name() {
    // /Scale here is a single positive number, not scatter's own
    // [lo hi] range -- a caller reaching for the sibling-shaped value
    // out of habit must hit printkit's own error, not scatter's or
    // hatch's, and must never silently misbehave.
    let got = printkit_err(&format!("{RECT} << /Scale [ 0.4 1.4 ] >> woodcut"));
    assert_eq!(got, "printkit-scale-must-be-a-number");
}

#[test]
fn a_procedure_standing_in_for_color_cannot_auto_execute() {
    // The array-boxing discipline hatchkit/surfacekit both learned
    // from Codex review: a caller-supplied procedure passed as a
    // would-be numeric/array option must be type-checked, never
    // auto-executed, before this file's own validation ever runs.
    let got = printkit_err(&format!("{RECT} << /Color {{ /boom cvx exec }} >> woodcut"));
    assert_eq!(got, "printkit-color-must-be-an-rgb-array");
}

// --- specimen sheet -------------------------------------------------------

#[test]
fn the_printkit_specimen_sheet_renders_ink_in_every_panel() {
    let source = std::fs::read("examples/printkit.ps").expect("read the specimen");
    let mut it = Interp::with_page(1000, 360).expect("page");
    it.run_source(&source)
        .unwrap_or_else(|e| panic!("examples/printkit.ps failed: {}", it.error_report(&e)));
    assert!(it.gfx().page_shown, "showpage must have run");

    // Inset well clear of each panel's own 0.75pt border stroke --
    // the halftonekit/stipplekit lesson: counting the frame alone
    // would let a silently broken panel still pass.
    const INSET: u32 = 10;
    for (label, x0) in [
        ("woodcut", 40),
        ("linocut", 280),
        ("engraving", 520),
        ("paper", 760),
    ] {
        let mut marked = 0;
        for dx in INSET..(200 - INSET) {
            for dy in INSET..(200 - INSET) {
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
            marked > 50,
            "{label} panel looks blank ({marked} non-white interior pixels)"
        );
    }
}

#[test]
fn every_example_tag_runs_clean() {
    let src = std::fs::read_to_string("lib/printkit.ps").expect("printkit present");
    let examples: Vec<&str> = src
        .lines()
        .filter_map(|l| l.trim().strip_prefix("% @example:"))
        .map(str::trim)
        .collect();
    assert_eq!(examples.len(), 3, "expected one @example: per preset");
    for example in examples {
        let mut it = with_lib(200, 200);
        it.run_str(example)
            .unwrap_or_else(|e| panic!("@example {example:?} failed: {}", it.error_report(&e)));
    }
}

#[test]
fn ghostscript_accepts_the_printkit_specimen_sheet() {
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
            "-g1000x360",
            "-r72",
            "-o/dev/null",
            "examples/printkit.ps",
        ])
        .status()
        .expect("run gs");
    assert!(status.success(), "gs rejected examples/printkit.ps");
}
