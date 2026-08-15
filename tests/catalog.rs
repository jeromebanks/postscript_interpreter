//! Font-catalog tests: runtime-loaded faces from `fonts/catalog/`,
//! the standard-35 alias table, and the Symbol/Dingbats encodings.
//! Metric assertions are pinned against Ghostscript (its URW faces and
//! our TeX Gyre faces are both metric-compatible with the Adobe
//! originals, so widths agree to well under a point).

use pscat::{Interp, Value};

fn run(src: &str) -> Interp {
    let mut it = Interp::with_page(100, 100).expect("test page");
    it.run_str(src)
        .unwrap_or_else(|e| panic!("run of {src:?} failed: {e}"));
    it
}

fn top_name(it: &mut Interp) -> String {
    match &it.pop().expect("operand").value {
        Value::Name(n) => n.to_string(),
        v => panic!(
            "expected name, got {v:?}",
            v = pscat::Object::lit(v.clone())
        ),
    }
}

fn top_f64(it: &mut Interp) -> f64 {
    match it.pop().expect("operand").value {
        Value::Integer(i) => i as f64,
        Value::Real(r) => r,
        ref v => panic!(
            "expected number, got {v:?}",
            v = pscat::Object::lit(v.clone())
        ),
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

// --- resolution ------------------------------------------------------------

#[test]
fn standard_35_aliases_resolve_to_texgyre() {
    for (alias, expect) in [
        ("Palatino-Roman", "texgyrepagella-regular"),
        ("Bookman-Demi", "texgyrebonum-bold"),
        ("NewCenturySchlbk-Italic", "texgyreschola-italic"),
        ("AvantGarde-Book", "texgyreadventor-regular"),
        ("ZapfChancery-MediumItalic", "texgyrechorus-mediumitalic"),
        ("Helvetica-Narrow", "texgyreheroscn-regular"),
    ] {
        let mut it = run(&format!("/{alias} findfont /FontName get"));
        assert_eq!(top_name(&mut it), expect, "alias {alias}");
    }
}

#[test]
fn symbol_and_dingbats_resolve_to_urw() {
    let mut it = run("/Symbol findfont /FontName get");
    assert_eq!(top_name(&mut it), "StandardSymbolsPS");
    let mut it = run("/ZapfDingbats findfont /FontName get");
    assert_eq!(top_name(&mut it), "D050000L");
}

#[test]
fn family_shorthand_finds_the_regular_face() {
    // `/Bangers findfont` should mean Bangers-Regular, not substitution.
    let mut it = run("/Bangers findfont /FontName get");
    assert_eq!(top_name(&mut it), "Bangers-Regular");
}

#[test]
fn catalog_names_load_by_file_stem() {
    let mut it = run("/EBGaramond-Regular findfont /FontName get");
    assert_eq!(top_name(&mut it), "EBGaramond-Regular");
}

#[test]
fn unknown_names_still_substitute() {
    let mut it = run("/NoSuchFace-Regular findfont /FontName get");
    assert_eq!(top_name(&mut it), "Helvetica");
}

#[test]
fn catalog_findfont_twice_returns_the_same_dict() {
    let it = run("/Palatino-Roman findfont /Palatino-Roman findfont eq");
    assert_eq!(it.operand_stack()[0].repr(), "true");
}

// --- encodings -------------------------------------------------------------

#[test]
fn symbol_font_carries_the_symbol_encoding() {
    let mut it = run("/Symbol findfont /Encoding get 97 get");
    assert_eq!(top_name(&mut it), "alpha");
    let mut it = run("/Symbol findfont /Encoding get 34 get");
    assert_eq!(top_name(&mut it), "universal");
}

#[test]
fn dingbats_font_carries_the_dingbats_encoding() {
    let mut it = run("/ZapfDingbats findfont /Encoding get 33 get");
    assert_eq!(top_name(&mut it), "a1");
    let mut it = run("/ZapfDingbats findfont /Encoding get 32 get");
    assert_eq!(top_name(&mut it), "space");
}

// --- metrics (gs-pinned) ---------------------------------------------------

#[test]
fn palatino_is_metric_compatible() {
    // gs (URW P052): (Hamburgefonstiv) at 24pt = 188.796875
    let mut it =
        run("/Palatino-Roman findfont 24 scalefont setfont (Hamburgefonstiv) stringwidth pop");
    let w = top_f64(&mut it);
    assert!((w - 188.8).abs() < 1.0, "Palatino width {w}");
}

#[test]
fn symbol_metrics_match_gs() {
    // gs (StandardSymbolsPS): (abg) at 24pt = 38.1796875
    let mut it = run("/Symbol findfont 24 scalefont setfont (abg) stringwidth pop");
    let w = top_f64(&mut it);
    assert!((w - 38.18).abs() < 0.05, "Symbol width {w}");
}

// --- rendering -------------------------------------------------------------

#[test]
fn catalog_cff_outlines_paint() {
    // TeX Gyre faces are CFF-flavored OTFs — a different outline table
    // than the builtins' glyf; make sure ink actually lands.
    let it = run("/Palatino-Roman findfont 40 scalefont setfont 5 40 moveto (Pag) show");
    assert!(ink_count(&it) > 100, "expected Palatino ink");
}

#[test]
fn dingbats_glyphs_paint() {
    let it = run("/ZapfDingbats findfont 60 scalefont setfont 10 30 moveto (aH) show");
    assert!(ink_count(&it) > 100, "expected dingbat ink");
}

// --- Unicode-mode faces (Korean/Japanese/Thai) ------------------------------
//
// Noto Sans KR/JP/Thai are CatalogEncoding::Unicode faces (FONTS.md's
// "Unicode-mode catalog faces" addendum): `show` decodes the string as
// UTF-8 and maps codepoints straight to glyphs via `cmap`, since Hangul
// and kanji vastly exceed the 256-slot Encoding model everything else
// here uses. These tests are the regression gate for that: byte-mode
// fonts above must keep behaving exactly as before, and these three
// faces must actually paint the scripts they were added for.

#[test]
fn korean_family_loads_by_file_stem() {
    let mut it = run("/NotoSansKR-Regular findfont /FontName get");
    assert_eq!(top_name(&mut it), "NotoSansKR-Regular");
}

#[test]
fn japanese_family_loads_by_file_stem() {
    let mut it = run("/NotoSansJP-Regular findfont /FontName get");
    assert_eq!(top_name(&mut it), "NotoSansJP-Regular");
}

#[test]
fn thai_family_loads_by_file_stem() {
    let mut it = run("/NotoSansThai-Regular findfont /FontName get");
    assert_eq!(top_name(&mut it), "NotoSansThai-Regular");
}

#[test]
fn korean_calligraphy_family_loads_by_file_stem() {
    // Nanum Brush Script (#5) — a second Korean face, brush-calligraphy
    // style rather than Noto's plain sans, reusing the same
    // CatalogEncoding::Unicode mechanism.
    let mut it = run("/NanumBrushScript-Regular findfont /FontName get");
    assert_eq!(top_name(&mut it), "NanumBrushScript-Regular");
}

#[test]
fn hangul_glyphs_paint() {
    let it = run(
        "/NotoSansKR-Regular findfont 40 scalefont setfont 5 40 moveto (\u{c548}\u{b155}) show",
    );
    assert!(ink_count(&it) > 100, "expected Hangul ink");
}

#[test]
fn kana_and_kanji_glyphs_paint() {
    let it = run(
        "/NotoSansJP-Regular findfont 40 scalefont setfont 5 40 moveto (\u{3053}\u{4e16}) show",
    );
    assert!(ink_count(&it) > 100, "expected kana/kanji ink");
}

#[test]
fn thai_glyphs_paint() {
    let it = run(
        "/NotoSansThai-Regular findfont 40 scalefont setfont 5 40 moveto (\u{e2a}\u{e27}\u{e31}\u{e2a}\u{e14}\u{e35}) show",
    );
    assert!(ink_count(&it) > 100, "expected Thai ink");
}

#[test]
fn korean_calligraphy_glyphs_paint() {
    let it = run(
        "/NanumBrushScript-Regular findfont 40 scalefont setfont 5 40 moveto (\u{c548}\u{b155}) show",
    );
    assert!(ink_count(&it) > 100, "expected Nanum Brush Script ink");
}

#[test]
fn hangul_charpath_produces_a_nondegenerate_outline() {
    // charpath is the third ShowMode (paint/width already covered
    // above) — confirm it appends real outlines for a Unicode-mode
    // face too, not just an empty/degenerate path.
    let mut it = run(
        "/NotoSansKR-Regular findfont 40 scalefont setfont 5 40 moveto \
         (\u{c548}\u{b155}) false charpath pathbbox",
    );
    let ury = top_f64(&mut it);
    let urx = top_f64(&mut it);
    let lly = top_f64(&mut it);
    let llx = top_f64(&mut it);
    assert!(
        urx > llx + 10.0 && ury > lly + 10.0,
        "expected a non-degenerate bbox, got ({llx}, {lly}, {urx}, {ury})"
    );
}

#[test]
fn unicode_mode_stringwidth_is_positive_and_scales_with_length() {
    let mut one =
        run("/NotoSansKR-Regular findfont 40 scalefont setfont (\u{c548}) stringwidth pop");
    let mut two =
        run("/NotoSansKR-Regular findfont 40 scalefont setfont (\u{c548}\u{b155}) stringwidth pop");
    let w1 = top_f64(&mut one);
    let w2 = top_f64(&mut two);
    assert!(w1 > 0.0, "expected positive width for one Hangul syllable");
    assert!(
        w2 > w1 * 1.5,
        "expected two-syllable width ({w2}) to roughly double one-syllable width ({w1})"
    );
}

#[test]
fn unicode_mode_mixes_with_ascii_in_one_string() {
    // Noto Sans KR also covers Latin, so a single Unicode-mode string
    // can mix scripts without switching fonts mid-show.
    let it = run(
        "/NotoSansKR-Regular findfont 40 scalefont setfont 5 40 moveto (Hi \u{c548}\u{b155}) show",
    );
    assert!(ink_count(&it) > 100, "expected mixed ASCII+Hangul ink");
}

#[test]
fn kshow_pushes_unicode_scalars_not_bytes_in_unicode_mode() {
    // Between the two Hangul syllables, kshow's proc should see their
    // actual (large) Unicode scalar values, not truncated byte codes.
    // The proc combines prev/next into one number (prev*1e6 + next) and
    // leaves it on the stack for the assertion below.
    let mut it = run(
        "/NotoSansKR-Regular findfont 20 scalefont setfont 5 20 moveto \
         { exch 1000000 mul add } (\u{c548}\u{b155}) kshow",
    );
    let combined = top_f64(&mut it) as i64;
    let expected = 0xC548i64 * 1_000_000 + 0xB155i64;
    assert_eq!(
        combined, expected,
        "kshow should see Unicode scalar codes, not byte-truncated ones"
    );
}

#[test]
fn kshow_font_switch_into_unicode_catalog_font_recombines_utf8_bytes() {
    // Issue #31: `ShowCtx` used to decide `unicode_mode` once, from the
    // font active when the show began. A kshow proc switching from an
    // ordinary byte-mode font (Helvetica) into a Unicode-mode catalog
    // face (NotoSansKR-Regular) mid-string would leave U+C548's three
    // UTF-8 bytes (already split apart on the byte-mode assumption)
    // unable to recombine into one codepoint. The fix re-segments the
    // remaining raw bytes against whichever font is live at each glyph,
    // so the switch recovers the UTF-8 boundary and the total advance
    // matches showing each piece under its own font separately.
    let mut it = run("/Helvetica findfont 40 scalefont setfont \
         0 0 moveto \
         {pop pop /NotoSansKR-Regular findfont 40 scalefont setfont} (A\u{c548}) kshow \
         currentpoint pop");
    let switched_x = top_f64(&mut it);

    let mut reference = run(
        "/Helvetica findfont 40 scalefont setfont 0 0 moveto (A) show \
         /NotoSansKR-Regular findfont 40 scalefont setfont (\u{c548}) show \
         currentpoint pop",
    );
    let expected_x = top_f64(&mut reference);

    assert!(
        (switched_x - expected_x).abs() < 1e-6,
        "expected the switch to recombine U+C548's UTF-8 bytes into one glyph \
         (advance {expected_x}), got {switched_x}"
    );
}
