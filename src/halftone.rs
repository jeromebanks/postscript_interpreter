//! Classic clustered-dot halftone screening — Stage 10's optional
//! "authentic laser printer" render mode (`--halftone`).
//!
//! This is a *render mode*, not the `setscreen` operator: the page is
//! rasterized normally and the finished raster is screened, the way a
//! mono printer's RIP screens a contone page. The screen is the
//! classic euclidean dot on a 45°-rotated lattice: round black dots
//! grow from the cell center, sized so ink coverage equals darkness;
//! past 50% the pattern inverts to white holes shrinking at the cell
//! corners — the same tone curve the PLRM's euclidean spot function
//! family produces (a naive `1 - r²` threshold overshoots midtones,
//! since a circle's area is only π/4 of its bounding square's).
//! Applied to the live window and `--png`; the vector targets
//! (`--svg`/`--pdf`) keep their contone art.

use tiny_skia::Pixmap;

/// Device pixels per screen cell. ≈ a 53 lpi screen at 300 dpi — the
/// LaserWriter's own numbers — kept constant in *device* space so the
/// dots read the same at any `--dpi`.
pub const CELL: f32 = 5.66;
/// The classic single-ink screen angle.
pub const ANGLE_DEG: f32 = 45.0;

/// Screen `src` to pure black-and-white with the default cell and
/// angle.
pub fn screen(src: &Pixmap) -> Pixmap {
    screen_with(src, CELL, ANGLE_DEG)
}

/// Screen `src` with an explicit cell size (device px) and angle.
/// Luminance is Rec. 709; output pixels are pure black or white.
pub fn screen_with(src: &Pixmap, cell: f32, angle_deg: f32) -> Pixmap {
    let (w, h) = (src.width(), src.height());
    let mut out = Pixmap::new(w, h).expect("same nonzero size as src");
    let (sin, cos) = angle_deg.to_radians().sin_cos();
    let data = src.data();
    let odata = out.data_mut();
    for y in 0..h {
        for x in 0..w {
            let i = ((y * w + x) * 4) as usize;
            // The canvas is opaque, so premultiplied RGBA is plain RGBA.
            let (r, g, b) = (data[i] as f32, data[i + 1] as f32, data[i + 2] as f32);
            let darkness = 1.0 - (0.2126 * r + 0.7152 * g + 0.0722 * b) / 255.0;
            // Pixel center, rotated into screen space, position in cell.
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            let sx = px * cos + py * sin;
            let sy = py * cos - px * sin;
            let u = (sx.rem_euclid(cell) / cell) * 2.0 - 1.0;
            let v = (sy.rem_euclid(cell) / cell) * 2.0 - 1.0;
            // Cell coords run [-1, 1). To 50% darkness: a center dot
            // with area πr²/4 = darkness. Beyond: solid ink minus
            // white corner holes with area 1 - darkness (each cell
            // holds four quarter-holes). Dots touch neither midline
            // nor each other before the flip (r ≈ 0.80 < 1 at 50%),
            // so coverage tracks darkness exactly.
            let ink = if darkness >= 0.999 {
                true
            } else if darkness <= 0.001 {
                false
            } else if darkness <= 0.5 {
                u * u + v * v < 4.0 * darkness / std::f32::consts::PI
            } else {
                let cu = 1.0 - u.abs();
                let cv = 1.0 - v.abs();
                cu * cu + cv * cv >= 4.0 * (1.0 - darkness) / std::f32::consts::PI
            };
            let val = if ink { 0 } else { 255 };
            odata[i] = val;
            odata[i + 1] = val;
            odata[i + 2] = val;
            odata[i + 3] = 255;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiny_skia::Color;

    fn solid(r: u8, g: u8, b: u8) -> Pixmap {
        let mut pm = Pixmap::new(64, 64).expect("pixmap");
        pm.fill(Color::from_rgba8(r, g, b, 255));
        pm
    }

    fn ink_fraction(pm: &Pixmap) -> f32 {
        let data = pm.data();
        let black = data.as_chunks::<4>().0.iter().filter(|p| p[0] == 0).count();
        black as f32 / (pm.width() * pm.height()) as f32
    }

    #[test]
    fn white_paper_stays_white() {
        assert_eq!(ink_fraction(&screen(&solid(255, 255, 255))), 0.0);
    }

    #[test]
    fn solid_black_stays_solid() {
        assert_eq!(ink_fraction(&screen(&solid(0, 0, 0))), 1.0);
    }

    #[test]
    fn mid_gray_covers_about_half() {
        let frac = ink_fraction(&screen(&solid(128, 128, 128)));
        assert!(
            (0.35..=0.65).contains(&frac),
            "mid gray should dot to roughly half coverage, got {frac}"
        );
    }

    #[test]
    fn darker_means_bigger_dots() {
        let light = ink_fraction(&screen(&solid(210, 210, 210)));
        let dark = ink_fraction(&screen(&solid(60, 60, 60)));
        assert!(light > 0.0, "a light tint still gets dots");
        assert!(dark < 1.0, "a dark tint still shows paper");
        assert!(
            dark > light + 0.2,
            "coverage tracks darkness: {light} vs {dark}"
        );
    }

    #[test]
    fn output_is_pure_black_and_white() {
        let screened = screen(&solid(90, 140, 200));
        assert!(
            screened
                .data()
                .as_chunks::<4>()
                .0
                .iter()
                .all(|p| (p[0] == 0 || p[0] == 255) && p[0] == p[1] && p[1] == p[2] && p[3] == 255)
        );
    }
}
