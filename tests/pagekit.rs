//! Parameterized page templates (issue #18, `lib/pagekit.ps`): five
//! `x y w h dict pgNAME` procs built on artkit's paragraph flow and
//! shapes. Two failure modes drove these tests directly: artkit's
//! `fitfont` scales unconditionally, which would blow a short string
//! (a two-letter awardee name, a short headline) up to fill its
//! target width instead of leaving it alone, so `pgzfitmax` clamps
//! the ratio at 1.0 -- tested directly, not just through a template.
//! And artkit's eight mood palettes are documented only as "five
//! colors," not as ordered by lightness -- `/parchment` runs light to
//! dark and `/carnival` is a hue wheel, so pagekit registers its own
//! `/vellum`/`/marigold`, each checked here for a monotonic dark-to-
//! light luminance ramp rather than trusted by inspection.

use pscat::Interp;

fn load(it: &mut Interp) {
    for path in ["lib/artkit.ps", "lib/pagekit.ps"] {
        let src = std::fs::read(path).unwrap_or_else(|e| panic!("{path}: {e}"));
        it.run_source(&src)
            .unwrap_or_else(|e| panic!("{path} failed to load: {}", it.error_report(&e)));
    }
}

// Luminance, not a per-channel threshold: pagekit's background tints are
// deliberately light overall but can still dip a single channel low (e.g.
// /ember's tint has a blue channel under 128 despite reading as pale
// yellow), which a per-channel OR check misclassifies as "ink" -- found
// by this exact test failing against pgposter before the switch.
fn ink_count(it: &Interp) -> usize {
    it.gfx()
        .pixmap
        .pixels()
        .iter()
        .filter(|p| {
            let lum = 0.3 * p.red() as f32 + 0.59 * p.green() as f32 + 0.11 * p.blue() as f32;
            lum < 180.0
        })
        .count()
}

fn fresh(w: u32, h: u32) -> Interp {
    let mut it = Interp::with_page(w, h).expect("page");
    load(&mut it);
    it.run_str("1 1 1 setrgbcolor clippath fill")
        .expect("white background");
    it
}

const TEMPLATES: [(&str, &str); 5] = [
    (
        "pgcard",
        "10 10 180 180 << /Title (Hi) /Body (a quick note) /Signoff (- me) >> pgcard",
    ),
    (
        "pgletter",
        "10 10 180 180 << /Date (Jan 1) /Salutation (Dear Sam,) \
         /Body (thanks for everything) /Signoff (Best, J) >> pgletter",
    ),
    (
        "pgcertificate",
        "10 10 180 180 << /Awardee (A. Lovelace) /Body (for excellence) \
         /Presenter (C. Babbage) /Date (1843) >> pgcertificate",
    ),
    (
        "pginvitation",
        "10 10 180 180 << /Title (Party) /When (Sat) /Where (Here) \
         /Host (Us) /Body (come by) >> pginvitation",
    ),
    (
        "pgposter",
        "10 10 180 180 << /Title (BIG) /Tagline (small) \
         /Body (poster body copy) /Footer (fine print) >> pgposter",
    ),
];

#[test]
fn loads_clean_and_registers_palettes() {
    let mut it = Interp::with_page(50, 50).expect("page");
    load(&mut it);
    assert_eq!(ink_count(&it), 0, "pagekit drew on load");
    it.run_str("Palettes length").expect("palettes");
    let got: Vec<_> = it.operand_stack().iter().map(|o| o.repr()).collect();
    assert_eq!(got, ["10"], "pagekit should add 2 palettes to artkit's 8");
}

#[test]
fn every_template_draws_its_content() {
    // A light background tint alone can clear a loose ink threshold, so
    // this compares each template's filled-content render against the
    // same template on an empty dict -- proving text/labels actually
    // land, not just that a background got painted.
    for (name, call) in TEMPLATES {
        let mut it_empty = fresh(200, 200);
        it_empty
            .run_str(&format!("10 10 180 180 << >> {name} pop"))
            .unwrap_or_else(|e| {
                panic!("{name} on empty dict failed: {}", it_empty.error_report(&e))
            });
        let empty_ink = ink_count(&it_empty);

        let mut it_filled = fresh(200, 200);
        it_filled
            .run_str(&format!("{call} pop"))
            .unwrap_or_else(|e| panic!("{name} failed: {}", it_filled.error_report(&e)));
        let filled_ink = ink_count(&it_filled);

        assert!(
            filled_ink > empty_ink,
            "{name}: filled content ({filled_ink} dark px) should paint more ink than an \
             empty dict ({empty_ink} dark px) -- otherwise text/labels aren't landing"
        );
    }
}

#[test]
fn empty_dict_does_not_error() {
    for (name, _) in TEMPLATES {
        let mut it = fresh(200, 200);
        it.run_str(&format!("10 10 180 180 << >> {name} pop"))
            .unwrap_or_else(|e| panic!("{name} on an empty dict failed: {}", it.error_report(&e)));
    }
}

#[test]
fn leftover_contract_matches_tfblock() {
    // A short body fits comfortably: leftover is the empty string.
    let mut it = fresh(200, 200);
    it.run_str(
        "10 10 180 180 << /Awardee (Al) /Body (short) \
         /Presenter (P) /Date (D) >> pgcertificate",
    )
    .unwrap_or_else(|e| panic!("pgcertificate failed: {}", it.error_report(&e)));
    let stack = it.operand_stack();
    assert_eq!(stack.len(), 1, "expected exactly one leftover string");
    assert_eq!(stack[0].repr(), "()", "short body should leave no leftover");

    // Body/box font size and citation-band size both scale with the box's
    // own height, so a *smaller* box doesn't force overflow (the line
    // capacity is roughly self-similar) -- only *more text* than any
    // reasonable citation band can hold does.
    let mut it = fresh(200, 200);
    let sentence = "This citation is one of many identical repeated sentences. ";
    let long_body = format!("({})", sentence.repeat(40));
    it.run_str(&format!(
        "10 10 180 180 << /Awardee (Al) /Body {long_body} \
         /Presenter (P) /Date (D) >> pgcertificate"
    ))
    .unwrap_or_else(|e| panic!("pgcertificate failed: {}", it.error_report(&e)));
    let stack = it.operand_stack();
    assert_eq!(stack.len(), 1, "expected exactly one leftover string");
    assert_ne!(
        stack[0].repr(),
        "()",
        "an oversized body in a tiny box must report leftover text"
    );
}

#[test]
fn pgzfitmax_never_enlarges_only_shrinks() {
    let mut it = fresh(200, 200);
    it.run_str(
        "/Helvetica findfont 20 scalefont setfont \
         (Al) stringwidth pop \
         (Al) dup 500 pgzfitmax stringwidth pop \
         eq",
    )
    .unwrap_or_else(|e| panic!("pgzfitmax (enlarge case) failed: {}", it.error_report(&e)));
    let stack = it.operand_stack();
    assert_eq!(
        stack.last().unwrap().repr(),
        "true",
        "a short string given a huge target width must not be scaled up"
    );

    let mut it = fresh(200, 200);
    it.run_str(
        "/Helvetica findfont 20 scalefont setfont \
         (a string long enough to overflow a narrow target width) stringwidth pop \
         (a string long enough to overflow a narrow target width) dup 50 pgzfitmax \
         stringwidth pop \
         gt",
    )
    .unwrap_or_else(|e| panic!("pgzfitmax (shrink case) failed: {}", it.error_report(&e)));
    let stack = it.operand_stack();
    assert_eq!(
        stack.last().unwrap().repr(),
        "true",
        "a string that overflows the target width must be shrunk (original width > \
         post-fitmax width)"
    );
}

#[test]
fn new_palettes_are_ordered_dark_to_light() {
    let mut it = Interp::new();
    load(&mut it);
    it.run_str(
        "/pglum { aload pop 0.11 mul 3 1 roll 0.59 mul 3 1 roll 0.3 mul add add } def \
         /checkmono { \
           pal /arr exch def \
           arr 0 get pglum arr 1 get pglum lt \
           arr 1 get pglum arr 2 get pglum lt and \
           arr 2 get pglum arr 3 get pglum lt and \
           arr 3 get pglum arr 4 get pglum lt and \
         } def \
         /vellum checkmono /marigold checkmono",
    )
    .unwrap_or_else(|e| panic!("palette monotonicity check failed: {}", it.error_report(&e)));
    let stack = it.operand_stack();
    let got: Vec<_> = stack.iter().map(|o| o.repr()).collect();
    assert_eq!(
        got,
        ["true", "true"],
        "vellum and marigold must both run dark to light (luminance strictly increasing \
         index 0 to 4), or role-indexed ink/background picks break"
    );
}

#[test]
fn ghostscript_accepts_pagekit() {
    let gs_ok = std::process::Command::new("gs")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success());
    if !gs_ok {
        eprintln!("skipping gs compatibility check: gs not installed");
        return;
    }
    let artkit = std::fs::read_to_string("lib/artkit.ps").expect("artkit");
    let pagekit = std::fs::read_to_string("lib/pagekit.ps").expect("pagekit");
    let driver = TEMPLATES
        .iter()
        .map(|(_, call)| format!("{call} pop"))
        .collect::<Vec<_>>()
        .join(" ");
    let dir = std::env::temp_dir().join(format!("pscat-pagekit-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let combined = dir.join("pagekit_gs.ps");
    std::fs::write(
        &combined,
        format!("{artkit}\n{pagekit}\n{driver}\nshowpage\n"),
    )
    .expect("write");
    let status = std::process::Command::new("gs")
        .args([
            "-dNOPAUSE",
            "-dBATCH",
            "-q",
            "-sDEVICE=png16m",
            "-g400x400",
            "-r72",
            "-o/dev/null",
        ])
        .arg(&combined)
        .status()
        .expect("run gs");
    assert!(status.success(), "gs rejected pagekit");
}
