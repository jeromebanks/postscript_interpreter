//! Output and interpreter-control operators: the pieces that make a REPL
//! and `--eval` mode actually demonstrable.

use std::io::Write;

use crate::error::PsError;
use crate::interp::Interp;
use crate::object::{Dict, Object, Value};

pub fn install(dict: &mut Dict) {
    use super::op;
    op(dict, "=", print_text);
    op(dict, "==", print_syntactic);
    op(dict, "stack", stack_dump);
    op(dict, "pstack", pstack_dump);
    op(dict, "print", print_string);
    op(dict, "quit", quit);
    op(dict, "handleerror", handleerror);
    // Access-control attributes are not modeled (nothing enforces
    // them), but the operators must exist as identities: every real
    // Type 1 font says `executeonly`/`noaccess` on its way in.
    op(dict, "executeonly", access_noop);
    op(dict, "readonly", access_noop);
    op(dict, "noaccess", access_noop);
    op(dict, "rcheck", access_check);
    op(dict, "wcheck", access_check);
}

fn access_noop(it: &mut Interp) -> Result<(), PsError> {
    // operand ... operand: identity, but stackunderflow still applies.
    let obj = it.pop()?;
    it.push(obj);
    Ok(())
}

/// Everything reads/writes as permitted here; report true.
fn access_check(it: &mut Interp) -> Result<(), PsError> {
    it.pop()?;
    it.push(Object::bool(true));
    Ok(())
}

/// The default error reporter: prints from $error the way the CLI does,
/// then clears newerror so the next error is distinguishable.
fn handleerror(it: &mut Interp) -> Result<(), PsError> {
    let Some(obj) = it.load("$error") else {
        return Ok(());
    };
    let Value::Dict(d) = &obj.value else {
        return Ok(());
    };
    let (name, command) = {
        let d = d.borrow();
        (
            d.get("errorname").map(|o| o.text()),
            d.get("command").map(|o| o.text()),
        )
    };
    eprintln!(
        "%%[ Error: {}; OffendingCommand: {} ]%%",
        name.unwrap_or_else(|| "--none--".to_string()),
        command.unwrap_or_else(|| "--none--".to_string())
    );
    d.borrow_mut().put("newerror".into(), Object::bool(false));
    Ok(())
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
