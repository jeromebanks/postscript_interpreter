//! `shfill` — Level 3's shading-fill operator (issue #20). Thin
//! operand adapter; the shading-dictionary engine is `crate::shading`,
//! the painting is `Gfx::shfill`.
//!
//! Named `shfill`, not `sh`: confirmed against gs 10.07.1 (`gs
//! -dNODISPLAY -c "/shfill where =="` finds it, `/sh where ==` doesn't)
//! — a first draft named it `sh` after PDF's content-stream operator
//! of that name, which takes a shading *resource name*, not the
//! dictionary directly; PostScript's own Level 3 operator for exactly
//! this (a shading dict straight off the stack) is `shfill`. Caught by
//! a Codex review at the PR stage, not before — every hand-test during
//! development called it `sh` too, so nothing forced this open until
//! reviewed against gs itself.

use crate::error::PsError;
use crate::interp::Interp;
use crate::object::{Dict, Value};

pub fn install(dict: &mut Dict) {
    use super::op;
    op(dict, "shfill", shfill);
}

fn shfill(it: &mut Interp) -> Result<(), PsError> {
    let obj = it.pop()?;
    let Value::Dict(d) = &obj.value else {
        return Err(PsError::Typecheck);
    };
    let spec = crate::shading::parse_shading_dict(&d.borrow())?;
    it.gfx.shfill(spec)
}
