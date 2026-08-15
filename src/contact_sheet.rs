//! Composite same-sized rendered pages into one grid PNG (issue #21):
//! a seed/parameter sweep's frames laid out side by side so an agent
//! can compare results in one image instead of opening N files.
//!
//! Every sweep frame renders at the same `--page`/`--dpi`, so cells
//! are always uniform size — this is a plain row-major byte copy, no
//! scaling or blending. Pages are opaque (see `halftone.rs`'s same
//! observation), so a straight RGBA copy is exact.

use tiny_skia::Pixmap;

/// Matches `--page`'s own per-side ceiling (`main.rs`) — a sweep
/// shouldn't be able to blow past the allocation guard a single
/// render already respects just by asking for more frames.
pub const MAX_SIDE_PX: u32 = 8000;

/// Gap in device pixels painted white between cells.
pub const GAP: u32 = 4;

/// Allocate a blank (white) `cols`×`rows` grid sized for `cell_w`×
/// `cell_h` cells with `gap` device pixels between them. Errors
/// instead of allocating past [`MAX_SIDE_PX`] on a side — a caller
/// (`main.rs`'s sweep loop) calls this *before* rendering any frames,
/// so an oversized request fails fast instead of after N renders.
pub fn new_sheet(
    cols: u32,
    rows: u32,
    cell_w: u32,
    cell_h: u32,
    gap: u32,
) -> Result<Pixmap, String> {
    let sheet_w = cols * cell_w + gap * cols.saturating_sub(1);
    let sheet_h = rows * cell_h + gap * rows.saturating_sub(1);
    if sheet_w > MAX_SIDE_PX || sheet_h > MAX_SIDE_PX {
        return Err(format!(
            "contact sheet would be {sheet_w}x{sheet_h}px, over the {MAX_SIDE_PX}px-per-side \
             limit -- use fewer sweep values, a smaller --page/--dpi, or drop --contact-sheet \
             for individual --png frames"
        ));
    }
    let mut sheet = Pixmap::new(sheet_w, sheet_h)
        .ok_or_else(|| format!("cannot allocate a {sheet_w}x{sheet_h}px contact sheet"))?;
    sheet.fill(tiny_skia::Color::WHITE);
    Ok(sheet)
}

/// Copy `frame` into cell `index` (row-major: 0 top-left, 1 to its
/// right, ...) of a `cols`-wide grid on `sheet`, with `gap` device
/// pixels between cells. `frame` must fit within one cell (true for
/// every sweep frame -- all render at the same `--page`/`--dpi`).
pub fn blit_cell(sheet: &mut Pixmap, cols: u32, gap: u32, index: usize, frame: &Pixmap) {
    let (w, h) = (frame.width(), frame.height());
    let (col, row) = (index as u32 % cols, index as u32 / cols);
    let (ox, oy) = (col * (w + gap), row * (h + gap));
    let sw = sheet.width();
    let sdata = sheet.data_mut();
    let pdata = frame.data();
    for y in 0..h {
        let src = &pdata[(y * w * 4) as usize..((y * w + w) * 4) as usize];
        let dst = (((oy + y) * sw + ox) * 4) as usize;
        sdata[dst..dst + src.len()].copy_from_slice(src);
    }
}

/// Lay `pages` into a `cols`×`rows` grid in one call — the batch form
/// of [`new_sheet`]/[`blit_cell`], for a caller that already has every
/// frame in memory (this module's own tests; `main.rs`'s sweep loop
/// uses the two primitives directly to stream frames instead). Extra
/// cells beyond `pages.len()` stay white.
pub fn compose(pages: &[Pixmap], cols: u32, rows: u32, gap: u32) -> Result<Pixmap, String> {
    let Some(first) = pages.first() else {
        return Err("contact sheet: no frames to compose".to_string());
    };
    let mut sheet = new_sheet(cols, rows, first.width(), first.height(), gap)?;
    for (i, page) in pages.iter().enumerate() {
        blit_cell(&mut sheet, cols, gap, i, page);
    }
    Ok(sheet)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(w: u32, h: u32, rgba: [u8; 4]) -> Pixmap {
        let mut p = Pixmap::new(w, h).unwrap();
        p.fill(tiny_skia::Color::from_rgba8(
            rgba[0], rgba[1], rgba[2], rgba[3],
        ));
        p
    }

    #[test]
    fn two_by_one_places_cells_left_to_right() {
        let red = solid(4, 4, [255, 0, 0, 255]);
        let blue = solid(4, 4, [0, 0, 255, 255]);
        let sheet = compose(&[red, blue], 2, 1, 0).unwrap();
        assert_eq!(sheet.width(), 8);
        assert_eq!(sheet.height(), 4);
        let px = |x: u32, y: u32| {
            let i = ((y * sheet.width() + x) * 4) as usize;
            &sheet.data()[i..i + 4]
        };
        assert_eq!(px(0, 0), &[255, 0, 0, 255]);
        assert_eq!(px(4, 0), &[0, 0, 255, 255]);
    }

    #[test]
    fn gap_leaves_a_white_seam() {
        let a = solid(2, 2, [0, 0, 0, 255]);
        let b = solid(2, 2, [0, 0, 0, 255]);
        let sheet = compose(&[a, b], 2, 1, 2).unwrap();
        assert_eq!(sheet.width(), 6); // 2 + 2(gap) + 2
        let i = (2 * 4) as usize; // first gap column, row 0
        assert_eq!(&sheet.data()[i..i + 4], &[255, 255, 255, 255]);
    }

    #[test]
    fn oversized_grid_errors_instead_of_allocating() {
        let page = solid(1000, 1000, [0, 0, 0, 255]);
        let pages: Vec<_> = std::iter::repeat_n(page, 9).collect();
        let err = compose(&pages, 9, 9, 0).unwrap_err();
        assert!(err.contains("over the"), "{err}");
    }

    #[test]
    fn empty_pages_errors() {
        let err = compose(&[], 1, 1, 0).unwrap_err();
        assert!(err.contains("no frames"), "{err}");
    }
}
