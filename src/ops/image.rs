//! image / imagemask / colorimage operand adapters, plus the minimal
//! color-space bookkeeping the Level 2 dict form needs. The engine is
//! `crate::image`.

use crate::error::PsError;
use crate::image::{ImageCtx, ImageSource};
use crate::interp::Interp;
use crate::object::{Dict, Object, Value};

pub fn install(dict: &mut Dict) {
    use super::op;
    op(dict, "image", image);
    op(dict, "imagemask", imagemask);
    op(dict, "colorimage", colorimage);
    op(dict, "setcolorspace", setcolorspace);
    op(dict, "currentcolorspace", currentcolorspace);
}

fn read_matrix6(obj: &Object) -> Result<[f64; 6], PsError> {
    let Value::Array(a) = &obj.value else {
        return Err(PsError::Typecheck);
    };
    if a.len() != 6 {
        return Err(PsError::Rangecheck);
    }
    let mut m = [0.0; 6];
    for (i, slot) in m.iter_mut().enumerate() {
        *slot = match a.get(i).as_ref().map(|o| &o.value) {
            Some(Value::Integer(n)) => *n as f64,
            Some(Value::Real(r)) => *r,
            _ => return Err(PsError::Typecheck),
        };
    }
    Ok(m)
}

fn data_source(obj: &Object) -> Result<ImageSource, PsError> {
    Ok(match &obj.value {
        Value::Array(_) if obj.executable => ImageSource::Proc(obj.clone()),
        Value::File(f) => ImageSource::File(f.clone()),
        Value::String(s) => ImageSource::Bytes(s.to_vec()),
        _ => return Err(PsError::Typecheck),
    })
}

fn pop_dims(it: &mut Interp) -> Result<(usize, usize), PsError> {
    let h = it.pop_int()?;
    let w = it.pop_int()?;
    if w < 0 || h < 0 {
        return Err(PsError::Rangecheck);
    }
    Ok((w as usize, h as usize))
}

fn default_decode(ncomp: u32) -> Vec<f32> {
    let mut d = Vec::with_capacity(ncomp as usize * 2);
    for _ in 0..ncomp {
        d.extend([0.0, 1.0]);
    }
    d
}

/// Level 1: width height bits matrix datasrc image (DeviceGray), or
/// Level 2: dict image (components from the current color space).
fn image(it: &mut Interp) -> Result<(), PsError> {
    let top = it.pop()?;
    if let Value::Dict(d) = &top.value {
        let ncomp = it.gfx.colorspace_ncomp();
        return dict_image(it, d.clone(), ncomp, None);
    }
    let source = data_source(&top)?;
    let matrix = read_matrix6(&it.pop()?)?;
    let bits = it.pop_int()?;
    let (w, h) = pop_dims(it)?;
    if !matches!(bits, 1 | 2 | 4 | 8) {
        return Err(PsError::Rangecheck);
    }
    it.begin_image(ImageCtx::new(
        w,
        h,
        bits as u32,
        1,
        default_decode(1),
        matrix,
        None,
        source,
    ))
}

/// width height polarity matrix datasrc imagemask, or dict imagemask.
/// Paints the current color where the (1-bit) sample matches polarity —
/// true ≡ Decode [1 0] ≡ paint the 1s (pinned against gs).
fn imagemask(it: &mut Interp) -> Result<(), PsError> {
    let top = it.pop()?;
    if let Value::Dict(d) = &top.value {
        return dict_image(it, d.clone(), 1, Some(true));
    }
    let source = data_source(&top)?;
    let matrix = read_matrix6(&it.pop()?)?;
    let Value::Boolean(polarity) = it.pop()?.value else {
        return Err(PsError::Typecheck);
    };
    let (w, h) = pop_dims(it)?;
    it.begin_image(ImageCtx::new(
        w,
        h,
        1,
        1,
        default_decode(1),
        matrix,
        Some(polarity),
        source,
    ))
}

/// width height bits matrix datasrc(s) multi ncomp colorimage.
/// Only the single interleaved source form (multi = false); separate
/// per-component sources are a documented gap.
fn colorimage(it: &mut Interp) -> Result<(), PsError> {
    let ncomp = it.pop_int()?;
    let Value::Boolean(multi) = it.pop()?.value else {
        return Err(PsError::Typecheck);
    };
    if multi {
        return Err(PsError::Limitcheck);
    }
    if !matches!(ncomp, 1 | 3 | 4) {
        return Err(PsError::Rangecheck);
    }
    let source = data_source(&it.pop()?)?;
    let matrix = read_matrix6(&it.pop()?)?;
    let bits = it.pop_int()?;
    let (w, h) = pop_dims(it)?;
    if !matches!(bits, 1 | 2 | 4 | 8) {
        return Err(PsError::Rangecheck);
    }
    it.begin_image(ImageCtx::new(
        w,
        h,
        bits as u32,
        ncomp as u32,
        default_decode(ncomp as u32),
        matrix,
        None,
        source,
    ))
}

/// The Level 2 dict form, shared by image and imagemask.
fn dict_image(
    it: &mut Interp,
    d: std::rc::Rc<std::cell::RefCell<Dict>>,
    ncomp: u32,
    mask: Option<bool>,
) -> Result<(), PsError> {
    let d = d.borrow();
    let geti = |k: &str| -> Result<i64, PsError> {
        match d.get(k).as_ref().map(|o| &o.value) {
            Some(Value::Integer(n)) => Ok(*n),
            _ => Err(PsError::Typecheck),
        }
    };
    let w = geti("Width")?;
    let h = geti("Height")?;
    if w < 0 || h < 0 {
        return Err(PsError::Rangecheck);
    }
    let bits = if mask.is_some() {
        1
    } else {
        geti("BitsPerComponent")?
    };
    if !matches!(bits, 1 | 2 | 4 | 8) {
        return Err(PsError::Rangecheck);
    }
    let matrix = read_matrix6(&d.get("ImageMatrix").ok_or(PsError::Typecheck)?)?;
    let source = data_source(&d.get("DataSource").ok_or(PsError::Typecheck)?)?;
    let mut decode = default_decode(ncomp);
    if let Some(Value::Array(a)) = d.get("Decode").as_ref().map(|o| &o.value) {
        decode.clear();
        for i in 0..a.len() {
            match a.get(i).as_ref().map(|o| &o.value) {
                Some(Value::Integer(n)) => decode.push(*n as f32),
                Some(Value::Real(r)) => decode.push(*r as f32),
                _ => return Err(PsError::Typecheck),
            }
        }
    }
    // Mask polarity from Decode: [1 0] paints the 1s (≡ operand true);
    // the default [0 1] paints the 0s.
    let mask = mask.map(|_| decode.first().copied().unwrap_or(0.0) == 1.0);
    it.begin_image(ImageCtx::new(
        w as usize,
        h as usize,
        bits as u32,
        ncomp,
        decode,
        matrix,
        mask,
        source,
    ))
}

/// Minimal Level 2 color-space selection: the three device spaces,
/// name or one-element-array form. Images in dict form need this to
/// know their component count.
fn setcolorspace(it: &mut Interp) -> Result<(), PsError> {
    let obj = it.pop()?;
    let name = match &obj.value {
        Value::Name(n) => n.clone(),
        Value::Array(a) => match a.get(0).as_ref().map(|o| &o.value) {
            Some(Value::Name(n)) => n.clone(),
            _ => return Err(PsError::Typecheck),
        },
        _ => return Err(PsError::Typecheck),
    };
    let ncomp = match &*name {
        "DeviceGray" => 1,
        "DeviceRGB" => 3,
        "DeviceCMYK" => 4,
        _ => return Err(PsError::Undefined(name.to_string())),
    };
    it.gfx.set_colorspace(ncomp);
    Ok(())
}

fn currentcolorspace(it: &mut Interp) -> Result<(), PsError> {
    let name = match it.gfx.colorspace_ncomp() {
        3 => "DeviceRGB",
        4 => "DeviceCMYK",
        _ => "DeviceGray",
    };
    it.push(Object::array(vec![Object::name(name)]));
    Ok(())
}
