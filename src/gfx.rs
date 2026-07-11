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

    fn move_to(&mut self, p: DevPoint) {
        self.segs.push(Seg::Move(p));
        self.current = Some(p);
        self.subpath_start = Some(p);
    }

    fn line_to(&mut self, p: DevPoint) {
        self.segs.push(Seg::Line(p));
        self.current = Some(p);
    }

    fn curve_to(&mut self, c1: DevPoint, c2: DevPoint, p: DevPoint) {
        self.segs.push(Seg::Curve(c1, c2, p));
        self.current = Some(p);
    }

    fn close(&mut self) {
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

/// Everything `gsave`/`grestore` snapshots. The current path is part of
/// the graphics state per the PLRM — that's what makes the
/// `gsave fill grestore stroke` idiom work.
#[derive(Clone)]
pub struct GraphicsState {
    pub ctm: Transform,
    pub rgb: (f32, f32, f32),
    /// In user-space units, per the spec; converted at stroke time.
    pub line_width: f64,
    pub line_cap: LineCap,
    pub line_join: LineJoin,
    pub miter_limit: f32,
    pub path: PsPath,
}

pub struct Gfx {
    pub pixmap: Pixmap,
    state: GraphicsState,
    saved: Vec<GraphicsState>,
    /// Set by anything that paints; the window loop presents and clears it.
    pub dirty: bool,
    /// Set by `showpage`; lets the front end know the program considers
    /// the page complete.
    pub page_shown: bool,
}

impl Gfx {
    pub fn new(width: u32, height: u32) -> Option<Self> {
        let mut pixmap = Pixmap::new(width, height)?;
        pixmap.fill(tiny_skia::Color::WHITE);
        Some(Gfx {
            pixmap,
            state: GraphicsState {
                // Device y grows downward; PostScript y grows upward.
                // The base CTM flips the axis so user (0,0) is the
                // bottom-left corner, as the LaserWriter intended.
                ctm: Transform::from_row(1.0, 0.0, 0.0, -1.0, 0.0, height as f32),
                rgb: (0.0, 0.0, 0.0),
                line_width: 1.0,
                line_cap: LineCap::Butt,
                line_join: LineJoin::Miter,
                miter_limit: 10.0,
                path: PsPath::default(),
            },
            saved: Vec::new(),
            dirty: false,
            page_shown: false,
        })
    }

    pub fn state(&self) -> &GraphicsState {
        &self.state
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

    pub fn fill(&mut self, rule: FillRule) {
        if let Some(path) = self.state.path.to_skia() {
            self.pixmap
                .fill_path(&path, &self.paint(), rule, Transform::identity(), None);
            self.dirty = true;
        }
        // Painting consumes the path (implicit newpath), filled or not.
        self.newpath();
    }

    pub fn stroke(&mut self) {
        if let Some(path) = self.state.path.to_skia() {
            let stroke = Stroke {
                width: self.device_line_width(),
                line_cap: self.state.line_cap,
                line_join: self.state.line_join,
                miter_limit: self.state.miter_limit,
                dash: None,
            };
            self.pixmap
                .stroke_path(&path, &self.paint(), &stroke, Transform::identity(), None);
            self.dirty = true;
        }
        self.newpath();
    }

    /// √|det CTM| scales user-space width to device space — see the
    /// module comment for why this approximation.
    fn device_line_width(&self) -> f32 {
        let m = &self.state.ctm;
        let det = f64::from(m.sx * m.sy - m.kx * m.ky);
        let w = self.state.line_width * det.abs().sqrt();
        if self.state.line_width <= 0.0 {
            // PostScript width 0 means "thinnest line the device can
            // render" — one device pixel, never invisible.
            1.0
        } else {
            (w as f32).max(0.1)
        }
    }

    pub fn erase(&mut self) {
        self.pixmap.fill(tiny_skia::Color::WHITE);
        self.dirty = true;
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

    pub fn set_rgb(&mut self, r: f64, g: f64, b: f64) {
        self.state.rgb = (clamp01(r), clamp01(g), clamp01(b));
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
}

fn clamp01(v: f64) -> f32 {
    v.clamp(0.0, 1.0) as f32
}

fn to_u8(v: f32) -> u8 {
    (v * 255.0).round() as u8
}
