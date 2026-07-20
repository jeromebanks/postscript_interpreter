//! The style packs (lib/styles/*.ps, Stage 20): each loads on top of
//! artkit, registers its palettes, and its motifs leave ink where the
//! header promises — the corpus policy for rand-driven art. One gs
//! run per pack pins Ghostscript compatibility.

use pscat::Interp;

const PACKS: [&str; 4] = ["steampunk", "psychedelic", "scifi", "toon"];

fn load(it: &mut Interp, pack: &str) {
    for path in ["lib/artkit.ps".into(), format!("lib/styles/{pack}.ps")] {
        let src = std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
        it.run_source(&src)
            .unwrap_or_else(|e| panic!("{path} failed to load: {}", it.error_report(&e)));
    }
}

fn ink_count(it: &Interp) -> usize {
    it.gfx()
        .pixmap
        .pixels()
        .iter()
        .filter(|p| p.red() < 128 || p.green() < 128 || p.blue() < 128)
        .count()
}

/// Renders `src` (after `1 srand`) on a white 200x200 page with the
/// pack loaded, and returns how many pixels the motif touched.
fn ink_of(pack: &str, src: &str) -> usize {
    let mut it = Interp::with_page(200, 200).expect("page");
    load(&mut it, pack);
    it.run_str(&format!("1 srand {src}"))
        .unwrap_or_else(|e| panic!("{pack}: {src:?} failed: {}", it.error_report(&e)));
    ink_count(&it)
}

#[test]
fn packs_load_clean_and_register_palettes() {
    for pack in PACKS {
        let mut it = Interp::with_page(50, 50).expect("page");
        load(&mut it, pack);
        assert_eq!(ink_count(&it), 0, "{pack} drew on load");
        // 8 artkit moods + 3 per pack, each five [r g b] triples
        it.run_str("Palettes length").expect("palettes");
        let got: Vec<_> = it.operand_stack().iter().map(|o| o.repr()).collect();
        assert_eq!(got, ["11"], "{pack} should add 3 palettes to artkit's 8");
    }
}

#[test]
fn steampunk_motifs_leave_ink() {
    for motif in [
        "newpath 100 100 60 12 gear fill",
        "100 100 20 rivet",
        "20 100 180 100 16 pipe",
        "100 100 50 0.6 gauge",
        "10 10 180 180 20 plateframe",
    ] {
        assert!(ink_of("steampunk", motif) > 200, "{motif} left no ink");
    }
    // the gear's bore stays open under a plain fill
    let bored = ink_of("steampunk", "newpath 100 100 80 10 gear fill");
    let solid = ink_of("steampunk", "newpath 100 100 80 0 360 arc fill");
    assert!(bored < solid, "gear bore should be unpainted");
}

#[test]
fn psychedelic_motifs_leave_ink() {
    for motif in [
        "newpath 100 100 90 12 rays fill",
        "newpath 100 100 60 8 7 blob fill",
        "newpath 100 100 80 5 spiral stroke",
        "newpath 10 100 190 100 12 4 wavy stroke",
        "100 100 8 { newpath 100 150 15 3 5 blob fill } kaleido",
    ] {
        assert!(ink_of("psychedelic", motif) > 200, "{motif} left no ink");
    }
    // rainbow is sethsbcolor's hue wheel
    let mut it = Interp::new();
    load(&mut it, "psychedelic");
    it.run_str("0 rainbow currentrgbcolor").expect("rainbow");
    let got: Vec<_> = it.operand_stack().iter().map(|o| o.repr()).collect();
    assert_eq!(got, ["1.0", "0.0", "0.0"], "hue 0 is red");
}

#[test]
fn scifi_motifs_leave_ink() {
    // starfield paints light marks: count lit pixels over a black ground
    let mut it = Interp::with_page(200, 200).expect("page");
    load(&mut it, "scifi");
    it.run_str("1 srand 0 setgray clippath fill 10 10 180 180 400 starfield")
        .unwrap_or_else(|e| panic!("starfield failed: {}", it.error_report(&e)));
    let lit = it
        .gfx()
        .pixmap
        .pixels()
        .iter()
        .filter(|p| p.red() > 128)
        .count();
    assert!(lit > 150, "starfield left no stars");
    for motif in [
        "newpath 20 100 moveto 180 100 lineto [0.3 0.8 0.9] 2 glowstroke",
        "100 100 70 planet",
        "100 100 50 -15 planetring",
        "newpath 10 10 180 180 25 hudcorners stroke",
        "newpath 100 100 60 reticle stroke",
        "newpath 10 10 180 180 20 hexfield stroke",
        "newpath 10 10 180 120 10 8 gridfloor stroke",
    ] {
        assert!(ink_of("scifi", motif) > 150, "{motif} left no ink");
    }
}

#[test]
fn toon_motifs_leave_ink() {
    for motif in [
        "newpath 100 100 40 80 9 burst [1 0.8 0.1] 4 celfill",
        "40 110 120 60 12 170 40 bubble",
        "100 100 50 90 20 0 360 speedlines",
        "newpath 100 100 70 0 360 arc 12 3 dotfill",
        "100 100 30 45 eye",
        "newpath 20 100 160 70 5 dripbox [0.4 0.8 0.4] 3 celfill",
    ] {
        assert!(ink_of("toon", motif) > 150, "{motif} left no ink");
    }
    // dotfill promises the walked path survives
    let mut it = Interp::with_page(200, 200).expect("page");
    load(&mut it, "toon");
    it.run_str("newpath 100 100 70 0 360 arc 12 3 dotfill currentpoint pop")
        .unwrap_or_else(|e| panic!("path lost: {}", it.error_report(&e)));
}

#[test]
fn ghostscript_accepts_the_packs() {
    let gs_ok = std::process::Command::new("gs")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success());
    if !gs_ok {
        eprintln!("skipping gs compatibility check: gs not installed");
        return;
    }
    let artkit = std::fs::read_to_string("lib/artkit.ps").expect("artkit");
    let drivers = [
        (
            "steampunk",
            "newpath 200 200 90 12 gear fill 60 60 15 rivet 20 350 380 350 14 pipe 320 80 40 0.7 gauge 5 5 390 390 24 plateframe",
        ),
        (
            "psychedelic",
            "newpath 200 200 190 16 rays fill 200 200 6 { newpath 200 280 20 4 5 blob 0.5 rainbow fill } kaleido newpath 200 200 90 4 spiral stroke newpath 10 30 390 30 8 4 wavy stroke",
        ),
        (
            "scifi",
            "10 10 380 380 200 starfield 200 240 80 planet 200 240 80 -12 planetring newpath 20 20 360 360 30 hudcorners [0.3 0.9 0.8] 1.5 glowstroke newpath 30 40 340 120 12 8 gridfloor stroke newpath 40 300 100 80 12 hexfield stroke",
        ),
        (
            "toon",
            "newpath 200 260 50 100 11 burst [1 0.8 0.1] 4 celfill 40 60 150 70 14 320 240 bubble 200 260 105 150 20 0 360 speedlines 90 330 25 -60 eye newpath 30 120 340 60 6 dripbox [0.4 0.8 0.4] 4 celfill",
        ),
    ];
    let dir = std::env::temp_dir().join(format!("pscat-styles-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    for (pack, driver) in drivers {
        let lib = std::fs::read_to_string(format!("lib/styles/{pack}.ps")).expect(pack);
        let combined = dir.join(format!("{pack}_gs.ps"));
        std::fs::write(
            &combined,
            format!("{artkit}\n{lib}\n7 srand {driver}\nshowpage\n"),
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
        assert!(status.success(), "gs rejected {pack}");
    }
}
