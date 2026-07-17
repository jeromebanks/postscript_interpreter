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
    // Graphics state
    op(dict, "gsave", gsave);
    op(dict, "grestore", grestore);
    op(dict, "grestoreall", grestoreall);
    op(dict, "setgray", setgray);
    op(dict, "setrgbcolor", setrgbcolor);
    op(dict, "setlinewidth", setlinewidth);
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

/// Deliberate deviation: real showpage transmits the page and erases it
/// for the next one. We mark the page complete and leave the image up —
/// erasing would defeat the entire point of watching it draw.
fn showpage(it: &mut Interp) -> Result<(), PsError> {
    it.gfx.page_shown = true;
    it.gfx.dirty = true;
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
