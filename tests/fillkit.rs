//! GIMP-style fills (issue #154, `lib/fillkit.ps`): determinism, seed
//! behaviour, containment in the caller's clip, that a call leaves the
//! caller's `rand` stream / artkit noise table / graphics state alone,
//! self-documenting errors, and the `psyletter` fill treatments.
//!
//! Debug-build tests, so every fill here uses a tiny /Resolution.

use pscat::{Interp, PsError};

const FILLS: [(&str, &str); 4] = [
    ("plasma", "plasmafill"),
    ("solid", "solidnoisefill"),
    ("diff", "diffcloudsfill"),
    ("grad", "gradshapefill"),
];

fn with_libs(w: u32, h: u32, libs: &[&str]) -> Interp {
    let mut it = Interp::with_page(w, h).expect("page");
    for f in libs {
        let src = std::fs::read(f).expect("lib present");
        it.run_source(&src)
            .unwrap_or_else(|e| panic!("{f} failed: {}", it.error_report(&e)));
    }
    it
}

fn kit(w: u32, h: u32) -> Interp {
    with_libs(w, h, &["lib/artkit.ps", "lib/fillkit.ps"])
}

fn run(it: &mut Interp, src: &str) {
    it.run_str(src)
        .unwrap_or_else(|e| panic!("eval of {src:?} failed: {}", it.error_report(&e)));
}

fn rgb(it: &Interp) -> Vec<u8> {
    it.gfx()
        .pixmap
        .pixels()
        .iter()
        .flat_map(|p| [p.red(), p.green(), p.blue()])
        .collect()
}

/// White page, then one fill over the whole 60x40 page.
fn paint(proc_name: &str, opts: &str) -> Vec<u8> {
    let mut it = kit(60, 40);
    run(&mut it, "1 setgray clippath fill");
    run(
        &mut it,
        &format!("0 0 60 40 << /Resolution 16 {opts} >> {proc_name}"),
    );
    assert!(it.operand_stack().is_empty());
    rgb(&it)
}

fn err_of(src: &str) -> String {
    let mut it = kit(60, 40);
    match it.run_str(src).unwrap_err() {
        PsError::Undefined(name) => name,
        other => panic!("expected a self-documenting undefined name, got {other}"),
    }
}

#[test]
fn same_seed_same_pixels_different_seed_differs() {
    for (name, proc_name) in FILLS {
        let a = paint(proc_name, "/Seed 5");
        let b = paint(proc_name, "/Seed 5");
        assert_eq!(a, b, "{name}: identical inputs must render identically");
        if name != "grad" {
            // a gradient shape has no random field; only grain/spread do
            assert_ne!(
                a,
                paint(proc_name, "/Seed 6"),
                "{name}: seed must change the field"
            );
        }
        assert_ne!(a, vec![255; a.len()], "{name}: something was painted");
    }
    // grain is seeded too
    let g = |seed: u32| paint("gradshapefill", &format!("/Seed {seed} /RGBNoise 0.2"));
    assert_eq!(g(1), g(1));
    assert_ne!(g(1), g(2));
}

#[test]
fn grain_options_off_by_default_and_never_change_the_field_when_zero() {
    for (_, proc_name) in FILLS {
        assert_eq!(
            paint(proc_name, "/Seed 4"),
            paint(proc_name, "/Seed 4 /RGBNoise 0 /Spread 0 /HSVNoise [0 0 0]"),
        );
    }
    assert_ne!(
        paint("solidnoisefill", "/Seed 4"),
        paint("solidnoisefill", "/Seed 4 /HSVNoise [0.1 0.2 0.2]")
    );
    assert_ne!(
        paint("solidnoisefill", "/Seed 4"),
        paint("solidnoisefill", "/Seed 4 /Spread 0.1")
    );
}

#[test]
fn a_palette_blends_a_ramp_and_plasma_defaults_to_random_rgb() {
    // red -> blue ramp: green channel is exactly 0 wherever it painted
    for (_, proc_name) in FILLS {
        let px = paint(proc_name, "/Seed 2 /Palette [[1 0 0] [0 0 1]]");
        assert!(
            px.chunks(3).all(|p| p[1] == 0),
            "{proc_name}: off-ramp colour"
        );
        assert!(px.chunks(3).any(|p| p[0] > 0 || p[2] > 0));
    }
    let px = paint("plasmafill", "/Seed 2");
    assert!(
        px.chunks(3).any(|p| p[1] > 40),
        "random-RGB plasma should use green"
    );
    // named palettes resolve too
    let named = paint("plasmafill", "/Seed 2 /Palette /dusk");
    assert_ne!(named, px);
}

#[test]
fn a_fill_stays_inside_the_callers_clip() {
    let mut it = kit(60, 40);
    run(&mut it, "1 setgray clippath fill");
    run(
        &mut it,
        "gsave newpath 20 10 moveto 40 10 lineto 30 30 lineto closepath clip
         0 0 60 40 << /Resolution 16 /Palette [[0 0 0] [0.2 0.2 0.2]] >> plasmafill grestore",
    );
    let px = rgb(&it);
    let w = 60;
    // the page corners are far outside the triangle
    for (x, y) in [(2, 2), (57, 2), (2, 37), (57, 37), (5, 20), (54, 20)] {
        let i = (y * w + x) * 3;
        assert_eq!(&px[i..i + 3], [255, 255, 255], "leaked at ({x},{y})");
    }
    // ... and the middle of the triangle is dark
    let i = (22 * w + 30) * 3;
    assert!(px[i] < 80, "fill missing inside the clip");
}

#[test]
fn caller_rand_noise_table_and_graphics_state_are_untouched() {
    for (name, proc_name) in FILLS {
        let mut it = kit(60, 40);
        let truthy = |it: &mut Interp, src: &str| {
            it.run_str(src)
                .unwrap_or_else(|e| panic!("{src}: {}", it.error_report(&e)));
            let top = it.operand_stack().last().unwrap().repr();
            it.run_str("pop").unwrap();
            top == "true"
        };
        run(&mut it, "7 srand noiseinit 0.3 0.7 noise2 /n0 exch def");
        run(&mut it, "7 srand /want [rand rand rand] def");
        run(
            &mut it,
            "0.25 0.5 0.75 setrgbcolor 3 setlinewidth /Times-Roman findfont 9 scalefont setfont",
        );
        run(&mut it, "newpath 1 2 moveto 3 4 lineto");
        run(&mut it, "7 srand");
        run(
            &mut it,
            &format!("0 0 60 40 << /Seed 99 /Resolution 12 /RGBNoise 0.1 >> {proc_name}"),
        );
        run(
            &mut it,
            "/got [rand rand rand] def 0.3 0.7 noise2 /n1 exch def",
        );
        assert!(
            truthy(&mut it, "n0 n1 eq"),
            "{name}: caller's noise table replaced"
        );
        assert!(
            truthy(
                &mut it,
                "want 0 get got 0 get eq want 1 get got 1 get eq and want 2 get got 2 get eq and"
            ),
            "{name}: caller's rand stream disturbed"
        );
        assert!(
            truthy(&mut it, "currentlinewidth 3 eq"),
            "{name}: linewidth"
        );
        assert!(
            truthy(
                &mut it,
                "currentrgbcolor 0.75 eq 3 1 roll 0.5 eq 3 1 roll 0.25 eq and and"
            ),
            "{name}: colour"
        );
        assert!(
            truthy(&mut it, "currentfont /FontName get (Times-Roman) cvn eq"),
            "{name}: font"
        );
        assert!(
            truthy(&mut it, "currentpoint 4 eq exch 3 eq and"),
            "{name}: path"
        );
        assert!(it.operand_stack().is_empty(), "{name}: stack");
    }
}

#[test]
fn self_documenting_errors() {
    let cases = [
        ("0 0 10 10 3 plasmafill", "fillkit-opts-must-be-a-dict"),
        (
            "(a) 0 10 10 << >> plasmafill",
            "fillkit-x0-must-be-a-number",
        ),
        (
            "0 0 0 10 << >> plasmafill",
            "fillkit-box-must-have-positive-width",
        ),
        (
            "0 5 10 5 << >> plasmafill",
            "fillkit-box-must-have-positive-height",
        ),
        (
            "0 0 10 10 << /Seed (x) >> plasmafill",
            "fillkit-seed-must-be-a-number",
        ),
        (
            "0 0 10 10 << /Palette /nope >> plasmafill",
            "fillkit-unknown-palette",
        ),
        (
            "0 0 10 10 << /Palette [] >> plasmafill",
            "fillkit-palette-must-not-be-empty",
        ),
        (
            "0 0 10 10 << /Palette [[1 2]] >> plasmafill",
            "fillkit-palette-entries-must-be-rgb",
        ),
        (
            "0 0 10 10 << /Resolution /huge >> plasmafill",
            "fillkit-resolution-must-be-a-number-or-device",
        ),
        (
            "0 0 10 10 << /HSVNoise [1 2] >> plasmafill",
            "fillkit-hsvnoise-must-be-an-array-of-3-numbers",
        ),
        (
            "0 0 10 10 << /Scale 0 >> solidnoisefill",
            "fillkit-scale-must-be-positive",
        ),
        (
            "0 0 10 10 << /Turbulent 1 >> solidnoisefill",
            "fillkit-turbulent-must-be-a-bool",
        ),
        (
            "0 0 10 10 << /Shape /blob >> gradshapefill",
            "fillkit-shape-must-be-bilinear-square-conical-or-spiral",
        ),
    ];
    for (src, want) in cases {
        assert_eq!(err_of(src), want, "{src}");
    }
}

#[test]
fn errors_are_raised_before_the_seed_or_graphics_state_change() {
    let mut it = kit(60, 40);
    run(&mut it, "5 srand 0.5 setgray /want [rand rand] def 5 srand");
    assert!(
        it.run_str("0 0 10 10 << /Palette /nope /Seed 99 >> plasmafill")
            .is_err()
    );
    run(&mut it, "clear /got [rand rand] def");
    run(
        &mut it,
        "want 0 get got 0 get eq want 1 get got 1 get eq and currentgray 0.5 sub abs 0.001 lt and",
    );
    assert_eq!(it.operand_stack().last().unwrap().repr(), "true");
}

fn lettering(fill_libs: bool, opts: &str) -> Result<Vec<u8>, String> {
    let libs: &[&str] = if fill_libs {
        &["lib/artkit.ps", "lib/fillkit.ps", "lib/lettering.ps"]
    } else {
        &["lib/artkit.ps", "lib/lettering.ps"]
    };
    let mut it = with_libs(300, 140, libs);
    run(&mut it, "1 setgray clippath fill");
    it.run_str(&format!("(Real) 150 30 << /Size 100 {opts} >> psyletter"))
        .map_err(|e| match e {
            PsError::Undefined(n) => n,
            other => other.to_string(),
        })?;
    Ok(rgb(&it))
}

#[test]
fn psyletter_fill_treatments_are_deterministic_and_seeded() {
    for t in ["plasma", "clouds", "diffclouds", "conical"] {
        let o = |seed: u32| format!("/Treatment /{t} /Seed {seed} /Fill << /Resolution 24 >>");
        let a = lettering(true, &o(3)).unwrap();
        assert_eq!(a, lettering(true, &o(3)).unwrap(), "{t}");
        if t != "conical" {
            assert_ne!(a, lettering(true, &o(4)).unwrap(), "{t}: seed");
        }
    }
}

#[test]
fn psyletter_fill_treatments_stay_inside_the_letters() {
    let solid = lettering(true, "/Treatment /solid /Palette [[0 0 0]]").unwrap();
    for t in ["plasma", "clouds", "diffclouds", "conical"] {
        let px = lettering(true, &format!("/Treatment /{t} /Fill << /Resolution 24 >>")).unwrap();
        for (i, (a, b)) in px.chunks(3).zip(solid.chunks(3)).enumerate() {
            if a != [255, 255, 255] {
                assert_ne!(
                    b,
                    [255, 255, 255],
                    "{t}: paint outside the glyphs at pixel {i}"
                );
            }
        }
        assert_ne!(px, vec![255; px.len()], "{t}: painted nothing");
    }
}

#[test]
fn psyletter_plasma_uses_palette_only_when_given_and_fill_overrides() {
    let random = lettering(true, "/Treatment /plasma /Fill << /Resolution 24 >>").unwrap();
    let ramp = lettering(
        true,
        "/Treatment /plasma /Palette [[1 0 0] [0 0 1]] /Fill << /Resolution 24 >>",
    )
    .unwrap();
    assert_ne!(random, ramp);
    // the ramp interior has no green (contour is near-black, edges blend
    // toward the white page, so allow only tiny green from the outline)
    let greenish = ramp
        .chunks(3)
        .filter(|p| p[1] as i32 > p[0].max(p[2]) as i32 + 12)
        .count();
    assert_eq!(greenish, 0);
    let over = lettering(
        true,
        "/Treatment /plasma /Seed 1 /Fill << /Resolution 24 /Seed 9 >>",
    )
    .unwrap();
    assert_eq!(
        over,
        lettering(
            true,
            "/Treatment /plasma /Seed 9 /Fill << /Resolution 24 >>"
        )
        .unwrap()
    );
}

#[test]
fn psyletter_fill_treatment_without_fillkit_is_a_clear_error() {
    assert_eq!(
        lettering(false, "/Treatment /plasma").unwrap_err(),
        "lettering-treatment-needs-fillkit"
    );
    assert_eq!(
        lettering(true, "/Treatment /plasma /Fill 3").unwrap_err(),
        "lettering-fill-must-be-a-dict"
    );
}

/// Mean absolute difference between horizontally adjacent pixels' red channel.
fn roughness(px: &[u8]) -> f64 {
    let (w, h) = (60usize, 40usize);
    let mut sum = 0.0;
    for y in 0..h {
        for x in 0..w - 1 {
            sum += (px[(y * w + x) * 3] as f64 - px[(y * w + x + 1) * 3] as f64).abs();
        }
    }
    sum / ((w - 1) * h) as f64
}

#[test]
fn plasma_turbulence_controls_roughness() {
    let rough = |t: &str| {
        (1..=3)
            .map(|seed| {
                roughness(&paint(
                    "plasmafill",
                    &format!(
                        "/Seed {seed} /Palette [[0 0 0] [1 1 1]] /Turbulence {t} /Resolution 60"
                    ),
                ))
            })
            .sum::<f64>()
    };
    let (lo, mid, hi) = (rough("0.2"), rough("1"), rough("5"));
    assert!(
        lo < mid && mid < hi,
        "roughness should rise with turbulence: {lo} {mid} {hi}"
    );
}

#[test]
fn resolution_device_matches_an_explicit_resolution() {
    // 20x20 box at 72 dpi = 20 device pixels per side
    let paint20 = |res: &str| {
        let mut it = kit(30, 30);
        run(&mut it, "1 setgray clippath fill");
        run(
            &mut it,
            &format!("5 5 25 25 << /Seed 3 /Resolution {res} >> plasmafill"),
        );
        rgb(&it)
    };
    assert_eq!(paint20("/device"), paint20("20"));
    assert_ne!(paint20("/device"), paint20("9"));
}

#[test]
fn device_resolution_exceeds_the_numeric_cap_for_print_sized_boxes() {
    // 600 device pixels wide: a numeric /Resolution stays clamped to 512
    // (so 600 == 512), while /device keeps one sample per pixel instead of
    // upscaling 512 samples in blocks (issue #162).
    let paint = |res: &str| {
        let mut it = kit(600, 8);
        run(&mut it, "1 setgray clippath fill");
        run(
            &mut it,
            &format!("0 0 600 8 << /Seed 3 /Resolution {res} >> plasmafill"),
        );
        rgb(&it)
    };
    assert_eq!(paint("600"), paint("512"));
    assert_ne!(paint("/device"), paint("512"));
}

#[test]
fn gradient_shapes_survive_a_sample_exactly_on_the_centre() {
    // a square box at /Resolution 9: the middle sample sits exactly on
    // /Center [0.5 0.5], where atan(0,0) would be undefinedresult
    for shape in ["conical", "spiral", "bilinear", "square"] {
        let mut it = kit(30, 30);
        run(&mut it, "1 setgray clippath fill");
        run(
            &mut it,
            &format!("0 0 30 30 << /Shape /{shape} /Resolution 9 /Palette /dusk >> gradshapefill"),
        );
        let px = rgb(&it);
        assert_ne!(px, vec![255; px.len()], "{shape}");
        assert!(it.operand_stack().is_empty(), "{shape}");
    }
}
