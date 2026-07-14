//! Golden-image tests: render each example with pscat and with
//! Ghostscript, then compare downsampled images. Downsampling to 10x10
//! blocks averages away antialiasing and sub-pixel stroke differences
//! between rasterizers while still catching any real geometry or color
//! error (a missing shape, a flipped axis, a wrong transform).
//!
//! Skipped (with a note) when Ghostscript isn't installed.

use std::process::Command;

use pscat::Interp;
use tiny_skia::Pixmap;

const SIZE: u32 = 500;
const BLOCK: u32 = 10;

fn gs_available() -> bool {
    Command::new("gs")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

fn render_pscat(path: &str) -> Pixmap {
    let source = std::fs::read(path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    let mut it = Interp::with_page(SIZE, SIZE).expect("page");
    it.run_source(&source)
        .unwrap_or_else(|e| panic!("{path}: {}", it.error_report(&e)));
    it.gfx().pixmap.clone()
}

fn render_gs(path: &str) -> Pixmap {
    let out = std::env::temp_dir().join(format!("pscat_gs_{}.png", path.replace(['/', '.'], "_")));
    let status = Command::new("gs")
        .args([
            "-q",
            "-dNOPAUSE",
            "-dBATCH",
            "-sDEVICE=png16m",
            &format!("-g{SIZE}x{SIZE}"),
            "-r72",
            &format!("-o{}", out.display()),
            path,
        ])
        .status()
        .expect("run gs");
    assert!(status.success(), "gs failed on {path}");
    let data = std::fs::read(&out).expect("read gs output");
    let _ = std::fs::remove_file(&out);
    Pixmap::decode_png(&data).expect("decode gs png")
}

/// Mean RGB per BLOCK×BLOCK cell.
fn downsample(pm: &Pixmap) -> Vec<[f64; 3]> {
    let (w, h) = (pm.width(), pm.height());
    let (bw, bh) = (w / BLOCK, h / BLOCK);
    let mut cells = vec![[0f64; 3]; (bw * bh) as usize];
    let data = pm.data();
    for y in 0..h {
        for x in 0..w {
            let i = ((y * w + x) * 4) as usize;
            let cell = &mut cells[((y / BLOCK) * bw + x / BLOCK) as usize];
            cell[0] += f64::from(data[i]);
            cell[1] += f64::from(data[i + 1]);
            cell[2] += f64::from(data[i + 2]);
        }
    }
    let n = f64::from(BLOCK * BLOCK);
    for c in &mut cells {
        c[0] /= n;
        c[1] /= n;
        c[2] /= n;
    }
    cells
}

fn luma(rgb: &[f64; 3]) -> f64 {
    (rgb[0] + rgb[1] + rgb[2]) / 3.0
}

fn assert_close(name: &str, text: bool) {
    let ours = downsample(&render_pscat(&format!("examples/{name}")));
    let theirs = downsample(&render_gs(&format!("examples/{name}")));
    assert_eq!(ours.len(), theirs.len());
    // The sharp check: a block with ink in one render and none in the
    // other is a missing or misplaced shape, not an antialiasing
    // difference. (Pure AA-density differences — e.g. dozens of
    // overlapping half-pixel strokes in the burst hub — leave both
    // renders clearly inked.) Text needs wider bands: pscat's Liberation
    // faces and gs's URW faces are metric-compatible but not
    // shape-identical, so glyph *edges* put a few pixels in blocks the
    // other render grazes — solidly-inked-vs-untouched still fails.
    let (ink, blank) = if text { (200.0, 253.0) } else { (230.0, 250.0) };
    let mut total = 0f64;
    for (i, (a, b)) in ours.iter().zip(&theirs).enumerate() {
        for ch in 0..3 {
            total += (a[ch] - b[ch]).abs();
        }
        let (la, lb) = (luma(a), luma(b));
        let disjoint = (la < ink && lb > blank) || (lb < ink && la > blank);
        assert!(
            !disjoint,
            "{name}: block {i} inked in one render only ({la:.0} vs {lb:.0})"
        );
    }
    let mean = total / (ours.len() * 3) as f64;
    // Observed mean ≤ 2.1 across all four examples; a real geometry error
    // moves it by an order of magnitude.
    assert!(mean < 6.0, "{name}: mean block diff {mean:.2}");
}

#[test]
fn examples_match_ghostscript() {
    if !gs_available() {
        eprintln!("skipping golden tests: ghostscript (gs) not installed");
        return;
    }
    for name in [
        "sierpinski.ps",
        "koch_snowflake.ps",
        "golden_spiral.ps",
        "stage2_demo.ps",
        "testcard.ps",
    ] {
        assert_close(name, false);
    }
    // Text compares with the wider ink bands (see assert_close).
    assert_close("specimen.ps", true);
    // Type 3 glyphs are pure PostScript geometry — deterministic in
    // both interpreters (the ransom demo, which uses rand, stays out:
    // rand is implementation-specific).
    assert_close("type3_demo.ps", true);
    // Inline images: image/colorimage/imagemask + filter chains.
    assert_close("postcard.ps", true);
}
