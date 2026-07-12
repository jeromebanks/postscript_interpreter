//! Output and interpreter-control operators: the pieces that make a REPL
//! and `--eval` mode actually demonstrable.

use std::io::Write;

use crate::error::PsError;
use crate::interp::Interp;
use crate::object::{Dict, Value};

pub fn install(dict: &mut Dict) {
    use super::op;
    op(dict, "=", print_text);
    op(dict, "==", print_syntactic);
    op(dict, "stack", stack_dump);
    op(dict, "pstack", pstack_dump);
    op(dict, "print", print_string);
    op(dict, "quit", quit);
}

fn print_text(it: &mut Interp) -> Result<(), PsError> {
    let obj = it.pop()?;
    println!("{}", obj.text());
    Ok(())
}

fn print_syntactic(it: &mut Interp) -> Result<(), PsError> {
    let obj = it.pop()?;
    println!("{}", obj.repr());
    Ok(())
}

/// Print the whole stack, top first, without disturbing it (PLRM `stack`).
fn stack_dump(it: &mut Interp) -> Result<(), PsError> {
    for obj in it.ostack.iter().rev() {
        println!("{}", obj.text());
    }
    Ok(())
}

fn pstack_dump(it: &mut Interp) -> Result<(), PsError> {
    for obj in it.ostack.iter().rev() {
        println!("{}", obj.repr());
    }
    Ok(())
}

fn print_string(it: &mut Interp) -> Result<(), PsError> {
    let obj = it.pop()?;
    let Value::String(s) = &obj.value else {
        return Err(PsError::Typecheck);
    };
    let mut out = std::io::stdout();
    out.write_all(&s.borrow_bytes())
        .and_then(|()| out.flush())
        .map_err(|_| PsError::Io)
}

fn quit(it: &mut Interp) -> Result<(), PsError> {
    it.quit_requested = true;
    Ok(())
}
