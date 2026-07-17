//! Arithmetic and math operators.
//!
//! Type discipline follows the PLRM: integer ⊕ integer stays integer
//! unless the result overflows (then it becomes a real); any real operand
//! makes the result real; `div` always produces a real; `idiv`/`mod` are
//! integer-only. Trig operates in degrees.

use crate::error::PsError;
use crate::interp::Interp;
use crate::object::{Dict, Num, Object};

pub fn install(dict: &mut Dict) {
    use super::op;
    op(dict, "add", add);
    op(dict, "sub", sub);
    op(dict, "mul", mul);
    op(dict, "div", div);
    op(dict, "idiv", idiv);
    op(dict, "mod", ps_mod);
    op(dict, "neg", neg);
    // Not PLRM operators, but gs defines them (min/max in its init
    // file, .min/.max internally) and found files use all four.
    op(dict, "min", min);
    op(dict, "max", max);
    op(dict, ".min", min);
    op(dict, ".max", max);
    op(dict, "abs", abs);
    op(dict, "ceiling", ceiling);
    op(dict, "floor", floor);
    op(dict, "round", round);
    op(dict, "truncate", truncate);
    op(dict, "sqrt", sqrt);
    op(dict, "sin", sin);
    op(dict, "cos", cos);
    op(dict, "atan", atan);
    op(dict, "exp", exp);
    op(dict, "ln", ln);
    op(dict, "log", log);
    op(dict, "rand", rand);
    op(dict, "srand", srand);
    op(dict, "rrand", rrand);
    op(dict, "bitshift", bitshift);
}

/// The classic LCG; range [0, 2^31), per the PLRM's `rand`.
fn rand(it: &mut Interp) -> Result<(), PsError> {
    it.rand_state = (it
        .rand_state
        .wrapping_mul(1_103_515_245)
        .wrapping_add(12_345))
        & 0x7fff_ffff;
    it.push(Object::int(it.rand_state));
    Ok(())
}

fn srand(it: &mut Interp) -> Result<(), PsError> {
    it.rand_state = it.pop_int()? & 0x7fff_ffff;
    Ok(())
}

fn rrand(it: &mut Interp) -> Result<(), PsError> {
    it.push(Object::int(it.rand_state));
    Ok(())
}

fn bitshift(it: &mut Interp) -> Result<(), PsError> {
    let shift = it.pop_int()?;
    let x = it.pop_int()?;
    let result = if shift >= 64 {
        0
    } else if shift <= -64 {
        x >> 63
    } else if shift >= 0 {
        x.wrapping_shl(shift as u32)
    } else {
        x >> (-shift) as u32
    };
    it.push(Object::int(result));
    Ok(())
}

fn pop2(it: &mut Interp) -> Result<(Num, Num), PsError> {
    let b = it.pop_num()?;
    let a = it.pop_num()?;
    Ok((a, b))
}

/// Integer op that promotes to real on overflow, real op otherwise.
fn binary(
    it: &mut Interp,
    int_op: fn(i64, i64) -> Option<i64>,
    real_op: fn(f64, f64) -> f64,
) -> Result<(), PsError> {
    let (a, b) = pop2(it)?;
    let result = match (a, b) {
        (Num::Int(x), Num::Int(y)) => int_op(x, y)
            .map(Object::int)
            .unwrap_or_else(|| Object::real(real_op(x as f64, y as f64))),
        _ => Object::real(real_op(a.to_f64(), b.to_f64())),
    };
    it.push(result);
    Ok(())
}

fn min(it: &mut Interp) -> Result<(), PsError> {
    binary(it, |x, y| Some(x.min(y)), f64::min)
}

fn max(it: &mut Interp) -> Result<(), PsError> {
    binary(it, |x, y| Some(x.max(y)), f64::max)
}

fn add(it: &mut Interp) -> Result<(), PsError> {
    binary(it, i64::checked_add, |x, y| x + y)
}

fn sub(it: &mut Interp) -> Result<(), PsError> {
    binary(it, i64::checked_sub, |x, y| x - y)
}

fn mul(it: &mut Interp) -> Result<(), PsError> {
    binary(it, i64::checked_mul, |x, y| x * y)
}

fn div(it: &mut Interp) -> Result<(), PsError> {
    let (a, b) = pop2(it)?;
    let d = b.to_f64();
    if d == 0.0 {
        return Err(PsError::UndefinedResult);
    }
    it.push(Object::real(a.to_f64() / d));
    Ok(())
}

fn idiv(it: &mut Interp) -> Result<(), PsError> {
    let b = it.pop_int()?;
    let a = it.pop_int()?;
    if b == 0 {
        return Err(PsError::UndefinedResult);
    }
    // checked_div only fails on i64::MIN / -1, whose true result has no
    // integer representation — undefinedresult, same as the spec's spirit.
    let q = a.checked_div(b).ok_or(PsError::UndefinedResult)?;
    it.push(Object::int(q));
    Ok(())
}

fn ps_mod(it: &mut Interp) -> Result<(), PsError> {
    let b = it.pop_int()?;
    let a = it.pop_int()?;
    if b == 0 {
        return Err(PsError::UndefinedResult);
    }
    // Rust's % matches the PLRM: the result takes the sign of the dividend.
    let r = a.checked_rem(b).ok_or(PsError::UndefinedResult)?;
    it.push(Object::int(r));
    Ok(())
}

fn neg(it: &mut Interp) -> Result<(), PsError> {
    let obj = match it.pop_num()? {
        Num::Int(i) => i
            .checked_neg()
            .map(Object::int)
            .unwrap_or_else(|| Object::real(-(i as f64))),
        Num::Real(r) => Object::real(-r),
    };
    it.push(obj);
    Ok(())
}

fn abs(it: &mut Interp) -> Result<(), PsError> {
    let obj = match it.pop_num()? {
        Num::Int(i) => i
            .checked_abs()
            .map(Object::int)
            .unwrap_or_else(|| Object::real(-(i as f64))),
        Num::Real(r) => Object::real(r.abs()),
    };
    it.push(obj);
    Ok(())
}

/// ceiling/floor/round/truncate return the same type they were given:
/// an integer is already its own rounding.
fn rounding(it: &mut Interp, f: fn(f64) -> f64) -> Result<(), PsError> {
    let obj = match it.pop_num()? {
        Num::Int(i) => Object::int(i),
        Num::Real(r) => Object::real(f(r)),
    };
    it.push(obj);
    Ok(())
}

fn ceiling(it: &mut Interp) -> Result<(), PsError> {
    rounding(it, f64::ceil)
}

fn floor(it: &mut Interp) -> Result<(), PsError> {
    rounding(it, f64::floor)
}

fn truncate(it: &mut Interp) -> Result<(), PsError> {
    rounding(it, f64::trunc)
}

fn round(it: &mut Interp) -> Result<(), PsError> {
    // PLRM: ties round to the *greater* value (-0.5 rounds to 0), which is
    // neither Rust's round (away from zero) nor round_ties_even.
    rounding(it, |x| {
        let f = x.floor();
        if x - f >= 0.5 { f + 1.0 } else { f }
    })
}

fn unary_real(it: &mut Interp, f: impl Fn(f64) -> Result<f64, PsError>) -> Result<(), PsError> {
    let x = it.pop_num()?.to_f64();
    let r = f(x)?;
    it.push(Object::real(r));
    Ok(())
}

fn sqrt(it: &mut Interp) -> Result<(), PsError> {
    unary_real(it, |x| {
        if x < 0.0 {
            Err(PsError::Rangecheck)
        } else {
            Ok(x.sqrt())
        }
    })
}

fn sin(it: &mut Interp) -> Result<(), PsError> {
    unary_real(it, |x| Ok(x.to_radians().sin()))
}

fn cos(it: &mut Interp) -> Result<(), PsError> {
    unary_real(it, |x| Ok(x.to_radians().cos()))
}

fn ln(it: &mut Interp) -> Result<(), PsError> {
    unary_real(it, |x| {
        if x <= 0.0 {
            Err(PsError::Rangecheck)
        } else {
            Ok(x.ln())
        }
    })
}

fn log(it: &mut Interp) -> Result<(), PsError> {
    unary_real(it, |x| {
        if x <= 0.0 {
            Err(PsError::Rangecheck)
        } else {
            Ok(x.log10())
        }
    })
}

fn atan(it: &mut Interp) -> Result<(), PsError> {
    let den = it.pop_num()?.to_f64();
    let num = it.pop_num()?.to_f64();
    if num == 0.0 && den == 0.0 {
        return Err(PsError::UndefinedResult);
    }
    // PLRM specifies degrees in [0, 360); atan2 gives (-180, 180].
    let mut angle = num.atan2(den).to_degrees();
    if angle < 0.0 {
        angle += 360.0;
    }
    it.push(Object::real(angle));
    Ok(())
}

fn exp(it: &mut Interp) -> Result<(), PsError> {
    let exponent = it.pop_num()?.to_f64();
    let base = it.pop_num()?.to_f64();
    let r = base.powf(exponent);
    // NaN here means a negative base with a non-integral exponent.
    if r.is_nan() {
        return Err(PsError::UndefinedResult);
    }
    it.push(Object::real(r));
    Ok(())
}
