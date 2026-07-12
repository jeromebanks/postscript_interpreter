//! Dictionary and definition operators, plus `bind`.

use std::cell::RefCell;
use std::rc::Rc;

use crate::error::PsError;
use crate::interp::Interp;
use crate::object::{Dict, Object, PsArray, Value};

pub fn install(dict: &mut Dict) {
    use super::op;
    op(dict, "def", def);
    op(dict, "dict", make_dict);
    op(dict, "begin", begin);
    op(dict, "end", end);
    op(dict, "load", load);
    op(dict, "bind", bind);
    op(dict, "known", known);
    op(dict, "where", where_op);
    op(dict, "store", store);
    op(dict, "undef", undef);
    op(dict, "currentdict", currentdict);
    op(dict, "countdictstack", countdictstack);
    op(dict, "cleardictstack", cleardictstack);
    op(dict, "maxlength", maxlength);
    op(dict, ">>", dict_from_mark);
    // `<<` is just a mark, installed as a constant in ops::install_all.
}

/// def/load/store keys are names; strings convert per the PLRM. (put/get
/// with arbitrary keys live in ops::data.)
fn pop_key(it: &mut Interp) -> Result<Rc<str>, PsError> {
    match &it.pop()?.value {
        Value::Name(n) => Ok(n.clone()),
        Value::String(s) => Ok(s.text().into()),
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

fn pop_dict_operand(it: &mut Interp) -> Result<Rc<RefCell<Dict>>, PsError> {
    match &it.pop()?.value {
        Value::Dict(d) => Ok(d.clone()),
        _ => Err(PsError::Typecheck),
    }
}

fn begin(it: &mut Interp) -> Result<(), PsError> {
    let d = pop_dict_operand(it)?;
    it.push_dict(d);
    Ok(())
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

fn known(it: &mut Interp) -> Result<(), PsError> {
    let key = it.pop()?;
    let d = pop_dict_operand(it)?;
    let found = d.borrow().known(&key)?;
    it.push(Object::bool(found));
    Ok(())
}

/// key where -> dict true (topmost defining dict) | false
fn where_op(it: &mut Interp) -> Result<(), PsError> {
    let key = it.pop()?;
    match it.find_defining_dict(&key)? {
        Some(d) => {
            it.push(Object::lit(Value::Dict(d)));
            it.push(Object::bool(true));
        }
        None => it.push(Object::bool(false)),
    }
    Ok(())
}

/// key value store: replace in the topmost dict that defines key, else
/// define in the current dict.
fn store(it: &mut Interp) -> Result<(), PsError> {
    let value = it.pop()?;
    let key = it.pop()?;
    match it.find_defining_dict(&key)? {
        Some(d) => d.borrow_mut().put_obj(&key, value)?,
        None => it.current_dict().borrow_mut().put_obj(&key, value)?,
    }
    Ok(())
}

fn undef(it: &mut Interp) -> Result<(), PsError> {
    let key = it.pop()?;
    let d = pop_dict_operand(it)?;
    d.borrow_mut().undef(&key)
}

fn currentdict(it: &mut Interp) -> Result<(), PsError> {
    let d = it.current_dict();
    it.push(Object::lit(Value::Dict(d)));
    Ok(())
}

fn countdictstack(it: &mut Interp) -> Result<(), PsError> {
    it.push(Object::int(it.dict_stack_len() as i64));
    Ok(())
}

fn cleardictstack(it: &mut Interp) -> Result<(), PsError> {
    it.clear_dict_stack();
    Ok(())
}

fn maxlength(it: &mut Interp) -> Result<(), PsError> {
    let d = pop_dict_operand(it)?;
    // Level 2: dictionaries grow, so maxlength just reports capacity
    // "at least current length"; length + slack matches Ghostscript's
    // spirit without modeling capacity.
    let n = d.borrow().len() as i64;
    it.push(Object::int(n.max(1)));
    Ok(())
}

/// `>>`: collect key/value pairs down to the mark into a new dict.
fn dict_from_mark(it: &mut Interp) -> Result<(), PsError> {
    let mut items = Vec::new();
    loop {
        let obj = it.pop().map_err(|_| PsError::UnmatchedMark)?;
        if matches!(obj.value, Value::Mark) {
            break;
        }
        items.push(obj);
    }
    if items.len() % 2 != 0 {
        return Err(PsError::Rangecheck);
    }
    let mut d = Dict::new();
    // `items` holds the operands top-of-stack-first, so popping walks
    // them in original order: key, value, key, value, ...
    while let (Some(key), Some(value)) = (items.pop(), items.pop()) {
        d.put_obj(&key, value)?;
    }
    it.push(Object::lit(Value::Dict(Rc::new(RefCell::new(d)))));
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
fn bind_proc(it: &Interp, body: &PsArray, depth: usize) {
    if depth > 100 {
        return;
    }
    let mut nested: Vec<PsArray> = Vec::new();
    // for_each_mut declines (harmlessly) if the storage is already
    // borrowed — the self-referential case.
    body.for_each_mut(|el| match (&el.value, el.executable) {
        (Value::Name(n), true) => {
            if let Some(resolved) = it.load(n)
                && resolved.executable
                && matches!(resolved.value, Value::Operator(_))
            {
                *el = resolved;
            }
        }
        (Value::Array(inner), true) => nested.push(inner.clone()),
        _ => {}
    });
    for inner in nested {
        bind_proc(it, &inner, depth + 1);
    }
}
