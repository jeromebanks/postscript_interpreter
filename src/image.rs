//! The sampled-image engine behind `image`, `imagemask`, and
//! `colorimage` (Stage 7 task 4).
//!
//! Like the show family, an image is an execution-stack frame: data
//! arrives incrementally (a file read, a string, or — the Level 1
//! classic — a procedure called repeatedly for more scanlines, which
//! the machine runs as an ordinary frame above this one). Once the
//! buffer is complete, rasterization is one pass: every device pixel
//! inside the image's unit-square footprint is inverse-mapped through
//! CTM⁻¹ and the ImageMatrix to a source sample (nearest neighbor —
//! found files at found sizes don't miss the interpolation).

use crate::error::PsError;
use crate::file::FileHandle;
use crate::gfx::{Gfx, IndexedTable};
use crate::object::Object;

/// How decoded samples become colors. `Direct` is the ncomp-driven
/// gray/RGB/CMYK interpretation; the other two come from the current
/// color space when the dict form starts.
pub(crate) enum ImageColor {
    Direct,
    /// Samples are indices into an Indexed lookup table.
    Indexed(std::rc::Rc<IndexedTable>),
    /// Samples are Separation tints, rendered as `1 - tint` gray —
    /// a documented approximation (running the tint transform per
    /// pixel through the machine is not feasible; most separations
    /// are dark inks, for which this is close).
    SeparationApprox,
}

pub(crate) enum ImageSource {
    /// A procedure returning strings; empty string = end of data.
    Proc(Object),
    File(FileHandle),
    /// A string source (or already-complete data).
    Bytes(Vec<u8>),
}

/// What the machine should do after a step.
pub(crate) enum ImageStep {
    /// Run the data-source procedure; feed me its result next step.
    Exec(Object),
    /// Rasterized (or abandoned at EOD); pop the frame.
    Done,
}

pub(crate) struct ImageCtx {
    pub width: usize,
    pub height: usize,
    pub bits: u32,
    pub ncomp: u32,
    /// Per-component [min, max] sample mapping, `2*ncomp` entries.
    pub decode: Vec<f32>,
    /// User-space unit square → image space (the ImageMatrix).
    pub matrix: [f64; 6],
    /// Some(paint_ones): imagemask stenciling the current color.
    pub mask: Option<bool>,
    pub color: ImageColor,
    pub source: ImageSource,
    pub data: Vec<u8>,
    waiting: bool,
    eod: bool,
}

impl ImageCtx {
    // One argument per image parameter; a builder would be ceremony.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        width: usize,
        height: usize,
        bits: u32,
        ncomp: u32,
        decode: Vec<f32>,
        matrix: [f64; 6],
        mask: Option<bool>,
        color: ImageColor,
        source: ImageSource,
    ) -> Self {
        ImageCtx {
            width,
            height,
            bits,
            ncomp,
            decode,
            matrix,
            mask,
            color,
            source,
            data: Vec::new(),
            waiting: false,
            eod: false,
        }
    }

    fn row_bytes(&self) -> usize {
        (self.width * self.ncomp as usize * self.bits as usize).div_ceil(8)
    }

    fn needed(&self) -> usize {
        self.row_bytes() * self.height
    }

    pub(crate) fn waiting(&self) -> bool {
        self.waiting
    }

    /// The data-source procedure's result, handed over by the machine.
    pub(crate) fn supply(&mut self, obj: Object) -> Result<(), PsError> {
        self.waiting = false;
        match &obj.value {
            crate::object::Value::String(s) => {
                if s.is_empty() {
                    self.eod = true;
                } else {
                    self.data.extend(s.to_vec());
                }
                Ok(())
            }
            _ => Err(PsError::Typecheck),
        }
    }

    pub(crate) fn step(&mut self, gfx: &mut Gfx) -> Result<ImageStep, PsError> {
        while self.data.len() < self.needed() && !self.eod {
            match &self.source {
                ImageSource::Bytes(_) => {
                    let ImageSource::Bytes(b) =
                        std::mem::replace(&mut self.source, ImageSource::Bytes(Vec::new()))
                    else {
                        unreachable!("matched above");
                    };
                    self.data.extend(b);
                    self.eod = true;
                }
                ImageSource::File(f) => {
                    let want = self.needed() - self.data.len();
                    let mut f = f.borrow_mut();
                    for _ in 0..want {
                        match f.read_byte()? {
                            Some(b) => self.data.push(b),
                            None => {
                                self.eod = true;
                                break;
                            }
                        }
                    }
                }
                ImageSource::Proc(p) => {
                    self.waiting = true;
                    return Ok(ImageStep::Exec(p.clone()));
                }
            }
        }
        // A filter source is drained to its own EOD marker — every
        // layer of the chain — so the underlying file (usually the
        // program) resumes right after the encoded data. gs-pinned
        // inline-image behavior.
        if let ImageSource::File(f) = &self.source {
            let mut layer = f.clone();
            while layer.borrow().is_filter() {
                while layer.borrow_mut().read_byte()?.is_some() {}
                let Some(next) = layer.borrow().filter_source() else {
                    break;
                };
                layer = next;
            }
        }
        // Premature EOD renders what arrived; missing samples read as 0.
        self.rasterize(gfx);
        Ok(ImageStep::Done)
    }

    /// One sample component, MSB-first within each byte, rows padded to
    /// byte boundaries.
    fn sample(&self, ix: usize, iy: usize, comp: usize) -> u32 {
        let bitpos =
            iy * self.row_bytes() * 8 + (ix * self.ncomp as usize + comp) * self.bits as usize;
        let mut v: u32 = 0;
        for i in 0..self.bits as usize {
            let bit = bitpos + i;
            let byte = self.data.get(bit / 8).copied().unwrap_or(0);
            v = (v << 1) | u32::from((byte >> (7 - bit % 8)) & 1);
        }
        v
    }

    fn decoded(&self, ix: usize, iy: usize, comp: usize) -> f32 {
        let maxval = ((1u32 << self.bits) - 1) as f32;
        let v = self.sample(ix, iy, comp) as f32 / maxval;
        let d0 = self.decode.get(comp * 2).copied().unwrap_or(0.0);
        let d1 = self.decode.get(comp * 2 + 1).copied().unwrap_or(1.0);
        (d0 + v * (d1 - d0)).clamp(0.0, 1.0)
    }

    fn rgb_at(&self, ix: usize, iy: usize) -> (f32, f32, f32) {
        match &self.color {
            ImageColor::Indexed(t) => {
                // Decode maps the sample straight to an index (default
                // [0, 2^bits-1]) — deliberately not the unit-clamped
                // `decoded()` path.
                let maxval = ((1u32 << self.bits) - 1) as f32;
                let v = self.sample(ix, iy, 0) as f32 / maxval;
                let d0 = self.decode.first().copied().unwrap_or(0.0);
                let d1 = self.decode.get(1).copied().unwrap_or(maxval);
                let idx = (d0 + v * (d1 - d0)).round().max(0.0) as u32;
                return t.rgb_at(idx.min(t.hival) as usize);
            }
            ImageColor::SeparationApprox => {
                let g = 1.0 - self.decoded(ix, iy, 0);
                return (g, g, g);
            }
            ImageColor::Direct => {}
        }
        match self.ncomp {
            1 => {
                let g = self.decoded(ix, iy, 0);
                (g, g, g)
            }
            3 => (
                self.decoded(ix, iy, 0),
                self.decoded(ix, iy, 1),
                self.decoded(ix, iy, 2),
            ),
            _ => {
                let c = self.decoded(ix, iy, 0);
                let m = self.decoded(ix, iy, 1);
                let y = self.decoded(ix, iy, 2);
                let k = self.decoded(ix, iy, 3);
                (
                    (1.0 - c) * (1.0 - k),
                    (1.0 - m) * (1.0 - k),
                    (1.0 - y) * (1.0 - k),
                )
            }
        }
    }

    fn rasterize(&self, gfx: &mut Gfx) {
        if self.width == 0 || self.height == 0 || self.bits == 0 {
            return;
        }
        let ctm = gfx.ctm();
        let Some(inv) = ctm.invert() else {
            return; // singular CTM: the image has no area
        };
        // Device bounding box of the user-space unit square.
        let (pw, ph) = (gfx.pixmap.width() as i64, gfx.pixmap.height() as i64);
        let mut x0 = f32::MAX;
        let mut y0 = f32::MAX;
        let mut x1 = f32::MIN;
        let mut y1 = f32::MIN;
        for (ux, uy) in [(0.0f32, 0.0f32), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)] {
            let dx = ctm.sx * ux + ctm.kx * uy + ctm.tx;
            let dy = ctm.ky * ux + ctm.sy * uy + ctm.ty;
            x0 = x0.min(dx);
            y0 = y0.min(dy);
            x1 = x1.max(dx);
            y1 = y1.max(dy);
        }
        let px0 = (x0.floor() as i64).max(0);
        let py0 = (y0.floor() as i64).max(0);
        let px1 = (x1.ceil() as i64).min(pw);
        let py1 = (y1.ceil() as i64).min(ph);

        let m = self.matrix;
        let paint_rgb = gfx.rgb();
        let mask_data = gfx.clip_mask_data();
        let width = gfx.pixmap.width() as usize;
        let data = gfx.pixmap.data_mut();
        let mut painted = false;
        for py in py0..py1 {
            for px in px0..px1 {
                // Device pixel center → user space → image space.
                let (cx, cy) = (px as f32 + 0.5, py as f32 + 0.5);
                let ux = inv.sx * cx + inv.kx * cy + inv.tx;
                let uy = inv.ky * cx + inv.sy * cy + inv.ty;
                if !(0.0..1.0).contains(&ux) || !(0.0..1.0).contains(&uy) {
                    continue;
                }
                let (ux, uy) = (f64::from(ux), f64::from(uy));
                let ix = (m[0] * ux + m[2] * uy + m[4]).floor();
                let iy = (m[1] * ux + m[3] * uy + m[5]).floor();
                if ix < 0.0 || iy < 0.0 {
                    continue;
                }
                let (ix, iy) = (ix as usize, iy as usize);
                if ix >= self.width || iy >= self.height {
                    continue;
                }
                let rgb = match self.mask {
                    Some(paint_ones) => {
                        let one = self.sample(ix, iy, 0) != 0;
                        if one != paint_ones {
                            continue;
                        }
                        (paint_rgb.0 as f32, paint_rgb.1 as f32, paint_rgb.2 as f32)
                    }
                    None => self.rgb_at(ix, iy),
                };
                let clip = mask_data
                    .as_ref()
                    .map_or(255, |m| m.data()[py as usize * width + px as usize]);
                if clip == 0 {
                    continue;
                }
                let a = f32::from(clip) / 255.0;
                let idx = (py as usize * width + px as usize) * 4;
                for (ch, v) in [rgb.0, rgb.1, rgb.2].into_iter().enumerate() {
                    let src = v * 255.0;
                    let dst = f32::from(data[idx + ch]);
                    data[idx + ch] = (src * a + dst * (1.0 - a)).round() as u8;
                }
                data[idx + 3] = 255;
                painted = true;
            }
        }
        if painted {
            gfx.dirty = true;
        }
    }
}
