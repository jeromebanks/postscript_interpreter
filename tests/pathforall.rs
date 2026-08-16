//! `pathforall` / `flattenpath` (Stage 19): path enumeration as an
//! execution-stack frame, coordinates reported in user space, curves
//! flattened to chords. Behavior pinned against gs with the snippet in
//! `pathforall_reports_user_space_coords` (byte-identical output).

use pscat::{Interp, PsError};

fn eval(src: &str) -> Vec<String> {
    let mut it = Interp::new();
    it.run_str(src)
        .unwrap_or_else(|e| panic!("eval of {src:?} failed: {e}"));
    it.operand_stack().iter().map(|o| o.repr()).collect()
}

#[test]
fn pathforall_reports_user_space_coords() {
    // gs-pinned: under translate+scale the reported coordinates are the
    // user-space ones the program wrote, not device pixels. Empty procs
    // leave the pushed coordinates on the stack (coords go on *before*
    // the proc runs); close pushes nothing.
    assert_eq!(
        eval(
            "72 72 translate 2 2 scale newpath 10 20 moveto 30 20 lineto closepath \
             { } { } { } { } pathforall"
        ),
        ["10.0", "20.0", "30.0", "20.0"]
    );
}

#[test]
fn pathforall_dispatches_all_four_procs() {
    assert_eq!(
        eval(
            "newpath 1 2 moveto 3 4 lineto 5 6 7 8 9 10 curveto closepath \
             /m 0 def /l 0 def /c 0 def /z 0 def \
             { pop pop /m m 1 add def } { pop pop /l l 1 add def } \
             { 6 { pop } repeat /c c 1 add def } { /z z 1 add def } pathforall \
             m l c z"
        ),
        ["1", "1", "1", "1"]
    );
}

#[test]
fn implicit_moveto_after_closepath_without_explicit_moveto() {
    // gs-pinned: per the PLRM, a lineto/curveto immediately following
    // closepath with no intervening moveto behaves as though a moveto
    // to the current point (closepath's own return-to-start point) had
    // been inserted first, starting a fresh subpath there rather than
    // silently continuing the just-closed one. Confirmed against real
    // Ghostscript: 2 movetos for this path, not 1 (a cross-model review
    // finding on the paintkit PR, #76 -- walkpath relies on pathforall
    // reporting subpath boundaries correctly to split a source path
    // into independent runs, and this pattern was silently merging a
    // closed run with whatever open drawing followed it).
    assert_eq!(
        eval(
            "newpath 0 0 moveto 10 0 lineto 10 10 lineto closepath \
             50 50 lineto \
             /m 0 def \
             { pop pop /m m 1 add def } { pop pop } { pop pop pop pop pop pop } { } \
             pathforall m"
        ),
        ["2"]
    );
}

#[test]
fn pathforall_procs_must_be_procedures() {
    let mut it = Interp::new();
    let err = it
        .run_str("newpath 0 0 moveto {} {} {} 4 pathforall")
        .unwrap_err();
    assert!(matches!(err, PsError::Typecheck), "got {err}");
}

#[test]
fn exit_leaves_pathforall() {
    // gs-pinned: exit inside a pathforall proc ends the enumeration,
    // like the other looping operators.
    assert_eq!(
        eval(
            "newpath 0 0 moveto 1 1 lineto 2 2 lineto 3 3 lineto \
             /n 0 def \
             { pop pop } { pop pop /n n 1 add def n 2 eq { exit } if } {} {} pathforall \
             n"
        ),
        ["2"]
    );
}

#[test]
fn flattenpath_removes_curves() {
    // After flattenpath the enumeration sees moves and lines only —
    // and enough lines to actually trace the curve.
    let got = eval(
        "newpath 0 0 moveto 10 0 20 10 30 0 curveto flattenpath \
         /nl 0 def /nc 0 def \
         { pop pop } { pop pop /nl nl 1 add def } \
         { 6 { pop } repeat /nc nc 1 add def } {} pathforall \
         nl nc",
    );
    assert_eq!(got[1], "0", "no curves survive flattenpath");
    // Chord count is ours, not gs's (we don't model setflat; fixed
    // quarter-pixel tolerance) — assert shape, not a particular count.
    let lines: i64 = got[0].parse().unwrap();
    assert!(lines > 4, "expected several chords, got {lines}");
}

#[test]
fn charpath_outlines_are_walkable() {
    // The idiom the art toolkit is built on: text → charpath →
    // flattenpath → pathforall gives stampable points along real
    // glyph outlines, in any font.
    let got = eval(
        "/Helvetica findfont 100 scalefont setfont \
         newpath 10 30 moveto (S) true charpath flattenpath \
         /n 0 def { pop pop /n n 1 add def } { pop pop /n n 1 add def } {} {} pathforall \
         n",
    );
    let n: i64 = got[0].parse().unwrap();
    assert!(n > 40, "expected a walkable S outline, got {n} points");
}

#[test]
fn pathforall_snapshot_survives_path_mutation() {
    // The enumeration walks the path as it was when pathforall ran;
    // building a new path inside the procs (the rebuild idiom) is safe.
    assert_eq!(
        eval(
            "newpath 0 0 moveto 5 5 lineto 9 9 lineto \
             /n 0 def \
             { pop pop newpath } { pop pop /n n 1 add def } {} {} pathforall \
             n"
        ),
        ["2"]
    );
}
