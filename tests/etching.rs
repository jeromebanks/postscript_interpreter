//! The photo-to-line-etching library (issue #15, `lib/etching.ps`):
//! JPEG header scanning (`et-dims`) and the tone-driven hatch renderer
//! (`et-draw`). Ink placement is a deterministic function of the
//! source photo's decoded samples (no randomness involved), so unlike
//! `lib/handscript.ps`'s jittered ink this is asserted on directly —
//! coverage should track darkness, and known-bad input should fail
//! cleanly rather than hang or panic.

use pscat::{Interp, PsError};

fn with_lib(w: u32, h: u32) -> Interp {
    let lib = std::fs::read("lib/etching.ps").expect("library present");
    let mut it = Interp::with_page(w, h).expect("page");
    it.run_source(&lib)
        .unwrap_or_else(|e| panic!("etching.ps failed: {}", it.error_report(&e)));
    it
}

fn dims(it: &mut Interp, photo: &str) -> (i64, i64, i64) {
    it.run_str(&format!("clear << /Photo ({photo}) >> et-dims"))
        .unwrap_or_else(|e| panic!("et-dims failed: {}", it.error_report(&e)));
    let stack = it.operand_stack();
    let parse = |i: usize| -> i64 {
        stack[i]
            .repr()
            .parse()
            .unwrap_or_else(|_| panic!("expected an integer, got {}", stack[i].repr()))
    };
    let n = stack.len();
    assert_eq!(n, 3, "et-dims should leave width height ncomp");
    (parse(n - 3), parse(n - 2), parse(n - 1))
}

#[test]
fn et_dims_reads_known_fixtures() {
    let mut it = with_lib(10, 10);
    assert_eq!(dims(&mut it, "tests/data/gray8.jpg"), (8, 8, 1));
    assert_eq!(dims(&mut it, "tests/data/red4.jpg"), (4, 4, 3));
    assert_eq!(dims(&mut it, "tests/data/sphere.jpg"), (200, 150, 3));
}

#[test]
fn et_dims_rejects_non_jpeg_data() {
    let mut it = with_lib(10, 10);
    let tmp = std::env::temp_dir().join("etching_test_not_a_jpeg.jpg");
    std::fs::write(&tmp, b"not a jpeg at all, just text").unwrap();
    let err = it
        .run_str(&format!("<< /Photo ({}) >> et-dims", tmp.to_str().unwrap()))
        .expect_err("garbage input must not parse as a JPEG");
    assert!(
        matches!(&err, PsError::Undefined(name) if name.contains("et-jpeg-not-a-jpeg")),
        "expected the et-jpeg-not-a-jpeg guard, got {err:?}"
    );
    std::fs::remove_file(&tmp).ok();
}

#[test]
fn et_dims_rejects_truncated_jpeg() {
    let mut it = with_lib(10, 10);
    let tmp = std::env::temp_dir().join("etching_test_truncated.jpg");
    std::fs::write(&tmp, [0xFF, 0xD8, 0xFF]).unwrap();
    let err = it
        .run_str(&format!("<< /Photo ({}) >> et-dims", tmp.to_str().unwrap()))
        .expect_err("a file cut off mid-header must not hang or panic");
    assert!(
        matches!(&err, PsError::Undefined(name) if name.contains("et-jpeg-truncated")),
        "expected the et-jpeg-truncated guard, got {err:?}"
    );
    std::fs::remove_file(&tmp).ok();
}

/// Count non-paper pixels in a page-space rectangle — a proxy for ink
/// coverage that doesn't depend on exact stroke placement.
fn ink_coverage(it: &Interp, x0: u32, y0: u32, x1: u32, y1: u32) -> f64 {
    let gfx = it.gfx();
    let mut inked = 0u32;
    let mut total = 0u32;
    for y in y0..y1 {
        for x in x0..x1 {
            let Some(p) = gfx.pixmap.pixel(x, y) else {
                continue;
            };
            total += 1;
            // Paper is a light off-white; anything noticeably darker
            // than that is a hatch stroke.
            if p.red() < 200 {
                inked += 1;
            }
        }
    }
    f64::from(inked) / f64::from(total.max(1))
}

#[test]
fn darker_regions_get_denser_hatching() {
    // sphere.jpg (tests/data/sphere.jpg): a vertical-gradient backdrop,
    // darkest along the top edge, lightest near the bottom. Sample
    // corner strips clear of the sphere itself (which sits roughly
    // x 40..150 of a 200-wide page) so only the backdrop is measured.
    let mut it = with_lib(200, 150);
    it.run_str(
        "<< /Photo (tests/data/sphere.jpg) /PageWidth 200 /PageHeight 150 >> et-draw showpage",
    )
    .unwrap_or_else(|e| panic!("et-draw failed: {}", it.error_report(&e)));
    let dark_strip = ink_coverage(&it, 0, 0, 35, 15);
    let light_strip = ink_coverage(&it, 0, 135, 35, 150);
    assert!(
        dark_strip > light_strip,
        "the darker backdrop band should hatch denser: dark={dark_strip} light={light_strip}"
    );
}

#[test]
fn a_bright_highlight_stays_mostly_paper() {
    // The sphere's specular highlight (built into the generator around
    // its lit side) should be too bright to hatch at all — the
    // /Threshold2 default of 0.55 and the primary pass's own low floor
    // both gate on darkness, so pure highlight stays paper-white.
    let mut it = with_lib(200, 150);
    it.run_str(
        "<< /Photo (tests/data/sphere.jpg) /PageWidth 200 /PageHeight 150 >> et-draw showpage",
    )
    .unwrap_or_else(|e| panic!("et-draw failed: {}", it.error_report(&e)));
    // The highlight sits upper-left of the ball's center.
    let highlight = ink_coverage(&it, 60, 35, 100, 58);
    assert!(
        highlight < 0.25,
        "the specular highlight should render mostly as paper, got {highlight}"
    );
}

#[test]
fn photo_orientation_is_not_flipped() {
    // tests/data/topdark.jpg: dark top rows over a light field. The
    // decoded sample buffer's rows run top-down (row 0 first out of
    // the JPEG) but PostScript's y runs bottom-up — a caught-in-review
    // bug rendered every photo upside down until et-hatch's sy mapping
    // flipped for that. ink_coverage's y here is raster space (row 0 =
    // top of the PNG, same as gfx.pixmap always uses), so a correctly
    // oriented render must be denser near the top than the bottom.
    let mut it = with_lib(64, 64);
    it.run_str(
        "<< /Photo (tests/data/topdark.jpg) /PageWidth 64 /PageHeight 64 >> et-draw showpage",
    )
    .unwrap_or_else(|e| panic!("et-draw failed: {}", it.error_report(&e)));
    let top = ink_coverage(&it, 0, 0, 64, 20);
    let bottom = ink_coverage(&it, 0, 44, 64, 64);
    assert!(
        top > bottom,
        "the dark source rows are at the top; rendered top should be denser: top={top} bottom={bottom}"
    );
}
