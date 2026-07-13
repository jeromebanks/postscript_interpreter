//! Stage 6 font tests: dict plumbing, metrics, show/charpath rendering,
//! and the re-encoding idiom. Liberation faces are metric-compatible
//! with the Adobe base fonts, so Courier advances land at 600/1000 em
//! (within TTF rounding) — the metric assertions lean on that.

use pscat::{Interp, PsError, Value};

fn run(src: &str) -> Interp {
    let mut it = Interp::with_page(100, 100).expect("test page");
    it.run_str(src)
        .unwrap_or_else(|e| panic!("run of {src:?} failed: {e}"));
    it
}

fn pop_f64(it: &mut Interp) -> f64 {
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

// --- dict plumbing ---------------------------------------------------------

#[test]
fn findfont_returns_a_font_dict() {
    let mut it = run("/Helvetica findfont dup /FontName get exch /FontType get");
    assert_eq!(it.pop().expect("FontType").repr(), "42");
    assert_eq!(it.pop().expect("FontName").repr(), "/Helvetica");
}

#[test]
fn findfont_twice_returns_the_same_dict() {
    let mut it = run("/Times-Roman findfont /Times-Roman findfont eq");
    assert_eq!(it.pop().expect("eq result").repr(), "true");
}

#[test]
fn unknown_font_substitutes_instead_of_erroring() {
    let mut it = run("/Frutiger-Weird findfont /FontType get");
    assert_eq!(it.pop().expect("FontType").repr(), "42");
}

#[test]
fn scalefont_composes_the_matrix_and_leaves_the_original() {
    let mut it = run(
        "/Helvetica findfont dup 12 scalefont /FontMatrix get 0 get \
         exch /FontMatrix get 0 get",
    );
    let original = pop_f64(&mut it);
    let scaled = pop_f64(&mut it);
    assert!((original - 0.001).abs() < 1e-12, "original {original}");
    assert!((scaled - 0.012).abs() < 1e-12, "scaled {scaled}");
}

#[test]
fn scaled_font_shares_the_encoding_array() {
    // Shallow copy per the PLRM: same Encoding object, not a duplicate.
    let mut it = run("/Helvetica findfont dup 12 scalefont /Encoding get \
         exch /Encoding get eq");
    assert_eq!(it.pop().expect("eq").repr(), "true");
}

#[test]
fn definefont_installs_into_fontdirectory() {
    let mut it = run(
        "/My /Helvetica findfont dup length dict copy definefont pop \
         FontDirectory /My known \
         /My findfont /FontName get",
    );
    assert_eq!(it.pop().expect("FontName").repr(), "/Helvetica");
    assert_eq!(it.pop().expect("known").repr(), "true");
}

#[test]
fn selectfont_sets_the_scaled_font() {
    let mut it = run("/Courier 24 selectfont currentfont /FontMatrix get 3 get");
    let d = pop_f64(&mut it);
    assert!((d - 0.024).abs() < 1e-12, "matrix d {d}");
}

#[test]
fn gsave_grestore_restores_the_font() {
    let mut it = run("/Courier 12 selectfont \
         gsave /Helvetica 24 selectfont grestore \
         currentfont /FontName get");
    assert_eq!(it.pop().expect("FontName").repr(), "/Courier");
}

#[test]
fn font_errors() {
    let mut it = Interp::with_page(100, 100).expect("test page");
    // show with no font set
    assert_eq!(
        it.run_str("0 0 moveto (hi) show"),
        Err(PsError::InvalidFont)
    );
    // show with a font but no current point
    assert_eq!(
        it.run_str("/Helvetica 12 selectfont newpath (hi) show"),
        Err(PsError::NoCurrentPoint)
    );
    // setfont on a non-font dict
    assert_eq!(it.run_str("1 dict setfont"), Err(PsError::InvalidFont));
    // stringwidth needs no current point — pure metrics.
    assert!(it.run_str("newpath (hi) stringwidth").is_ok());
}

// --- metrics ---------------------------------------------------------------

#[test]
fn courier_advances_are_600_per_em() {
    let mut it = run("/Courier 12 selectfont (abc) stringwidth");
    let wy = pop_f64(&mut it);
    let wx = pop_f64(&mut it);
    assert_eq!(wy, 0.0);
    // 3 chars × 600/1000 × 12pt = 21.6, within TTF unit rounding.
    assert!((wx - 21.6).abs() < 0.05, "courier width {wx}");
}

#[test]
fn monospace_is_monospaced_and_helvetica_is_not() {
    let mut it = run(
        "/Courier 12 selectfont (i) stringwidth pop (m) stringwidth pop \
         /Helvetica 12 selectfont (i) stringwidth pop (m) stringwidth pop",
    );
    let helv_m = pop_f64(&mut it);
    let helv_i = pop_f64(&mut it);
    let cour_m = pop_f64(&mut it);
    let cour_i = pop_f64(&mut it);
    assert!(
        (cour_i - cour_m).abs() < 1e-9,
        "courier {cour_i} vs {cour_m}"
    );
    assert!(helv_i < helv_m, "helvetica {helv_i} vs {helv_m}");
}

#[test]
fn stringwidth_scales_linearly() {
    let mut it = run("/Times-Roman 10 selectfont (Width) stringwidth pop \
         /Times-Roman 20 selectfont (Width) stringwidth pop");
    let w20 = pop_f64(&mut it);
    let w10 = pop_f64(&mut it);
    assert!((w20 - 2.0 * w10).abs() < 1e-9, "{w10} vs {w20}");
}

#[test]
fn widthshow_adds_per_character_advance() {
    let mut it = run("/Helvetica 12 selectfont \
         (aa) stringwidth pop \
         0 0 moveto 5 0 97 (aa) widthshow currentpoint pop \
         exch sub");
    let extra = pop_f64(&mut it);
    assert!((extra - 10.0).abs() < 1e-6, "widthshow extra {extra}");
}

#[test]
fn ashow_adds_uniform_advance() {
    let mut it = run("/Helvetica 12 selectfont \
         (ab) stringwidth pop \
         0 0 moveto 3 0 (ab) ashow currentpoint pop \
         exch sub");
    let extra = pop_f64(&mut it);
    assert!((extra - 6.0).abs() < 1e-6, "ashow extra {extra}");
}

// --- rendering -------------------------------------------------------------

#[test]
fn show_paints_and_advances_the_current_point() {
    let mut it = run("/Helvetica 40 selectfont 10 20 moveto (Hi) show currentpoint");
    let y = pop_f64(&mut it);
    let x = pop_f64(&mut it);
    assert!(ink_count(&it) > 100, "expected glyph ink");
    assert_eq!(y, 20.0);
    assert!(x > 30.0, "current point advanced to {x}");
    // show must not leave glyph outlines behind in the current path:
    // a fill afterwards paints nothing new.
    let before = ink_count(&it);
    it.run_str("fill").expect("fill");
    assert_eq!(ink_count(&it), before);
}

#[test]
fn show_respects_color_and_rotation() {
    let it = run("1 0 0 setrgbcolor /Times-Bold 30 selectfont \
         50 10 translate 45 rotate 0 0 moveto (Ab) show");
    let red = it
        .gfx()
        .pixmap
        .pixels()
        .iter()
        .filter(|p| p.red() > 128 && p.green() < 128)
        .count();
    assert!(red > 50, "expected rotated red text, got {red} px");
}

#[test]
fn glyphs_respect_the_clip() {
    // Clip to a box left of the text start: nothing may paint outside.
    let unclipped = ink_count(&run("/Helvetica 40 selectfont 10 20 moveto (HHH) show"));
    let clipped = ink_count(&run(
        "newpath 10 10 moveto 30 10 lineto 30 60 lineto 10 60 lineto closepath clip newpath \
         /Helvetica 40 selectfont 10 20 moveto (HHH) show",
    ));
    assert!(clipped > 0, "some ink survives inside the clip");
    assert!(
        clipped < unclipped / 2,
        "clip must cut most of the text: {clipped} vs {unclipped}"
    );
}

#[test]
fn charpath_builds_geometry() {
    let mut it = run("/Helvetica 40 selectfont 10 20 moveto (Hi) true charpath pathbbox");
    let uy = pop_f64(&mut it);
    let ux = pop_f64(&mut it);
    let ly = pop_f64(&mut it);
    let lx = pop_f64(&mut it);
    assert!((9.0..20.0).contains(&lx), "bbox lx {lx}");
    assert!((19.0..25.0).contains(&ly), "bbox ly {ly}");
    assert!(ux > lx + 20.0, "bbox spans the text: {lx}..{ux}");
    assert!(uy > ly + 20.0, "bbox spans cap height: {ly}..{uy}");
    // Nothing painted yet — charpath only builds the path.
    assert_eq!(ink_count(&it), 0);
    it.run_str("fill").expect("fill charpath outlines");
    assert!(ink_count(&it) > 100, "filled charpath outlines");
}

// --- encodings -------------------------------------------------------------

#[test]
fn systemdict_carries_the_standard_encodings() {
    let mut it = run("StandardEncoding 65 get ISOLatin1Encoding 65 get \
         StandardEncoding 233 get ISOLatin1Encoding 233 get");
    assert_eq!(it.pop().expect("iso 233").repr(), "/eacute");
    assert_eq!(it.pop().expect("std 233").repr(), "/Oslash");
    assert_eq!(it.pop().expect("iso 65").repr(), "/A");
    assert_eq!(it.pop().expect("std 65").repr(), "/A");
}

#[test]
fn reencoding_changes_which_glyph_a_byte_selects() {
    // The PLRM re-encoding idiom: copy the font dict, swap Encoding,
    // definefont. Byte 233 is Oslash in Standard, eacute in ISOLatin1 —
    // different widths in Helvetica, so stringwidth observes the swap.
    let mut it = run("/Helvetica findfont 12 scalefont setfont
         <e9> stringwidth pop
         /Helvetica findfont dup length dict copy
         dup /Encoding ISOLatin1Encoding put
         /Helvetica-Latin1 exch definefont 12 scalefont setfont
         <e9> stringwidth pop");
    let latin1 = pop_f64(&mut it);
    let standard = pop_f64(&mut it);
    assert!(
        (latin1 - standard).abs() > 0.5,
        "eacute vs Oslash widths: {latin1} vs {standard}"
    );
}

#[test]
fn encoding_is_read_live_from_the_font_dict() {
    // Mutating the Encoding array after setfont must affect the next
    // show — the engine reads it live, it isn't cached at setfont.
    let mut it = run("/Helvetica findfont dup length dict copy
         dup /Encoding StandardEncoding dup length array copy put
         /Live exch definefont 12 scalefont setfont
         (x) stringwidth pop
         currentfont /Encoding get 120 /m put
         (x) stringwidth pop");
    let after = pop_f64(&mut it);
    let before = pop_f64(&mut it);
    assert!(
        after > before + 0.5,
        "x remapped to m must widen: {before} -> {after}"
    );
}
