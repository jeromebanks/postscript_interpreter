//! `sh` — Level 3's shading-fill operator (issue #20). Thin operand
//! adapter; the shading-dictionary engine is `crate::shading`, the
//! painting is `Gfx::sh`.

use crate::error::PsError;
use crate::interp::Interp;
use crate::object::{Dict, Value};

pub fn install(dict: &mut Dict) {
    use super::op;
    op(dict, "sh", sh);
}

fn sh(it: &mut Interp) -> Result<(), PsError> {
    let obj = it.pop()?;
    let Value::Dict(d) = &obj.value else {
        return Err(PsError::Typecheck);
    };
    let spec = crate::shading::parse_shading_dict(&d.borrow())?;
    it.gfx.sh(spec)
}
