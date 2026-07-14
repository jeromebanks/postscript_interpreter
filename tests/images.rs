//! image / imagemask / colorimage — Stage 7 task 4. Orientation and
//! mask polarity are pinned against Ghostscript (see the imagemask
//! polarity note in ops/image.rs).

use pscat::{Interp, PsError};

fn run(src: &str) -> Interp {
    let mut it = Interp::with_page(100, 100).expect("test page");
    it.run_str(src)
        .unwrap_or_else(|e| panic!("run failed: {}", it.error_report(&e)));
    it
}

fn pixel(it: &Interp, x: u32, y: u32) -> (u8, u8, u8) {
    let p = it.gfx().pixmap.pixel(x, y).expect("pixel in bounds");
    (p.red(), p.green(), p.blue())
}

const BLACK: (u8, u8, u8) = (0, 0, 0);
const WHITE: (u8, u8, u8) = (255, 255, 255);

/// 2x2 gray checker: top row black/white, bottom row white/black,
/// occupying user (10..90)^2. Matrix [2 0 0 -2 0 2] puts sample row 0
/// at the top, PostScript-style.
const CHECKER: &str = "10 10 translate 80 80 scale
     2 2 8 [2 0 0 -2 0 2] {<00ff ff00>} image";

#[test]
fn level1_image_paints_the_checker() {
    let it = run(CHECKER);
    // User top-left (30,70) = device (30,30): sample (0,0) = 00.
    assert_eq!(pixel(&it, 30, 30), BLACK);
    assert_eq!(pixel(&it, 70, 30), WHITE);
    assert_eq!(pixel(&it, 30, 70), WHITE);
    assert_eq!(pixel(&it, 70, 70), BLACK);
    // Outside the unit square: untouched.
    assert_eq!(pixel(&it, 5, 5), WHITE);
}

#[test]
fn image_respects_rotation_and_clip() {
    let it = run(
        "newpath 50 0 moveto 100 0 lineto 100 100 lineto 50 100 lineto closepath clip newpath
         50 50 translate 30 rotate -40 -40 translate 80 80 scale
         2 2 8 [2 0 0 -2 0 2] {<00ff ff00>} image",
    );
    // Left of the clip boundary must be untouched.
    for y in [20, 50, 80] {
        assert_eq!(pixel(&it, 30, y), WHITE, "clipped at (30,{y})");
    }
    // Something dark must have landed on the right side.
    let dark = (50..100)
        .flat_map(|x| (0..100).map(move |y| (x, y)))
        .filter(|&(x, y)| pixel(&it, x, y).0 < 100)
        .count();
    assert!(dark > 100, "rotated image painted {dark} dark px");
}

#[test]
fn imagemask_polarity_true_paints_ones() {
    // Samples <80> on a 2x1 mask: [1, 0]. Pinned against gs: with
    // true, the '1' half paints in the current color.
    let it = run("1 0 0 setrgbcolor 10 40 translate 80 20 scale
         2 1 true [2 0 0 -1 0 1] {<80>} imagemask");
    assert_eq!(pixel(&it, 30, 50), (255, 0, 0), "sample 1 painted");
    assert_eq!(pixel(&it, 70, 50), WHITE, "sample 0 masked out");
}

#[test]
fn imagemask_polarity_false_paints_zeros() {
    let it = run("0 0 1 setrgbcolor 10 40 translate 80 20 scale
         2 1 false [2 0 0 -1 0 1] {<80>} imagemask");
    assert_eq!(pixel(&it, 30, 50), WHITE);
    assert_eq!(pixel(&it, 70, 50), (0, 0, 255));
}

#[test]
fn colorimage_rgb_interleaved() {
    let it = run("10 10 translate 80 80 scale
         2 1 8 [2 0 0 -1 0 1] {<ff0000 0000ff>} false 3 colorimage");
    assert_eq!(pixel(&it, 30, 50), (255, 0, 0));
    assert_eq!(pixel(&it, 70, 50), (0, 0, 255));
}

#[test]
fn dict_image_uses_colorspace_and_decode() {
    // DeviceRGB via setcolorspace; Decode [1 0 1 0 1 0] inverts.
    let it = run("/DeviceRGB setcolorspace
         10 10 translate 80 80 scale
         << /ImageType 1 /Width 1 /Height 1 /BitsPerComponent 8
            /ImageMatrix [1 0 0 -1 0 1]
            /Decode [1 0 1 0 1 0]
            /DataSource <00ff00>
         >> image");
    // 00ff00 inverted = ff00ff.
    assert_eq!(pixel(&it, 50, 50), (255, 0, 255));
}

#[test]
fn file_source_reads_inline_data() {
    // The classic: image data inline in the program, through a filter
    // on currentfile. The scanner must resume cleanly afterwards.
    let it = run("10 10 translate 80 80 scale
         1 1 8 [1 0 0 -1 0 1]
         currentfile /ASCIIHexDecode filter image
         00>
         initmatrix 0 0 1 setrgbcolor 4 setlinewidth
         newpath 2 95 moveto 8 95 lineto stroke");
    assert_eq!(pixel(&it, 50, 50), BLACK, "inline sample painted");
    assert_eq!(pixel(&it, 5, 5), (0, 0, 255), "program resumed after data");
}

#[test]
fn proc_source_called_per_chunk() {
    // The proc returns one row per call; a counter proves it ran
    // once per scanline.
    let mut it = run("/calls 0 def
         /row 2 string def
         10 10 translate 80 80 scale
         2 2 8 [2 0 0 -2 0 2]
         { /calls calls 1 add def
           calls 1 eq { (\\000\\377) } { (\\377\\000) } ifelse }
         image
         calls");
    assert_eq!(it.pop().expect("calls").repr(), "2");
    assert_eq!(pixel(&it, 30, 30), BLACK);
    assert_eq!(pixel(&it, 70, 70), BLACK);
}

#[test]
fn empty_string_from_proc_ends_the_image_early() {
    // EOD after one of two rows: the second row reads as sample 0
    // (black) — premature end renders what arrived, no error.
    let it = run("/first true def
         10 10 translate 80 80 scale
         2 2 8 [2 0 0 -2 0 2]
         { first { /first false def (\\377\\377) } { () } ifelse }
         image");
    assert_eq!(pixel(&it, 30, 30), WHITE, "delivered row");
    assert_eq!(pixel(&it, 30, 70), BLACK, "missing row reads as 0");
}

#[test]
fn exit_inside_data_proc_is_invalidexit() {
    let mut it = Interp::with_page(100, 100).expect("page");
    assert_eq!(
        it.run_str("2 2 8 [2 0 0 -2 0 2] {exit} image"),
        Err(PsError::InvalidExit)
    );
}

#[test]
fn image_argument_errors() {
    let mut it = Interp::with_page(100, 100).expect("page");
    assert_eq!(
        it.run_str("2 2 3 [2 0 0 -2 0 2] {<00>} image"),
        Err(PsError::Rangecheck),
        "bits must be 1/2/4/8"
    );
    assert_eq!(
        it.run_str("2 2 8 [2 0 0 -2 0 2] {<00>} true 3 colorimage"),
        Err(PsError::Limitcheck),
        "multi-source colorimage is a documented gap"
    );
}
