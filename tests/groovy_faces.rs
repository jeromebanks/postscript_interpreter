//! Groovy 60s/70s display faces (issue #155): Chewy and Bagel Fat One
//! resolve from the catalog, are outline (not Type 3) faces so
//! `charpath` captures their ink, and work through `psyletter` and
//! `hllayout` (counters, apostrophes, the `ea` tuck).

use pscat::Interp;

const FACES: [&str; 2] = ["Chewy-Regular", "BagelFatOne-Regular"];

fn with_libs() -> Interp {
    let mut it = Interp::with_page(600, 400).expect("page");
    for f in ["lib/artkit.ps", "lib/lettering.ps", "lib/headline.ps"] {
        let src = std::fs::read(f).expect("lib present");
        it.run_source(&src)
            .unwrap_or_else(|e| panic!("{f} failed: {}", it.error_report(&e)));
    }
    it
}

fn ink(it: &Interp) -> usize {
    it.gfx()
        .pixmap
        .pixels()
        .iter()
        .filter(|p| p.red() < 128)
        .count()
}

#[test]
fn faces_resolve_by_stem_and_are_not_type3() {
    for face in FACES {
        let mut it = with_libs();
        // FontType 3 would make charpath capture nothing; the stem must
        // round-trip so findfont did not fall back to Helvetica.
        it.run_str(&format!(
            "/{face} findfont dup /FontName get 1 index /FontType get"
        ))
        .unwrap_or_else(|e| panic!("{face}: {}", it.error_report(&e)));
        let ty = it.pop().expect("FontType");
        let name = it.pop().expect("FontName");
        assert_eq!(
            name.repr(),
            format!("/{face}"),
            "findfont substituted for {face}"
        );
        assert_ne!(ty.repr(), "3", "{face} must be an outline face");

        // The family name must resolve through the `-Regular` fallback,
        // and the apostrophe glyph must exist (a missing quoteright would
        // still pass an ink count from the other letters).
        let family = face.split('-').next().unwrap();
        it.run_str(&format!(
            "/{family} findfont /FontName get /{face} findfont 100 scalefont setfont \
             (') stringwidth pop"
        ))
        .unwrap_or_else(|e| panic!("{face}: {}", it.error_report(&e)));
        let w = it.pop().expect("width");
        let fam = it.pop().expect("family FontName");
        assert_eq!(
            fam.repr(),
            format!("/{face}"),
            "family fallback for {family}"
        );
        assert!(
            w.repr().parse::<f64>().unwrap_or(0.0) > 0.0,
            "{face}: no apostrophe"
        );
    }
}

#[test]
fn psyletter_draws_real_in_each_face() {
    for face in FACES {
        let mut it = with_libs();
        it.run_str(&format!(
            "0 setgray (Real) 40 60 << /Font /{face} /Size 200 >> psyletter"
        ))
        .unwrap_or_else(|e| panic!("{face}: {}", it.error_report(&e)));
        assert!(ink(&it) > 5000, "{face}: psyletter drew almost nothing");
    }
}

#[test]
fn hllayout_tucks_shirt_front_in_each_face() {
    for face in FACES {
        let mut it = with_libs();
        it.run_str(&format!(
            "/lettered {{ /ls exch def
               << /Font /{face} /Size ls /Align /left /Treatment /mottled /Seed 7 >>
               psyletter }} def
             /lay [ 20 20 580 380 ] [
               << /Name /real /Text (Real) /Font /{face} /Draw {{ lettered }} /FitBox [ 500 200 ] /At [ 40 40 ] >>
               << /Name /keep /Text (Keepin' it) /Font /{face} /Draw {{ lettered }} /SizeOf [ /real 0.17 ]
                  /At << /To /real /Chars [ 1 3 ] /H 0 /V 1 >>
                  /Anchor [ /left /bottom ] /Offset [ 8 18 ]
                  /Avoid [ [ /real 0 1 ] [ /real 1 3 ] [ /real 3 4 ] ] /Clearance 10 >>
               << /Name /im /Text (I'm) /Font /{face} /Draw {{ lettered }} /SizeOf [ /keep 0.75 ]
                  /At << /To /keep /H 0 /V 1 >>
                  /Anchor [ /left /bottom ] /Offset [ 0 12 ]
                  /Avoid [ /keep [ /real 0 1 ] [ /real 1 3 ] [ /real 3 4 ] ] /Clearance 10 >>
             ] hllayout def
             lay hldraw"
        ))
        .unwrap_or_else(|e| panic!("{face}: {}", it.error_report(&e)));
        assert!(ink(&it) > 5000, "{face}: layout drew almost nothing");
    }
}
