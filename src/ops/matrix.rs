//! Matrix operators. A PostScript matrix is a six-element array
//! [a b c d tx ty] mapping x' = ax + cy + tx, y' = bx + dy + ty — which
//! is exactly tiny-skia's `from_row(sx, ky, kx, sy, tx, ty)` order.
//!
//! `translate`/`scale`/`rotate` keep their CTM forms in ops::graphics;
//! this module re-registers them with dispatch so the matrix-operand
//! forms (`tx ty matrix translate`) work too.

use tiny_skia::Transform;

use crate::error::PsError;
use crate::interp::Interp;
use crate::object::{Dict, Object, Value};

pub fn install(dict: &mut Dict) {
    use super::op;
    op(dict, "matrix", matrix);
    op(dict, "identmatrix", identmatrix);
    op(dict, "defaultmatrix", defaultmatrix);
    op(dict, "currentmatrix", currentmatrix);
    op(dict, "setmatrix", setmatrix);
    op(dict, "initmatrix", initmatrix);
    op(dict, "concat", concat);
    op(dict, "concatmatrix", concatmatrix);
    op(dict, "invertmatrix", invertmatrix);
    op(dict, "transform", transform);
    op(dict, "itransform", itransform);
    op(dict, "dtransform", dtransform);
    op(dict, "idtransform", idtransform);
    // Dispatching wrappers over the ops::graphics CTM forms; installed
    // after them (see ops::install_all ordering), so these win.
    op(dict, "translate", translate);
    op(dict, "scale", scale);
    op(dict, "rotate", rotate);
}

pub(crate) fn read_matrix(obj: &Object) -> Result<Transform, PsError> {
    let Value::Array(a) = &obj.value else {
        return Err(PsError::Typecheck);
    };
    if a.len() != 6 {
        return Err(PsError::Rangecheck);
    }
    let mut m = [0f32; 6];
    for (i, slot) in m.iter_mut().enumerate() {
        *slot = match a.get(i).as_ref().map(|o| &o.value) {
            Some(Value::Integer(n)) => *n as f32,
            Some(Value::Real(r)) => *r as f32,
            _ => return Err(PsError::Typecheck),
        };
    }
    Ok(Transform::from_row(m[0], m[1], m[2], m[3], m[4], m[5]))
}

fn write_matrix(obj: &Object, t: Transform) -> Result<(), PsError> {
    let Value::Array(a) = &obj.value else {
        return Err(PsError::Typecheck);
    };
    if a.len() != 6 {
        return Err(PsError::Rangecheck);
    }
    for (i, v) in [t.sx, t.ky, t.kx, t.sy, t.tx, t.ty].into_iter().enumerate() {
        a.put(i, Object::real(f64::from(v)))?;
    }
    Ok(())
}

fn matrix(it: &mut Interp) -> Result<(), PsError> {
    let obj = Object::array(vec![
        Object::real(1.0),
        Object::real(0.0),
        Object::real(0.0),
        Object::real(1.0),
        Object::real(0.0),
        Object::real(0.0),
    ]);
    it.push(obj);
    Ok(())
}

fn identmatrix(it: &mut Interp) -> Result<(), PsError> {
    let obj = it.pop()?;
    write_matrix(&obj, Transform::identity())?;
    it.push(obj.clone());
    Ok(())
}

fn defaultmatrix(it: &mut Interp) -> Result<(), PsError> {
    let obj = it.pop()?;
    write_matrix(&obj, it.gfx.base_ctm())?;
    it.push(obj.clone());
    Ok(())
}

fn currentmatrix(it: &mut Interp) -> Result<(), PsError> {
    let obj = it.pop()?;
    write_matrix(&obj, it.gfx.ctm())?;
    it.push(obj.clone());
    Ok(())
}

fn setmatrix(it: &mut Interp) -> Result<(), PsError> {
    let obj = it.pop()?;
    let m = read_matrix(&obj)?;
    it.gfx.set_ctm(m);
    Ok(())
}

fn initmatrix(it: &mut Interp) -> Result<(), PsError> {
    let base = it.gfx.base_ctm();
    it.gfx.set_ctm(base);
    Ok(())
}

fn concat(it: &mut Interp) -> Result<(), PsError> {
    let obj = it.pop()?;
    let m = read_matrix(&obj)?;
    it.gfx.concat_ctm(m);
    Ok(())
}

/// m1 m2 m3 concatmatrix -> m3 = m1 then m2.
fn concatmatrix(it: &mut Interp) -> Result<(), PsError> {
    let m3 = it.pop()?;
    let m2 = read_matrix(&it.pop()?)?;
    let m1 = read_matrix(&it.pop()?)?;
    write_matrix(&m3, m2.pre_concat(m1))?;
    it.push(m3.clone());
    Ok(())
}

fn invertmatrix(it: &mut Interp) -> Result<(), PsError> {
    let m2 = it.pop()?;
    let m1 = read_matrix(&it.pop()?)?;
    let inv = m1.invert().ok_or(PsError::UndefinedResult)?;
    write_matrix(&m2, inv)?;
    it.push(m2.clone());
    Ok(())
}

fn apply(t: Transform, x: f64, y: f64, distance: bool) -> (f64, f64) {
    let (x, y) = (x as f32, y as f32);
    let (tx, ty) = if distance { (0.0, 0.0) } else { (t.tx, t.ty) };
    (
        f64::from(t.sx * x + t.kx * y + tx),
        f64::from(t.ky * x + t.sy * y + ty),
    )
}

/// The transform quartet: optional matrix operand on top, else the CTM.
fn transform_common(it: &mut Interp, invert: bool, distance: bool) -> Result<(), PsError> {
    let top = it.pop()?;
    let (m, y) = match &top.value {
        Value::Array(_) => (read_matrix(&top)?, it.pop_f64()?),
        Value::Integer(n) => (it.gfx.ctm(), *n as f64),
        Value::Real(r) => (it.gfx.ctm(), *r),
        _ => return Err(PsError::Typecheck),
    };
    let x = it.pop_f64()?;
    let m = if invert {
        m.invert().ok_or(PsError::UndefinedResult)?
    } else {
        m
    };
    let (nx, ny) = apply(m, x, y, distance);
    it.push(Object::real(nx));
    it.push(Object::real(ny));
    Ok(())
}

fn transform(it: &mut Interp) -> Result<(), PsError> {
    transform_common(it, false, false)
}

fn itransform(it: &mut Interp) -> Result<(), PsError> {
    transform_common(it, true, false)
}

fn dtransform(it: &mut Interp) -> Result<(), PsError> {
    transform_common(it, false, true)
}

fn idtransform(it: &mut Interp) -> Result<(), PsError> {
    transform_common(it, true, true)
}

/// tx ty translate | tx ty matrix translate — and likewise for scale
/// and rotate. With a matrix operand the CTM is untouched.
fn two_arg_matrix_form(it: &mut Interp, build: fn(f64, f64) -> Transform) -> Result<(), PsError> {
    let top = it.pop()?;
    match &top.value {
        Value::Array(_) => {
            let b = it.pop_f64()?;
            let a = it.pop_f64()?;
            write_matrix(&top, build(a, b))?;
            it.push(top.clone());
        }
        Value::Integer(n) => {
            let b = *n as f64;
            let a = it.pop_f64()?;
            it.gfx.concat_ctm(build(a, b));
        }
        Value::Real(r) => {
            let b = *r;
            let a = it.pop_f64()?;
            it.gfx.concat_ctm(build(a, b));
        }
        _ => return Err(PsError::Typecheck),
    }
    Ok(())
}

fn translate(it: &mut Interp) -> Result<(), PsError> {
    two_arg_matrix_form(it, |tx, ty| Transform::from_translate(tx as f32, ty as f32))
}

fn scale(it: &mut Interp) -> Result<(), PsError> {
    two_arg_matrix_form(it, |sx, sy| Transform::from_scale(sx as f32, sy as f32))
}

fn rotate(it: &mut Interp) -> Result<(), PsError> {
    let top = it.pop()?;
    match &top.value {
        Value::Array(_) => {
            let angle = it.pop_f64()?;
            write_matrix(&top, Transform::from_rotate(angle as f32))?;
            it.push(top.clone());
        }
        Value::Integer(_) | Value::Real(_) => {
            let angle = match top.value {
                Value::Integer(n) => n as f64,
                Value::Real(r) => r,
                _ => unreachable!("matched numeric above"),
            };
            it.gfx.concat_ctm(Transform::from_rotate(angle as f32));
        }
        _ => return Err(PsError::Typecheck),
    }
    Ok(())
}
