//! Indexed and Separation color spaces + setcolor — Stage 8 task 4.
//! All expectations pinned against gs (values in comments are gs's).

use pscat::{Interp, PsError};

fn run(src: &str) -> Interp {
    let mut it = Interp::with_page(100, 100).expect("test page");
    it.run_str(src)
        .unwrap_or_else(|e| panic!("run failed: {}", it.error_report(&e)));
    it
}

fn top_repr(it: &mut Interp) -> String {
    it.pop().expect("operand").repr()
}

fn pixel(it: &Interp, x: u32, y: u32) -> (u8, u8, u8) {
    let p = it.gfx().pixmap.pixel(x, y).expect("pixel in bounds");
    (p.red(), p.green(), p.blue())
}

// A two-entry RGB palette: index 0 = red, 1 = green.
const PALETTE: &str = "[/Indexed /DeviceRGB 1 <ff000000ff00>]";

#[test]
fn indexed_setcolor_looks_up_the_palette() {
    // gs: 1 setcolor -> currentrgbcolor 0.0 1.0 0.0.
    let mut it = run(&format!(
        "{PALETTE} setcolorspace 1 setcolor currentrgbcolor"
    ));
    assert_eq!(top_repr(&mut it), "0.0");
    assert_eq!(top_repr(&mut it), "1.0");
    assert_eq!(top_repr(&mut it), "0.0");
}

#[test]
fn indexed_initial_color_is_index_zero() {
    // gs: currentrgbcolor 1.0 0.0 0.0 right after setcolorspace.
    let mut it = run(&format!("{PALETTE} setcolorspace currentrgbcolor"));
    assert_eq!(top_repr(&mut it), "0.0");
    assert_eq!(top_repr(&mut it), "0.0");
    assert_eq!(top_repr(&mut it), "1.0");
}

#[test]
fn indexed_out_of_range_clamps_like_gs() {
    // The PLRM says rangecheck; gs renders (clamps). We follow gs.
    let mut it = run(&format!(
        "{PALETTE} setcolorspace 7 setcolor currentrgbcolor"
    ));
    assert_eq!(top_repr(&mut it), "0.0");
    assert_eq!(top_repr(&mut it), "1.0", "clamped to hival = green");
    assert_eq!(top_repr(&mut it), "0.0");
}

#[test]
fn indexed_image_paints_palette_colors() {
    // 2x1 8-bit indices <00 01>: left red, right green.
    let it = run(&format!(
        "{PALETTE} setcolorspace
         10 10 translate 80 80 scale
         << /ImageType 1 /Width 2 /Height 1 /BitsPerComponent 8
            /ImageMatrix [2 0 0 -1 0 1] /DataSource <0001> >> image"
    ));
    assert_eq!(pixel(&it, 30, 50), (255, 0, 0));
    assert_eq!(pixel(&it, 70, 50), (0, 255, 0));
}

#[test]
fn indexed_4bit_image_with_cmyk_base() {
    // 4-bit indices into a CMYK palette: 0 = pure cyan, 1 = pure
    // magenta. CMYK->RGB conversion matches the colorimage path.
    let it = run("[/Indexed /DeviceCMYK 1 <ff000000 00ff0000>] setcolorspace
         10 10 translate 80 80 scale
         << /ImageType 1 /Width 2 /Height 1 /BitsPerComponent 4
            /ImageMatrix [2 0 0 -1 0 1] /DataSource <01> >> image");
    assert_eq!(pixel(&it, 30, 50), (0, 255, 255), "cyan");
    assert_eq!(pixel(&it, 70, 50), (255, 0, 255), "magenta");
}

#[test]
fn separation_tint_transform_runs_as_a_procedure() {
    // gs: initial color (tint 1) -> 0 0 0; 0.25 setcolor -> 0.75 gray.
    let mut it = run(
        "[/Separation /Ink /DeviceRGB {1 exch sub dup dup}] setcolorspace
         currentrgbcolor
         0.25 setcolor currentrgbcolor",
    );
    for _ in 0..3 {
        assert_eq!(top_repr(&mut it), "0.75");
    }
    for _ in 0..3 {
        assert_eq!(top_repr(&mut it), "0.0", "initial tint is 1.0");
    }
}

#[test]
fn separation_transform_to_cmyk_alt_space() {
    // Tint 1 through a CMYK alt space: full cyan.
    let mut it = run(
        "[/Separation (PANTONE-ish) /DeviceCMYK {dup 0 0 3 -1 roll 0 mul}] setcolorspace
         1 setcolor currentrgbcolor",
    );
    assert_eq!(top_repr(&mut it), "1.0", "b");
    assert_eq!(top_repr(&mut it), "1.0", "g");
    assert_eq!(top_repr(&mut it), "0.0", "r: full cyan");
}

#[test]
fn separation_fill_paints() {
    let it = run(
        "[/Separation /Ink /DeviceRGB {1 exch sub dup dup}] setcolorspace
         0.5 setcolor
         newpath 20 20 moveto 80 20 lineto 80 80 lineto 20 80 lineto closepath fill",
    );
    let (r, g, b) = pixel(&it, 50, 50);
    assert_eq!((r, g), (g, b), "gray");
    assert!((i32::from(r) - 128).abs() <= 1, "0.5 gray, got {r}");
}

#[test]
fn device_setcolor_by_current_space() {
    // Values chosen to be exact in f32 (color state is f32).
    let mut it = run("/DeviceRGB setcolorspace 0.25 0.5 0.75 setcolor currentrgbcolor");
    assert_eq!(top_repr(&mut it), "0.75");
    assert_eq!(top_repr(&mut it), "0.5");
    assert_eq!(top_repr(&mut it), "0.25");
}

#[test]
fn currentcolorspace_hands_back_the_array() {
    let mut it = run(&format!("{PALETTE} setcolorspace currentcolorspace 0 get"));
    assert_eq!(top_repr(&mut it), "/Indexed");
}

#[test]
fn colorspace_rolls_back_with_grestore() {
    // Color space is part of the graphics state.
    let mut it = run(&format!(
        "gsave {PALETTE} setcolorspace grestore currentcolorspace 0 get"
    ));
    assert_eq!(top_repr(&mut it), "/DeviceGray");
}

#[test]
fn exit_cannot_cross_a_tint_transform() {
    let mut it = Interp::with_page(100, 100).expect("page");
    assert_eq!(
        it.run_str("[/Separation /X /DeviceGray {exit}] setcolorspace"),
        Err(PsError::InvalidExit)
    );
}

#[test]
fn indexed_lookup_procedures_are_a_documented_gap() {
    let mut it = Interp::with_page(100, 100).expect("page");
    assert_eq!(
        it.run_str("[/Indexed /DeviceRGB 1 {3 mul dup dup}] setcolorspace"),
        Err(PsError::Typecheck)
    );
}
