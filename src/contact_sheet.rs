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
    // u64 + checked arithmetic (not u32 `*`/`+`, which `main.rs`'s own
    // caller happens to stay under but this is a public function): a
    // caller passing large enough values could overflow u32 before
    // the MAX_SIDE_PX check ever runs -- a debug-build panic, or
    // release-mode wraparound silently accepting an oversized request
    // as a small one. u64 alone pushes the same risk out rather than
    // closing it (two near-u32::MAX terms summed can still overflow
    // u64), so this checks for real, not just further away (round-5
    // cross-model review, PR #66).
    let too_large = || "contact sheet dimensions overflow".to_string();
    let dim = |cells: u32, cell: u32| -> Result<u64, String> {
        (cells as u64)
            .checked_mul(cell as u64)
            .and_then(|a| a.checked_add((gap as u64).checked_mul(cells.saturating_sub(1) as u64)?))
            .ok_or_else(too_large)
    };
    let (sheet_w, sheet_h) = (dim(cols, cell_w)?, dim(rows, cell_h)?);
    if sheet_w > MAX_SIDE_PX as u64 || sheet_h > MAX_SIDE_PX as u64 {
        return Err(format!(
            "contact sheet would be {sheet_w}x{sheet_h}px, over the {MAX_SIDE_PX}px-per-side \
             limit -- use fewer sweep values, a smaller --page/--dpi, or drop --contact-sheet \
             for individual --png frames"
        ));
    }
    // Safe: just checked both are <= MAX_SIDE_PX, which fits u32.
    let (sheet_w, sheet_h) = (sheet_w as u32, sheet_h as u32);
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
    // main.rs's sweep loop validates this itself before ever calling
    // new_sheet/blit_cell directly, but compose is a public API of
    // its own -- a caller that skips that check would otherwise send
    // an out-of-grid index into blit_cell's slice arithmetic and
    // panic (round-4 cross-model review, PR #66).
    if (cols as u64) * (rows as u64) < pages.len() as u64 {
        return Err(format!(
            "contact sheet: {cols}x{rows} grid has only {} cells for {} frames",
            cols as u64 * rows as u64,
            pages.len()
        ));
    }
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

    /// Regression (round-4 cross-model review, PR #66): a grid with
    /// fewer cells than pages used to send an out-of-grid index into
    /// `blit_cell`'s slice arithmetic and panic, rather than erroring.
    #[test]
    fn grid_too_small_for_pages_errors_instead_of_panicking() {
        let a = solid(2, 2, [0, 0, 0, 255]);
        let b = solid(2, 2, [0, 0, 0, 255]);
        let err = compose(&[a, b], 1, 1, 0).unwrap_err();
        assert!(err.contains("only 1 cells for 2 frames"), "{err}");
    }

    /// Regression (round-5 cross-model review, PR #66): `new_sheet`
    /// is a public function, not just `main.rs`'s own already-bounded
    /// caller -- large enough `cols`/`cell_w`/`gap` must error, not
    /// overflow `u32` (a debug-build panic) or wrap around in release.
    #[test]
    fn extreme_dimensions_error_instead_of_overflowing() {
        assert!(new_sheet(u32::MAX, 2, u32::MAX, 2, u32::MAX).is_err());
        assert!(new_sheet(u32::MAX / 2, 1, 3, 1, 0).is_err());
    }

    #[test]
    fn empty_pages_errors() {
        let err = compose(&[], 1, 1, 0).unwrap_err();
        assert!(err.contains("no frames"), "{err}");
    }
}
