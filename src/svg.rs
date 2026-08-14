//! SVG export — Stage 9 task 4.
//!
//! A recorder that mirrors the paint pipeline: paths are already in
//! device space when they're painted, so every `fill`/`stroke` maps
//! to one SVG element with the same coordinates the raster got
//! (glyphs included — they arrive as outlines). Clips become
//! `clipPath` defs; the *intersection* of nested clips is expressed
//! by wrapping the element in one `<g clip-path=…>` per link of the
//! clip chain. Images are embedded as base64 PNG `<image>` elements
//! carrying the exact device transform the rasterizer used.
//!
//! One SVG document per page, mirroring the multi-page model: the
//! recorder ends a page when showpage/copypage snapshots one, and a
//! pending lazy erase clears the body exactly when it clears the
//! canvas.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::rc::Rc;

use tiny_skia::FillRule;

use crate::gfx::ClipNode;

pub struct SvgRecorder {
    width: u32,
    height: u32,
    body: String,
    defs: String,
    /// ClipNode identity → def id, per page (defs reset with the page).
    clip_ids: HashMap<usize, usize>,
    next_clip: usize,
    /// Next `<linearGradient>`/`<radialGradient>` id (issue #20's `sh`).
    next_grad: usize,
    pages: Vec<String>,
}

pub(crate) type Chain = Option<Rc<ClipNode>>;

/// `sh`'s geometry, mirroring `crate::shading::ShadingKind` without
/// pulling that module's PsFunction/parsing machinery in here.
pub(crate) enum ShKind {
    Axial {
        x0: f32,
        y0: f32,
        x1: f32,
        y1: f32,
    },
    Radial {
        x0: f32,
        y0: f32,
        r0: f32,
        x1: f32,
        y1: f32,
        r1: f32,
    },
}

impl SvgRecorder {
    pub fn new(width: u32, height: u32) -> Self {
        SvgRecorder {
            width,
            height,
            body: String::new(),
            defs: String::new(),
            clip_ids: HashMap::new(),
            next_clip: 0,
            next_grad: 0,
            pages: Vec::new(),
        }
    }

    fn rule_attr(rule: FillRule) -> &'static str {
        match rule {
            FillRule::EvenOdd => "evenodd",
            _ => "nonzero",
        }
    }

    /// Register the whole clip chain, return the group-open prefix and
    /// the matching close suffix for one painted element.
    fn clip_wrappers(&mut self, chain: &Chain) -> (String, String) {
        let mut ids = Vec::new();
        let mut node = chain.clone();
        while let Some(n) = node {
            let key = Rc::as_ptr(&n) as usize;
            let id = match self.clip_ids.get(&key) {
                Some(&id) => id,
                None => {
                    let id = self.next_clip;
                    self.next_clip += 1;
                    self.clip_ids.insert(key, id);
                    let _ = write!(
                        self.defs,
                        "<clipPath id=\"c{id}\" clipPathUnits=\"userSpaceOnUse\">\
                         <path d=\"{}\" clip-rule=\"{}\"/></clipPath>",
                        n.path.to_svg_data(),
                        Self::rule_attr(n.rule),
                    );
                    id
                }
            };
            ids.push(id);
            node = n.parent.clone();
        }
        let mut open = String::new();
        for id in &ids {
            let _ = write!(open, "<g clip-path=\"url(#c{id})\">");
        }
        (open, "</g>".repeat(ids.len()))
    }

    fn color(rgb: (f32, f32, f32)) -> String {
        let c = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        format!("#{:02x}{:02x}{:02x}", c(rgb.0), c(rgb.1), c(rgb.2))
    }

    pub(crate) fn fill(&mut self, d: &str, rule: FillRule, rgb: (f32, f32, f32), chain: &Chain) {
        let (open, close) = self.clip_wrappers(chain);
        let _ = write!(
            self.body,
            "{open}<path d=\"{d}\" fill=\"{}\" fill-rule=\"{}\"/>{close}",
            Self::color(rgb),
            Self::rule_attr(rule),
        );
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn stroke(
        &mut self,
        d: &str,
        rgb: (f32, f32, f32),
        width: f32,
        cap: tiny_skia::LineCap,
        join: tiny_skia::LineJoin,
        miter_limit: f32,
        dash: Option<(&[f32], f32)>,
        chain: &Chain,
    ) {
        let (open, close) = self.clip_wrappers(chain);
        let cap = match cap {
            tiny_skia::LineCap::Round => "round",
            tiny_skia::LineCap::Square => "square",
            _ => "butt",
        };
        let join = match join {
            tiny_skia::LineJoin::Round => "round",
            tiny_skia::LineJoin::Bevel => "bevel",
            _ => "miter",
        };
        let mut extra = String::new();
        if let Some((pattern, phase)) = dash {
            let list: Vec<String> = pattern.iter().map(|d| fmt_f32(*d)).collect();
            let _ = write!(
                extra,
                " stroke-dasharray=\"{}\" stroke-dashoffset=\"{}\"",
                list.join(" "),
                fmt_f32(phase)
            );
        }
        let _ = write!(
            self.body,
            "{open}<path d=\"{d}\" fill=\"none\" stroke=\"{}\" stroke-width=\"{}\" \
             stroke-linecap=\"{cap}\" stroke-linejoin=\"{join}\" stroke-miterlimit=\"{}\"{extra}/>{close}",
            Self::color(rgb),
            fmt_f32(width),
            fmt_f32(miter_limit),
        );
    }

    /// An image (or imagemask) as embedded PNG: `png` is the encoded
    /// W×H source raster, `t` the device transform mapping source
    /// pixel coordinates to device space.
    pub(crate) fn image(&mut self, png: &[u8], w: usize, h: usize, t: [f32; 6], chain: &Chain) {
        let (open, close) = self.clip_wrappers(chain);
        let _ = write!(
            self.body,
            "{open}<image width=\"{w}\" height=\"{h}\" preserveAspectRatio=\"none\" \
             image-rendering=\"pixelated\" \
             transform=\"matrix({} {} {} {} {} {})\" \
             xlink:href=\"data:image/png;base64,{}\"/>{close}",
            fmt_f32(t[0]),
            fmt_f32(t[1]),
            fmt_f32(t[2]),
            fmt_f32(t[3]),
            fmt_f32(t[4]),
            fmt_f32(t[5]),
            base64(png),
        );
    }

    /// `sh` (issue #20): a native `<linearGradient>`/`<radialGradient>`
    /// def, painted as a full-page rect (clipped like any other fill).
    /// `gradientUnits="userSpaceOnUse"` plus `gradientTransform` set to
    /// the CTM mirrors the same trick `Gfx::sh` uses for the raster
    /// path — Coords stay in user space, the CTM does the mapping.
    /// SVG's matrix(a b c d e f) is the same convention as the CTM's
    /// own [a b c d tx ty] (`ops/matrix.rs`), so `ctm` here is passed
    /// through as-is: `[sx, ky, kx, sy, tx, ty]`.
    pub(crate) fn sh(
        &mut self,
        kind: &ShKind,
        ctm: [f32; 6],
        stops: &[(f32, f32, f32, f32)],
        chain: &Chain,
    ) {
        let id = self.next_grad;
        self.next_grad += 1;
        let mut stop_tags = String::new();
        for &(pos, r, g, b) in stops {
            let _ = write!(
                stop_tags,
                "<stop offset=\"{}\" stop-color=\"{}\"/>",
                fmt_f32(pos),
                Self::color((r, g, b)),
            );
        }
        let matrix = format!(
            "matrix({} {} {} {} {} {})",
            fmt_f32(ctm[0]),
            fmt_f32(ctm[1]),
            fmt_f32(ctm[2]),
            fmt_f32(ctm[3]),
            fmt_f32(ctm[4]),
            fmt_f32(ctm[5]),
        );
        match *kind {
            ShKind::Axial { x0, y0, x1, y1 } => {
                let _ = write!(
                    self.defs,
                    "<linearGradient id=\"g{id}\" gradientUnits=\"userSpaceOnUse\" \
                     x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" spreadMethod=\"pad\" \
                     gradientTransform=\"{matrix}\">{stop_tags}</linearGradient>",
                    fmt_f32(x0),
                    fmt_f32(y0),
                    fmt_f32(x1),
                    fmt_f32(y1),
                );
            }
            ShKind::Radial {
                x0,
                y0,
                r0,
                x1,
                y1,
                r1,
            } => {
                // SVG's radialGradient is (cx,cy,r) for the outer/end
                // circle plus an optional (fx,fy,fr) focal circle for
                // the start — exactly ShadingType 3's two-circle
                // model. `fr` only emits when r0 > 0: the common case
                // (a burst from a point, r0 = 0) is then plain SVG 1.1
                // markup that renders identically everywhere; only the
                // rarer two-circle case depends on SVG2's `fr`, which
                // some older renderers ignore (falling back to fr=0 —
                // an approximate, not broken, render).
                let mut extra = String::new();
                if r0 > 0.0 {
                    let _ = write!(
                        extra,
                        " fx=\"{}\" fy=\"{}\" fr=\"{}\"",
                        fmt_f32(x0),
                        fmt_f32(y0),
                        fmt_f32(r0),
                    );
                }
                let _ = write!(
                    self.defs,
                    "<radialGradient id=\"g{id}\" gradientUnits=\"userSpaceOnUse\" \
                     cx=\"{}\" cy=\"{}\" r=\"{}\"{extra} spreadMethod=\"pad\" \
                     gradientTransform=\"{matrix}\">{stop_tags}</radialGradient>",
                    fmt_f32(x1),
                    fmt_f32(y1),
                    fmt_f32(r1),
                );
            }
        }
        let (open, close) = self.clip_wrappers(chain);
        let (w, h) = (self.width, self.height);
        let _ = write!(
            self.body,
            "{open}<rect x=\"0\" y=\"0\" width=\"{w}\" height=\"{h}\" fill=\"url(#g{id})\"/>{close}",
        );
    }

    /// The lazy showpage erase (or an explicit erasepage): the new
    /// page starts blank.
    pub(crate) fn erase(&mut self) {
        self.body.clear();
        self.defs.clear();
        self.clip_ids.clear();
    }

    fn document(&self) -> String {
        format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" \
             xmlns:xlink=\"http://www.w3.org/1999/xlink\" \
             width=\"{w}\" height=\"{h}\" viewBox=\"0 0 {w} {h}\">\
             <rect width=\"{w}\" height=\"{h}\" fill=\"#ffffff\"/>\
             {defs}{body}</svg>\n",
            w = self.width,
            h = self.height,
            defs = if self.defs.is_empty() {
                String::new()
            } else {
                format!("<defs>{}</defs>", self.defs)
            },
            body = self.body,
        )
    }

    /// showpage/copypage: snapshot the page document. (The body is
    /// *not* cleared here — the erase is lazy, same as the canvas.)
    pub(crate) fn end_page(&mut self) {
        self.pages.push(self.document());
    }

    /// All finished pages, plus the live body if it holds unshown art.
    pub fn finish(&self, trailing_art: bool) -> Vec<String> {
        let mut pages = self.pages.clone();
        if pages.is_empty() || trailing_art {
            pages.push(self.document());
        }
        pages
    }
}

fn fmt_f32(v: f32) -> String {
    // Enough precision for device coordinates, no trailing noise.
    let s = format!("{v:.3}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// Standard base64, no external dependency needed for one use.
fn base64(data: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = u32::from(b[0]) << 16 | u32::from(b[1]) << 8 | u32::from(b[2]);
        out.push(ALPHABET[(n >> 18) as usize & 63] as char);
        out.push(ALPHABET[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            ALPHABET[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_known_vectors() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }
}
