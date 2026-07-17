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
        }
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
                "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref_at}\n%%EOF\n",
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
