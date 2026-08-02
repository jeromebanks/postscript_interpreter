//! lib/hangul.ps (issue #6): the jamo-composition Hangul face and its
//! hg-write/hg-linecount wrap engine. No Ghostscript compatibility
//! test — unlike lib/handscript.ps, this face needs the interpreter's
//! `/UnicodeBuildChar true` extension (FONTS.md's "Unicode-mode Type 3
//! BuildChar" addendum) to get a full codepoint into BuildChar, which
//! is a pscat-only deviation with no gs equivalent.

use pscat::Interp;

fn with_lib(w: u32, h: u32) -> Interp {
    let lib = std::fs::read("lib/hangul.ps").expect("library present");
    let mut it = Interp::with_page(w, h).expect("page");
    it.run_source(&lib)
        .unwrap_or_else(|e| panic!("hangul.ps failed: {}", it.error_report(&e)));
    it
}

/// Run `opts hg-linecount` and return the count.
fn linecount(it: &mut Interp, opts: &str) -> i64 {
    it.run_str(&format!("clear {opts} hg-linecount"))
        .unwrap_or_else(|e| panic!("hg-linecount failed: {}", it.error_report(&e)));
    let repr = it
        .operand_stack()
        .last()
        .expect("hg-linecount pushes a count")
        .repr();
    repr.parse()
        .unwrap_or_else(|_| panic!("expected an integer line count, got {repr}"))
}

#[test]
fn decompose_matches_unicode_arithmetic_at_the_block_endpoints() {
    // U+AC00 (가), the first Hangul syllable: L=0 V=0 T=0. U+D7A3
    // (힣), the last: L=18 V=20 T=27 — both hand-checkable directly
    // from Unicode's arithmetic (si = cp - 0xAC00; L = si/588;
    // V = si%588/28; T = si%28).
    let mut it = with_lib(50, 50);
    it.run_str(
        "HangulDict begin work begin
         16#AC00 decompose
         end end",
    )
    .unwrap_or_else(|e| panic!("decompose failed: {}", it.error_report(&e)));
    let t = it.pop().expect("T").repr();
    let v = it.pop().expect("V").repr();
    let l = it.pop().expect("L").repr();
    assert_eq!((l.as_str(), v.as_str(), t.as_str()), ("0", "0", "0"));

    let mut it = with_lib(50, 50);
    it.run_str(
        "HangulDict begin work begin
         16#D7A3 decompose
         end end",
    )
    .unwrap_or_else(|e| panic!("decompose failed: {}", it.error_report(&e)));
    let t = it.pop().expect("T").repr();
    let v = it.pop().expect("V").repr();
    let l = it.pop().expect("L").repr();
    assert_eq!((l.as_str(), v.as_str(), t.as_str()), ("18", "20", "27"));
}

#[test]
fn a_syllable_from_each_layout_class_paints_ink() {
    // 가 (V, no final), 각 (V+final), 고 (H, no final), 곡 (H+final),
    // 과 (M, no final), 곽 (M+final) — the six standard Hangul
    // syllable-block layout classes every jamo-composition face needs.
    // Checked one at a time, not summed over one canvas: a summed
    // total can't tell "all six drew a little" from "five drew a lot
    // and one drew nothing" — exactly the failure mode the
    // `drawshapes` x1/y0 swap produced (a doubled/paired jamo
    // collapsing to a near-invisible sliver instead of erroring).
    for (name, syllable) in [
        ("가 (V, no final)", '\u{AC00}'),
        ("각 (V, final)", '\u{AC01}'),
        ("고 (H, no final)", '\u{ACE0}'),
        ("곡 (H, final)", '\u{ACE1}'),
        ("과 (M, no final)", '\u{ACFC}'),
        ("곽 (M, final)", '\u{ACFD}'),
    ] {
        let mut it = with_lib(200, 200);
        it.run_str(&format!(
            "/HangulScript findfont 100 scalefont setfont 30 50 moveto ({syllable}) show"
        ))
        .unwrap_or_else(|e| panic!("show failed: {}", it.error_report(&e)));
        let dark = it
            .gfx()
            .pixmap
            .data()
            .chunks_exact(4)
            .filter(|p| p[0] < 128)
            .count();
        assert!(dark > 30, "{name} should paint ink, got {dark}");
    }
}

#[test]
fn jitter_and_pen_options_actually_apply() {
    // hg-write's /Jitter and /Pen override HangulDict's /J and /W
    // through `work` (its Rc is shared across scalefont's copied font
    // dicts; HangulDict's own top-level /J and /W are not — see
    // lib/hangul.ps's hg-write comment). A dead override would render
    // identically regardless of /Jitter; a live one must not.
    let opts_no_jitter = "<< /Text (\u{AC00}\u{AC01}\u{ACE0}) /Size 40 /Width 300 \
                             /Height 300 /Margin 20 /Seed 7 /Jitter 0 >>";
    let opts_more_jitter = "<< /Text (\u{AC00}\u{AC01}\u{ACE0}) /Size 40 /Width 300 \
                               /Height 300 /Margin 20 /Seed 7 /Jitter 45 >>";
    let mut a = with_lib(300, 300);
    a.run_str(&format!("{opts_no_jitter} hg-write"))
        .unwrap_or_else(|e| panic!("hg-write failed: {}", a.error_report(&e)));
    let mut b = with_lib(300, 300);
    b.run_str(&format!("{opts_more_jitter} hg-write"))
        .unwrap_or_else(|e| panic!("hg-write failed: {}", b.error_report(&e)));
    assert_ne!(
        a.gfx().pixmap.data(),
        b.gfx().pixmap.data(),
        "/Jitter 0 vs /Jitter 45 must render differently"
    );
}

#[test]
fn no_two_glyph_instances_match() {
    // Same invariant as handwriting.rs's test of the same name: the
    // rand stream advances between glyphs, so two instances of the
    // same syllable must never render pixel-identical.
    let mut it = with_lib(400, 200);
    it.run_str(
        "/HangulScript findfont 80 scalefont setfont
         30 100 moveto (\u{AC00}) show
         230 100 moveto (\u{AC00}) show",
    )
    .unwrap_or_else(|e| panic!("show failed: {}", it.error_report(&e)));
    let pm = &it.gfx().pixmap;
    let mut differing = 0;
    let mut inked = 0;
    for dy in 0..150u32 {
        for dx in 0..150u32 {
            let a = pm.pixel(20 + dx, 30 + dy).expect("a");
            let b = pm.pixel(220 + dx, 30 + dy).expect("b");
            if a.red() < 128 || b.red() < 128 {
                inked += 1;
            }
            if a.red() != b.red() {
                differing += 1;
            }
        }
    }
    assert!(inked > 50, "both syllables drew something, got {inked}");
    assert!(
        differing > 20,
        "two instances of 가 should never match ({differing} differing pixels)"
    );
}

#[test]
fn non_hangul_codepoints_do_not_error() {
    // ASCII mixed in with Hangul: outside the U+AC00..U+D7A3 block,
    // BuildChar advances a half-cell and draws nothing, rather than
    // erroring (this is a dedicated Hangul face, not a mixed-script
    // one — see lib/hangul.ps's header).
    let mut it = with_lib(200, 100);
    it.run_str(
        "/HangulScript findfont 30 scalefont setfont
         10 50 moveto (Hi \u{AC00}\u{AC01}! 123) show",
    )
    .unwrap_or_else(|e| panic!("show failed: {}", it.error_report(&e)));
}

#[test]
fn newlines_force_breaks_and_narrower_columns_wrap_more() {
    let mut it = with_lib(100, 100);
    let one = linecount(
        &mut it,
        "<< /Text (\u{AC00} \u{AC01}) /Size 20 /Width 600 >>",
    );
    assert_eq!(one, 1, "two short syllable-words fit one line");
    let forced = linecount(
        &mut it,
        "<< /Text (\u{AC00}\n\u{AC01}\n\u{ACE0}) /Size 20 /Width 600 >>",
    );
    assert_eq!(forced, 3, "newlines force breaks");

    let text = "(\u{AC00}\u{AC01}\u{ACE0}\u{ACE1}\u{ACFC}\u{ACFD} \u{AC00}\u{AC01}\u{ACE0}\u{ACE1}\u{ACFC}\u{ACFD})";
    let wide = linecount(&mut it, &format!("<< /Text {text} /Size 30 /Width 600 >>"));
    let narrow = linecount(&mut it, &format!("<< /Text {text} /Size 30 /Width 150 >>"));
    assert!(
        narrow > wide,
        "narrower column must wrap more: {narrow} vs {wide}"
    );
}

#[test]
fn measuring_agrees_with_writing() {
    // hg-linecount consumes rand the same way a pre-pass would; a
    // following hg-write must still break identically, because breaks
    // come from the fixed 0.9em/0.45em advance choice, never from
    // jitter (see lib/hangul.ps's cpcount/wordadv comment).
    let opts = "<< /Text (\u{AC00}\u{AC01}\u{ACE0}\u{ACE1}\u{ACFC}\u{ACFD} \
                \u{B2ED}\u{C4CC}\u{BF41}\u{D798}\u{D62C})
                   /Size 24 /Width 300 /Height 300 /Margin 20
                   /Ink [0 0 0] /Seed 42 >>";
    let mut a = with_lib(300, 300);
    a.run_str(&format!("{opts} hg-write"))
        .unwrap_or_else(|e| panic!("hg-write failed: {}", a.error_report(&e)));
    let mut b = with_lib(300, 300);
    b.run_str(&format!("{opts} hg-linecount pop {opts} hg-write"))
        .unwrap_or_else(|e| panic!("hg-write failed: {}", b.error_report(&e)));
    assert_eq!(
        a.gfx().pixmap.data(),
        b.gfx().pixmap.data(),
        "the same seed writes the same page, pre-pass or not"
    );
}

#[test]
fn deterministic_across_runs() {
    let mut a = with_lib(300, 300);
    a.run_str(
        "<< /Text (\u{AC00}\u{AC01}\u{ACE0}) /Size 24 /Width 300 /Height 300
            /Margin 20 /Seed 7 >> hg-write",
    )
    .unwrap_or_else(|e| panic!("hg-write failed: {}", a.error_report(&e)));
    let mut b = with_lib(300, 300);
    b.run_str(
        "<< /Text (\u{AC00}\u{AC01}\u{ACE0}) /Size 24 /Width 300 /Height 300
            /Margin 20 /Seed 7 >> hg-write",
    )
    .unwrap_or_else(|e| panic!("hg-write failed: {}", b.error_report(&e)));
    assert_eq!(
        a.gfx().pixmap.data(),
        b.gfx().pixmap.data(),
        "same seed, same page"
    );
}

#[test]
fn the_example_runs_and_leaves_ink() {
    let src = std::fs::read("examples/hangul_handscript.ps").expect("example present");
    let mut it = Interp::with_page(612, 792).expect("page");
    it.run_source(&src)
        .unwrap_or_else(|e| panic!("hangul_handscript.ps failed: {}", it.error_report(&e)));
    let dark = it
        .gfx()
        .pixmap
        .data()
        .chunks_exact(4)
        .filter(|p| p[0] < 200)
        .count();
    assert!(dark > 300, "the note left ink, got {dark} inked pixels");
}
