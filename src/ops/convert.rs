//! Type conversion and inspection operators.

use crate::error::PsError;
use crate::interp::Interp;
use crate::lexer::{Token, parse_number};
use crate::object::{Dict, Num, Object, Value};

pub fn install(dict: &mut Dict) {
    use super::op;
    op(dict, "cvi", cvi);
    op(dict, "cvr", cvr);
    op(dict, "cvn", cvn);
    op(dict, "cvs", cvs);
    op(dict, "cvx", cvx);
    op(dict, "cvlit", cvlit);
    op(dict, "type", type_op);
    op(dict, "xcheck", xcheck);
    op(dict, "cvrs", cvrs);
}

fn real_to_int(r: f64) -> Result<i64, PsError> {
    let t = r.trunc();
    if !t.is_finite() || t < i64::MIN as f64 || t > i64::MAX as f64 {
        return Err(PsError::Rangecheck);
    }
    Ok(t as i64)
}

/// Number syntax in strings is the scanner's — radix forms included.
fn parse_string_number(obj: &Object) -> Result<Num, PsError> {
    let Value::String(s) = &obj.value else {
        return Err(PsError::Typecheck);
    };
    let text = s.text();
    match parse_number(text.trim()) {
        Some(Token::Integer(i)) => Ok(Num::Int(i)),
        Some(Token::Real(r)) => Ok(Num::Real(r)),
        _ => Err(PsError::Syntax(format!("not a number: {text}"))),
    }
}

fn cvi(it: &mut Interp) -> Result<(), PsError> {
    let obj = it.pop()?;
    let n = match obj.value {
        Value::Integer(i) => i,
        Value::Real(r) => real_to_int(r)?,
        Value::String(_) => match parse_string_number(&obj)? {
            Num::Int(i) => i,
            Num::Real(r) => real_to_int(r)?,
        },
        _ => return Err(PsError::Typecheck),
    };
    it.push(Object::int(n));
    Ok(())
}

fn cvr(it: &mut Interp) -> Result<(), PsError> {
    let obj = it.pop()?;
    let r = match obj.value {
        Value::Integer(i) => i as f64,
        Value::Real(r) => r,
        Value::String(_) => parse_string_number(&obj)?.to_f64(),
        _ => return Err(PsError::Typecheck),
    };
    it.push(Object::real(r));
    Ok(())
}

fn cvn(it: &mut Interp) -> Result<(), PsError> {
    let obj = it.pop()?;
    let Value::String(s) = &obj.value else {
        return Err(PsError::Typecheck);
    };
    it.push(Object {
        value: Value::Name(s.text().into()),
        executable: obj.executable,
    });
    Ok(())
}

/// any buffer cvs -> the prefix of buffer holding any's text — a view,
/// which is why `getinterval` semantics had to come first.
fn cvs(it: &mut Interp) -> Result<(), PsError> {
    let buf_obj = it.pop()?;
    let Value::String(buf) = &buf_obj.value else {
        return Err(PsError::Typecheck);
    };
    let any = it.pop()?;
    let text = any.text();
    buf.write_at(0, text.as_bytes())?;
    it.push(Object::lit(Value::String(buf.interval(0, text.len())?)));
    Ok(())
}

/// num radix buffer cvrs -> radix representation (uppercase digits, no
/// sign handling beyond two's-complement, per the PLRM).
fn cvrs(it: &mut Interp) -> Result<(), PsError> {
    let buf_obj = it.pop()?;
    let Value::String(buf) = &buf_obj.value else {
        return Err(PsError::Typecheck);
    };
    let radix = it.pop_int()?;
    if !(2..=36).contains(&radix) {
        return Err(PsError::Rangecheck);
    }
    let n = match it.pop_num()? {
        Num::Int(i) => i,
        Num::Real(r) => real_to_int(r)?,
    };
    let text = if radix == 10 {
        n.to_string()
    } else {
        // Non-decimal radices interpret the value as unsigned 32-bit,
        // matching the PLRM's two's-complement behavior.
        to_radix(n as u32 as u64, radix as u64)
    };
    buf.write_at(0, text.as_bytes())?;
    it.push(Object::lit(Value::String(buf.interval(0, text.len())?)));
    Ok(())
}

fn to_radix(mut n: u64, radix: u64) -> String {
    const DIGITS: &[u8; 36] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ";
    if n == 0 {
        return "0".to_string();
    }
    let mut out = Vec::new();
    while n > 0 {
        out.push(DIGITS[(n % radix) as usize]);
        n /= radix;
    }
    out.reverse();
    String::from_utf8_lossy(&out).into_owned()
}

fn cvx(it: &mut Interp) -> Result<(), PsError> {
    let mut obj = it.pop()?;
    obj.executable = true;
    it.push(obj);
    Ok(())
}

fn cvlit(it: &mut Interp) -> Result<(), PsError> {
    let mut obj = it.pop()?;
    obj.executable = false;
    it.push(obj);
    Ok(())
}

fn type_op(it: &mut Interp) -> Result<(), PsError> {
    let obj = it.pop()?;
    // The PLRM specifies the returned name is executable.
    it.push(Object::exec(Value::Name(obj.type_name().into())));
    Ok(())
}

fn xcheck(it: &mut Interp) -> Result<(), PsError> {
    let obj = it.pop()?;
    it.push(Object::bool(obj.executable));
    Ok(())
}
