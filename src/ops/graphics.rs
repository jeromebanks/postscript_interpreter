//! Path construction, painting, graphics state, and coordinate-system
//! operators. These are thin operand-stack adapters; the geometry lives
//! in `crate::gfx`.

use tiny_skia::{FillRule, Transform};

use crate::error::PsError;
use crate::interp::Interp;
use crate::object::{Dict, Object, Value};

pub fn install(dict: &mut Dict) {
    use super::op;
    // Path construction
    op(dict, "newpath", newpath);
    op(dict, "moveto", moveto);
    op(dict, "lineto", lineto);
    op(dict, "rmoveto", rmoveto);
    op(dict, "rlineto", rlineto);
    op(dict, "curveto", curveto);
    op(dict, "rcurveto", rcurveto);
    op(dict, "arc", arc);
    op(dict, "arcn", arcn);
    op(dict, "closepath", closepath);
    op(dict, "currentpoint", currentpoint);
    // Painting
    op(dict, "fill", fill);
    op(dict, "eofill", eofill);
    op(dict, "stroke", stroke);
    op(dict, "erasepage", erasepage);
    op(dict, "showpage", showpage);
    op(dict, "copypage", copypage);
    op(dict, "initgraphics", initgraphics);
    // Graphics state
    op(dict, "gsave", gsave);
    op(dict, "grestore", grestore);
    op(dict, "grestoreall", grestoreall);
    op(dict, "setgray", setgray);
    op(dict, "setrgbcolor", setrgbcolor);
    op(dict, "setlinewidth", setlinewidth);
    op(dict, "setflat", setflat);
    op(dict, "currentflat", currentflat);
    op(dict, "setlinecap", setlinecap);
    op(dict, "setlinejoin", setlinejoin);
    op(dict, "setmiterlimit", setmiterlimit);
    op(dict, "setdash", setdash);
    op(dict, "currentdash", currentdash);
    op(dict, "sethsbcolor", sethsbcolor);
    op(dict, "currenthsbcolor", currenthsbcolor);
    op(dict, "setcmykcolor", setcmykcolor);
    op(dict, "currentrgbcolor", currentrgbcolor);
    op(dict, "currentgray", currentgray);
    op(dict, "currentlinewidth", currentlinewidth);
    // Rectangle conveniences (Level 2)
    op(dict, "rectfill", rectfill);
    op(dict, "rectstroke", rectstroke);
    op(dict, "rectclip", rectclip);
    // Clipping
    op(dict, "clip", clip);
    op(dict, "eoclip", eoclip);
    op(dict, "clippath", clippath);
    op(dict, "initclip", initclip);
    op(dict, "pathbbox", pathbbox);
    // Coordinate system (the matrix-operand forms live in ops::matrix)
    op(dict, "translate", translate);
    op(dict, "scale", scale);
    op(dict, "rotate", rotate);
}

fn pop_xy(it: &mut Interp) -> Result<(f64, f64), PsError> {
    let y = it.pop_f64()?;
    let x = it.pop_f64()?;
    Ok((x, y))
}

fn newpath(it: &mut Interp) -> Result<(), PsError> {
    it.gfx.newpath();
    Ok(())
}

fn moveto(it: &mut Interp) -> Result<(), PsError> {
    let (x, y) = pop_xy(it)?;
    it.gfx.moveto(x, y);
    Ok(())
}

fn lineto(it: &mut Interp) -> Result<(), PsError> {
    let (x, y) = pop_xy(it)?;
    it.gfx.lineto(x, y)
}

fn rmoveto(it: &mut Interp) -> Result<(), PsError> {
    let (dx, dy) = pop_xy(it)?;
    it.gfx.rmoveto(dx, dy)
}

fn rlineto(it: &mut Interp) -> Result<(), PsError> {
    let (dx, dy) = pop_xy(it)?;
    it.gfx.rlineto(dx, dy)
}

fn pop6(it: &mut Interp) -> Result<[f64; 6], PsError> {
    let mut v = [0.0; 6];
    for slot in v.iter_mut().rev() {
        *slot = it.pop_f64()?;
    }
    Ok(v)
}

fn curveto(it: &mut Interp) -> Result<(), PsError> {
    let [x1, y1, x2, y2, x3, y3] = pop6(it)?;
    it.gfx.curveto(x1, y1, x2, y2, x3, y3)
}

fn rcurveto(it: &mut Interp) -> Result<(), PsError> {
    let [dx1, dy1, dx2, dy2, dx3, dy3] = pop6(it)?;
    it.gfx.rcurveto(dx1, dy1, dx2, dy2, dx3, dy3)
}

fn arc_common(it: &mut Interp, ccw: bool) -> Result<(), PsError> {
    let ang2 = it.pop_f64()?;
    let ang1 = it.pop_f64()?;
    let r = it.pop_f64()?;
    let (x, y) = pop_xy(it)?;
    it.gfx.arc(x, y, r, ang1, ang2, ccw)
}

fn arc(it: &mut Interp) -> Result<(), PsError> {
    arc_common(it, true)
}

fn arcn(it: &mut Interp) -> Result<(), PsError> {
    arc_common(it, false)
}

fn closepath(it: &mut Interp) -> Result<(), PsError> {
    it.gfx.closepath();
    Ok(())
}

fn currentpoint(it: &mut Interp) -> Result<(), PsError> {
    let (x, y) = it.gfx.current_user_point()?;
    it.push(Object::real(x));
    it.push(Object::real(y));
    Ok(())
}

fn fill(it: &mut Interp) -> Result<(), PsError> {
    it.gfx.fill(FillRule::Winding);
    Ok(())
}

fn eofill(it: &mut Interp) -> Result<(), PsError> {
    it.gfx.fill(FillRule::EvenOdd);
    Ok(())
}

fn stroke(it: &mut Interp) -> Result<(), PsError> {
    it.gfx.stroke();
    Ok(())
}

fn erasepage(it: &mut Interp) -> Result<(), PsError> {
    it.gfx.erase();
    Ok(())
}

/// showpage snapshots the finished page and resets the graphics state
/// (full initgraphics — pinned against gs: CTM, color, width all
/// reset). The erase is lazy: the image stays on the canvas until the
/// next painting op, so single-page programs keep their picture and
/// the window keeps showing it — resolving the Stage 2 deviation
/// without defeating the point of watching.
fn showpage(it: &mut Interp) -> Result<(), PsError> {
    it.gfx.showpage();
    it.gfx.init_graphics();
    it.gfx.dirty = true;
    Ok(())
}

/// Level 1 copypage: emit the page, keep canvas and graphics state.
fn copypage(it: &mut Interp) -> Result<(), PsError> {
    it.gfx.copypage();
    Ok(())
}

fn initgraphics(it: &mut Interp) -> Result<(), PsError> {
    it.gfx.init_graphics();
    Ok(())
}

fn gsave(it: &mut Interp) -> Result<(), PsError> {
    it.gfx.gsave();
    Ok(())
}

/// Pop to the innermost save's boundary (the state stays available for
/// that save's restore), or to the bottom when no save is live.
fn grestoreall(it: &mut Interp) -> Result<(), PsError> {
    it.do_grestoreall();
    Ok(())
}

fn grestore(it: &mut Interp) -> Result<(), PsError> {
    it.gfx.grestore();
    Ok(())
}

fn setgray(it: &mut Interp) -> Result<(), PsError> {
    let g = it.pop_f64()?;
    it.gfx.set_rgb(g, g, g);
    // The color operators select their color space implicitly, per the
    // PLRM — the image dict form reads it for its component count.
    it.gfx.set_colorspace(crate::gfx::ColorSpace::Gray);
    Ok(())
}

fn setrgbcolor(it: &mut Interp) -> Result<(), PsError> {
    let b = it.pop_f64()?;
    let g = it.pop_f64()?;
    let r = it.pop_f64()?;
    it.gfx.set_rgb(r, g, b);
    it.gfx.set_colorspace(crate::gfx::ColorSpace::Rgb);
    Ok(())
}

/// Flatness is a curve-approximation hint; we store it (clamped to
/// the PLRM's 0.2..100) and let tiny-skia flatten as it pleases.
fn setflat(it: &mut Interp) -> Result<(), PsError> {
    let f = it.pop_f64()?;
    it.gfx.state_mut().flatness = f.clamp(0.2, 100.0);
    Ok(())
}

fn currentflat(it: &mut Interp) -> Result<(), PsError> {
    let f = it.gfx.state().flatness;
    it.push(Object::real(f));
    Ok(())
}

fn setlinewidth(it: &mut Interp) -> Result<(), PsError> {
    let w = it.pop_f64()?;
    it.gfx.set_line_width(w);
    Ok(())
}

fn setlinecap(it: &mut Interp) -> Result<(), PsError> {
    let n = it.pop_int()?;
    it.gfx.set_line_cap(n)
}

fn setlinejoin(it: &mut Interp) -> Result<(), PsError> {
    let n = it.pop_int()?;
    it.gfx.set_line_join(n)
}

fn setmiterlimit(it: &mut Interp) -> Result<(), PsError> {
    let limit = it.pop_f64()?;
    it.gfx.set_miter_limit(limit)
}

fn setdash(it: &mut Interp) -> Result<(), PsError> {
    let phase = it.pop_f64()?;
    let obj = it.pop()?;
    let Value::Array(a) = &obj.value else {
        return Err(PsError::Typecheck);
    };
    let mut pattern = Vec::with_capacity(a.len());
    for i in 0..a.len() {
        match a.get(i).as_ref().map(|o| &o.value) {
            Some(Value::Integer(n)) => pattern.push(*n as f64),
            Some(Value::Real(r)) => pattern.push(*r),
            _ => return Err(PsError::Typecheck),
        }
    }
    it.gfx.set_dash(pattern, phase)
}

fn currentdash(it: &mut Interp) -> Result<(), PsError> {
    let (pattern, phase) = it.gfx.current_dash();
    it.push(Object::array(
        pattern.into_iter().map(Object::real).collect(),
    ));
    it.push(Object::real(phase));
    Ok(())
}

fn sethsbcolor(it: &mut Interp) -> Result<(), PsError> {
    let b = it.pop_f64()?;
    let s = it.pop_f64()?;
    let h = it.pop_f64()?;
    it.gfx.set_hsb(h, s, b);
    Ok(())
}

fn currenthsbcolor(it: &mut Interp) -> Result<(), PsError> {
    let (h, s, b) = it.gfx.hsb();
    it.push(Object::real(h));
    it.push(Object::real(s));
    it.push(Object::real(b));
    Ok(())
}

fn setcmykcolor(it: &mut Interp) -> Result<(), PsError> {
    let k = it.pop_f64()?;
    let y = it.pop_f64()?;
    let m = it.pop_f64()?;
    let c = it.pop_f64()?;
    it.gfx.set_cmyk(c, m, y, k);
    it.gfx.set_colorspace(crate::gfx::ColorSpace::Cmyk);
    Ok(())
}

fn currentrgbcolor(it: &mut Interp) -> Result<(), PsError> {
    let (r, g, b) = it.gfx.rgb();
    it.push(Object::real(r));
    it.push(Object::real(g));
    it.push(Object::real(b));
    Ok(())
}

fn currentgray(it: &mut Interp) -> Result<(), PsError> {
    let g = it.gfx.gray();
    it.push(Object::real(g));
    Ok(())
}

fn currentlinewidth(it: &mut Interp) -> Result<(), PsError> {
    let w = it.gfx.line_width();
    it.push(Object::real(w));
    Ok(())
}

/// The rect operand forms shared by rectfill/rectstroke/rectclip:
/// `x y width height`, or a flat array of 4n numbers (each quad one
/// rectangle; empty allowed, other lengths typecheck — gs-pinned).
/// The PLRM's encoded-number-string form is not supported; a string
/// typechecks, the same answer gs gives a plain string.
fn pop_rects(it: &mut Interp) -> Result<Vec<[f64; 4]>, PsError> {
    let top = it.pop()?;
    match &top.value {
        Value::Array(a) => {
            if a.len() % 4 != 0 {
                return Err(PsError::Typecheck);
            }
            let mut rects = Vec::with_capacity(a.len() / 4);
            for q in 0..a.len() / 4 {
                let mut r = [0.0; 4];
                for (k, slot) in r.iter_mut().enumerate() {
                    *slot = match a.get(q * 4 + k).as_ref().map(|o| &o.value) {
                        Some(Value::Integer(n)) => *n as f64,
                        Some(Value::Real(x)) => *x,
                        _ => return Err(PsError::Typecheck),
                    };
                }
                rects.push(r);
            }
            Ok(rects)
        }
        Value::Integer(n) => {
            let h = *n as f64;
            pop_rect_under_height(it, h)
        }
        Value::Real(h) => {
            let h = *h;
            pop_rect_under_height(it, h)
        }
        _ => Err(PsError::Typecheck),
    }
}

fn pop_rect_under_height(it: &mut Interp, h: f64) -> Result<Vec<[f64; 4]>, PsError> {
    let w = it.pop_f64()?;
    let (x, y) = pop_xy(it)?;
    Ok(vec![[x, y, w, h]])
}

fn append_rects(it: &mut Interp, rects: &[[f64; 4]]) -> Result<(), PsError> {
    for &[x, y, w, h] in rects {
        it.gfx.moveto(x, y);
        it.gfx.lineto(x + w, y)?;
        it.gfx.lineto(x + w, y + h)?;
        it.gfx.lineto(x, y + h)?;
        it.gfx.closepath();
    }
    Ok(())
}

/// `x y w h rectfill` / `numarray rectfill` — paint inside an implicit
/// gsave: current path and graphics state untouched (gs-pinned).
fn rectfill(it: &mut Interp) -> Result<(), PsError> {
    let rects = pop_rects(it)?;
    it.gfx.gsave();
    it.gfx.newpath();
    let built = append_rects(it, &rects);
    if built.is_ok() {
        it.gfx.fill(FillRule::Winding);
    }
    it.gfx.grestore();
    built
}

/// `x y w h rectstroke` / `numarray rectstroke`, optionally with a
/// matrix on top. gs pin: a 6-element array in top position is always
/// the matrix, never a rect list; any other array is rects.
fn rectstroke(it: &mut Interp) -> Result<(), PsError> {
    let top = it.pop()?;
    let matrix = if matches!(&top.value, Value::Array(a) if a.len() == 6) {
        Some(super::matrix::read_matrix(&top)?)
    } else {
        it.push(top);
        None
    };
    let rects = pop_rects(it)?;
    it.gfx.gsave();
    it.gfx.newpath();
    let built = append_rects(it, &rects);
    if built.is_ok() {
        // The matrix concats *after* the path is built: it shapes the
        // pen, not the rectangles (the PLRM's gsave…concat…stroke form).
        if let Some(m) = matrix {
            it.gfx.concat_ctm(m);
        }
        it.gfx.stroke();
    }
    it.gfx.grestore();
    built
}

/// `x y w h rectclip` / `numarray rectclip` — intersect the clip and
/// leave the current path empty (gs-pinned; an empty rect list clips
/// everything away, also pinned).
fn rectclip(it: &mut Interp) -> Result<(), PsError> {
    let rects = pop_rects(it)?;
    it.gfx.newpath();
    append_rects(it, &rects)?;
    it.gfx.clip(FillRule::Winding)?;
    it.gfx.newpath();
    Ok(())
}

fn clip(it: &mut Interp) -> Result<(), PsError> {
    it.gfx.clip(FillRule::Winding)
}

fn eoclip(it: &mut Interp) -> Result<(), PsError> {
    it.gfx.clip(FillRule::EvenOdd)
}

fn clippath(it: &mut Interp) -> Result<(), PsError> {
    it.gfx.set_path_to_clip();
    Ok(())
}

fn initclip(it: &mut Interp) -> Result<(), PsError> {
    it.gfx.initclip();
    Ok(())
}

fn pathbbox(it: &mut Interp) -> Result<(), PsError> {
    let (lx, ly, ux, uy) = it.gfx.path_bbox()?;
    it.push(Object::real(lx));
    it.push(Object::real(ly));
    it.push(Object::real(ux));
    it.push(Object::real(uy));
    Ok(())
}

fn translate(it: &mut Interp) -> Result<(), PsError> {
    let (tx, ty) = pop_xy(it)?;
    it.gfx
        .concat_ctm(Transform::from_translate(tx as f32, ty as f32));
    Ok(())
}

fn scale(it: &mut Interp) -> Result<(), PsError> {
    let (sx, sy) = pop_xy(it)?;
    it.gfx
        .concat_ctm(Transform::from_scale(sx as f32, sy as f32));
    Ok(())
}

fn rotate(it: &mut Interp) -> Result<(), PsError> {
    let angle = it.pop_f64()?;
    it.gfx.concat_ctm(Transform::from_rotate(angle as f32));
    Ok(())
}
