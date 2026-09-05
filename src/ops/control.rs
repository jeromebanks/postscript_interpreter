//! Control-flow operators. The looping ones don't recurse into the
//! interpreter — they push loop frames onto the execution stack, which is
//! what keeps deep iteration steppable (for live rendering) and immune to
//! Rust stack depth.

use crate::error::PsError;
use crate::interp::{ForControl, Interp};
use crate::object::{Dict, Object, PsArray, Value};

pub fn install(dict: &mut Dict) {
    use super::op;
    op(dict, "if", ps_if);
    op(dict, "ifelse", ifelse);
    op(dict, "exec", exec);
    op(dict, "repeat", repeat);
    op(dict, "loop", ps_loop);
    op(dict, "for", ps_for);
    op(dict, "exit", exit);
    op(dict, "stop", stop);
    op(dict, "stopped", stopped);
}

/// The loop operators require a procedure body specifically (not any
/// executable object) because the body runs repeatedly from a frame.
pub(crate) fn pop_proc(it: &mut Interp) -> Result<PsArray, PsError> {
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

fn stop(it: &mut Interp) -> Result<(), PsError> {
    if it.do_stop() {
        it.push(Object::bool(true));
        return Ok(());
    }
    // No enclosing stopped: do_stop drained the exec stack, which ends
    // the program — the PLRM's outermost job-server context would do the
    // same. Nothing is pushed.
    //
    // But if an error is still pending in `$error`, ending *silently*
    // loses it (issue #142). Ghostscript's outermost wrapper reports it
    // instead, which is what makes the standard cleanup idiom usable:
    //
    //     { userproc } stopped { restore-my-state stop } if
    //
    // Without this, re-raising turned a hard error into no error at all
    // for every caller that did not itself catch, so a library wrapping
    // a caller's procedure had to choose between leaking its own state
    // and swallowing the failure. `lib/paintkit.ps`'s `pkwet` hit
    // exactly that.
    match it.top_level_stop_error() {
        Some(e) => Err(e),
        None => Ok(()),
    }
}

fn stopped(it: &mut Interp) -> Result<(), PsError> {
    let obj = it.pop()?;
    it.begin_stopped()?;
    it.exec_object(obj)
}
