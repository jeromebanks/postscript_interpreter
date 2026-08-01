//! PDF export — Stage 9 task 2.
//!
//! **Design note** (the [opus+review] decision the roadmap asked
//! for): three candidates were weighed. *Re-execution* (run the
//! program once per output target) loses — programs have side
//! effects, `rand` streams, and file reads that must not happen
//! twice. A *retained display list* (record neutral paint ops, then
//! serialize to SVG/PDF/…) is the textbook answer but adds a third
//! representation of every op; with only two streaming consumers it
//! buys nothing yet. Chosen: the **paint-pipeline mirror** the SVG
//! exporter proved out — a recorder hooked at the same six seams in
//! `Gfx`, streaming device-space ops straight into PDF content
//! syntax. If a third export target ever appears, promote the shared
//! hook calls into a display list and make SVG/PDF serializers of it.
//!
//! Mapping notes:
//! - Every page's content stream starts with `1 0 0 -1 0 H cm`, so
//!   the interpreter's y-down device coordinates are used verbatim.
//! - Each element is wrapped in `q … Q` carrying its own clip chain
//!   (`W n` per link) — stateless, exactly like the SVG exporter's
//!   nested groups.
//! - Text arrives as glyph outlines (same as SVG); no fonts are
//!   embedded.
//! - Images become Flate-compressed RGB XObjects; imagemasks become
//!   1-bit `/ImageMask` stencils painted in the current fill color,
//!   which is precisely PostScript's own semantics.
//!
//! Verified end-to-end by rasterizing the output with gs and
//! block-comparing against our own canvas (tests/pdf.rs).

use std::fmt::Write as _;
use std::rc::Rc;

use tiny_skia::FillRule;

use crate::gfx::ClipNode;
use crate::svg::Chain;

pub struct PdfRecorder {
    width: u32,
    height: u32,
    /// Current page's content stream (PDF operators, text form).
    content: String,
    /// Finished pages' content streams.
    pages: Vec<String>,
    /// Image XObjects (raw stream bytes, already entity-complete),
    /// document-global; pages reference them by index.
    images: Vec<Vec<u8>>,
    /// Which images each finished page references.
    page_images: Vec<Vec<usize>>,
    current_images: Vec<usize>,
    /// Document `/Info` fields (issue #8) — from `%%Title:`/`%%For:`
    /// DSC header comments via `scan_document_info`, when present.
    title: Option<String>,
    author: Option<String>,
}

impl PdfRecorder {
    pub fn new(width: u32, height: u32) -> Self {
        PdfRecorder {
            width,
            height,
            content: String::new(),
            pages: Vec::new(),
            images: Vec::new(),
            page_images: Vec::new(),
            current_images: Vec::new(),
            title: None,
            author: None,
        }
    }

    /// Set document metadata for the `/Info` dictionary. `None` leaves
    /// that field out entirely (no empty `/Title ()` clutter).
    pub fn set_info(&mut self, title: Option<String>, author: Option<String>) {
        self.title = title;
        self.author = author;
    }

    fn clip_prelude(&mut self, chain: &Chain) -> String {
        let mut out = String::new();
        let mut node: Option<Rc<ClipNode>> = chain.clone();
        while let Some(n) = node {
            let _ = write!(
                out,
                "{} {} n ",
                path_ops(&n.path),
                match n.rule {
                    FillRule::EvenOdd => "W*",
                    _ => "W",
                }
            );
            node = n.parent.clone();
        }
        out
    }

    pub(crate) fn fill(
        &mut self,
        path: &crate::gfx::PsPath,
        rule: FillRule,
        rgb: (f32, f32, f32),
        chain: &Chain,
    ) {
        let clip = self.clip_prelude(chain);
        let _ = writeln!(
            self.content,
            "q {clip}{} {} {} rg {} {}
Q",
            f(rgb.0),
            f(rgb.1),
            f(rgb.2),
            path_ops(path),
            match rule {
                FillRule::EvenOdd => "f*",
                _ => "f",
            }
        );
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn stroke(
        &mut self,
        path: &crate::gfx::PsPath,
        rgb: (f32, f32, f32),
        width: f32,
        cap: tiny_skia::LineCap,
        join: tiny_skia::LineJoin,
        miter_limit: f32,
        dash: Option<(&[f32], f32)>,
        chain: &Chain,
    ) {
        let clip = self.clip_prelude(chain);
        let cap = match cap {
            tiny_skia::LineCap::Round => 1,
            tiny_skia::LineCap::Square => 2,
            _ => 0,
        };
        let join = match join {
            tiny_skia::LineJoin::Round => 1,
            tiny_skia::LineJoin::Bevel => 2,
            _ => 0,
        };
        let dash = match dash {
            Some((pattern, phase)) => {
                let list: Vec<String> = pattern.iter().map(|d| f(*d)).collect();
                format!("[{}] {} d ", list.join(" "), f(phase))
            }
            None => String::new(),
        };
        let _ = writeln!(
            self.content,
            "q {clip}{} {} {} RG {} w {cap} J {join} j {} M {dash}{} S
Q",
            f(rgb.0),
            f(rgb.1),
            f(rgb.2),
            f(width),
            f(miter_limit),
            path_ops(path),
        );
    }

    /// An image: `rgb_or_mask` is either interleaved RGB samples or,
    /// for masks, packed 1-bit rows; `t` maps sample coords to device
    /// space (the rasterizer's own transform).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn image(
        &mut self,
        w: usize,
        h: usize,
        data: ImageData,
        t: [f32; 6],
        fill_rgb: (f32, f32, f32),
        chain: &Chain,
    ) {
        use flate2::{Compression, write::ZlibEncoder};
        use std::io::Write as _;

        let (dict_extra, raw): (String, Vec<u8>) = match data {
            ImageData::Rgb(px) => ("/ColorSpace /DeviceRGB /BitsPerComponent 8".to_string(), px),
            ImageData::Mask { bits, paint_ones } => (
                format!(
                    "/ImageMask true /BitsPerComponent 1 /Decode [{} {}]",
                    if paint_ones { 1 } else { 0 },
                    if paint_ones { 0 } else { 1 }
                ),
                bits,
            ),
        };
        let mut enc = ZlibEncoder::new(Vec::new(), Compression::default());
        let _ = enc.write_all(&raw);
        let compressed = enc.finish().unwrap_or(raw);
        let mut obj = format!(
            "<< /Type /XObject /Subtype /Image /Width {w} /Height {h} {dict_extra} \
             /Filter /FlateDecode /Length {} >>\nstream\n",
            compressed.len()
        )
        .into_bytes();
        obj.extend_from_slice(&compressed);
        obj.extend_from_slice(b"\nendstream");
        let idx = self.images.len();
        self.images.push(obj);
        self.current_images.push(idx);

        // cm maps the unit square to device space: sample→device `t`
        // composed with unit→sample [W 0 0 -H 0 H] (PDF images put
        // row 0 along the top edge of the unit square).
        let unit = tiny_skia::Transform::from_row(w as f32, 0.0, 0.0, -(h as f32), 0.0, h as f32);
        let m = unit.post_concat(tiny_skia::Transform::from_row(
            t[0], t[1], t[2], t[3], t[4], t[5],
        ));
        let clip = self.clip_prelude(chain);
        let _ = writeln!(
            self.content,
            "q {clip}{} {} {} rg {} {} {} {} {} {} cm /Im{idx} Do
Q",
            f(fill_rgb.0),
            f(fill_rgb.1),
            f(fill_rgb.2),
            f(m.sx),
            f(m.ky),
            f(m.kx),
            f(m.sy),
            f(m.tx),
            f(m.ty),
        );
    }

    pub(crate) fn erase(&mut self) {
        self.content.clear();
        self.current_images.clear();
    }

    pub(crate) fn end_page(&mut self) {
        self.pages.push(self.content.clone());
        self.page_images.push(self.current_images.clone());
    }

    /// Serialize the whole document.
    pub fn finish(&self, trailing_art: bool) -> Vec<u8> {
        let mut pages = self.pages.clone();
        let mut page_images = self.page_images.clone();
        if pages.is_empty() || trailing_art {
            pages.push(self.content.clone());
            page_images.push(self.current_images.clone());
        }
        let npages = pages.len();

        // Object layout: 1 catalog, 2 pages tree, then per page
        // [page, contents], then all images, in that order.
        let mut objects: Vec<Vec<u8>> = Vec::new();
        let kids: Vec<String> = (0..npages).map(|i| format!("{} 0 R", 3 + i * 2)).collect();
        objects.push(b"<< /Type /Catalog /Pages 2 0 R >>".to_vec());
        objects.push(
            format!(
                "<< /Type /Pages /Kids [{}] /Count {npages} >>",
                kids.join(" ")
            )
            .into_bytes(),
        );
        let image_base = 3 + npages * 2;
        for (i, (content, imgs)) in pages.iter().zip(&page_images).enumerate() {
            let xobjects: Vec<String> = imgs
                .iter()
                .map(|&idx| format!("/Im{idx} {} 0 R", image_base + idx))
                .collect();
            let resources = if xobjects.is_empty() {
                String::new()
            } else {
                format!(" /Resources << /XObject << {} >> >>", xobjects.join(" "))
            };
            objects.push(
                format!(
                    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {} {}]{resources} \
                     /Contents {} 0 R >>",
                    self.width,
                    self.height,
                    3 + i * 2 + 1,
                )
                .into_bytes(),
            );
            let body = format!("1 0 0 -1 0 {} cm\n{}", self.height, content);
            let mut stream = format!("<< /Length {} >>\nstream\n", body.len()).into_bytes();
            stream.extend_from_slice(body.as_bytes());
            stream.extend_from_slice(b"\nendstream");
            objects.push(stream);
        }
        for img in &self.images {
            objects.push(img.clone());
        }

        // /Info is appended last, after every page/image object, so it
        // never shifts the kids/image_base positional math above —
        // those were computed against the object count *before* this
        // point. Only emitted when there's something to say; every
        // pscat PDF still gets /Producer.
        let mut info = String::from("<< /Producer (pscat)");
        if let Some(t) = &self.title {
            let _ = write!(info, " /Title {}", pdf_string(t));
        }
        if let Some(a) = &self.author {
            let _ = write!(info, " /Author {}", pdf_string(a));
        }
        info.push_str(" >>");
        let info_obj_num = objects.len() + 1;
        objects.push(info.into_bytes());

        let mut out = b"%PDF-1.4\n".to_vec();
        let mut offsets = Vec::with_capacity(objects.len());
        for (i, obj) in objects.iter().enumerate() {
            offsets.push(out.len());
            out.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
            out.extend_from_slice(obj);
            out.extend_from_slice(b"\nendobj\n");
        }
        let xref_at = out.len();
        out.extend_from_slice(format!("xref\n0 {}\n", objects.len() + 1).as_bytes());
        out.extend_from_slice(b"0000000000 65535 f \n");
        for off in offsets {
            out.extend_from_slice(format!("{off:010} 00000 n \n").as_bytes());
        }
        out.extend_from_slice(
            format!(
                "trailer\n<< /Size {} /Root 1 0 R /Info {info_obj_num} 0 R >>\nstartxref\n{xref_at}\n%%EOF\n",
                objects.len() + 1
            )
            .as_bytes(),
        );
        out
    }
}

pub(crate) enum ImageData {
    /// Interleaved 8-bit RGB, W*H*3 bytes.
    Rgb(Vec<u8>),
    /// Packed 1-bit rows (byte-aligned), stencil polarity included.
    Mask { bits: Vec<u8>, paint_ones: bool },
}

/// PDF path-construction operators for a device-space path.
fn path_ops(path: &crate::gfx::PsPath) -> String {
    path.to_pdf_ops()
}

fn f(v: f32) -> String {
    let s = format!("{v:.3}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// Encode a PDF text-string object. Printable-ASCII input becomes an
/// escaped literal string `(...)`; anything else (non-ASCII text —
/// plausible for a title given this project's Korean/Japanese font
/// work, or stray control bytes) becomes a UTF-16BE hex string with
/// the standard `FEFF` BOM prefix, the PDF spec's documented mechanism
/// for non-PDFDocEncoding text strings. Either form round-trips
/// through gs (tests/pdf.rs), so there's no silent mojibake either way.
fn pdf_string(s: &str) -> String {
    if s.chars().all(|c| c.is_ascii() && !c.is_ascii_control()) {
        let mut out = String::from("(");
        for c in s.chars() {
            if c == '(' || c == ')' || c == '\\' {
                out.push('\\');
            }
            out.push(c);
        }
        out.push(')');
        out
    } else {
        let mut out = String::from("<FEFF");
        for unit in s.encode_utf16() {
            let _ = write!(out, "{unit:04X}");
        }
        out.push('>');
        out
    }
}

/// Scan a PostScript source's DSC header comments for document
/// metadata: `%%Title:` and, for the author, `%%For:` (the DSC-correct
/// keyword — takes precedence if both appear) or `%%Author:` (a common
/// non-standard alias, used only when `%%For:` is absent). Stops at
/// the first non-comment, non-blank line, since DSC requires header
/// comments to precede any program content — this also keeps the scan
/// from matching a `%%Title:`-looking string buried later in a
/// comment, string literal, or data block.
///
/// Verified against gs, this project's semantics oracle: `gs
/// -sDEVICE=pdfwrite` on a file with `%%Title:`/`%%For:` header
/// comments embeds them as the output PDF's document metadata
/// (`dc:title`/`dc:creator` XMP entries) — so honoring the same two
/// comments here matches an established convention, not a new one.
pub fn scan_document_info(source: &[u8]) -> (Option<String>, Option<String>) {
    let mut title = None;
    let mut author = None;
    for raw_line in source.split(|&b| b == b'\n') {
        let line = String::from_utf8_lossy(raw_line);
        let line = line.trim_end_matches('\r').trim_start();
        if line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix("%%Title:") {
            title = Some(rest.trim().to_string());
        } else if let Some(rest) = line.strip_prefix("%%For:") {
            author = Some(rest.trim().to_string());
        } else if let Some(rest) = line.strip_prefix("%%Author:") {
            author.get_or_insert_with(|| rest.trim().to_string());
        } else if !line.starts_with('%') {
            break;
        }
    }
    (title, author)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scans_title_and_for() {
        let src = b"%!PS-Adobe-3.0\n%%Title: Moonlit Poem\n%%For: Jerome\n%%EndComments\n/x 1 def\n%%Title: not this one\n";
        let (title, author) = scan_document_info(src);
        assert_eq!(title.as_deref(), Some("Moonlit Poem"));
        assert_eq!(author.as_deref(), Some("Jerome"));
    }

    #[test]
    fn falls_back_to_author_when_for_absent() {
        let src = b"%!PS-Adobe-3.0\n%%Author: Jerome\n%%EndComments\n";
        let (_, author) = scan_document_info(src);
        assert_eq!(author.as_deref(), Some("Jerome"));
    }

    #[test]
    fn for_wins_over_author_regardless_of_order() {
        let src = b"%%Author: Wrong\n%%For: Right\n";
        let (_, author) = scan_document_info(src);
        assert_eq!(author.as_deref(), Some("Right"));

        let src2 = b"%%For: Right\n%%Author: Wrong\n";
        let (_, author2) = scan_document_info(src2);
        assert_eq!(author2.as_deref(), Some("Right"));
    }

    #[test]
    fn stops_at_first_non_comment_line() {
        let src = b"%%Title: Real Title\n/x 1 def\n%%Title: Ignored\n";
        let (title, _) = scan_document_info(src);
        assert_eq!(title.as_deref(), Some("Real Title"));
    }

    #[test]
    fn no_header_comments_yields_none() {
        let (title, author) = scan_document_info(b"/x 1 def\n");
        assert_eq!(title, None);
        assert_eq!(author, None);
    }

    #[test]
    fn pdf_string_escapes_ascii_specials() {
        assert_eq!(pdf_string("plain"), "(plain)");
        assert_eq!(pdf_string("A (Test) Title"), "(A \\(Test\\) Title)");
        assert_eq!(pdf_string("back\\slash"), "(back\\\\slash)");
    }

    #[test]
    fn pdf_string_uses_utf16be_hex_for_non_ascii() {
        // U+B2EC (달, "moon") -> UTF-16BE 0xB2EC, BOM-prefixed hex string.
        let s = pdf_string("달");
        assert_eq!(s, "<FEFFB2EC>");
    }
}
