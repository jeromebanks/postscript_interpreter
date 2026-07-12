//! Dictionary and definition operators, plus `bind`.

use std::cell::RefCell;
use std::rc::Rc;

use crate::error::PsError;
use crate::interp::Interp;
use crate::object::{Dict, Object, Value};

pub fn install(dict: &mut Dict) {
    use super::op;
    op(dict, "def", def);
    op(dict, "dict", make_dict);
    op(dict, "begin", begin);
    op(dict, "end", end);
    op(dict, "load", load);
    op(dict, "bind", bind);
}

/// Keys are names; strings convert to names per the PLRM. Other key
/// types await the dict-key generalization noted in `object.rs`.
fn pop_key(it: &mut Interp) -> Result<Rc<str>, PsError> {
    match it.pop()?.value {
        Value::Name(n) => Ok(n),
        Value::String(s) => Ok(String::from_utf8_lossy(&s.borrow()).into_owned().into()),
        _ => Err(PsError::Typecheck),
    }
}

fn def(it: &mut Interp) -> Result<(), PsError> {
    let value = it.pop()?;
    let key = pop_key(it)?;
    it.define(&key, value);
    Ok(())
}

fn make_dict(it: &mut Interp) -> Result<(), PsError> {
    let capacity = it.pop_int()?;
    if capacity < 0 {
        return Err(PsError::Rangecheck);
    }
    // Capacity is a Level 1 fiction we don't enforce; Level 2 made
    // dictionaries grow automatically anyway.
    it.push(Object::lit(Value::Dict(Rc::new(RefCell::new(Dict::new())))));
    Ok(())
}

fn begin(it: &mut Interp) -> Result<(), PsError> {
    match it.pop()?.value {
        Value::Dict(d) => {
            it.push_dict(d);
            Ok(())
        }
        _ => Err(PsError::Typecheck),
    }
}

fn end(it: &mut Interp) -> Result<(), PsError> {
    it.pop_dict()
}

fn load(it: &mut Interp) -> Result<(), PsError> {
    let key = pop_key(it)?;
    let obj = it
        .load(&key)
        .ok_or_else(|| PsError::Undefined(key.to_string()))?;
    it.push(obj);
    Ok(())
}

fn bind(it: &mut Interp) -> Result<(), PsError> {
    let obj = it.pop()?;
    match (&obj.value, obj.executable) {
        (Value::Array(body), true) => {
            bind_proc(it, body, 0);
            it.push(obj);
            Ok(())
        }
        _ => Err(PsError::Typecheck),
    }
}

/// Replace executable names that currently resolve to operators with the
/// operators themselves, recursing into nested procedures — the classic
/// "immune to later redefinition, and faster" transform.
fn bind_proc(it: &Interp, body: &Rc<RefCell<Vec<Object>>>, depth: usize) {
    if depth > 100 {
        return;
    }
    // try_borrow_mut: a self-referential procedure (not constructible
    // today, but cheap insurance) is silently left unbound rather than
    // panicking.
    let Ok(mut elements) = body.try_borrow_mut() else {
        return;
    };
    for el in elements.iter_mut() {
        match (&el.value, el.executable) {
            (Value::Name(n), true) => {
                if let Some(resolved) = it.load(n)
                    && resolved.executable
                    && matches!(resolved.value, Value::Operator(_))
                {
                    *el = resolved;
                }
            }
            (Value::Array(inner), true) => {
                let inner = inner.clone();
                bind_proc(it, &inner, depth + 1);
            }
            _ => {}
        }
    }
}
