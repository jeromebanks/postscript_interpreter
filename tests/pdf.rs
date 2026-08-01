//! PDF export — Stage 9 task 2. The strong test rasterizes our PDF
//! *with gs* and block-compares against our own canvas: the document
//! must mean what the raster meant. Skipped without gs, like golden.

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

fn pdf_and_canvas(src: &[u8]) -> (Vec<u8>, Pixmap) {
    let mut it = Interp::with_page(SIZE, SIZE).expect("page");
    it.gfx_mut().enable_pdf();
    it.run_source(src)
        .unwrap_or_else(|e| panic!("run failed: {}", it.error_report(&e)));
    (
        it.gfx().pdf_document().expect("recording on"),
        it.gfx().pixmap.clone(),
    )
}

fn gs_rasterize(pdf: &[u8], name: &str) -> Pixmap {
    let dir = std::env::temp_dir();
    let pdf_path = dir.join(format!("pscat_pdftest_{name}.pdf"));
    let png_path = dir.join(format!("pscat_pdftest_{name}.png"));
    std::fs::write(&pdf_path, pdf).expect("write pdf");
    let status = Command::new("gs")
        .args([
            "-q",
            "-dNOPAUSE",
            "-dBATCH",
            "-sDEVICE=png16m",
            "-dGraphicsAlphaBits=4",
            "-dTextAlphaBits=4",
            &format!("-g{SIZE}x{SIZE}"),
            "-r72",
        ])
        .arg(format!("-o{}", png_path.display()))
        .arg(&pdf_path)
        .status()
        .expect("run gs");
    assert!(status.success(), "gs rejected our PDF");
    let data = std::fs::read(&png_path).expect("read gs png");
    let _ = std::fs::remove_file(&pdf_path);
    let _ = std::fs::remove_file(&png_path);
    Pixmap::decode_png(&data).expect("decode png")
}

fn mean_block_diff(a: &Pixmap, b: &Pixmap) -> f64 {
    let down = |pm: &Pixmap| {
        let (w, h) = (pm.width(), pm.height());
        let (bw, bh) = (w / BLOCK, h / BLOCK);
        let mut cells = vec![[0f64; 3]; (bw * bh) as usize];
        let data = pm.data();
        for y in 0..h {
            for x in 0..w {
                let i = ((y * w + x) * 4) as usize;
                let cell = &mut cells[((y / BLOCK) * bw + x / BLOCK) as usize];
                for ch in 0..3 {
                    cell[ch] += f64::from(data[i + ch]);
                }
            }
        }
        let n = f64::from(BLOCK * BLOCK);
        for c in &mut cells {
            for ch in c.iter_mut() {
                *ch /= n;
            }
        }
        cells
    };
    let (da, db) = (down(a), down(b));
    let total: f64 = da
        .iter()
        .zip(&db)
        .map(|(x, y)| (0..3).map(|c| (x[c] - y[c]).abs()).sum::<f64>())
        .sum();
    total / (da.len() * 3) as f64
}

#[test]
fn postcard_pdf_means_what_the_raster_meant() {
    if !gs_available() {
        eprintln!("skipping pdf test: gs not installed");
        return;
    }
    let src = std::fs::read("examples/postcard.ps").expect("example");
    let (pdf, canvas) = pdf_and_canvas(&src);
    let theirs = gs_rasterize(&pdf, "postcard");
    let mean = mean_block_diff(&canvas, &theirs);
    assert!(mean < 6.0, "gs render of our PDF diverges: mean {mean:.2}");
}

#[test]
fn specimen_text_survives_as_outlines() {
    if !gs_available() {
        eprintln!("skipping pdf test: gs not installed");
        return;
    }
    let src = std::fs::read("examples/specimen.ps").expect("example");
    let (pdf, canvas) = pdf_and_canvas(&src);
    let theirs = gs_rasterize(&pdf, "specimen");
    let mean = mean_block_diff(&canvas, &theirs);
    assert!(mean < 6.0, "outline text diverges: mean {mean:.2}");
}

#[test]
fn document_structure_is_sane() {
    let (pdf, _) = pdf_and_canvas(
        b"newpath 10 10 moveto 90 10 lineto 50 90 lineto closepath fill showpage
          newpath 20 20 moveto 80 20 lineto 80 80 lineto 20 80 lineto closepath fill showpage",
    );
    let text = String::from_utf8_lossy(&pdf);
    assert!(text.starts_with("%PDF-1.4"));
    assert!(text.contains("/Count 2"), "two pages");
    assert!(text.contains("startxref"));
    assert!(text.ends_with("%%EOF\n"));
}

#[test]
fn images_become_xobjects() {
    let (pdf, _) = pdf_and_canvas(
        b"10 10 translate 80 80 scale
          2 2 8 [2 0 0 -2 0 2] {<00ff ff00>} image",
    );
    let text = String::from_utf8_lossy(&pdf);
    assert!(text.contains("/Subtype /Image"), "XObject present");
    assert!(text.contains("/Filter /FlateDecode"));
    assert!(text.contains("/Im0 Do"));
}

/// Regression guard: `stroke`'s PDF recording used to live inside
/// `if self.svg.is_some()`, so a `--pdf`-only export (the common case
/// -- no `--svg`) silently dropped every stroked line. `fill` and
/// `fill_path_direct` never had this bug; `pdf_and_canvas` here
/// deliberately enables *only* PDF recording, matching `--pdf` alone,
/// to catch a regression back to the svg-gated version.
#[test]
fn strokes_appear_in_pdf_only_export() {
    let (pdf, _) = pdf_and_canvas(
        b"0.7 0.7 0.75 setrgbcolor 2 setlinewidth
          newpath 10 50 moveto 90 50 lineto stroke",
    );
    let text = String::from_utf8_lossy(&pdf);
    assert!(text.contains(" S\n") || text.contains(" S\r"), "{text}");
    assert!(
        text.contains("0.7 0.7 0.75 RG"),
        "stroke color recorded: {text}"
    );
}

#[test]
fn imagemasks_are_stencils() {
    let (pdf, _) = pdf_and_canvas(
        b"1 0 0 setrgbcolor 10 40 translate 80 20 scale
          2 1 true [2 0 0 -1 0 1] {<80>} imagemask",
    );
    let text = String::from_utf8_lossy(&pdf);
    assert!(text.contains("/ImageMask true"), "{text}");
    assert!(text.contains("1 0 0 rg"), "painted in current color");
}

/// Issue #8: `%%Title:`/`%%For:` DSC header comments become the PDF's
/// `/Info` metadata, same convention `main.rs` wires via
/// `scan_document_info`. Exercised end to end here (not just the
/// pdf.rs unit tests) so a regression in the Gfx/PdfRecorder wiring
/// itself, not just the scanner, would fail a test.
#[test]
fn dsc_header_comments_become_pdf_info() {
    let src = b"%%Title: Moonlit Poem\n%%For: Jerome\n%%EndComments\n\
                newpath 10 10 moveto 90 10 lineto stroke showpage";
    let mut it = Interp::with_page(SIZE, SIZE).expect("page");
    it.gfx_mut().enable_pdf();
    let (title, author) = pscat::pdf::scan_document_info(src);
    it.gfx_mut().set_pdf_info(title, author);
    it.run_source(src)
        .unwrap_or_else(|e| panic!("run failed: {}", it.error_report(&e)));
    let pdf = it.gfx().pdf_document().expect("recording on");
    let text = String::from_utf8_lossy(&pdf);
    assert!(text.contains("/Producer (pscat)"));
    assert!(text.contains("/Title (Moonlit Poem)"));
    assert!(text.contains("/Author (Jerome)"));
    assert!(text.contains("/Info "), "trailer references /Info");

    if gs_available() {
        // A broken object/xref layout (e.g. /Info shifting the page or
        // image object numbers) would make gs reject the file outright
        // -- this is a validity smoke test, not a pixel comparison.
        let _ = gs_rasterize(&pdf, "dsc_info");
    } else {
        eprintln!("skipping gs validity check: gs not installed");
    }
}

#[test]
fn no_dsc_comments_still_gets_producer_but_no_title() {
    let (pdf, _) = pdf_and_canvas(b"newpath 10 10 moveto 90 10 lineto stroke showpage");
    let text = String::from_utf8_lossy(&pdf);
    assert!(text.contains("/Producer (pscat)"));
    assert!(!text.contains("/Title"));
    assert!(!text.contains("/Author"));
}
