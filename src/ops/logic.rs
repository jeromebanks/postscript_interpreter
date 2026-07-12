//! Relational, boolean, and bitwise operators.

use std::cmp::Ordering;
use std::rc::Rc;

use crate::error::PsError;
use crate::interp::Interp;
use crate::object::{Dict, Object, Value};

pub fn install(dict: &mut Dict) {
    use super::op;
    op(dict, "eq", eq);
    op(dict, "ne", ne);
    op(dict, "gt", gt);
    op(dict, "ge", ge);
    op(dict, "lt", lt);
    op(dict, "le", le);
    op(dict, "and", and);
    op(dict, "or", or);
    op(dict, "xor", xor);
    op(dict, "not", not);
}

/// PostScript `eq` semantics: numbers compare numerically across types,
/// strings by content, names and strings with the same text are equal,
/// composites by identity. The literal/executable attribute never
/// matters.
fn objects_equal(a: &Object, b: &Object) -> bool {
    use Value::*;
    match (&a.value, &b.value) {
        (Integer(x), Integer(y)) => x == y,
        (Real(x), Real(y)) => x == y,
        (Integer(x), Real(y)) | (Real(y), Integer(x)) => *x as f64 == *y,
        (Boolean(x), Boolean(y)) => x == y,
        (Mark, Mark) | (Null, Null) => true,
        (Name(x), Name(y)) => x == y,
        (String(x), String(y)) => Rc::ptr_eq(x, y) || *x.borrow() == *y.borrow(),
        (Name(n), String(s)) | (String(s), Name(n)) => n.as_bytes() == &s.borrow()[..],
        (Array(x), Array(y)) => Rc::ptr_eq(x, y),
        (Dict(x), Dict(y)) => Rc::ptr_eq(x, y),
        (Operator(x), Operator(y)) => std::ptr::fn_addr_eq(x.func, y.func),
        _ => false,
    }
}

fn eq(it: &mut Interp) -> Result<(), PsError> {
    let b = it.pop()?;
    let a = it.pop()?;
    it.push(Object::lit(Value::Boolean(objects_equal(&a, &b))));
    Ok(())
}

fn ne(it: &mut Interp) -> Result<(), PsError> {
    let b = it.pop()?;
    let a = it.pop()?;
    it.push(Object::lit(Value::Boolean(!objects_equal(&a, &b))));
    Ok(())
}

/// Ordering is defined for number pairs and string pairs only.
fn compare(it: &mut Interp) -> Result<Ordering, PsError> {
    let b = it.pop()?;
    let a = it.pop()?;
    use Value::*;
    let ord = match (&a.value, &b.value) {
        (Integer(x), Integer(y)) => x.cmp(y),
        (Integer(_) | Real(_), Integer(_) | Real(_)) => {
            let (x, y) = (num_of(&a), num_of(&b));
            x.partial_cmp(&y).ok_or(PsError::UndefinedResult)?
        }
        (String(x), String(y)) => x.borrow().cmp(&y.borrow()),
        _ => return Err(PsError::Typecheck),
    };
    Ok(ord)
}

fn num_of(o: &Object) -> f64 {
    match o.value {
        Value::Integer(i) => i as f64,
        Value::Real(r) => r,
        // compare() only calls this on numeric operands.
        _ => unreachable!("num_of on non-number"),
    }
}

fn gt(it: &mut Interp) -> Result<(), PsError> {
    let ord = compare(it)?;
    it.push(Object::lit(Value::Boolean(ord == Ordering::Greater)));
    Ok(())
}

fn ge(it: &mut Interp) -> Result<(), PsError> {
    let ord = compare(it)?;
    it.push(Object::lit(Value::Boolean(ord != Ordering::Less)));
    Ok(())
}

fn lt(it: &mut Interp) -> Result<(), PsError> {
    let ord = compare(it)?;
    it.push(Object::lit(Value::Boolean(ord == Ordering::Less)));
    Ok(())
}

fn le(it: &mut Interp) -> Result<(), PsError> {
    let ord = compare(it)?;
    it.push(Object::lit(Value::Boolean(ord != Ordering::Greater)));
    Ok(())
}

/// and/or/xor are boolean on booleans, bitwise on integers.
fn bitwise(
    it: &mut Interp,
    bool_op: fn(bool, bool) -> bool,
    int_op: fn(i64, i64) -> i64,
) -> Result<(), PsError> {
    let b = it.pop()?;
    let a = it.pop()?;
    let result = match (&a.value, &b.value) {
        (Value::Boolean(x), Value::Boolean(y)) => Value::Boolean(bool_op(*x, *y)),
        (Value::Integer(x), Value::Integer(y)) => Value::Integer(int_op(*x, *y)),
        _ => return Err(PsError::Typecheck),
    };
    it.push(Object::lit(result));
    Ok(())
}

fn and(it: &mut Interp) -> Result<(), PsError> {
    bitwise(it, |x, y| x && y, |x, y| x & y)
}

fn or(it: &mut Interp) -> Result<(), PsError> {
    bitwise(it, |x, y| x || y, |x, y| x | y)
}

fn xor(it: &mut Interp) -> Result<(), PsError> {
    bitwise(it, |x, y| x ^ y, |x, y| x ^ y)
}

fn not(it: &mut Interp) -> Result<(), PsError> {
    let result = match it.pop()?.value {
        Value::Boolean(b) => Value::Boolean(!b),
        Value::Integer(i) => Value::Integer(!i),
        _ => return Err(PsError::Typecheck),
    };
    it.push(Object::lit(result));
    Ok(())
}
