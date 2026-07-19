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
