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
    // The font is re-read per glyph, and (issue #31) so is the
    // segmentation of the remaining raw bytes — a kshow proc switching
    // from a Unicode-mode font to an ordinary Type 3 font mid-string
    // must not leave that font's BuildChar fed anything past a byte:
    // its own Encoding-array lookup has no reason to expect more, and
    // a stray multi-byte codepoint would rangecheck in a real font
    // (this one just stashes it). The string's second character is
    // plain ASCII 'A' precisely so it lands on a clean byte boundary
    // after the first (3-byte) Hangul syllable is consumed — see
    // `kshow_font_switch_leaves_leftover_utf8_bytes_as_separate_glyphs`
    // below for what happens when the switch instead lands mid-codepoint.
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
         {{pop pop /PlainFont findfont 12 scalefont setfont}} (\u{AC00}A) kshow
         Got"
    ));
    match it.pop().expect("Got").value {
        Value::Integer(n) => assert_eq!(n, i64::from(b'A')),
        _ => panic!("expected integer"),
    }
}

#[test]
fn kshow_font_switch_narrows_for_an_ordinary_outline_font() {
    // The same collision, in ShowCtx::step's *other* branch
    // (`fs.fid >= 0`): switching from a /UnicodeBuildChar true Type 3
    // font to an ordinary registered outline font (Helvetica) must not
    // route the outline font's glyphs through `unicode_glyph`
    // (cmap-based) instead of `outline_glyph` (byte/Encoding-based) —
    // the wrong resolution path entirely for a byte-oriented face.
    //
    // Plain ASCII doesn't discriminate the two paths: byte 'A' (0x41)
    // resolves to the same glyph whether looked up via Helvetica's
    // Encoding (outline_glyph, correct) or via cmap on codepoint 0x41
    // (unicode_glyph, the bug) — both give /A, so a wrong-path call
    // would still pass a same-advance assertion. Instead, re-encode
    // (the PLRM idiom, see `fonts.rs`'s `reencoding_changes_which_
    // glyph_a_byte_selects`) byte 105 ('i') to select glyph /A: the
    // *byte* 105 now means /A, but the *codepoint* 105 (U+0069) still
    // means 'i' via cmap. Correct routing (outline_glyph reading the
    // re-encoded byte) advances by 'A''s width; the bug (unicode_glyph
    // reading the codepoint straight through cmap, ignoring Encoding
    // entirely) would advance by 'i''s width instead — and Helvetica's
    // 'A' and 'i' widths are not remotely close, so this test actually
    // fails if the wrong path runs. The string's second character is
    // still a single ASCII byte so the switch lands on a clean UTF-8
    // boundary (see the unflagged-Type3 test above for why that matters).
    let mut it = run(&format!(
        "{UNICODE_T3} /UniFont findfont 12 scalefont setfont
         0 0 moveto
         {{
           pop pop
           /Helvetica findfont dup length dict copy
           dup /Encoding StandardEncoding dup length array copy put
           dup /Encoding get 105 /A put
           /HelveticaIMeansA exch definefont 12 scalefont setfont
         }} (\u{AC00}i) kshow
         currentpoint pop"
    ));
    let switched_x = pop_f64(&mut it);

    let mut reference =
        run("/Helvetica findfont 12 scalefont setfont 0 0 moveto (A) show currentpoint pop");
    let a_advance = pop_f64(&mut reference);

    assert!(
        (switched_x - a_advance).abs() < 1e-6,
        "expected Helvetica's re-encoded byte 105 to advance by 'A''s width \
         ({a_advance}), got {switched_x} — looks like unicode_glyph's cmap \
         path ran on codepoint 0x69 ('i') instead of outline_glyph's \
         Encoding-array lookup"
    );
}

#[test]
fn kshow_font_switch_leaves_leftover_utf8_bytes_as_separate_glyphs() {
    // Issue #31's fix re-segments the *remaining* raw bytes against
    // whichever font is live at each glyph, rather than trusting a
    // decision made once when the show began. That's deliberately
    // asymmetric with the reverse (byte-mode -> Unicode-mode) fix:
    // switching a byte-mode font in mid-string can never "know" that
    // three leftover bytes were meant to be one codepoint (see
    // `unflagged_type3_still_gets_raw_bytes_not_utf8_decoded` above —
    // an ordinary font always gets raw bytes, one glyph per byte). So
    // when the switch lands *inside* a multi-byte codepoint rather than
    // on a clean boundary, the ordinary font correctly receives each of
    // the leftover UTF-8 bytes as its own separate glyph, not one
    // glyph, not a rangecheck, not silently dropped bytes.
    let mut it = run(&format!(
        "{UNICODE_T3}
         /P 6 dict def
         P begin
           /FontType 3 def
           /FontMatrix [0.001 0 0 0.001 0 0] def
           /Encoding 256 array def 0 1 255 {{ Encoding exch /.notdef put }} for
           /BuildChar {{ exch pop /Got exch def /Count Count 1 add def 0 0 setcharwidth }} def
         end
         /PlainFont P definefont pop
         /Count 0 def
         /UniFont findfont 12 scalefont setfont
         0 0 moveto
         {{pop pop /PlainFont findfont 12 scalefont setfont}} (\u{AC00}\u{AC01}) kshow
         Got Count"
    ));
    // \u{AC00}\u{AC01} is EA B0 80 EA B0 81 in UTF-8. UniFont consumes
    // the first 3 bytes as one Hangul glyph; the kshow proc then
    // switches to PlainFont for the remaining 3 raw bytes EA, B0, 81 —
    // three separate BuildChar calls, the last leaving Got = 0x81.
    let count = match it.pop().expect("Count").value {
        Value::Integer(n) => n,
        _ => panic!("expected integer"),
    };
    let got = match it.pop().expect("Got").value {
        Value::Integer(n) => n,
        _ => panic!("expected integer"),
    };
    assert_eq!(count, 3, "expected 3 separate byte glyphs on PlainFont");
    assert_eq!(got, 0x81, "expected the last leftover byte in Got");
}

#[test]
fn kshow_font_switch_into_unicode_type3_recombines_utf8_bytes() {
    // Issue #31's actual bug: a kshow proc switching *into* a
    // /UnicodeBuildChar true font from a byte-mode font. Before the
    // fix, `ShowCtx` decided `unicode_mode` once, from the font active
    // when the show began — an ordinary byte-mode font here — and
    // pre-split the whole string into individual bytes. By the time the
    // proc switched to UniFont, U+AC00's three UTF-8 bytes (EA B0 80)
    // had already been split apart and could never be recombined: they
    // would arrive at BuildChar as three separate byte-sized calls
    // (0xEA, 0xB0, 0x80) instead of one call with the full codepoint
    // 0xAC00. The fix re-segments the *remaining* raw bytes against
    // whichever font is live at each glyph, so the switch recovers the
    // UTF-8 boundary correctly.
    let mut it = run(&format!(
        "{UNICODE_T3}
         /P 6 dict def
         P begin
           /FontType 3 def
           /FontMatrix [0.001 0 0 0.001 0 0] def
           /Encoding 256 array def 0 1 255 {{ Encoding exch /.notdef put }} for
           /BuildChar {{ exch pop /Count Count 1 add def 0 0 setcharwidth }} def
         end
         /PlainFont P definefont pop
         /Count 0 def
         /PlainFont findfont 12 scalefont setfont
         0 0 moveto
         {{pop pop /UniFont findfont 12 scalefont setfont}} (A\u{AC00}) kshow
         Got Count"
    ));
    let count = match it.pop().expect("Count").value {
        Value::Integer(n) => n,
        _ => panic!("expected integer"),
    };
    let got = match it.pop().expect("Got").value {
        Value::Integer(n) => n,
        _ => panic!("expected integer"),
    };
    assert_eq!(
        count, 1,
        "PlainFont's BuildChar should run once, for 'A' only — not again \
         for any part of U+AC00 after the switch"
    );
    assert_eq!(
        got, 0xAC00,
        "UniFont's BuildChar should receive one recombined codepoint, \
         not the first raw UTF-8 byte of U+AC00"
    );
}
