//! The handwrite library (Stage 13, `lib/handscript.ps`): wrap-engine
//! behavior through the interpreter, plus gs compatibility. The ink
//! itself is rand-driven art, so pixel comparison with gs is excluded
//! (same policy as the Stage 12 example); layout, though, is exact —
//! line breaks come from skeleton advances, never from jitter.

use pscat::Interp;

fn with_lib(w: u32, h: u32) -> Interp {
    let lib = std::fs::read("lib/handscript.ps").expect("library present");
    let mut it = Interp::with_page(w, h).expect("page");
    it.run_source(&lib)
        .unwrap_or_else(|e| panic!("handscript.ps failed: {}", it.error_report(&e)));
    it
}

/// Run `opts hs-linecount` and return the count.
fn linecount(it: &mut Interp, opts: &str) -> i64 {
    it.run_str(&format!("clear {opts} hs-linecount"))
        .unwrap_or_else(|e| panic!("hs-linecount failed: {}", it.error_report(&e)));
    let repr = it
        .operand_stack()
        .last()
        .expect("hs-linecount pushes a count")
        .repr();
    repr.parse()
        .unwrap_or_else(|_| panic!("expected an integer line count, got {repr}"))
}

#[test]
fn a_narrower_column_needs_more_lines() {
    let mut it = with_lib(100, 100);
    let text = "(the quick brown fox jumps over the lazy dog and keeps going)";
    let wide = linecount(&mut it, &format!("<< /Text {text} /Size 20 /Width 600 >>"));
    let narrow = linecount(&mut it, &format!("<< /Text {text} /Size 20 /Width 200 >>"));
    assert!(wide >= 1, "some lines at width 600, got {wide}");
    assert!(
        narrow > wide,
        "narrower column must wrap more: {narrow} vs {wide}"
    );
}

#[test]
fn newlines_force_breaks_and_blank_lines_survive() {
    let mut it = with_lib(100, 100);
    let one = linecount(&mut it, "<< /Text (a b c) /Size 20 /Width 600 >>");
    assert_eq!(one, 1, "three short words fit one line");
    let forced = linecount(&mut it, "<< /Text (a\nb\nc) /Size 20 /Width 600 >>");
    assert_eq!(forced, 3, "newlines force breaks");
    let blank = linecount(&mut it, "<< /Text (a\n\nb) /Size 20 /Width 600 >>");
    assert_eq!(blank, 3, "a blank line still takes a line");
}

#[test]
fn unknown_characters_do_not_error() {
    let mut it = with_lib(100, 100);
    // Uppercase, braces, tilde: all outside the font. They render as
    // .notdef (invisible advance) rather than erroring.
    let n = linecount(&mut it, "<< /Text (mixed CASE ~{}#) /Size 20 /Width 600 >>");
    assert_eq!(n, 1);
    it.run_str("clear << /Text (ALL UNKNOWN ~~~) /Size 30 /Width 300 >> hs-write")
        .unwrap_or_else(|e| panic!("hs-write failed: {}", it.error_report(&e)));
}

#[test]
fn measuring_agrees_with_writing() {
    // hs-linecount consumes rand (start-of-line wobble); a following
    // hs-write must still break identically, because breaks come from
    // fixed skeleton advances. Render two interps, one with a
    // pre-pass, and compare pages.
    let opts = "<< /Text (the pen is mightier than the sword, probably)
                   /Size 24 /Width 300 /Height 300 /Margin 20
                   /Ink [0 0 0] /Seed 42 >>";
    let mut a = with_lib(300, 300);
    a.run_str(&format!("{opts} hs-write"))
        .unwrap_or_else(|e| panic!("hs-write failed: {}", a.error_report(&e)));
    let mut b = with_lib(300, 300);
    b.run_str(&format!("{opts} hs-linecount pop {opts} hs-write"))
        .unwrap_or_else(|e| panic!("hs-write failed: {}", b.error_report(&e)));
    assert_eq!(
        a.gfx().pixmap.data(),
        b.gfx().pixmap.data(),
        "the same seed writes the same page, pre-pass or not"
    );
    let dark = a
        .gfx()
        .pixmap
        .data()
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|p| p[0] < 200)
        .count();
    assert!(dark > 300, "the note left ink, got {dark} inked pixels");
}

#[test]
fn ghostscript_accepts_the_library() {
    let gs_ok = std::process::Command::new("gs")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success());
    if !gs_ok {
        eprintln!("skipping gs compatibility check: gs not installed");
        return;
    }
    let lib = std::fs::read_to_string("lib/handscript.ps").expect("library present");
    let driver = "/HandOpts << /Text (hello from ghostscript land)\n\
                  /Size 30 /Width 400 /Height 200 /Margin 20 /Paper /ruled >> def\n\
                  HandOpts hs-linecount pop HandOpts hs-write showpage\n";
    let dir = std::env::temp_dir().join(format!("pscat-handwrite-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join("combined.ps");
    std::fs::write(&path, format!("{lib}\n{driver}")).expect("write combined file");
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
        .arg(&path)
        .status()
        .expect("run gs");
    assert!(status.success(), "gs rejected the handwrite library");
}
