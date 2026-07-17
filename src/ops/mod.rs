//! Built-in operators, grouped roughly the way the PLRM's operator summary
//! groups them. Each module installs its operators into systemdict.

pub mod arith;
pub mod color;
pub mod control;
pub mod convert;
pub mod data;
pub mod dict;
pub mod file;
pub mod font;
pub mod graphics;
pub mod image;
pub mod logic;
pub mod matrix;
pub mod misc;
pub mod stack;
pub mod vm;

use crate::object::{Dict, Object, OpFn, Operator, Value};

pub(crate) fn op(dict: &mut Dict, name: &'static str, func: OpFn) {
    dict.put(
        name.into(),
        Object::exec(Value::Operator(Operator { name, func })),
    );
}

pub fn install_all(dict: &mut Dict) {
    stack::install(dict);
    arith::install(dict);
    control::install(dict);
    convert::install(dict);
    data::install(dict);
    dict::install(dict);
    graphics::install(dict);
    // matrix installs after graphics: its translate/scale/rotate
    // dispatchers (matrix-operand aware) replace the plain CTM forms.
    matrix::install(dict);
    font::install(dict);
    file::install(dict);
    image::install(dict);
    logic::install(dict);
    misc::install(dict);
    vm::install(dict);
    color::install(dict);

    // Constants: executing a name bound to a literal object pushes the
    // object, which is exactly the behavior `true`, `mark`, etc. need —
    // no operator indirection required.
    dict.put("true".into(), Object::lit(Value::Boolean(true)));
    dict.put("false".into(), Object::lit(Value::Boolean(false)));
    dict.put("null".into(), Object::lit(Value::Null));
    dict.put("mark".into(), Object::lit(Value::Mark));
    dict.put("[".into(), Object::lit(Value::Mark));
    dict.put("<<".into(), Object::lit(Value::Mark));
}
