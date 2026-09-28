//! Psychedelic display lettering (issue #144, `lib/lettering.ps`):
//! determinism, "the seed moves texture and nothing else", the
//! never-substitute font guard, containment of the interior, and that
//! a call leaves the caller's operand stack and graphics state alone.

use pscat::{Interp, PsError};

fn with_lib(w: u32, h: u32) -> Interp {
    let mut it = Interp::with_page(w, h).expect("page");
    for f in ["lib/artkit.ps", "lib/lettering.ps"] {
        let src = std::fs::read(f).expect("lib present");
        it.run_source(&src)
            .unwrap_or_else(|e| panic!("{f} failed: {}", it.error_report(&e)));
    }
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

/// Run `src` and return whatever it leaves on the operand stack.
fn stack_of(src: &str) -> Vec<String> {
    let mut it = with_lib(400, 200);
    it.run_str(src)
        .unwrap_or_else(|e| panic!("eval of {src:?} failed: {}", it.error_report(&e)));
    it.operand_stack().iter().map(|o| o.repr()).collect()
}

fn render(opts: &str) -> Vec<u8> {
    let mut it = with_lib(400, 200);
    run(&mut it, "1 setgray clippath fill");
    run(&mut it, &format!("(Real) 200 60 << {opts} >> psyletter"));
    it.gfx()
        .pixmap
        .pixels()
        .iter()
        .flat_map(|p| [p.red(), p.green(), p.blue()])
        .collect()
}

/// One bool per pixel: is it anything but the white page?
fn silhouette(rgb: &[u8]) -> Vec<bool> {
    rgb.chunks(3).map(|p| p != [255, 255, 255]).collect()
}

fn lettering_err(src: &str) -> String {
    let mut it = with_lib(400, 200);
    match it.run_str(src).unwrap_err() {
        PsError::Undefined(name) => name,
        other => panic!("expected a self-documenting undefined name, got {other}"),
    }
}

#[test]
fn identical_inputs_render_identically() {
    let a = render("/Size 120 /Treatment /mottled /Seed 5");
    let b = render("/Size 120 /Treatment /mottled /Seed 5");
    assert_eq!(a, b);
}

#[test]
fn seed_changes_texture_but_not_silhouette_or_metrics() {
    let a = render("/Size 120 /Treatment /mottled /Seed 1");
    let b = render("/Size 120 /Treatment /mottled /Seed 2");
    assert_ne!(a, b, "a different seed must change the texture");
    assert_eq!(
        silhouette(&a),
        silhouette(&b),
        "outline, position and metrics must not depend on the seed"
    );
    // The geometry half is seed-free by construction; pin its bbox too.
    let bbox = |seed: i32| {
        stack_of(&format!(
            "newpath (Real) 200 60 << /Size 120 /Seed {seed} >> psyletterpath pathbbox"
        ))
    };
    assert_eq!(bbox(1), bbox(2));
}

#[test]
fn every_treatment_draws_and_stays_in_the_silhouette() {
    let solid = silhouette(&render("/Size 120 /Treatment /solid"));
    assert!(solid.iter().filter(|b| **b).count() > 2000, "letters drawn");
    for t in ["transition", "patches", "mottled"] {
        let s = silhouette(&render(&format!("/Size 120 /Treatment /{t} /Seed 3")));
        assert_eq!(s, solid, "{t} must not paint outside the glyphs+contour");
    }
}

#[test]
fn treatments_actually_differ_and_use_the_palette() {
    let solid = render("/Size 120 /Treatment /solid");
    for t in ["transition", "patches", "mottled"] {
        assert_ne!(solid, render(&format!("/Size 120 /Treatment /{t}")), "{t}");
    }
    // Default palette is red/green/blue: the patches treatment must
    // actually put a green- and a blue-dominant pixel on the page.
    let p = render("/Size 120 /Treatment /patches /Seed 3");
    assert!(
        p.chunks(3).any(|c| c[1] > 120 && c[0] < 60 && c[2] < 90),
        "green"
    );
    assert!(p.chunks(3).any(|c| c[2] > 150 && c[0] < 60), "blue");
}

#[test]
fn contour_is_dark_and_zero_width_omits_it() {
    let with = render("/Size 120 /Treatment /solid");
    let without = render("/Size 120 /Treatment /solid /OutlineWidth 0");
    let dark = |v: &[u8]| v.chunks(3).filter(|c| c[0] < 40 && c[1] < 40).count();
    assert!(dark(&with) > 500);
    assert_eq!(dark(&without), 0);
}

#[test]
fn missing_font_is_an_error_not_a_substitution() {
    assert_eq!(
        lettering_err("(Real) 100 100 << /Font /NoSuchFace >> psyletter"),
        "lettering-font-not-found"
    );
    assert_eq!(
        lettering_err("newpath (Real) 100 100 << /Font /NoSuchFace >> psyletterpath"),
        "lettering-font-not-found"
    );
}

#[test]
fn catalog_face_name_and_stem_both_resolve() {
    for f in ["Bungee", "Bungee-Regular", "Helvetica"] {
        let mut it = with_lib(400, 200);
        run(
            &mut it,
            &format!("(Real) 200 60 << /Font /{f} /Size 80 >> psyletter"),
        );
    }
}

#[test]
fn width_fits_the_text_and_align_anchors_it() {
    let bb = |opts: &str| -> Vec<f64> {
        stack_of(&format!(
            "newpath (Real) 50 60 << {opts} >> psyletterpath pathbbox"
        ))
        .iter()
        .map(|r| r.parse().expect("number"))
        .collect()
    };
    let left = bb("/Width 300 /Align /left");
    // The advance width is 300, so the ink sits within [50, 350] and
    // reaches most of the way across it.
    assert!(left[0] >= 45.0 && left[2] <= 355.0, "{left:?}");
    assert!(left[2] - left[0] > 250.0, "{left:?}");
    let right = bb("/Width 300 /Align /right");
    assert!(right[2] <= 55.0 && right[0] >= -255.0, "{right:?}");
    assert!((right[2] - right[0] - (left[2] - left[0])).abs() < 0.01);
}

#[test]
fn bad_options_raise_self_documenting_errors() {
    for (src, want) in [
        (
            "(Real) 1 1 << /Treatment /plaid >> psyletter",
            "lettering-treatment-must-be-solid-transition-patches-or-mottled",
        ),
        (
            "(Real) 1 1 << /Size (big) >> psyletter",
            "lettering-size-must-be-a-number",
        ),
        (
            "(Real) 1 1 << /Palette /nope >> psyletter",
            "lettering-unknown-palette",
        ),
        (
            "(Real) 1 1 << /Palette [[1 0]] >> psyletter",
            "lettering-palette-entries-must-be-rgb",
        ),
        (
            "(Real) 1 1 << /Outline [1 2] >> psyletter",
            "lettering-outline-must-be-an-rgb-array",
        ),
        (
            "(Real) 1 1 << /Align /up >> psyletter",
            "lettering-align-must-be-left-center-or-right",
        ),
        ("(Real) 1 1 42 psyletter", "lettering-opts-must-be-a-dict"),
        ("42 1 1 << >> psyletter", "lettering-text-must-be-a-string"),
    ] {
        assert_eq!(lettering_err(src), want, "{src}");
    }
}

#[test]
fn leaves_stack_state_and_caller_random_stream_alone() {
    let after = |call: &str| {
        stack_of(&format!(
            "0.1 0.2 0.3 setrgbcolor /Courier findfont 9 scalefont setfont 42 srand \
             {call} currentrgbcolor currentfont /FontName get 20 string cvs rand"
        ))
    };
    let plain = after("");
    let drawn = after("(Real) 200 60 << /Seed 9 /Treatment /mottled >> psyletter");
    assert_eq!(plain, drawn, "colour, font and the caller's rand stream");
    assert_eq!(plain.len(), 5, "rgb + font name + rand");
}
