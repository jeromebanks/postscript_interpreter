//! Type 3 (BuildChar/BuildGlyph) fonts and kshow — the ShowFrame
//! machinery. Width/advance expectations cross-checked against
//! Ghostscript (e.g. the 600/1000-em box glyph at 12pt advances 7.2).

use pscat::{Interp, PsError, Value};

/// A minimal Type 3 font: 'a' (and only 'a') is a filled 500x700 box,
/// width 600, in a 1000-unit glyph space.
const BOX_FONT: &str = "
    /T3 7 dict def
    T3 begin
      /FontType 3 def
      /FontMatrix [0.001 0 0 0.001 0 0] def
      /FontBBox [0 0 1000 1000] def
      /Encoding 256 array def
      0 1 255 { Encoding exch /.notdef put } for
      Encoding 97 /box put
      /BuildChar {
        exch begin
          600 0 0 0 500 700 setcachedevice
          0 0 moveto 500 0 lineto 500 700 lineto 0 700 lineto closepath fill
        end
      } def
    end
    /BoxFont T3 definefont pop
";

fn run(src: &str) -> Interp {
    let mut it = Interp::with_page(100, 100).expect("test page");
    it.run_str(src)
        .unwrap_or_else(|e| panic!("run failed: {e}"));
    it
}

fn pop_f64(it: &mut Interp) -> f64 {
    match it.pop().expect("operand").value {
        Value::Integer(i) => i as f64,
        Value::Real(r) => r,
        _ => panic!("expected number"),
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

#[test]
fn buildchar_paints_and_advances() {
    let mut it = run(&format!(
        "{BOX_FONT} /BoxFont findfont 12 scalefont setfont \
         10 20 moveto (aa) show currentpoint"
    ));
    let y = pop_f64(&mut it);
    let x = pop_f64(&mut it);
    // Two glyphs, 600 units each at 12pt: 2 x 7.2 (gs: 14.398 with
    // device rounding; we compute in f64).
    assert!((x - 24.4).abs() < 1e-3, "advance to {x}");
    assert!((y - 20.0).abs() < 1e-3, "baseline {y}");
    // Two 6x8.4pt boxes of ink.
    assert!(
        ink_count(&it) > 80,
        "expected glyph ink, got {}",
        ink_count(&it)
    );
}

#[test]
fn type3_stringwidth_measures_without_painting() {
    let mut it = run(&format!(
        "{BOX_FONT} /BoxFont findfont 12 scalefont setfont (aaa) stringwidth"
    ));
    let wy = pop_f64(&mut it);
    let wx = pop_f64(&mut it);
    assert!((wx - 21.6).abs() < 1e-6, "width {wx}");
    assert_eq!(wy, 0.0);
    // BuildChar ran (that's where the width came from) but painted
    // nothing — and needed no current point.
    assert_eq!(ink_count(&it), 0);
}

#[test]
fn setcharwidth_works_like_setcachedevice() {
    let mut it = run("/T3 5 dict def
         T3 begin
           /FontType 3 def
           /FontMatrix [0.001 0 0 0.001 0 0] def
           /Encoding 256 array def 0 1 255 { Encoding exch /.notdef put } for
           /BuildChar { pop pop 250 0 setcharwidth } def
         end
         /W T3 definefont pop /W findfont 40 scalefont setfont
         (xy) stringwidth");
    let _wy = pop_f64(&mut it);
    let wx = pop_f64(&mut it);
    assert!((wx - 20.0).abs() < 1e-6, "2 x 250 x 0.04 = 20, got {wx}");
}

#[test]
fn missing_width_declaration_advances_zero() {
    let mut it = run(&format!(
        "{BOX_FONT} T3 /BuildChar {{ pop pop }} put \
         /Z T3 definefont pop /Z findfont 12 scalefont setfont \
         5 5 moveto (aaa) show currentpoint pop"
    ));
    let x = pop_f64(&mut it);
    assert!((x - 5.0).abs() < 1e-3, "no setcachedevice, no advance: {x}");
}

#[test]
fn buildglyph_is_preferred_and_gets_the_name() {
    // BuildGlyph receives the *encoded name* (and wins over BuildChar);
    // remapping the encoding changes which name arrives.
    let mut it = run("/T3 6 dict def
         T3 begin
           /FontType 3 def
           /FontMatrix [0.001 0 0 0.001 0 0] def
           /Encoding 256 array def 0 1 255 { Encoding exch /.notdef put } for
           Encoding 97 /alpha put
           /BuildChar { pop pop 0 0 setcharwidth } def
           /BuildGlyph { exch pop /GotName exch def 300 0 setcharwidth } def
         end
         /G T3 definefont pop /G findfont 10 scalefont setfont
         0 0 moveto (a) show
         GotName");
    assert_eq!(it.pop().expect("name").repr(), "/alpha");
}

#[test]
fn glyph_context_is_sealed() {
    // Color and CTM changes inside BuildChar must not leak out, even
    // with an unbalanced gsave left behind.
    let mut it = run(&format!(
        "{BOX_FONT} T3 /BuildChar {{
             pop pop 600 0 setcharwidth
             1 0 0 setrgbcolor 3 3 scale gsave
         }} put
         /Leaky T3 definefont pop
         0 0 1 setrgbcolor
         /Leaky findfont 12 scalefont setfont
         10 10 moveto (aa) show
         currentrgbcolor"
    ));
    let b = pop_f64(&mut it);
    let g = pop_f64(&mut it);
    let r = pop_f64(&mut it);
    assert_eq!((r, g, b), (0.0, 0.0, 1.0), "color must not leak");
    // The CTM is intact: a known rectangle lands where it always does.
    it.run_str("newpath 40 40 moveto 60 40 lineto 60 60 lineto 40 60 lineto closepath fill")
        .expect("fill");
    let p = it.gfx().pixmap.pixel(50, 50).expect("pixel");
    assert_eq!((p.red(), p.green(), p.blue()), (0, 0, 255));
}

#[test]
fn buildchar_errors_are_catchable_and_contained() {
    let mut it = run(&format!(
        "{BOX_FONT} T3 /BuildChar {{ pop pop 1 0 add undefined_operator_xyz }} put
         /Bad T3 definefont pop
         0.5 setgray
         /Bad findfont 12 scalefont setfont
         10 10 moveto
         {{ (aa) show }} stopped
         currentgray"
    ));
    let gray = pop_f64(&mut it);
    let caught = it.pop().expect("stopped result");
    assert_eq!(caught.repr(), "true");
    assert!(
        (gray - 0.5).abs() < 0.01,
        "state restored after error: {gray}"
    );
}

#[test]
fn exit_inside_buildchar_is_invalidexit() {
    let mut it = Interp::with_page(100, 100).expect("page");
    let src = format!(
        "{BOX_FONT} T3 /BuildChar {{ pop pop exit }} put
         /E T3 definefont pop /E findfont 12 scalefont setfont
         0 0 moveto (a) show"
    );
    assert_eq!(it.run_str(&src), Err(PsError::InvalidExit));
}

#[test]
fn nested_show_inside_buildchar() {
    // A Type 3 glyph that renders itself by showing an outline glyph —
    // recursion through the frame machinery.
    let mut it = run(&format!(
        "{BOX_FONT} T3 /BuildChar {{
             pop pop
             1000 0 setcharwidth
             /Helvetica findfont 1000 scalefont setfont
             0 0 moveto (X) show
         }} put
         /Nest T3 definefont pop /Nest findfont 30 scalefont setfont
         20 30 moveto (a) show currentpoint pop"
    ));
    let x = pop_f64(&mut it);
    assert!((x - 50.0).abs() < 1e-3, "outer advance 30pt: {x}");
    assert!(ink_count(&it) > 50, "nested X painted");
}

#[test]
fn type3_respects_ctm_and_clip() {
    let it = run(&format!(
        "{BOX_FONT} /BoxFont findfont 20 scalefont setfont \
         50 20 translate 30 rotate 0 0 moveto (a) show"
    ));
    assert!(ink_count(&it) > 100, "rotated Type 3 glyph painted");
}

#[test]
fn kshow_runs_proc_between_characters() {
    // gs cross-check: (abc) kshow with {pop pop 10 0 rmoveto} ends
    // exactly 2 x 10 further than plain stringwidth — the proc runs
    // between pairs (twice for three characters) and its rmoveto moves
    // where the next glyph starts.
    let mut it = run("/Helvetica findfont 12 scalefont setfont
         (abc) stringwidth pop
         0 0 moveto
         {pop pop 10 0 rmoveto} (abc) kshow
         currentpoint pop exch sub");
    let extra = pop_f64(&mut it);
    assert!((extra - 20.0).abs() < 1e-6, "two proc runs x 10: {extra}");
}

#[test]
fn kshow_proc_sees_character_codes() {
    let mut it = run("/codes 10 dict def
         /Helvetica findfont 12 scalefont setfont
         0 0 moveto
         {codes exch /second exch put codes exch /first exch put} (AB) kshow
         codes /first get codes /second get");
    let second = pop_f64(&mut it);
    let first = pop_f64(&mut it);
    assert_eq!((first, second), (65.0, 66.0));
}

#[test]
fn kshow_works_with_type3_fonts() {
    let mut it = run(&format!(
        "{BOX_FONT} /BoxFont findfont 12 scalefont setfont \
         0 0 moveto {{pop pop 5 0 rmoveto}} (aaa) kshow currentpoint pop"
    ));
    let x = pop_f64(&mut it);
    // 3 x 7.2 + 2 x 5 = 31.6
    assert!((x - 31.6).abs() < 1e-3, "type3 kshow advance {x}");
}

#[test]
fn setcachedevice_outside_buildchar_is_undefined() {
    let mut it = Interp::with_page(100, 100).expect("page");
    assert!(matches!(
        it.run_str("1 2 3 4 5 6 setcachedevice"),
        Err(PsError::Undefined(_))
    ));
    assert!(matches!(
        it.run_str("1 2 setcharwidth"),
        Err(PsError::Undefined(_))
    ));
}

#[test]
fn charpath_on_type3_advances_without_outlines() {
    let mut it = run(&format!(
        "{BOX_FONT} /BoxFont findfont 12 scalefont setfont \
         newpath 10 10 moveto (aa) false charpath currentpoint pop"
    ));
    let x = pop_f64(&mut it);
    assert!((x - 24.4).abs() < 1e-3, "charpath advance {x}");
    assert_eq!(ink_count(&it), 0, "charpath painted nothing");
}

/// `/UnicodeBuildChar true` (see FONTS.md's "Unicode-mode Type 3
/// BuildChar" addendum) — the mechanism `lib/hangul.ps` builds on.
/// BuildChar stashes the raw operand into userdict so the test can
/// read it back without relying on any drawing.
const UNICODE_T3: &str = "
    /U 6 dict def
    U begin
      /FontType 3 def
      /FontMatrix [0.001 0 0 0.001 0 0] def
      /UnicodeBuildChar true def
      /BuildChar {
        exch pop /Got exch def 0 0 setcharwidth
      } def
    end
    /UniFont U definefont pop
";

#[test]
fn unicode_buildchar_receives_the_full_codepoint() {
    // U+AC00 (가), UTF-8 EA B0 80 — three bytes, one glyph.
    let mut it = run(&format!(
        "{UNICODE_T3} /UniFont findfont 12 scalefont setfont 0 0 moveto (\u{AC00}) show Got"
    ));
    match it.pop().expect("Got").value {
        Value::Integer(n) => assert_eq!(n, 0xAC00),
        _ => panic!("expected integer"),
    }
}

#[test]
fn unflagged_type3_still_gets_raw_bytes_not_utf8_decoded() {
    // Same BuildChar body, no /UnicodeBuildChar — a 3-byte UTF-8
    // sequence must still arrive as three separate byte-sized glyphs,
    // proving the opt-in flag is what gates the new behavior and
    // nothing leaks into ordinary Type 3 fonts.
    let mut it = run("/U 6 dict def
         U begin
           /FontType 3 def
           /FontMatrix [0.001 0 0 0.001 0 0] def
           /Encoding 256 array def 0 1 255 { Encoding exch /.notdef put } for
           /BuildChar { exch pop /Got exch def 0 0 setcharwidth } def
         end
         /PlainFont U definefont pop
         /PlainFont findfont 12 scalefont setfont
         0 0 moveto (\u{AC00}) show Got");
    // Last byte of EA B0 80 is 0x80 = 128.
    match it.pop().expect("Got").value {
        Value::Integer(n) => assert_eq!(n, 0x80),
        _ => panic!("expected integer"),
    }
}

#[test]
fn kshow_font_switch_narrows_for_an_unflagged_type3_font() {
    // `unicode_mode` is decided once for the whole show, from the
    // font active when it began, but the font itself is re-read per
    // glyph — a kshow proc is exactly how it can change mid-string.
    // An opted-in font earlier in the string must not leave later
    // glyphs on an ordinary Type 3 font un-narrowed: that font's own
    // BuildChar has no reason to expect anything past a byte, and a
    // stray multi-byte codepoint reaching an Encoding-array lookup
    // would rangecheck in a real font (this one just stashes it).
    let mut it = run(&format!(
        "{UNICODE_T3}
         /P 6 dict def
         P begin
           /FontType 3 def
           /FontMatrix [0.001 0 0 0.001 0 0] def
           /Encoding 256 array def 0 1 255 {{ Encoding exch /.notdef put }} for
           /BuildChar {{ exch pop /Got exch def 0 0 setcharwidth }} def
         end
         /PlainFont P definefont pop
         /UniFont findfont 12 scalefont setfont
         0 0 moveto
         {{pop pop /PlainFont findfont 12 scalefont setfont}} (\u{AC00}\u{AC01}) kshow
         Got"
    ));
    // U+AC01's low byte is 0x01 — nowhere near the raw codepoint
    // (0xAC01 = 44033), which is what an un-narrowed pass-through
    // would have left in Got.
    match it.pop().expect("Got").value {
        Value::Integer(n) => assert_eq!(n, 0x01),
        _ => panic!("expected integer"),
    }
}

#[test]
fn kshow_font_switch_narrows_for_an_ordinary_outline_font() {
    // The same collision, in ShowCtx::step's *other* branch
    // (`fs.fid >= 0`): switching from a /UnicodeBuildChar true Type 3
    // font to an ordinary registered outline font (Helvetica) must not
    // leave `self.unicode_mode` routing the outline font's glyphs
    // through `unicode_glyph` (cmap-based) instead of `outline_glyph`
    // (byte/Encoding-based) — the wrong resolution path entirely for a
    // byte-oriented face. U+AC41's low byte is 0x41 = 'A' in
    // StandardEncoding: if the byte path ran, the total advance is
    // exactly Helvetica's own 'A' width (UniFont's first glyph is
    // zero-width, per UNICODE_T3's BuildChar); if `unicode_glyph` ran
    // instead, it resolves 0xAC41 through Helvetica's cmap — which
    // doesn't cover Hangul — to some other (`.notdef`-shaped) advance,
    // not 'A''s.
    let mut it = run(&format!(
        "{UNICODE_T3} /UniFont findfont 12 scalefont setfont
         0 0 moveto
         {{pop pop /Helvetica findfont 12 scalefont setfont}} (\u{AC00}\u{AC41}) kshow
         currentpoint pop"
    ));
    let switched_x = pop_f64(&mut it);

    let mut reference =
        run("/Helvetica findfont 12 scalefont setfont 0 0 moveto (A) show currentpoint pop");
    let a_advance = pop_f64(&mut reference);

    assert!(
        (switched_x - a_advance).abs() < 1e-6,
        "expected Helvetica's own 'A' advance ({a_advance}), got {switched_x} \
         — looks like unicode_glyph's cmap path ran instead of outline_glyph's"
    );
}
