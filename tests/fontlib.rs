//! The font library (lib/fonts/, Stages 15 and 18): every face loads,
//! inks, reproduces under a seed, and is accepted by Ghostscript. The
//! art itself is rand-driven for several faces, so pixel comparison
//! with gs is excluded (the corpus policy); Lapidary and Circuitry are
//! deliberately rand-free but stay under the same structural tests.

use pscat::Interp;

// (file, font name, minimum inked pixels for the AWX9! probe — stars
// are points, so Constellation legitimately inks far less than tubes)
const FONTS: [(&str, &str, usize); 7] = [
    ("lib/fonts/neon.ps", "Neon", 400),
    ("lib/fonts/marquee.ps", "Marquee", 400),
    ("lib/fonts/constellation.ps", "Constellation", 80),
    ("lib/fonts/lapidary.ps", "Lapidary", 400),
    ("lib/fonts/circuitry.ps", "Circuitry", 400),
    ("lib/fonts/stitchwork.ps", "Stitchwork", 400),
    ("lib/fonts/confetti.ps", "Confetti", 400),
];

fn ink_count(it: &Interp, dark_ground: bool) -> usize {
    it.gfx()
        .pixmap
        .data()
        .chunks_exact(4)
        .filter(|p| {
            if dark_ground {
                p[0] > 90 || p[1] > 90 || p[2] > 90 // lit pixels on night
            } else {
                p[0] < 160 // stone on pale ground
            }
        })
        .count()
}

#[test]
fn every_font_loads_and_inks() {
    for (path, name, min_ink) in FONTS {
        let src = std::fs::read(path).expect("font file present");
        let mut it = Interp::with_page(400, 200).expect("page");
        it.run_source(&src)
            .unwrap_or_else(|e| panic!("{path} failed to load: {}", it.error_report(&e)));
        let dark = !matches!(name, "Lapidary" | "Stitchwork");
        let ground = if dark {
            "0.02 0.02 0.05 setrgbcolor clippath fill"
        } else {
            "0.93 0.91 0.87 setrgbcolor clippath fill"
        };
        it.run_str(&format!(
            "2 srand {ground} /{name} findfont 70 scalefont setfont 20 60 moveto (AWX9!) show"
        ))
        .unwrap_or_else(|e| panic!("{name} show failed: {}", it.error_report(&e)));
        let ink = ink_count(&it, dark);
        assert!(ink > min_ink, "{name} left visible ink, got {ink} pixels");
    }
}

#[test]
fn both_cases_render_the_same_capitals() {
    // The library maps a..z onto the capital glyphs; Lapidary is
    // rand-free so the pages must match exactly.
    let src = std::fs::read("lib/fonts/lapidary.ps").expect("font file");
    let render = |text: &str| {
        let mut it = Interp::with_page(300, 120).expect("page");
        it.run_source(&src).expect("load");
        it.run_str(&format!(
            "1 setgray clippath fill /Lapidary findfont 60 scalefont setfont 10 30 moveto ({text}) show"
        ))
        .expect("show");
        it.gfx().pixmap.data().to_vec()
    };
    assert_eq!(
        render("Mixed"),
        render("MIXED"),
        "case-insensitive capitals"
    );
}

#[test]
fn seeded_pages_reproduce() {
    // Marquee and Constellation are rand-driven; under srand the same
    // page must come back (the reproducible-art doctrine).
    for font in ["Marquee", "Constellation", "Stitchwork", "Confetti"] {
        let path = format!("lib/fonts/{}.ps", font.to_lowercase());
        let src = std::fs::read(&path).expect("font file");
        let render = || {
            let mut it = Interp::with_page(400, 150).expect("page");
            it.run_source(&src).expect("load");
            it.run_str(&format!(
                "7 srand 0 setgray clippath fill /{font} findfont 80 scalefont setfont 15 40 moveto (RSVP) show"
            ))
            .expect("show");
            it.gfx().pixmap.data().to_vec()
        };
        assert_eq!(render(), render(), "{font}: same seed, same sign");
    }
}

#[test]
fn the_specimen_poster_renders_all_four_bands() {
    let src = std::fs::read("examples/font_library.ps").expect("specimen present");
    let mut it = Interp::with_page(612, 792).expect("page");
    it.run_source(&src)
        .unwrap_or_else(|e| panic!("font_library.ps failed: {}", it.error_report(&e)));
    assert_eq!(it.gfx().pages().len(), 1);
    let page = &it.gfx().pages()[0];
    // One probe per band: something must be lit (dark bands) or
    // carved (the pale band) in each 198pt strip.
    let band_has = |y0: u32, y1: u32, pred: &dyn Fn(u8, u8, u8) -> bool| {
        for y in (y0..y1).step_by(2) {
            for x in (0..612).step_by(2) {
                let p = page.pixel(x, y).expect("in bounds");
                if pred(p.red(), p.green(), p.blue()) {
                    return true;
                }
            }
        }
        false
    };
    let lit = |r: u8, g: u8, b: u8| r > 120 || g > 120 || b > 120;
    let carved = |r: u8, _g: u8, _b: u8| r < 150;
    assert!(band_has(0, 198, &lit), "neon band lit");
    assert!(band_has(198, 396, &lit), "marquee band lit");
    assert!(band_has(396, 594, &lit), "constellation band lit");
    assert!(band_has(594, 792, &carved), "lapidary band carved");
}

#[test]
fn the_second_folio_renders_all_three_bands() {
    let src = std::fs::read("examples/font_library2.ps").expect("specimen present");
    let mut it = Interp::with_page(612, 792).expect("page");
    it.run_source(&src)
        .unwrap_or_else(|e| panic!("font_library2.ps failed: {}", it.error_report(&e)));
    assert_eq!(it.gfx().pages().len(), 1);
    let page = &it.gfx().pages()[0];
    let band_has = |y0: u32, y1: u32, pred: &dyn Fn(u8, u8, u8) -> bool| {
        for y in (y0..y1).step_by(2) {
            for x in (0..612).step_by(2) {
                let p = page.pixel(x, y).expect("in bounds");
                if pred(p.red(), p.green(), p.blue()) {
                    return true;
                }
            }
        }
        false
    };
    let lit = |r: u8, g: u8, b: u8| r > 120 || g > 120 || b > 120;
    let stitched = |_r: u8, g: u8, _b: u8| g < 120; // floss on cream cloth
    assert!(band_has(0, 264, &lit), "circuitry band lit");
    assert!(band_has(264, 528, &stitched), "stitchwork band sewn");
    assert!(band_has(528, 792, &lit), "confetti band lit");
}

#[test]
fn ghostscript_accepts_every_font() {
    let gs_ok = std::process::Command::new("gs")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success());
    if !gs_ok {
        eprintln!("skipping gs compatibility check: gs not installed");
        return;
    }
    let dir = std::env::temp_dir().join(format!("pscat-fontlib-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    for (path, name, _) in FONTS {
        let lib = std::fs::read_to_string(path).expect("font file");
        let driver = format!(
            "3 srand 0.5 setgray clippath fill\n\
             /{name} findfont 60 scalefont setfont\n\
             10 100 moveto (ABC xyz 123!?) show showpage\n"
        );
        let combined = dir.join(format!("{name}.ps"));
        std::fs::write(&combined, format!("{lib}\n{driver}")).expect("write");
        let status = std::process::Command::new("gs")
            .args([
                "-dNOPAUSE",
                "-dBATCH",
                "-q",
                "-sDEVICE=png16m",
                "-g400x200",
                "-r72",
                "-o/dev/null",
            ])
            .arg(&combined)
            .status()
            .expect("run gs");
        assert!(status.success(), "gs rejected {path}");
    }
}
