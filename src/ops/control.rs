//! Control-flow operators. The looping ones don't recurse into the
//! interpreter — they push loop frames onto the execution stack, which is
//! what keeps deep iteration steppable (for live rendering) and immune to
//! Rust stack depth.

use std::cell::RefCell;
use std::rc::Rc;

use crate::error::PsError;
use crate::interp::{ForControl, Interp};
use crate::object::{Dict, Object, Value};

pub fn install(dict: &mut Dict) {
    use super::op;
    op(dict, "if", ps_if);
    op(dict, "ifelse", ifelse);
    op(dict, "exec", exec);
    op(dict, "repeat", repeat);
    op(dict, "loop", ps_loop);
    op(dict, "for", ps_for);
    op(dict, "exit", exit);
}

/// The loop operators require a procedure body specifically (not any
/// executable object) because the body runs repeatedly from a frame.
fn pop_proc(it: &mut Interp) -> Result<Rc<RefCell<Vec<Object>>>, PsError> {
    let obj = it.pop()?;
    match (&obj.value, obj.executable) {
        (Value::Array(body), true) => Ok(body.clone()),
        _ => Err(PsError::Typecheck),
    }
}

fn pop_bool(it: &mut Interp) -> Result<bool, PsError> {
    match it.pop()?.value {
        Value::Boolean(b) => Ok(b),
        _ => Err(PsError::Typecheck),
    }
}

fn ps_if(it: &mut Interp) -> Result<(), PsError> {
    let proc = it.pop()?;
    let cond = pop_bool(it)?;
    if cond { it.exec_object(proc) } else { Ok(()) }
}

fn ifelse(it: &mut Interp) -> Result<(), PsError> {
    let proc_false = it.pop()?;
    let proc_true = it.pop()?;
    let cond = pop_bool(it)?;
    it.exec_object(if cond { proc_true } else { proc_false })
}

fn exec(it: &mut Interp) -> Result<(), PsError> {
    let obj = it.pop()?;
    it.exec_object(obj)
}

fn repeat(it: &mut Interp) -> Result<(), PsError> {
    let body = pop_proc(it)?;
    let n = it.pop_int()?;
    if n < 0 {
        return Err(PsError::Rangecheck);
    }
    it.begin_repeat(body, n)
}

fn ps_loop(it: &mut Interp) -> Result<(), PsError> {
    let body = pop_proc(it)?;
    it.begin_loop(body)
}

fn ps_for(it: &mut Interp) -> Result<(), PsError> {
    let body = pop_proc(it)?;
    let limit = it.pop_num()?;
    let increment = it.pop_num()?;
    let initial = it.pop_num()?;
    it.begin_for(body, ForControl::new(initial, increment, limit))
}

fn exit(it: &mut Interp) -> Result<(), PsError> {
    it.exit_loop()
}
