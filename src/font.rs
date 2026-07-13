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
use crate::object::{Dict, Object, Value};

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

/// The engine behind the `show` family. Walks `text` byte by byte
/// through the current font's live Encoding, painting/appending/
/// measuring per `mode`. Returns the total advance in user space
/// (what `stringwidth` reports).
pub(crate) fn run_show(
    gfx: &mut Gfx,
    text: &[u8],
    params: &ShowParams,
    mode: ShowMode,
) -> Result<(f64, f64), PsError> {
    let fs = gfx.state().font.clone().ok_or(PsError::InvalidFont)?;
    if fs.fid < 0 {
        // A definefont'd dict with no outline source (e.g. Type 3 until
        // that task lands).
        return Err(PsError::InvalidFont);
    }
    let face = face_for(fs.fid)?;
    let k = 1000.0 / f64::from(face.units_per_em());
    let fm = fs.matrix;

    // CTM linear part in f64 (tiny-skia rows: sx ky kx sy tx ty).
    let ctm = gfx.ctm();
    let (c00, c01) = (f64::from(ctm.sx), f64::from(ctm.kx));
    let (c10, c11) = (f64::from(ctm.ky), f64::from(ctm.sy));
    let ctm_lin = |x: f64, y: f64| (c00 * x + c01 * y, c10 * x + c11 * y);

    // Pen position in device space. Width mode needs no current point,
    // per the PLRM (stringwidth is pure metrics).
    let mut pen = match mode {
        ShowMode::Width => DevPoint { x: 0.0, y: 0.0 },
        _ => gfx.device_current_point().ok_or(PsError::NoCurrentPoint)?,
    };

    // Glyph-space → user-space linear part (FontMatrix over 1000-unit
    // glyph space, with the em normalization folded in) …
    let g = [fm[0] * k, fm[1] * k, fm[2] * k, fm[3] * k];
    // … and the FontMatrix translation, a per-glyph user-space offset.
    let fm_off = ctm_lin(fm[4], fm[5]);

    // The Encoding array is read live from the font dict; a font dict
    // without one (hand-built) falls back to StandardEncoding.
    let enc = fs
        .dict
        .borrow()
        .get("Encoding")
        .and_then(|o| match &o.value {
            Value::Array(a) => Some(a.clone()),
            _ => None,
        });
    let std_names = encodings::standard_encoding();

    let mut total = (0.0f64, 0.0f64);
    for &byte in text {
        let name: Rc<str> = match &enc {
            Some(a) => match a.get(usize::from(byte)).as_ref().map(|o| &o.value) {
                Some(Value::Name(n)) => n.clone(),
                // Out-of-range or non-name entries behave as .notdef.
                _ => ".notdef".into(),
            },
            None => std_names[usize::from(byte)].into(),
        };
        let gid = resolve_glyph(&face, &name);

        if let Some(gid) = gid
            && !matches!(mode, ShowMode::Width)
        {
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
        // interpreter showing an unencoded character. Width is in raw
        // glyph units; `g` below already carries the em normalization.
        let w = f64::from(
            gid.and_then(|g| face.glyph_hor_advance(g))
                .or_else(|| face.glyph_hor_advance(GlyphId(0)))
                .unwrap_or(0),
        );
        let mut adv = (g[0] * w, g[1] * w);
        adv.0 += params.extra.0;
        adv.1 += params.extra.1;
        if let Some((ch, (cx, cy))) = params.char_extra
            && ch == byte
        {
            adv.0 += cx;
            adv.1 += cy;
        }
        total.0 += adv.0;
        total.1 += adv.1;
        let (dx, dy) = ctm_lin(adv.0, adv.1);
        pen.x += dx as f32;
        pen.y += dy as f32;
    }

    if !matches!(mode, ShowMode::Width) {
        gfx.move_to_device(pen);
    }
    Ok(total)
}
