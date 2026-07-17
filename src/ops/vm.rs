//! save / restore / vmstatus — VM snapshot semantics. The design (and
//! the gs pins behind every choice here) is `VM.md`; the machinery
//! lives on `Interp` (`do_save`/`do_restore`, the COW journal).
//!
//! Documented deviation: no invalidrestore scan of the stacks for
//! composites created since the save — under `Rc` such objects simply
//! stay valid, so programs that would error on Adobe render here.

use crate::error::PsError;
use crate::interp::Interp;
use crate::object::{Dict, Object};

pub fn install(dict: &mut Dict) {
    use super::op;
    op(dict, "save", save);
    op(dict, "restore", restore);
    op(dict, "vmstatus", vmstatus);
}

fn save(it: &mut Interp) -> Result<(), PsError> {
    let s = it.do_save();
    it.push(s);
    Ok(())
}

fn restore(it: &mut Interp) -> Result<(), PsError> {
    let s = it.pop()?;
    it.do_restore(&s)
}

/// `- vmstatus level used maximum`. The level is real; the byte
/// counts are fixed fictions (found code wants "is there free VM",
/// and 15MB of headroom says yes) — real accounting would be theater.
fn vmstatus(it: &mut Interp) -> Result<(), PsError> {
    it.push(Object::int(it.save_level() as i64));
    it.push(Object::int(1_000_000));
    it.push(Object::int(16_000_000));
    Ok(())
}
