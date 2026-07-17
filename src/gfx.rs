//! Graphics state and rasterization.
//!
//! Path points are stored in **device space**: PostScript transforms
//! coordinates through the CTM at path-construction time, not at paint
//! time, which is what lets programs change the CTM mid-path (the radial
//! burst in `examples/golden_spiral.ps` depends on this). Arcs are
//! flattened to cubic Béziers in user space and the control points
//! transformed, so an arc under a rotated or scaled CTM comes out as the
//! correct ellipse.
//!
//! One knowing approximation: stroke width. PostScript strokes in user
//! space (a circle-ish pen transformed by the CTM); we stroke in device
//! space with the width scaled by √|det CTM|. Exact for the uniform
//! scales and rotations hand-written programs use; anisotropic `scale`
//! will draw uniform-width strokes where real PostScript draws elliptical
//! pens. Revisit if it ever matters.

use tiny_skia::{FillRule, LineCap, LineJoin, Paint, PathBuilder, Pixmap, Stroke, Transform};

use crate::error::PsError;
use crate::font::FontState;

/// US Letter in points — the default page when none is specified.
pub const DEFAULT_PAGE: (u32, u32) = (612, 792);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DevPoint {
    pub x: f32,
    pub y: f32,
}

#[derive(Clone)]
enum Seg {
    Move(DevPoint),
    Line(DevPoint),
    Curve(DevPoint, DevPoint, DevPoint),
    Close,
}

/// The current path, in device space, plus the bookkeeping `currentpoint`
/// and `closepath` need.
#[derive(Clone, Default)]
pub struct PsPath {
    segs: Vec<Seg>,
    current: Option<DevPoint>,
    subpath_start: Option<DevPoint>,
}

impl PsPath {
    pub fn is_empty(&self) -> bool {
        self.segs.is_empty()
    }

    pub fn current(&self) -> Option<DevPoint> {
        self.current
    }

    pub(crate) fn move_to(&mut self, p: DevPoint) {
        // Collapse consecutive moves: the earlier one starts an empty
        // subpath nothing can ever reference. Keeps show's per-glyph
        // pen advances from bloating the path.
        if let Some(Seg::Move(last)) = self.segs.last_mut() {
            *last = p;
        } else {
            self.segs.push(Seg::Move(p));
        }
        self.current = Some(p);
        self.subpath_start = Some(p);
    }

    pub(crate) fn line_to(&mut self, p: DevPoint) {
        self.segs.push(Seg::Line(p));
        self.current = Some(p);
    }

    /// Every point in the path, control points included — enough for a
    /// conservative bounding box.
    pub fn points(&self) -> Vec<DevPoint> {
        let mut out = Vec::new();
        for seg in &self.segs {
            match *seg {
                Seg::Move(p) | Seg::Line(p) => out.push(p),
                Seg::Curve(c1, c2, p) => out.extend([c1, c2, p]),
                Seg::Close => {}
            }
        }
        out
    }

    pub(crate) fn curve_to(&mut self, c1: DevPoint, c2: DevPoint, p: DevPoint) {
        self.segs.push(Seg::Curve(c1, c2, p));
        self.current = Some(p);
    }

    /// Append another path's segments (charpath splicing glyph outlines
    /// into the current path).
    pub(crate) fn extend_from(&mut self, other: &PsPath) {
        self.segs.extend(other.segs.iter().cloned());
        if other.current.is_some() {
            self.current = other.current;
        }
        if other.subpath_start.is_some() {
            self.subpath_start = other.subpath_start;
        }
    }

    pub(crate) fn close(&mut self) {
        // closepath on an empty path is a legal no-op in PostScript.
        if let Some(start) = self.subpath_start {
            self.segs.push(Seg::Close);
            self.current = Some(start);
        }
    }

    fn to_skia(&self) -> Option<tiny_skia::Path> {
        let mut pb = PathBuilder::new();
        for seg in &self.segs {
            match *seg {
                Seg::Move(p) => pb.move_to(p.x, p.y),
                Seg::Line(p) => pb.line_to(p.x, p.y),
                Seg::Curve(c1, c2, p) => pb.cubic_to(c1.x, c1.y, c2.x, c2.y, p.x, p.y),
                Seg::Close => pb.close(),
            }
        }
        pb.finish()
    }
}

/// The current color space — what `setcolor` and the dict-form image
/// operator consult. Device spaces carry no payload; Indexed carries
/// its lookup table; Separation carries the tint-transform procedure
/// (run as a `Frame::PostOp` continuation — never recursively).
#[derive(Clone)]
pub(crate) enum ColorSpace {
    Gray,
    Rgb,
    Cmyk,
    Indexed {
        table: std::rc::Rc<IndexedTable>,
        /// The original setcolorspace operand, for currentcolorspace.
        src: crate::object::Object,
    },
    Separation {
        alt_ncomp: u32,
        tint_proc: crate::object::Object,
        src: crate::object::Object,
    },
}

pub(crate) struct IndexedTable {
    pub base_ncomp: u32,
    pub hival: u32,
    /// (hival+1) * base_ncomp bytes, one 0..255 value per component.
    pub lookup: Vec<u8>,
}

impl IndexedTable {
    /// The base-space color at `idx`, as RGB (CMYK converted the same
    /// way the image path converts it).
    pub fn rgb_at(&self, idx: usize) -> (f32, f32, f32) {
        let n = self.base_ncomp as usize;
        let comp = |k: usize| f32::from(self.lookup.get(idx * n + k).copied().unwrap_or(0)) / 255.0;
        match self.base_ncomp {
            1 => (comp(0), comp(0), comp(0)),
            3 => (comp(0), comp(1), comp(2)),
            _ => {
                let (c, m, y, k) = (comp(0), comp(1), comp(2), comp(3));
                (
                    (1.0 - c) * (1.0 - k),
                    (1.0 - m) * (1.0 - k),
                    (1.0 - y) * (1.0 - k),
                )
            }
        }
    }
}

impl ColorSpace {
    /// Components per sample as image data sees it (Indexed samples
    /// are single indices; Separation samples are single tints).
    pub fn ncomp(&self) -> u32 {
        match self {
            ColorSpace::Gray | ColorSpace::Indexed { .. } | ColorSpace::Separation { .. } => 1,
            ColorSpace::Rgb => 3,
            ColorSpace::Cmyk => 4,
        }
    }
}

/// Everything `gsave`/`grestore` snapshots. The current path is part of
/// the graphics state per the PLRM — that's what makes the
/// `gsave fill grestore stroke` idiom work.
#[derive(Clone)]
pub struct GraphicsState {
    pub ctm: Transform,
    pub rgb: (f32, f32, f32),
    /// Set implicitly by the color operators and explicitly by
    /// setcolorspace, per the PLRM; the Level 2 image dict form and
    /// setcolor read it.
    colorspace: ColorSpace,
    /// In user-space units, per the spec; converted at stroke time.
    pub line_width: f64,
    /// `setflat` hint; stored for round-tripping only — tiny-skia
    /// does its own curve flattening.
    pub flatness: f64,
    pub line_cap: LineCap,
    pub line_join: LineJoin,
    pub miter_limit: f32,
    /// Dash pattern and phase, in user-space units; empty pattern = solid.
    pub dash: Option<(Vec<f64>, f64)>,
    /// The current font, set by `setfont`; snapshotted here so gsave/
    /// grestore roll it back like everything else. None until the first
    /// setfont — `show` then reports invalidfont, per the PLRM.
    pub font: Option<FontState>,
    pub path: PsPath,
    /// Current clip: an alpha mask (intersection of all active clips) and
    /// the most recent clip path, which `clippath` hands back.
    clip: Option<ClipState>,
}

#[derive(Clone)]
struct ClipState {
    mask: std::rc::Rc<tiny_skia::Mask>,
    path: PsPath,
}

pub struct Gfx {
    pub pixmap: Pixmap,
    state: GraphicsState,
    saved: Vec<GraphicsState>,
    /// The device CTM before any program transforms — what `initmatrix`
    /// restores and `defaultmatrix` reports.
    base_ctm: Transform,
    /// Set by anything that paints; the window loop presents and clears it.
    pub dirty: bool,
    /// Set by `showpage`; lets the front end know the program considers
    /// the page complete.
    pub page_shown: bool,
    /// Pages completed by showpage/copypage, in order.
    completed: Vec<Pixmap>,
    /// showpage erases *lazily*: the finished page stays on the canvas
    /// until the program paints again — watching is the point, and
    /// single-page programs keep their final image. Multi-page
    /// programs get the erase the moment page N+1's first mark lands.
    pending_erase: bool,
    /// Anything painted since the last page boundary? Decides whether
    /// the trailing canvas counts as a final page.
    painted_since_page: bool,
    /// Nonzero while a Type 3 BuildChar runs for metrics only
    /// (stringwidth/charpath): painting operators consume their paths
    /// but leave the pixels alone. A counter, not a flag — shows nest.
    suppress_paint: u32,
}

impl Gfx {
    pub fn new(width: u32, height: u32) -> Option<Self> {
        Self::with_scale(width, height, 1.0)
    }

    /// A canvas of `width`x`height` device pixels where one PostScript
    /// point maps to `scale` pixels — the `--dpi` seam (scale =
    /// dpi/72). The CTM machinery needs nothing else; user space stays
    /// in points throughout.
    pub fn with_scale(width: u32, height: u32, scale: f32) -> Option<Self> {
        let mut pixmap = Pixmap::new(width, height)?;
        pixmap.fill(tiny_skia::Color::WHITE);
        // Device y grows downward; PostScript y grows upward. The base
        // CTM flips the axis so user (0,0) is the bottom-left corner, as
        // the LaserWriter intended.
        let base_ctm = Transform::from_row(scale, 0.0, 0.0, -scale, 0.0, height as f32);
        Some(Gfx {
            pixmap,
            state: GraphicsState {
                ctm: base_ctm,
                rgb: (0.0, 0.0, 0.0),
                colorspace: ColorSpace::Gray,
                line_width: 1.0,
                flatness: 1.0,
                line_cap: LineCap::Butt,
                line_join: LineJoin::Miter,
                miter_limit: 10.0,
                dash: None,
                font: None,
                path: PsPath::default(),
                clip: None,
            },
            saved: Vec::new(),
            base_ctm,
            dirty: false,
            page_shown: false,
            completed: Vec::new(),
            pending_erase: false,
            painted_since_page: false,
            suppress_paint: 0,
        })
    }

    pub fn state(&self) -> &GraphicsState {
        &self.state
    }

    pub(crate) fn state_mut(&mut self) -> &mut GraphicsState {
        &mut self.state
    }

    pub(crate) fn colorspace_ncomp(&self) -> u32 {
        self.state.colorspace.ncomp()
    }

    pub(crate) fn colorspace(&self) -> &ColorSpace {
        &self.state.colorspace
    }

    // --- coordinate plumbing -------------------------------------------

    pub fn user_to_device(&self, x: f64, y: f64) -> DevPoint {
        let m = &self.state.ctm;
        let (x, y) = (x as f32, y as f32);
        DevPoint {
            x: m.sx * x + m.kx * y + m.tx,
            y: m.ky * x + m.sy * y + m.ty,
        }
    }

    /// Inverse mapping, needed by `currentpoint` and the relative path
    /// operators. Fails only on a singular CTM (e.g. `0 0 scale`).
    pub fn device_to_user(&self, p: DevPoint) -> Result<(f64, f64), PsError> {
        let inv = self.state.ctm.invert().ok_or(PsError::UndefinedResult)?;
        Ok((
            f64::from(inv.sx * p.x + inv.kx * p.y + inv.tx),
            f64::from(inv.ky * p.x + inv.sy * p.y + inv.ty),
        ))
    }

    /// The user-space current point, or `nocurrentpoint`.
    pub fn current_user_point(&self) -> Result<(f64, f64), PsError> {
        let p = self.state.path.current().ok_or(PsError::NoCurrentPoint)?;
        self.device_to_user(p)
    }

    pub fn concat_ctm(&mut self, m: Transform) {
        // pre_concat: the new operation applies to points before the
        // existing CTM — PostScript's `translate`/`rotate`/`scale` order.
        self.state.ctm = self.state.ctm.pre_concat(m);
    }

    pub fn ctm(&self) -> Transform {
        self.state.ctm
    }

    pub fn set_ctm(&mut self, m: Transform) {
        self.state.ctm = m;
    }

    pub fn base_ctm(&self) -> Transform {
        self.base_ctm
    }

    // --- path construction ---------------------------------------------

    pub fn newpath(&mut self) {
        self.state.path = PsPath::default();
    }

    pub fn moveto(&mut self, x: f64, y: f64) {
        let p = self.user_to_device(x, y);
        self.state.path.move_to(p);
    }

    pub fn lineto(&mut self, x: f64, y: f64) -> Result<(), PsError> {
        if self.state.path.current().is_none() {
            return Err(PsError::NoCurrentPoint);
        }
        let p = self.user_to_device(x, y);
        self.state.path.line_to(p);
        Ok(())
    }

    pub fn rmoveto(&mut self, dx: f64, dy: f64) -> Result<(), PsError> {
        let (x, y) = self.current_user_point()?;
        self.moveto(x + dx, y + dy);
        Ok(())
    }

    pub fn rlineto(&mut self, dx: f64, dy: f64) -> Result<(), PsError> {
        let (x, y) = self.current_user_point()?;
        self.lineto(x + dx, y + dy)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn curveto(
        &mut self,
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
        x3: f64,
        y3: f64,
    ) -> Result<(), PsError> {
        if self.state.path.current().is_none() {
            return Err(PsError::NoCurrentPoint);
        }
        let c1 = self.user_to_device(x1, y1);
        let c2 = self.user_to_device(x2, y2);
        let p = self.user_to_device(x3, y3);
        self.state.path.curve_to(c1, c2, p);
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn rcurveto(
        &mut self,
        dx1: f64,
        dy1: f64,
        dx2: f64,
        dy2: f64,
        dx3: f64,
        dy3: f64,
    ) -> Result<(), PsError> {
        let (x, y) = self.current_user_point()?;
        self.curveto(x + dx1, y + dy1, x + dx2, y + dy2, x + dx3, y + dy3)
    }

    pub fn closepath(&mut self) {
        self.state.path.close();
    }

    /// `arc` (counterclockwise, `ccw = true`) and `arcn` (clockwise).
    /// Angles in degrees, per PostScript. Flattened to ≤90° Bézier
    /// segments in user space; each control point goes through the CTM.
    pub fn arc(
        &mut self,
        cx: f64,
        cy: f64,
        r: f64,
        ang1: f64,
        ang2: f64,
        ccw: bool,
    ) -> Result<(), PsError> {
        let (a1, mut a2) = (ang1.to_radians(), ang2.to_radians());
        if ccw {
            while a2 < a1 {
                a2 += std::f64::consts::TAU;
            }
        } else {
            while a2 > a1 {
                a2 -= std::f64::consts::TAU;
            }
        }

        let point_at = |a: f64| (cx + r * a.cos(), cy + r * a.sin());
        let start = point_at(a1);
        // Arc connects to an existing path with a line, else starts one.
        if self.state.path.current().is_some() {
            self.lineto(start.0, start.1)?;
        } else {
            self.moveto(start.0, start.1);
        }

        let sweep = a2 - a1;
        let n = (sweep.abs() / std::f64::consts::FRAC_PI_2).ceil().max(1.0) as usize;
        let step = sweep / n as f64;
        let mut a = a1;
        for _ in 0..n {
            let b = a + step;
            // Standard cubic approximation of a circular arc segment.
            let k = 4.0 / 3.0 * (step / 4.0).tan();
            let (x0, y0) = point_at(a);
            let (x3, y3) = point_at(b);
            let c1 = (x0 - k * r * a.sin(), y0 + k * r * a.cos());
            let c2 = (x3 + k * r * b.sin(), y3 - k * r * b.cos());
            let c1 = self.user_to_device(c1.0, c1.1);
            let c2 = self.user_to_device(c2.0, c2.1);
            let p = self.user_to_device(x3, y3);
            self.state.path.curve_to(c1, c2, p);
            a = b;
        }
        Ok(())
    }

    // --- painting --------------------------------------------------------

    fn paint(&self) -> Paint<'static> {
        let mut paint = Paint::default();
        let (r, g, b) = self.state.rgb;
        paint.set_color_rgba8(to_u8(r), to_u8(g), to_u8(b), 255);
        paint.anti_alias = true;
        paint
    }

    /// Rc clone so the mask can outlive the `&mut self.pixmap` borrow.
    pub(crate) fn clip_mask_data(&self) -> Option<std::rc::Rc<tiny_skia::Mask>> {
        self.clip_mask()
    }

    fn clip_mask(&self) -> Option<std::rc::Rc<tiny_skia::Mask>> {
        self.state.clip.as_ref().map(|c| c.mask.clone())
    }

    /// Every painting entry point calls this first: consume a pending
    /// showpage erase and note that the new page has art on it.
    pub(crate) fn prepare_paint(&mut self) {
        if self.pending_erase {
            self.pending_erase = false;
            self.pixmap.fill(tiny_skia::Color::WHITE);
            self.dirty = true;
        }
        self.painted_since_page = true;
    }

    pub fn fill(&mut self, rule: FillRule) {
        if self.suppress_paint == 0
            && let Some(path) = self.state.path.to_skia()
        {
            self.prepare_paint();
            let paint = self.paint();
            let mask = self.clip_mask();
            self.pixmap
                .fill_path(&path, &paint, rule, Transform::identity(), mask.as_deref());
            self.dirty = true;
        }
        // Painting consumes the path (implicit newpath), filled or not.
        self.newpath();
    }

    pub fn stroke(&mut self) {
        if self.suppress_paint > 0 {
            self.newpath();
            return;
        }
        if let Some(path) = self.state.path.to_skia() {
            self.prepare_paint();
            let scale = self.ctm_scale();
            let dash = self.state.dash.as_ref().and_then(|(pattern, phase)| {
                // Dash lengths are user-space; scale them like the width.
                let scaled: Vec<f32> = pattern.iter().map(|d| (*d * scale) as f32).collect();
                tiny_skia::StrokeDash::new(scaled, (*phase * scale) as f32)
            });
            let stroke = Stroke {
                width: self.device_line_width(),
                line_cap: self.state.line_cap,
                line_join: self.state.line_join,
                miter_limit: self.state.miter_limit,
                dash,
            };
            let paint = self.paint();
            let mask = self.clip_mask();
            self.pixmap.stroke_path(
                &path,
                &paint,
                &stroke,
                Transform::identity(),
                mask.as_deref(),
            );
            self.dirty = true;
        }
        self.newpath();
    }

    /// √|det CTM| — the user-to-device scale factor for widths and dash
    /// lengths; see the module comment for why this approximation.
    fn ctm_scale(&self) -> f64 {
        let m = &self.state.ctm;
        f64::from(m.sx * m.sy - m.kx * m.ky).abs().sqrt()
    }

    fn device_line_width(&self) -> f32 {
        let w = self.state.line_width * self.ctm_scale();
        if self.state.line_width <= 0.0 {
            // PostScript width 0 means "thinnest line the device can
            // render" — one device pixel, never invisible.
            1.0
        } else {
            (w as f32).max(0.1)
        }
    }

    pub fn erase(&mut self) {
        if self.suppress_paint > 0 {
            return;
        }
        self.pending_erase = false;
        self.painted_since_page = true;
        self.pixmap.fill(tiny_skia::Color::WHITE);
        self.dirty = true;
    }

    // --- pages ------------------------------------------------------------

    /// `showpage`: snapshot the finished page and arm the lazy erase.
    /// The graphics-state reset (initgraphics, pinned against gs) is
    /// the operator's job.
    pub fn showpage(&mut self) {
        self.completed.push(self.pixmap.clone());
        self.page_shown = true;
        self.pending_erase = true;
        self.painted_since_page = false;
    }

    /// `copypage` (Level 1): emit the page but keep canvas and state.
    pub fn copypage(&mut self) {
        self.completed.push(self.pixmap.clone());
        self.page_shown = true;
    }

    /// Completed page snapshots, oldest first.
    pub fn pages(&self) -> &[Pixmap] {
        &self.completed
    }

    /// Whether the live canvas holds art not yet emitted by showpage —
    /// i.e. whether it counts as a trailing page.
    pub fn has_trailing_art(&self) -> bool {
        self.painted_since_page
    }

    /// Reset everything `initgraphics` resets, per the PLRM: CTM,
    /// path, clip, color (black, DeviceGray), line parameters. Not the
    /// font — gs keeps it, and so do we.
    pub fn init_graphics(&mut self) {
        let font = self.state.font.take();
        self.state = GraphicsState {
            ctm: self.base_ctm,
            rgb: (0.0, 0.0, 0.0),
            colorspace: ColorSpace::Gray,
            line_width: 1.0,
            flatness: 1.0,
            line_cap: LineCap::Butt,
            line_join: LineJoin::Miter,
            miter_limit: 10.0,
            dash: None,
            font,
            path: PsPath::default(),
            clip: None,
        };
    }

    // --- state ----------------------------------------------------------

    pub fn gsave(&mut self) {
        self.saved.push(self.state.clone());
    }

    pub fn grestore(&mut self) {
        // grestore below the bottom of the stack is a no-op, per the PLRM.
        if let Some(s) = self.saved.pop() {
            self.state = s;
        }
    }

    /// `grestoreall` with no save active: pop everything, ending in the
    /// bottommost saved state.
    pub(crate) fn grestore_all_bottom(&mut self) {
        if let Some(first) = self.saved.first() {
            self.state = first.clone();
            self.saved.clear();
        }
    }

    pub fn set_rgb(&mut self, r: f64, g: f64, b: f64) {
        self.state.rgb = (clamp01(r), clamp01(g), clamp01(b));
    }

    pub(crate) fn set_colorspace(&mut self, cs: ColorSpace) {
        self.state.colorspace = cs;
    }

    pub fn set_line_width(&mut self, w: f64) {
        self.state.line_width = w.abs();
    }

    pub fn set_line_cap(&mut self, n: i64) -> Result<(), PsError> {
        self.state.line_cap = match n {
            0 => LineCap::Butt,
            1 => LineCap::Round,
            2 => LineCap::Square,
            _ => return Err(PsError::Rangecheck),
        };
        Ok(())
    }

    pub fn set_line_join(&mut self, n: i64) -> Result<(), PsError> {
        self.state.line_join = match n {
            0 => LineJoin::Miter,
            1 => LineJoin::Round,
            2 => LineJoin::Bevel,
            _ => return Err(PsError::Rangecheck),
        };
        Ok(())
    }

    pub fn set_miter_limit(&mut self, limit: f64) -> Result<(), PsError> {
        if limit < 1.0 {
            return Err(PsError::Rangecheck);
        }
        self.state.miter_limit = limit as f32;
        Ok(())
    }

    /// Empty pattern = solid. Rangecheck on negatives or an all-zero
    /// pattern, per the PLRM.
    pub fn set_dash(&mut self, pattern: Vec<f64>, phase: f64) -> Result<(), PsError> {
        if pattern.is_empty() {
            self.state.dash = None;
            return Ok(());
        }
        if pattern.iter().any(|d| *d < 0.0) || pattern.iter().all(|d| *d == 0.0) {
            return Err(PsError::Rangecheck);
        }
        self.state.dash = Some((pattern, phase));
        Ok(())
    }

    pub fn current_dash(&self) -> (Vec<f64>, f64) {
        match &self.state.dash {
            Some((p, ph)) => (p.clone(), *ph),
            None => (Vec::new(), 0.0),
        }
    }

    // --- text -------------------------------------------------------------

    pub fn set_font(&mut self, f: FontState) {
        self.state.font = Some(f);
    }

    /// The raw device-space current point — the show engine works in
    /// device space throughout (see `src/font.rs`).
    pub(crate) fn device_current_point(&self) -> Option<DevPoint> {
        self.state.path.current()
    }

    /// Advance the current point without touching the rest of the path —
    /// how `show` leaves the pen after the last glyph.
    pub(crate) fn move_to_device(&mut self, p: DevPoint) {
        self.state.path.move_to(p);
    }

    /// Fill a free-standing path (a glyph outline) with the current
    /// paint and clip, leaving the current path alone. TrueType outlines
    /// are wound for nonzero filling.
    pub(crate) fn fill_path_direct(&mut self, path: &PsPath) {
        if self.suppress_paint > 0 {
            return;
        }
        if let Some(p) = path.to_skia() {
            self.prepare_paint();
            let paint = self.paint();
            let mask = self.clip_mask();
            self.pixmap.fill_path(
                &p,
                &paint,
                FillRule::Winding,
                Transform::identity(),
                mask.as_deref(),
            );
            self.dirty = true;
        }
    }

    /// Splice a glyph outline into the current path (`charpath`).
    pub(crate) fn append_path(&mut self, path: &PsPath) {
        self.state.path.extend_from(path);
    }

    /// Enter a Type 3 glyph context: remember where the gsave stack is
    /// and what the state was, so the glyph can be sealed off no matter
    /// what BuildChar does (unbalanced gsave/grestore included).
    pub(crate) fn glyph_snapshot(&self) -> (usize, Box<GraphicsState>) {
        (self.saved.len(), Box::new(self.state.clone()))
    }

    pub(crate) fn restore_glyph_snapshot(&mut self, depth: usize, state: Box<GraphicsState>) {
        self.saved.truncate(depth);
        self.state = *state;
    }

    pub(crate) fn suppress_painting(&mut self) {
        self.suppress_paint += 1;
    }

    pub(crate) fn unsuppress_painting(&mut self) {
        self.suppress_paint = self.suppress_paint.saturating_sub(1);
    }

    // --- clipping ---------------------------------------------------------

    /// Intersect the clip with the current path. Unlike painting, `clip`
    /// leaves the current path in place, per the PLRM.
    pub fn clip(&mut self, rule: FillRule) -> Result<(), PsError> {
        let (w, h) = (self.pixmap.width(), self.pixmap.height());
        let mut mask = tiny_skia::Mask::new(w, h).ok_or(PsError::Limitcheck)?;
        if let Some(path) = self.state.path.to_skia() {
            mask.fill_path(&path, rule, true, Transform::identity());
        }
        // An empty path clips everything away: the fresh mask is already
        // all-zero, which is exactly that.
        if let Some(prev) = &self.state.clip {
            let prev_data = prev.mask.data();
            for (m, p) in mask.data_mut().iter_mut().zip(prev_data) {
                *m = ((u16::from(*m) * u16::from(*p)) / 255) as u8;
            }
        }
        self.state.clip = Some(ClipState {
            mask: std::rc::Rc::new(mask),
            path: self.state.path.clone(),
        });
        Ok(())
    }

    pub fn initclip(&mut self) {
        self.state.clip = None;
    }

    /// `clippath`: replace the current path with the clip boundary — the
    /// most recent clip path, or the whole page if unclipped. (The true
    /// intersection path isn't tracked; the mask is. Documented
    /// approximation.)
    pub fn set_path_to_clip(&mut self) {
        match &self.state.clip {
            Some(c) => self.state.path = c.path.clone(),
            None => {
                let (w, h) = (self.pixmap.width() as f32, self.pixmap.height() as f32);
                let mut p = PsPath::default();
                p.move_to(DevPoint { x: 0.0, y: 0.0 });
                p.line_to(DevPoint { x: w, y: 0.0 });
                p.line_to(DevPoint { x: w, y: h });
                p.line_to(DevPoint { x: 0.0, y: h });
                p.close();
                self.state.path = p;
            }
        }
    }

    /// Bounding box of the current path in user space (control points
    /// included, per the PLRM's allowance).
    pub fn path_bbox(&self) -> Result<(f64, f64, f64, f64), PsError> {
        let pts = self.state.path.points();
        if pts.is_empty() {
            return Err(PsError::NoCurrentPoint);
        }
        let (mut lx, mut ly, mut ux, mut uy) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
        for p in pts {
            let (x, y) = self.device_to_user(p)?;
            lx = lx.min(x);
            ly = ly.min(y);
            ux = ux.max(x);
            uy = uy.max(y);
        }
        Ok((lx, ly, ux, uy))
    }

    // --- color ------------------------------------------------------------

    pub fn rgb(&self) -> (f64, f64, f64) {
        let (r, g, b) = self.state.rgb;
        (f64::from(r), f64::from(g), f64::from(b))
    }

    /// NTSC luminance weighting, per the PLRM's `currentgray`.
    pub fn gray(&self) -> f64 {
        let (r, g, b) = self.rgb();
        0.3 * r + 0.59 * g + 0.11 * b
    }

    pub fn set_hsb(&mut self, h: f64, s: f64, v: f64) {
        let (h, s, v) = (h.rem_euclid(1.0), s.clamp(0.0, 1.0), v.clamp(0.0, 1.0));
        let sector = (h * 6.0).floor();
        let f = h * 6.0 - sector;
        let p = v * (1.0 - s);
        let q = v * (1.0 - s * f);
        let t = v * (1.0 - s * (1.0 - f));
        let (r, g, b) = match sector as i32 {
            0 => (v, t, p),
            1 => (q, v, p),
            2 => (p, v, t),
            3 => (p, q, v),
            4 => (t, p, v),
            _ => (v, p, q),
        };
        self.set_rgb(r, g, b);
    }

    pub fn hsb(&self) -> (f64, f64, f64) {
        let (r, g, b) = self.rgb();
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let delta = max - min;
        let v = max;
        let s = if max > 0.0 { delta / max } else { 0.0 };
        let h = if delta == 0.0 {
            0.0
        } else if max == r {
            ((g - b) / delta).rem_euclid(6.0) / 6.0
        } else if max == g {
            ((b - r) / delta + 2.0) / 6.0
        } else {
            ((r - g) / delta + 4.0) / 6.0
        };
        (h, s, v)
    }

    pub fn set_cmyk(&mut self, c: f64, m: f64, y: f64, k: f64) {
        let k = k.clamp(0.0, 1.0);
        self.set_rgb(
            (1.0 - c.clamp(0.0, 1.0)) * (1.0 - k),
            (1.0 - m.clamp(0.0, 1.0)) * (1.0 - k),
            (1.0 - y.clamp(0.0, 1.0)) * (1.0 - k),
        );
    }

    pub fn line_width(&self) -> f64 {
        self.state.line_width
    }
}

fn clamp01(v: f64) -> f32 {
    v.clamp(0.0, 1.0) as f32
}

fn to_u8(v: f32) -> u8 {
    (v * 255.0).round() as u8
}
