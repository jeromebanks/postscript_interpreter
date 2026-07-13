//! Font machinery: the bundled-font registry, the current-font graphics
//! state, and the glyph → path engine behind `show`/`stringwidth`/
//! `charpath`. The architecture (and its deliberate deviations) is
//! documented in `FONTS.md` at the repo root.
//!
//! The seam between PostScript font *dicts* (which programs own and
//! mutate) and Rust-side glyph data is the `FID` entry: an index into
//! [`BUILTINS`]. Faces are re-parsed per show call rather than cached —
//! `ttf_parser::Face` borrows its data, and parsing is table-directory
//! validation, not glyph work; the Stage 6 glyph-cache task measures
//! whether even that matters.

use std::cell::RefCell;
use std::rc::Rc;

use ttf_parser::{Face, GlyphId};

use crate::encodings;
use crate::error::PsError;
use crate::gfx::{DevPoint, Gfx, PsPath};
use crate::object::{Dict, Object, PsString, Value};

/// `FID` value for fonts with no outline source we can drive yet
/// (Type 3 dicts accepted by `definefont`); `show` reports invalidfont.
pub(crate) const FID_NONE: i64 = -1;

/// Registry index used when `findfont` substitutes for an unknown name.
pub(crate) const SUBSTITUTE: i64 = 0;

struct Builtin {
    ps_name: &'static str,
    data: &'static [u8],
}

/// The standard base-13 names minus Symbol (no metric-compatible open
/// TTF exists; see FONTS.md), backed by Liberation faces, which are
/// metric-compatible with the Adobe originals.
static BUILTINS: [Builtin; 12] = [
    Builtin {
        ps_name: "Helvetica",
        data: include_bytes!("../fonts/LiberationSans-Regular.ttf"),
    },
    Builtin {
        ps_name: "Helvetica-Bold",
        data: include_bytes!("../fonts/LiberationSans-Bold.ttf"),
    },
    Builtin {
        ps_name: "Helvetica-Oblique",
        data: include_bytes!("../fonts/LiberationSans-Italic.ttf"),
    },
    Builtin {
        ps_name: "Helvetica-BoldOblique",
        data: include_bytes!("../fonts/LiberationSans-BoldItalic.ttf"),
    },
    Builtin {
        ps_name: "Times-Roman",
        data: include_bytes!("../fonts/LiberationSerif-Regular.ttf"),
    },
    Builtin {
        ps_name: "Times-Bold",
        data: include_bytes!("../fonts/LiberationSerif-Bold.ttf"),
    },
    Builtin {
        ps_name: "Times-Italic",
        data: include_bytes!("../fonts/LiberationSerif-Italic.ttf"),
    },
    Builtin {
        ps_name: "Times-BoldItalic",
        data: include_bytes!("../fonts/LiberationSerif-BoldItalic.ttf"),
    },
    Builtin {
        ps_name: "Courier",
        data: include_bytes!("../fonts/LiberationMono-Regular.ttf"),
    },
    Builtin {
        ps_name: "Courier-Bold",
        data: include_bytes!("../fonts/LiberationMono-Bold.ttf"),
    },
    Builtin {
        ps_name: "Courier-Oblique",
        data: include_bytes!("../fonts/LiberationMono-Italic.ttf"),
    },
    Builtin {
        ps_name: "Courier-BoldOblique",
        data: include_bytes!("../fonts/LiberationMono-BoldItalic.ttf"),
    },
];

pub(crate) fn builtin_index(name: &str) -> Option<i64> {
    BUILTINS
        .iter()
        .position(|b| b.ps_name == name)
        .map(|i| i as i64)
}

fn face_for(fid: i64) -> Result<Face<'static>, PsError> {
    let idx = usize::try_from(fid).map_err(|_| PsError::InvalidFont)?;
    let b = BUILTINS.get(idx).ok_or(PsError::InvalidFont)?;
    Face::parse(b.data, 0).map_err(|_| PsError::InvalidFont)
}

/// The current font, snapshotted by `setfont` into the graphics state
/// (so `gsave`/`grestore` handle it like any other attribute). The
/// FontMatrix is cached here per the PLRM's set-time semantics; the
/// Encoding is deliberately *not* — `show` reads it live from the dict
/// so the re-encoding idiom works even after `setfont`.
#[derive(Clone)]
pub struct FontState {
    pub dict: Rc<RefCell<Dict>>,
    pub fid: i64,
    /// Composed FontMatrix `[a b c d tx ty]`: glyph space (1000/em) →
    /// user space, scale included.
    pub matrix: [f64; 6],
}

/// Build the font dict for a built-in face. Each dict gets its own
/// fresh Encoding array so re-encoding one font can't affect another.
pub(crate) fn build_builtin_dict(fid: i64) -> Result<Rc<RefCell<Dict>>, PsError> {
    let face = face_for(fid)?;
    let idx = fid as usize; // face_for validated the range
    let k = 1000.0 / f64::from(face.units_per_em());
    let bbox = face.global_bounding_box();

    let mut d = Dict::new();
    d.put("FontName".into(), Object::name(BUILTINS[idx].ps_name));
    // Honest about the outline source: Type 42 is PostScript's name for
    // TrueType-backed fonts.
    d.put("FontType".into(), Object::int(42));
    d.put(
        "FontMatrix".into(),
        Object::array(
            [0.001, 0.0, 0.0, 0.001, 0.0, 0.0]
                .into_iter()
                .map(Object::real)
                .collect(),
        ),
    );
    d.put(
        "FontBBox".into(),
        Object::array(
            [
                f64::from(bbox.x_min) * k,
                f64::from(bbox.y_min) * k,
                f64::from(bbox.x_max) * k,
                f64::from(bbox.y_max) * k,
            ]
            .into_iter()
            .map(Object::real)
            .collect(),
        ),
    );
    d.put(
        "Encoding".into(),
        Object::array(
            encodings::standard_encoding()
                .into_iter()
                .map(Object::name)
                .collect(),
        ),
    );
    d.put("FID".into(), Object::int(fid));
    Ok(Rc::new(RefCell::new(d)))
}

/// Row-vector affine composition: apply `first`, then `second`
/// (PS matrix layout `[a b c d tx ty]`, x' = ax + cy + tx).
pub(crate) fn compose(first: [f64; 6], second: [f64; 6]) -> [f64; 6] {
    let [a1, b1, c1, d1, tx1, ty1] = first;
    let [a2, b2, c2, d2, tx2, ty2] = second;
    [
        a1 * a2 + b1 * c2,
        a1 * b2 + b1 * d2,
        c1 * a2 + d1 * c2,
        c1 * b2 + d1 * d2,
        tx1 * a2 + ty1 * c2 + tx2,
        tx1 * b2 + ty1 * d2 + ty2,
    ]
}

/// Per-glyph text-placement extras: `ashow` adds a vector to every
/// glyph's advance; `widthshow` adds one to a designated byte's.
#[derive(Default)]
pub(crate) struct ShowParams {
    pub extra: (f64, f64),
    pub char_extra: Option<(u8, (f64, f64))>,
}

pub(crate) enum ShowMode {
    /// Paint glyphs; current point advances (`show` family).
    Paint,
    /// Append outlines to the current path (`charpath`).
    Charpath,
    /// Metrics only — no current point required (`stringwidth`).
    Width,
}

/// Full device-space affine for one glyph placement.
#[derive(Clone, Copy)]
struct GlyphTransform {
    a: f64,
    b: f64,
    c: f64,
    d: f64,
    tx: f64,
    ty: f64,
}

impl GlyphTransform {
    fn apply(&self, x: f64, y: f64) -> DevPoint {
        DevPoint {
            x: (self.a * x + self.c * y + self.tx) as f32,
            y: (self.b * x + self.d * y + self.ty) as f32,
        }
    }
}

/// Feeds `ttf-parser` outlines into a device-space `PsPath`, elevating
/// quadratic segments to the cubics our path model speaks.
struct PathSink {
    path: PsPath,
    m: GlyphTransform,
    cur: (f64, f64),
}

impl ttf_parser::OutlineBuilder for PathSink {
    fn move_to(&mut self, x: f32, y: f32) {
        self.cur = (f64::from(x), f64::from(y));
        let p = self.m.apply(self.cur.0, self.cur.1);
        self.path.move_to(p);
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.cur = (f64::from(x), f64::from(y));
        let p = self.m.apply(self.cur.0, self.cur.1);
        self.path.line_to(p);
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let (qx, qy) = (f64::from(x1), f64::from(y1));
        let (px, py) = (f64::from(x), f64::from(y));
        let (cx, cy) = self.cur;
        // Exact degree elevation: cubic controls at ⅓ and ⅔ along the
        // quad's control polygon.
        let c1 = (cx + 2.0 / 3.0 * (qx - cx), cy + 2.0 / 3.0 * (qy - cy));
        let c2 = (px + 2.0 / 3.0 * (qx - px), py + 2.0 / 3.0 * (qy - py));
        self.path.curve_to(
            self.m.apply(c1.0, c1.1),
            self.m.apply(c2.0, c2.1),
            self.m.apply(px, py),
        );
        self.cur = (px, py);
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.path.curve_to(
            self.m.apply(f64::from(x1), f64::from(y1)),
            self.m.apply(f64::from(x2), f64::from(y2)),
            self.m.apply(f64::from(x), f64::from(y)),
        );
        self.cur = (f64::from(x), f64::from(y));
    }

    fn close(&mut self) {
        self.path.close();
    }
}

/// Glyph name → glyph id: the face's own name table first (Liberation
/// carries full `post` names), then the Adobe-glyph-name → Unicode →
/// `cmap` fallback.
fn resolve_glyph(face: &Face, name: &str) -> Option<GlyphId> {
    if name == ".notdef" {
        return None;
    }
    face.glyph_index_by_name(name)
        .or_else(|| encodings::name_to_char(name).and_then(|c| face.glyph_index(c)))
}

/// CTM linear part in f64 (tiny-skia rows: sx ky kx sy tx ty), applied
/// to a user-space distance.
fn ctm_lin(gfx: &Gfx, (x, y): (f64, f64)) -> (f64, f64) {
    let m = gfx.ctm();
    (
        f64::from(m.sx) * x + f64::from(m.kx) * y,
        f64::from(m.ky) * x + f64::from(m.sy) * y,
    )
}

/// The Encoding lookup, read live from the font dict per the FONTS.md
/// set-time-semantics decision; a font dict without an Encoding
/// (hand-built) falls back to StandardEncoding. Out-of-range or
/// non-name entries behave as .notdef.
fn encoding_name(fs: &FontState, byte: u8) -> Rc<str> {
    let enc = fs
        .dict
        .borrow()
        .get("Encoding")
        .and_then(|o| match &o.value {
            Value::Array(a) => Some(a.clone()),
            _ => None,
        });
    match &enc {
        Some(a) => match a.get(usize::from(byte)).as_ref().map(|o| &o.value) {
            Some(Value::Name(n)) => n.clone(),
            _ => ".notdef".into(),
        },
        None => encodings::standard_encoding()[usize::from(byte)].into(),
    }
}

/// One outline-font glyph: paint/append/measure at `pen`, returning the
/// raw user-space advance (before per-show extras).
fn outline_glyph(
    gfx: &mut Gfx,
    fs: &FontState,
    byte: u8,
    mode: &ShowMode,
    pen: DevPoint,
) -> Result<(f64, f64), PsError> {
    let face = face_for(fs.fid)?;
    let k = 1000.0 / f64::from(face.units_per_em());
    let fm = fs.matrix;
    // Glyph-space → user-space linear part (FontMatrix over 1000-unit
    // glyph space, with the em normalization folded in).
    let g = [fm[0] * k, fm[1] * k, fm[2] * k, fm[3] * k];

    let name = encoding_name(fs, byte);
    let gid = resolve_glyph(&face, &name);

    if let Some(gid) = gid
        && !matches!(mode, ShowMode::Width)
    {
        let ctm = gfx.ctm();
        let (c00, c01) = (f64::from(ctm.sx), f64::from(ctm.kx));
        let (c10, c11) = (f64::from(ctm.ky), f64::from(ctm.sy));
        // The FontMatrix translation is a per-glyph user-space offset.
        let fm_off = ctm_lin(gfx, (fm[4], fm[5]));
        let m = GlyphTransform {
            a: c00 * g[0] + c01 * g[1],
            b: c10 * g[0] + c11 * g[1],
            c: c00 * g[2] + c01 * g[3],
            d: c10 * g[2] + c11 * g[3],
            tx: f64::from(pen.x) + fm_off.0,
            ty: f64::from(pen.y) + fm_off.1,
        };
        let mut sink = PathSink {
            path: PsPath::default(),
            m,
            cur: (0.0, 0.0),
        };
        face.outline_glyph(gid, &mut sink);
        if !sink.path.is_empty() {
            match mode {
                ShowMode::Paint => gfx.fill_path_direct(&sink.path),
                ShowMode::Charpath => gfx.append_path(&sink.path),
                ShowMode::Width => unreachable!("checked above"),
            }
        }
    }

    // Missing glyphs still advance — by .notdef's width, like a real
    // interpreter showing an unencoded character. Width is in raw glyph
    // units; `g` already carries the em normalization.
    let w = f64::from(
        gid.and_then(|g| face.glyph_hor_advance(g))
            .or_else(|| face.glyph_hor_advance(GlyphId(0)))
            .unwrap_or(0),
    );
    Ok((g[0] * w, g[1] * w))
}

/// One Type 1 glyph: decrypt the charstring, interpret it (hints
/// ignored, flex as curves — see `src/type1.rs`), and run the outline
/// through the same FontMatrix∘CTM pipeline as every other glyph
/// source. Charspace units go through FontMatrix directly (no em
/// normalization — the font's own 0.001 matrix is the convention).
fn type1_glyph(
    gfx: &mut Gfx,
    fs: &FontState,
    byte: u8,
    mode: &ShowMode,
    pen: DevPoint,
) -> Result<(f64, f64), PsError> {
    let name = encoding_name(fs, byte);
    let (charstrings, len_iv, subr_strings) = {
        let d = fs.dict.borrow();
        let cs = match d.get("CharStrings").as_ref().map(|o| &o.value) {
            Some(Value::Dict(cs)) => cs.clone(),
            _ => return Err(PsError::InvalidFont),
        };
        let mut len_iv = 4usize;
        let mut subr_strings: Vec<crate::object::PsString> = Vec::new();
        if let Some(Value::Dict(private)) = d.get("Private").as_ref().map(|o| &o.value) {
            let p = private.borrow();
            if let Some(Value::Integer(n)) = p.get("lenIV").as_ref().map(|o| &o.value) {
                len_iv = usize::try_from(*n).unwrap_or(4);
            }
            if let Some(Value::Array(a)) = p.get("Subrs").as_ref().map(|o| &o.value) {
                for i in 0..a.len() {
                    if let Some(Value::String(s)) = a.get(i).as_ref().map(|o| &o.value) {
                        subr_strings.push(s.clone());
                    } else {
                        subr_strings.push(PsString::from_bytes(Vec::new()));
                    }
                }
            }
        }
        (cs, len_iv, subr_strings)
    };
    let fetch = |n: &str| -> Option<Vec<u8>> {
        match charstrings.borrow().get(n).as_ref().map(|o| &o.value) {
            Some(Value::String(s)) => Some(crate::file::decrypt_charstring(&s.to_vec(), len_iv)),
            _ => None,
        }
    };
    let Some(code) = fetch(&name).or_else(|| fetch(".notdef")) else {
        return Ok((0.0, 0.0));
    };
    let subrs: Vec<Vec<u8>> = subr_strings
        .iter()
        .map(|s| crate::file::decrypt_charstring(&s.to_vec(), len_iv))
        .collect();
    let glyph = crate::type1::run(
        &code,
        &crate::type1::FontData {
            subrs: &subrs,
            charstring_by_name: &fetch,
        },
    )?;

    let fm = fs.matrix;
    if !matches!(mode, ShowMode::Width) {
        let ctm = gfx.ctm();
        let (c00, c01) = (f64::from(ctm.sx), f64::from(ctm.kx));
        let (c10, c11) = (f64::from(ctm.ky), f64::from(ctm.sy));
        let fm_off = ctm_lin(gfx, (fm[4], fm[5]));
        let m = GlyphTransform {
            a: c00 * fm[0] + c01 * fm[1],
            b: c10 * fm[0] + c11 * fm[1],
            c: c00 * fm[2] + c01 * fm[3],
            d: c10 * fm[2] + c11 * fm[3],
            tx: f64::from(pen.x) + fm_off.0,
            ty: f64::from(pen.y) + fm_off.1,
        };
        let mut path = PsPath::default();
        for cmd in &glyph.cmds {
            match *cmd {
                crate::type1::Cmd::Move(x, y) => path.move_to(m.apply(x, y)),
                crate::type1::Cmd::Line(x, y) => path.line_to(m.apply(x, y)),
                crate::type1::Cmd::Curve(x1, y1, x2, y2, x, y) => {
                    path.curve_to(m.apply(x1, y1), m.apply(x2, y2), m.apply(x, y));
                }
                crate::type1::Cmd::Close => path.close(),
            }
        }
        if !path.is_empty() {
            match mode {
                ShowMode::Paint => gfx.fill_path_direct(&path),
                ShowMode::Charpath => gfx.append_path(&path),
                ShowMode::Width => unreachable!("checked above"),
            }
        }
    }
    Ok((fm[0] * glyph.width, fm[1] * glyph.width))
}

/// A Type 3 glyph whose BuildChar/BuildGlyph procedure is currently
/// executing on the machine. Holds everything needed to seal the glyph
/// context back up, however the procedure exits.
pub(crate) struct PendingGlyph {
    byte: u8,
    /// Glyph-space width from setcachedevice/setcharwidth; a BuildChar
    /// that never declares one advances by zero.
    width: Option<(f64, f64)>,
    /// The FontState matrix at setup, for the advance transform.
    matrix: [f64; 6],
    depth: usize,
    saved: Box<crate::gfx::GraphicsState>,
    suppressed: bool,
}

/// What the machine should do after a ShowCtx step.
pub(crate) enum ShowStep {
    /// The show is complete; pop the frame.
    Pop,
    /// stringwidth complete; pop the frame and push wx wy.
    PopPushWidth(f64, f64),
    /// Push `operands`, then execute `target` (a BuildChar/BuildGlyph or
    /// kshow procedure); step again when it finishes.
    Exec {
        operands: Vec<Object>,
        target: Object,
    },
    /// Synchronous work done (an outline glyph painted); step again.
    Again,
}

/// The execution-stack frame behind the whole show family (`FONTS.md`
/// Decision 6). One glyph per machine step: outline glyphs paint
/// synchronously, Type 3 glyphs yield to their BuildChar/BuildGlyph
/// procedure, and kshow yields to its proc between characters. Because
/// it's a frame, text draws glyph by glyph in the live window, nested
/// shows inside BuildChar just work, and `stringwidth`'s results land
/// on the operand stack before the next token runs — frame order *is*
/// program order.
pub(crate) struct ShowCtx {
    text: Vec<u8>,
    idx: usize,
    /// Device-space pen. The current point is re-fetched after each
    /// kshow proc so the proc can move it — that's what kshow is *for*.
    pen: DevPoint,
    /// User-space accumulated advance (the stringwidth result).
    total: (f64, f64),
    params: ShowParams,
    mode: ShowMode,
    kshow_proc: Option<Object>,
    pending: Option<PendingGlyph>,
    /// A kshow proc is on the stack above us.
    kshow_wait: bool,
}

impl ShowCtx {
    pub(crate) fn new(
        text: Vec<u8>,
        params: ShowParams,
        mode: ShowMode,
        kshow_proc: Option<Object>,
        pen: DevPoint,
    ) -> Self {
        ShowCtx {
            text,
            idx: 0,
            pen,
            total: (0.0, 0.0),
            params,
            mode,
            kshow_proc,
            pending: None,
            kshow_wait: false,
        }
    }

    pub(crate) fn has_pending(&self) -> bool {
        self.pending.is_some()
    }

    /// setcachedevice/setcharwidth landing here, in glyph space.
    pub(crate) fn set_glyph_width(&mut self, w: (f64, f64)) {
        if let Some(p) = &mut self.pending {
            p.width = Some(w);
        }
    }

    /// Unwind bookkeeping when the frame is popped abnormally (stop, an
    /// uncaught error) with a glyph context still open.
    pub(crate) fn cleanup(&mut self, gfx: &mut Gfx) {
        if let Some(p) = self.pending.take() {
            if p.suppressed {
                gfx.unsuppress_painting();
            }
            gfx.restore_glyph_snapshot(p.depth, p.saved);
        }
    }

    pub(crate) fn step(&mut self, gfx: &mut Gfx) -> Result<ShowStep, PsError> {
        // A BuildChar just finished: seal the glyph context and advance.
        if let Some(p) = self.pending.take() {
            if p.suppressed {
                gfx.unsuppress_painting();
            }
            gfx.restore_glyph_snapshot(p.depth, p.saved);
            let (wx, wy) = p.width.unwrap_or((0.0, 0.0));
            let m = p.matrix;
            self.advance(gfx, p.byte, (m[0] * wx + m[2] * wy, m[1] * wx + m[3] * wy));
            if let Some(step) = self.maybe_kshow() {
                return Ok(step);
            }
            return Ok(ShowStep::Again);
        }
        // A kshow proc just finished: it may have moved the current
        // point; the next glyph starts wherever it left the pen.
        if self.kshow_wait {
            self.kshow_wait = false;
            if !matches!(self.mode, ShowMode::Width) {
                self.pen = gfx.device_current_point().ok_or(PsError::NoCurrentPoint)?;
            }
        }
        let Some(&byte) = self.text.get(self.idx) else {
            return Ok(match self.mode {
                ShowMode::Width => ShowStep::PopPushWidth(self.total.0, self.total.1),
                _ => ShowStep::Pop,
            });
        };
        // The font is read per glyph: a kshow proc (or a nested show
        // inside BuildChar) may have changed it.
        let fs = gfx.state().font.clone().ok_or(PsError::InvalidFont)?;
        if fs.fid >= 0 {
            let adv = outline_glyph(gfx, &fs, byte, &self.mode, self.pen)?;
            self.idx += 1;
            self.advance(gfx, byte, adv);
            if let Some(step) = self.maybe_kshow() {
                return Ok(step);
            }
            Ok(ShowStep::Again)
        } else {
            // No registry entry: a procedural font. Type 3 glyph
            // procedures win; otherwise Type 1 CharStrings render
            // synchronously like outlines.
            let is_type3 = {
                let d = fs.dict.borrow();
                d.get("BuildGlyph").is_some() || d.get("BuildChar").is_some()
            };
            if is_type3 {
                self.begin_type3_glyph(gfx, fs, byte)
            } else {
                let adv = type1_glyph(gfx, &fs, byte, &self.mode, self.pen)?;
                self.idx += 1;
                self.advance(gfx, byte, adv);
                if let Some(step) = self.maybe_kshow() {
                    return Ok(step);
                }
                Ok(ShowStep::Again)
            }
        }
    }

    /// Apply one glyph's advance (plus ashow/widthshow extras) to the
    /// running total and, when painting, to the pen and current point.
    fn advance(&mut self, gfx: &mut Gfx, byte: u8, mut adv: (f64, f64)) {
        adv.0 += self.params.extra.0;
        adv.1 += self.params.extra.1;
        if let Some((ch, (cx, cy))) = self.params.char_extra
            && ch == byte
        {
            adv.0 += cx;
            adv.1 += cy;
        }
        self.total.0 += adv.0;
        self.total.1 += adv.1;
        if !matches!(self.mode, ShowMode::Width) {
            let (dx, dy) = ctm_lin(gfx, adv);
            self.pen.x += dx as f32;
            self.pen.y += dy as f32;
            // Advance the visible current point per glyph, not at the
            // end: BuildChar and kshow procs are allowed to observe it.
            gfx.move_to_device(self.pen);
        }
    }

    /// Between two characters (never before the first or after the
    /// last), kshow pushes their codes and runs its proc.
    fn maybe_kshow(&mut self) -> Option<ShowStep> {
        let proc = self.kshow_proc.as_ref()?;
        let &next = self.text.get(self.idx)?;
        let prev = self.text[self.idx - 1];
        self.kshow_wait = true;
        Some(ShowStep::Exec {
            operands: vec![Object::int(i64::from(prev)), Object::int(i64::from(next))],
            target: proc.clone(),
        })
    }

    /// Open a Type 3 glyph context: snapshot the graphics state, set the
    /// CTM so BuildChar draws in glyph space at the pen, start a fresh
    /// path, and hand the machine the procedure to run. Metrics-only
    /// modes run the procedure with painting suppressed — that's how
    /// stringwidth learns a Type 3 width (charpath on Type 3 advances
    /// without appending outlines; documented in FONTS.md).
    fn begin_type3_glyph(
        &mut self,
        gfx: &mut Gfx,
        fs: FontState,
        byte: u8,
    ) -> Result<ShowStep, PsError> {
        let name = encoding_name(&fs, byte);
        let (target, char_operand) = {
            let d = fs.dict.borrow();
            // BuildGlyph (Level 2, name-driven) wins over BuildChar
            // (Level 1, code-driven), per the PLRM.
            if let Some(bg) = d.get("BuildGlyph") {
                (bg, Object::lit(Value::Name(name)))
            } else if let Some(bc) = d.get("BuildChar") {
                (bc, Object::int(i64::from(byte)))
            } else {
                return Err(PsError::InvalidFont);
            }
        };
        let (depth, saved) = gfx.glyph_snapshot();
        let suppressed = !matches!(self.mode, ShowMode::Paint);
        if suppressed {
            gfx.suppress_painting();
        }
        // Glyph space → FontMatrix → user space at the pen → device:
        // same pipeline as outline glyphs, installed as the CTM so
        // BuildChar's ordinary path operators land in the right place.
        let fm = fs.matrix;
        let ctm = gfx.ctm();
        let (c00, c01) = (f64::from(ctm.sx), f64::from(ctm.kx));
        let (c10, c11) = (f64::from(ctm.ky), f64::from(ctm.sy));
        let off = ctm_lin(gfx, (fm[4], fm[5]));
        gfx.set_ctm(tiny_skia::Transform::from_row(
            (c00 * fm[0] + c01 * fm[1]) as f32,
            (c10 * fm[0] + c11 * fm[1]) as f32,
            (c00 * fm[2] + c01 * fm[3]) as f32,
            (c10 * fm[2] + c11 * fm[3]) as f32,
            (f64::from(self.pen.x) + off.0) as f32,
            (f64::from(self.pen.y) + off.1) as f32,
        ));
        gfx.newpath();
        self.idx += 1;
        self.pending = Some(PendingGlyph {
            byte,
            width: None,
            matrix: fs.matrix,
            depth,
            saved,
            suppressed,
        });
        Ok(ShowStep::Exec {
            operands: vec![Object::lit(Value::Dict(fs.dict.clone())), char_operand],
            target,
        })
    }
}
