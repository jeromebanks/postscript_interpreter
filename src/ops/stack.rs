//! Operand-stack manipulation operators, plus `]` (which is mark-based
//! array construction and lives here until there's a real array module).

use crate::error::PsError;
use crate::interp::Interp;
use crate::object::{Dict, Object, Value};

pub fn install(dict: &mut Dict) {
    use super::op;
    op(dict, "pop", pop);
    op(dict, "exch", exch);
    op(dict, "dup", dup);
    // "copy" is registered by ops::data, which dispatches the composite
    // forms and calls copy_n below for the plain stack form.
    op(dict, "index", index);
    op(dict, "roll", roll);
    op(dict, "clear", clear);
    op(dict, "count", count);
    op(dict, "counttomark", counttomark);
    op(dict, "cleartomark", cleartomark);
    op(dict, "]", endarray);
}

fn pop(it: &mut Interp) -> Result<(), PsError> {
    it.pop()?;
    Ok(())
}

fn exch(it: &mut Interp) -> Result<(), PsError> {
    let len = it.ostack.len();
    if len < 2 {
        return Err(PsError::StackUnderflow);
    }
    it.ostack.swap(len - 1, len - 2);
    Ok(())
}

fn dup(it: &mut Interp) -> Result<(), PsError> {
    let top = it.ostack.last().cloned().ok_or(PsError::StackUnderflow)?;
    it.push(top);
    Ok(())
}

pub(crate) fn copy_n(it: &mut Interp, n: i64) -> Result<(), PsError> {
    if n < 0 {
        return Err(PsError::Rangecheck);
    }
    let n = n as usize;
    let len = it.ostack.len();
    if n > len {
        return Err(PsError::StackUnderflow);
    }
    it.ostack.extend_from_within(len - n..);
    Ok(())
}

fn index(it: &mut Interp) -> Result<(), PsError> {
    let n = it.pop_int()?;
    if n < 0 {
        return Err(PsError::Rangecheck);
    }
    let n = n as usize;
    let len = it.ostack.len();
    if n >= len {
        return Err(PsError::StackUnderflow);
    }
    it.push(it.ostack[len - 1 - n].clone());
    Ok(())
}

fn roll(it: &mut Interp) -> Result<(), PsError> {
    let j = it.pop_int()?;
    let n = it.pop_int()?;
    if n < 0 {
        return Err(PsError::Rangecheck);
    }
    let n = n as usize;
    let len = it.ostack.len();
    if n > len {
        return Err(PsError::StackUnderflow);
    }
    if n == 0 {
        return Ok(());
    }
    // rem_euclid folds any j (including negative) into a single
    // rotate_right amount.
    let j = j.rem_euclid(n as i64) as usize;
    it.ostack[len - n..].rotate_right(j);
    Ok(())
}

fn clear(it: &mut Interp) -> Result<(), PsError> {
    it.ostack.clear();
    Ok(())
}

fn count(it: &mut Interp) -> Result<(), PsError> {
    it.push(Object::int(it.ostack.len() as i64));
    Ok(())
}

/// Distance from the top of the stack to the topmost mark.
fn find_mark(it: &Interp) -> Option<usize> {
    it.ostack
        .iter()
        .rev()
        .position(|o| matches!(o.value, Value::Mark))
}

fn counttomark(it: &mut Interp) -> Result<(), PsError> {
    let d = find_mark(it).ok_or(PsError::UnmatchedMark)?;
    it.push(Object::int(d as i64));
    Ok(())
}

fn cleartomark(it: &mut Interp) -> Result<(), PsError> {
    let d = find_mark(it).ok_or(PsError::UnmatchedMark)?;
    let len = it.ostack.len();
    it.ostack.truncate(len - d - 1);
    Ok(())
}

fn endarray(it: &mut Interp) -> Result<(), PsError> {
    let d = find_mark(it).ok_or(PsError::UnmatchedMark)?;
    let len = it.ostack.len();
    let items: Vec<Object> = it.ostack.drain(len - d..).collect();
    it.ostack.pop(); // the mark itself
    it.push(Object::array(items));
    Ok(())
}
