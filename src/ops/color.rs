//! Color spaces beyond the implicit device three — Stage 8 task 4.
//!
//! `setcolorspace` accepts the device names plus the two array forms
//! found files actually use: `[/Indexed base hival lookup]` with a
//! string lookup table, and `[/Separation name alt tintProc]`. The
//! tint transform is a PostScript procedure; it runs through the
//! machine as a `Frame::PostOp` continuation (`interp.rs`), never by
//! recursing into the interpreter.
//!
//! Documented gaps: Indexed lookup *procedures* (strings are what
//! Photoshop-era EPS emits) and nested Indexed/Separation bases are
//! typecheck errors; CIE-based spaces are undefined.

use std::rc::Rc;

use crate::error::PsError;
use crate::gfx::{ColorSpace, IndexedTable};
use crate::interp::{Interp, PostOp};
use crate::object::{Dict, Object, Value};

pub fn install(dict: &mut Dict) {
    use super::op;
    op(dict, "setcolorspace", setcolorspace);
    op(dict, "currentcolorspace", currentcolorspace);
    op(dict, "setcolor", setcolor);
}

fn device_space(name: &str) -> Option<ColorSpace> {
    match name {
        "DeviceGray" => Some(ColorSpace::Gray),
        "DeviceRGB" => Some(ColorSpace::Rgb),
        "DeviceCMYK" => Some(ColorSpace::Cmyk),
        _ => None,
    }
}

/// A base-space operand for Indexed/Separation: a device-space name,
/// or a one-element array wrapping one.
fn base_ncomp(obj: &Object) -> Result<u32, PsError> {
    let name = match &obj.value {
        Value::Name(n) => n.clone(),
        Value::Array(a) => match a.get(0).as_ref().map(|o| &o.value) {
            Some(Value::Name(n)) => n.clone(),
            _ => return Err(PsError::Typecheck),
        },
        _ => return Err(PsError::Typecheck),
    };
    device_space(&name)
        .map(|cs| cs.ncomp())
        .ok_or(PsError::Typecheck)
}

fn setcolorspace(it: &mut Interp) -> Result<(), PsError> {
    let obj = it.pop()?;
    match &obj.value {
        Value::Name(n) => {
            let cs = device_space(n).ok_or_else(|| PsError::Undefined(n.to_string()))?;
            it.gfx.set_colorspace(cs);
            Ok(())
        }
        Value::Array(a) => {
            let family = match a.get(0).as_ref().map(|o| &o.value) {
                Some(Value::Name(n)) => n.clone(),
                _ => return Err(PsError::Typecheck),
            };
            match &*family {
                "DeviceGray" | "DeviceRGB" | "DeviceCMYK" => {
                    it.gfx
                        .set_colorspace(device_space(&family).expect("matched above"));
                    Ok(())
                }
                "Indexed" => {
                    if a.len() != 4 {
                        return Err(PsError::Rangecheck);
                    }
                    let base = base_ncomp(&a.get(1).expect("len checked"))?;
                    let hival = match a.get(2).as_ref().map(|o| &o.value) {
                        Some(Value::Integer(n)) if (0..=4095).contains(n) => *n as u32,
                        Some(Value::Integer(_)) => return Err(PsError::Rangecheck),
                        _ => return Err(PsError::Typecheck),
                    };
                    let lookup = match a.get(3).as_ref().map(|o| &o.value) {
                        Some(Value::String(s)) => s.to_vec(),
                        // Procedure lookups: documented gap (module doc).
                        _ => return Err(PsError::Typecheck),
                    };
                    if lookup.len() < ((hival + 1) * base) as usize {
                        return Err(PsError::Rangecheck);
                    }
                    let table = Rc::new(IndexedTable {
                        base_ncomp: base,
                        hival,
                        lookup,
                    });
                    // Initial color for Indexed is index 0, per the PLRM.
                    let (r, g, b) = table.rgb_at(0);
                    it.gfx.set_rgb(f64::from(r), f64::from(g), f64::from(b));
                    it.gfx
                        .set_colorspace(ColorSpace::Indexed { table, src: obj });
                    Ok(())
                }
                "Separation" => {
                    if a.len() != 4 {
                        return Err(PsError::Rangecheck);
                    }
                    // a[1] is the colorant name (name or string) — kept
                    // only inside `src` for currentcolorspace.
                    let alt = base_ncomp(&a.get(2).expect("len checked"))?;
                    let proc = a.get(3).expect("len checked");
                    if !proc.executable || !matches!(proc.value, Value::Array(_)) {
                        return Err(PsError::Typecheck);
                    }
                    it.gfx.set_colorspace(ColorSpace::Separation {
                        alt_ncomp: alt,
                        tint_proc: proc.clone(),
                        src: obj.clone(),
                    });
                    // Initial color is tint 1.0 (full colorant), per the
                    // PLRM — run the transform like any setcolor.
                    it.begin_postop(
                        PostOp::SeparationColor { alt_ncomp: alt },
                        vec![Object::real(1.0)],
                        &proc,
                    )
                }
                _ => Err(PsError::Undefined(family.to_string())),
            }
        }
        _ => Err(PsError::Typecheck),
    }
}

fn currentcolorspace(it: &mut Interp) -> Result<(), PsError> {
    let obj = match it.gfx.colorspace() {
        ColorSpace::Gray => Object::array(vec![Object::name("DeviceGray")]),
        ColorSpace::Rgb => Object::array(vec![Object::name("DeviceRGB")]),
        ColorSpace::Cmyk => Object::array(vec![Object::name("DeviceCMYK")]),
        ColorSpace::Indexed { src, .. } | ColorSpace::Separation { src, .. } => src.clone(),
    };
    it.push(obj);
    Ok(())
}

/// Component count of the current space's *setcolor operands* (for
/// Indexed that's one index; for Separation one tint).
fn setcolor(it: &mut Interp) -> Result<(), PsError> {
    match it.gfx.colorspace().clone() {
        ColorSpace::Gray => {
            let g = it.pop_f64()?;
            it.gfx.set_rgb(g, g, g);
            Ok(())
        }
        ColorSpace::Rgb => {
            let b = it.pop_f64()?;
            let g = it.pop_f64()?;
            let r = it.pop_f64()?;
            it.gfx.set_rgb(r, g, b);
            Ok(())
        }
        ColorSpace::Cmyk => {
            let k = it.pop_f64()?;
            let y = it.pop_f64()?;
            let m = it.pop_f64()?;
            let c = it.pop_f64()?;
            it.gfx.set_cmyk(c, m, y, k);
            Ok(())
        }
        ColorSpace::Indexed { table, .. } => {
            // Out-of-range indices clamp, pinned against gs (the PLRM
            // says rangecheck; gs renders — so do we).
            let idx = it.pop_int()?.clamp(0, i64::from(table.hival));
            let (r, g, b) = table.rgb_at(idx as usize);
            it.gfx.set_rgb(f64::from(r), f64::from(g), f64::from(b));
            Ok(())
        }
        ColorSpace::Separation {
            alt_ncomp,
            tint_proc,
            ..
        } => {
            let tint = it.pop_f64()?.clamp(0.0, 1.0);
            it.begin_postop(
                PostOp::SeparationColor { alt_ncomp },
                vec![Object::real(tint)],
                &tint_proc,
            )
        }
    }
}
