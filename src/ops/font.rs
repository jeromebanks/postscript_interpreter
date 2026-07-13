//! Font and text operators — thin operand-stack adapters over
//! `crate::font` (the glyph engine) plus the FontDirectory bookkeeping.
//! Architecture and deviations: `FONTS.md`.

use std::cell::RefCell;
use std::rc::Rc;

use crate::encodings;
use crate::error::PsError;
use crate::font::{self, FontState, ShowMode, ShowParams};
use crate::interp::Interp;
use crate::object::{Dict, Object, PsString, Value};

pub fn install(dict: &mut Dict) {
    use super::op;
    op(dict, "findfont", findfont);
    op(dict, "scalefont", scalefont);
    op(dict, "makefont", makefont);
    op(dict, "setfont", setfont);
    op(dict, "currentfont", currentfont);
    op(dict, "definefont", definefont);
    op(dict, "selectfont", selectfont);
    op(dict, "show", show);
    op(dict, "ashow", ashow);
    op(dict, "widthshow", widthshow);
    op(dict, "awidthshow", awidthshow);
    op(dict, "stringwidth", stringwidth);
    op(dict, "charpath", charpath);

    // The shared font-registration dict, and the two standard encoding
    // vectors, as ordinary systemdict values. Font dicts get their own
    // Encoding copies; these arrays are the reference instances programs
    // read (`ISOLatin1Encoding 233 get`) or install wholesale.
    dict.put(
        "FontDirectory".into(),
        Object::lit(Value::Dict(Rc::new(RefCell::new(Dict::new())))),
    );
    dict.put(
        "StandardEncoding".into(),
        encoding_array(encodings::standard_encoding()),
    );
    dict.put(
        "ISOLatin1Encoding".into(),
        encoding_array(encodings::iso_latin1_encoding()),
    );
}

fn encoding_array(names: [&'static str; 256]) -> Object {
    Object::array(names.into_iter().map(Object::name).collect())
}

fn font_directory(it: &Interp) -> Result<Rc<RefCell<Dict>>, PsError> {
    match it.load("FontDirectory").as_ref().map(|o| &o.value) {
        Some(Value::Dict(d)) => Ok(d.clone()),
        // Only reachable if a program clobbers the name with a non-dict.
        _ => Err(PsError::InvalidFont),
    }
}

fn pop_font_dict(it: &mut Interp) -> Result<Rc<RefCell<Dict>>, PsError> {
    match &it.pop()?.value {
        Value::Dict(d) => Ok(d.clone()),
        _ => Err(PsError::Typecheck),
    }
}

fn pop_string(it: &mut Interp) -> Result<PsString, PsError> {
    match &it.pop()?.value {
        Value::String(s) => Ok(s.clone()),
        _ => Err(PsError::Typecheck),
    }
}

/// key findfont → font dict. Registered fonts come back identically;
/// built-in names materialize on first use; unknown names substitute
/// the default face rather than erroring, Ghostscript-style, so found
/// files keep running (documented deviation).
fn findfont(it: &mut Interp) -> Result<(), PsError> {
    let key = it.pop()?;
    let dir = font_directory(it)?;
    let existing = dir.borrow().get_obj(&key)?;
    let dict = match existing.as_ref().map(|o| &o.value) {
        Some(Value::Dict(d)) => d.clone(),
        _ => {
            let requested = match &key.value {
                Value::Name(n) => n.to_string(),
                Value::String(s) => s.text(),
                _ => String::new(),
            };
            let fid = font::builtin_index(&requested).unwrap_or(font::SUBSTITUTE);
            let d = font::build_builtin_dict(fid)?;
            dir.borrow_mut()
                .put_obj(&key, Object::lit(Value::Dict(d.clone())))?;
            d
        }
    };
    it.push(Object::lit(Value::Dict(dict)));
    Ok(())
}

/// key font definefont → font. Installs into FontDirectory; gives dicts
/// that lack one an FID (the seam to the outline registry — see FONTS.md).
fn definefont(it: &mut Interp) -> Result<(), PsError> {
    let font_obj = it.pop()?;
    let key = it.pop()?;
    let Value::Dict(d) = &font_obj.value else {
        return Err(PsError::Typecheck);
    };
    let needs_fid = d.borrow().get("FID").is_none();
    if needs_fid {
        let font_type = match d.borrow().get("FontType").as_ref().map(|o| &o.value) {
            Some(Value::Integer(t)) => Some(*t),
            _ => None,
        };
        // Type 3 dicts are accepted and stored, but have no outline
        // source until BuildChar execution lands (Stage 6 task 5);
        // anything else gets the substitute face so it still renders.
        let fid = if font_type == Some(3) {
            font::FID_NONE
        } else {
            font::SUBSTITUTE
        };
        d.borrow_mut().put("FID".into(), Object::int(fid));
    }
    let dir = font_directory(it)?;
    dir.borrow_mut().put_obj(&key, font_obj.clone())?;
    it.push(font_obj);
    Ok(())
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

fn font_matrix_of(d: &Rc<RefCell<Dict>>) -> Result<[f64; 6], PsError> {
    let obj = d.borrow().get("FontMatrix").ok_or(PsError::InvalidFont)?;
    read_matrix6(&obj)
}

/// Shallow-copy a font dict and give it the composed FontMatrix — the
/// shared engine of scalefont/makefont. Entries (Encoding included) are
/// shared with the original, per the PLRM's copy semantics.
fn derive_font(d: &Rc<RefCell<Dict>>, m: [f64; 6]) -> Result<Rc<RefCell<Dict>>, PsError> {
    let fm = font_matrix_of(d)?;
    let composed = font::compose(fm, m);
    let mut copy = Dict::new();
    for (k, v) in d.borrow().pairs() {
        copy.put_obj(&k, v)?;
    }
    copy.put(
        "FontMatrix".into(),
        Object::array(composed.into_iter().map(Object::real).collect()),
    );
    Ok(Rc::new(RefCell::new(copy)))
}

/// font scale scalefont → font' — sugar for makefont with a uniform
/// scale matrix.
fn scalefont(it: &mut Interp) -> Result<(), PsError> {
    let s = it.pop_f64()?;
    let d = pop_font_dict(it)?;
    let derived = derive_font(&d, [s, 0.0, 0.0, s, 0.0, 0.0])?;
    it.push(Object::lit(Value::Dict(derived)));
    Ok(())
}

fn makefont(it: &mut Interp) -> Result<(), PsError> {
    let m = read_matrix6(&it.pop()?)?;
    let d = pop_font_dict(it)?;
    let derived = derive_font(&d, m)?;
    it.push(Object::lit(Value::Dict(derived)));
    Ok(())
}

fn font_state_of(d: Rc<RefCell<Dict>>) -> Result<FontState, PsError> {
    let fid = match d.borrow().get("FID").as_ref().map(|o| &o.value) {
        Some(Value::Integer(i)) => *i,
        // A dict never passed through findfont/definefont isn't a font.
        _ => return Err(PsError::InvalidFont),
    };
    let matrix = font_matrix_of(&d)?;
    Ok(FontState {
        dict: d,
        fid,
        matrix,
    })
}

fn setfont(it: &mut Interp) -> Result<(), PsError> {
    let d = pop_font_dict(it)?;
    let fs = font_state_of(d)?;
    it.gfx.set_font(fs);
    Ok(())
}

fn currentfont(it: &mut Interp) -> Result<(), PsError> {
    let fs = it.gfx.state().font.clone().ok_or(PsError::InvalidFont)?;
    it.push(Object::lit(Value::Dict(fs.dict)));
    Ok(())
}

/// key scale|matrix selectfont — Level 2 shorthand for
/// findfont scalefont/makefont setfont, and how most found files set
/// their type.
fn selectfont(it: &mut Interp) -> Result<(), PsError> {
    let top = it.pop()?;
    let m = match &top.value {
        Value::Array(_) => read_matrix6(&top)?,
        Value::Integer(n) => {
            let s = *n as f64;
            [s, 0.0, 0.0, s, 0.0, 0.0]
        }
        Value::Real(s) => [*s, 0.0, 0.0, *s, 0.0, 0.0],
        _ => return Err(PsError::Typecheck),
    };
    // findfont on the key, reusing its registry/substitution logic.
    findfont(it).and_then(|()| {
        let d = pop_font_dict(it)?;
        let derived = derive_font(&d, m)?;
        it.gfx.set_font(font_state_of(derived)?);
        Ok(())
    })
}

fn show(it: &mut Interp) -> Result<(), PsError> {
    let s = pop_string(it)?;
    font::run_show(
        &mut it.gfx,
        &s.to_vec(),
        &ShowParams::default(),
        ShowMode::Paint,
    )?;
    Ok(())
}

/// ax ay string ashow — per-glyph extra advance.
fn ashow(it: &mut Interp) -> Result<(), PsError> {
    let s = pop_string(it)?;
    let ay = it.pop_f64()?;
    let ax = it.pop_f64()?;
    let params = ShowParams {
        extra: (ax, ay),
        char_extra: None,
    };
    font::run_show(&mut it.gfx, &s.to_vec(), &params, ShowMode::Paint)?;
    Ok(())
}

fn pop_char_code(it: &mut Interp) -> Result<u8, PsError> {
    let c = it.pop_int()?;
    u8::try_from(c).map_err(|_| PsError::Rangecheck)
}

/// cx cy char string widthshow — extra advance on one byte (classically,
/// justifying by widening spaces).
fn widthshow(it: &mut Interp) -> Result<(), PsError> {
    let s = pop_string(it)?;
    let ch = pop_char_code(it)?;
    let cy = it.pop_f64()?;
    let cx = it.pop_f64()?;
    let params = ShowParams {
        extra: (0.0, 0.0),
        char_extra: Some((ch, (cx, cy))),
    };
    font::run_show(&mut it.gfx, &s.to_vec(), &params, ShowMode::Paint)?;
    Ok(())
}

/// cx cy char ax ay string awidthshow — both at once.
fn awidthshow(it: &mut Interp) -> Result<(), PsError> {
    let s = pop_string(it)?;
    let ay = it.pop_f64()?;
    let ax = it.pop_f64()?;
    let ch = pop_char_code(it)?;
    let cy = it.pop_f64()?;
    let cx = it.pop_f64()?;
    let params = ShowParams {
        extra: (ax, ay),
        char_extra: Some((ch, (cx, cy))),
    };
    font::run_show(&mut it.gfx, &s.to_vec(), &params, ShowMode::Paint)?;
    Ok(())
}

fn stringwidth(it: &mut Interp) -> Result<(), PsError> {
    let s = pop_string(it)?;
    let (wx, wy) = font::run_show(
        &mut it.gfx,
        &s.to_vec(),
        &ShowParams::default(),
        ShowMode::Width,
    )?;
    it.push(Object::real(wx));
    it.push(Object::real(wy));
    Ok(())
}

/// string bool charpath — append the text's outlines to the current
/// path. The bool selects stroke-path vs fill-path semantics in real
/// PostScript; outline fonts have only one useful form, so both operands
/// produce it (documented in FONTS.md).
fn charpath(it: &mut Interp) -> Result<(), PsError> {
    let Value::Boolean(_) = it.pop()?.value else {
        return Err(PsError::Typecheck);
    };
    let s = pop_string(it)?;
    font::run_show(
        &mut it.gfx,
        &s.to_vec(),
        &ShowParams::default(),
        ShowMode::Charpath,
    )?;
    Ok(())
}
