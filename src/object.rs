//! The PostScript object model.
//!
//! Every PostScript value is an [`Object`]: a [`Value`] plus the
//! literal/executable attribute. The attribute lives as a flag on the
//! object rather than as doubled-up enum variants because *any* type can
//! carry it (`cvx` can mark an integer executable), and because it keeps
//! the "same value, different behavior" pairs — name vs. executable name,
//! array vs. procedure — as one representation each.
//!
//! Composite objects (strings, arrays, dictionaries) use `Rc<RefCell<..>>`
//! because PostScript composites have *reference* semantics: `dup` on an
//! array yields a second handle to the same storage, and mutations through
//! one handle are visible through the other. The interpreter core is
//! single-threaded (Stage 2's live rendering is planned around stepping the
//! machine, not sharing it across threads), so `Rc` rather than `Arc`.

use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::rc::Rc;

use crate::error::PsError;
use crate::interp::Interp;

pub type OpFn = fn(&mut Interp) -> Result<(), PsError>;

/// A built-in operator: its systemdict name (kept for error reporting and
/// `==` output) and the function that implements it.
#[derive(Clone, Copy)]
pub struct Operator {
    pub name: &'static str,
    pub func: OpFn,
}

#[derive(Clone)]
pub enum Value {
    /// PLRM specifies 32-bit integers; we use 64-bit as the implementation
    /// limit (documented deviation — more headroom, no downside for
    /// hand-written programs). Arithmetic that overflows still promotes to
    /// a real, matching the spec's *behavior* if not its exact threshold.
    Integer(i64),
    Real(f64),
    Boolean(bool),
    Mark,
    Null,
    Name(Rc<str>),
    String(Rc<RefCell<Vec<u8>>>),
    Array(Rc<RefCell<Vec<Object>>>),
    Dict(Rc<RefCell<Dict>>),
    Operator(Operator),
}

#[derive(Clone)]
pub struct Object {
    pub value: Value,
    pub executable: bool,
}

impl Object {
    pub fn lit(value: Value) -> Self {
        Object {
            value,
            executable: false,
        }
    }

    pub fn exec(value: Value) -> Self {
        Object {
            value,
            executable: true,
        }
    }

    pub fn int(i: i64) -> Self {
        Self::lit(Value::Integer(i))
    }

    pub fn real(r: f64) -> Self {
        Self::lit(Value::Real(r))
    }

    pub fn array(items: Vec<Object>) -> Self {
        Self::lit(Value::Array(Rc::new(RefCell::new(items))))
    }

    pub fn procedure(items: Vec<Object>) -> Self {
        Self::exec(Value::Array(Rc::new(RefCell::new(items))))
    }

    pub fn string(bytes: Vec<u8>) -> Self {
        Self::lit(Value::String(Rc::new(RefCell::new(bytes))))
    }

    /// Text form, as the `=` operator prints it: the "natural" rendering
    /// for scalars and string contents, `--nostringval--` for everything
    /// that has no string value.
    pub fn text(&self) -> String {
        match &self.value {
            Value::Integer(i) => i.to_string(),
            Value::Real(r) => format_real(*r),
            Value::Boolean(b) => b.to_string(),
            Value::Name(n) => n.to_string(),
            Value::String(s) => String::from_utf8_lossy(&s.borrow()).into_owned(),
            Value::Mark | Value::Null | Value::Array(_) | Value::Dict(_) | Value::Operator(_) => {
                "--nostringval--".to_string()
            }
        }
    }

    /// Syntactic form, as the `==` operator prints it: valid PostScript
    /// source where possible (`/name`, `(string)`, `[array]`, `{proc}`).
    pub fn repr(&self) -> String {
        self.repr_depth(0)
    }

    /// Depth-capped so pathologically nested arrays (`[[[[...`) print an
    /// ellipsis instead of overflowing the Rust stack.
    fn repr_depth(&self, depth: usize) -> String {
        const MAX_REPR_DEPTH: usize = 32;
        match &self.value {
            Value::Integer(i) => i.to_string(),
            Value::Real(r) => format_real(*r),
            Value::Boolean(b) => b.to_string(),
            Value::Mark => "-mark-".to_string(),
            Value::Null => "null".to_string(),
            Value::Name(n) => {
                if self.executable {
                    n.to_string()
                } else {
                    format!("/{n}")
                }
            }
            Value::String(s) => format!("({})", escape_string(&s.borrow())),
            Value::Array(a) => {
                if depth >= MAX_REPR_DEPTH {
                    return "...".to_string();
                }
                let inner: Vec<String> =
                    a.borrow().iter().map(|o| o.repr_depth(depth + 1)).collect();
                if self.executable {
                    format!("{{{}}}", inner.join(" "))
                } else {
                    format!("[{}]", inner.join(" "))
                }
            }
            Value::Dict(_) => "-dict-".to_string(),
            Value::Operator(op) => format!("--{}--", op.name),
        }
    }
}

impl fmt::Debug for Object {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.repr())
    }
}

/// Deeply nested arrays (10k levels of `[`) would drop recursively — one
/// Rust stack frame per level — and abort the process. When this is the
/// last handle to an array, tear it down iteratively instead, reusing the
/// array's own element vector as the worklist.
impl Drop for Object {
    fn drop(&mut self) {
        let Value::Array(rc) = &self.value else {
            return;
        };
        if Rc::strong_count(rc) != 1 {
            return;
        }
        let Value::Array(rc) = std::mem::replace(&mut self.value, Value::Null) else {
            return;
        };
        let Ok(cell) = Rc::try_unwrap(rc) else {
            return;
        };
        let mut pending = cell.into_inner();
        while let Some(mut obj) = pending.pop() {
            if let Value::Array(inner) = &obj.value
                && Rc::strong_count(inner) == 1
                && let Value::Array(inner) = std::mem::replace(&mut obj.value, Value::Null)
                && let Ok(cell) = Rc::try_unwrap(inner)
            {
                pending.extend(cell.into_inner());
            }
        }
    }
}

/// `{:?}` rather than `{}` so whole reals keep their decimal point
/// (`2.0`, not `2`) — the type distinction is meaningful in PostScript.
pub(crate) fn format_real(r: f64) -> String {
    format!("{r:?}")
}

fn escape_string(bytes: &[u8]) -> String {
    let mut out = String::new();
    for &b in bytes {
        match b {
            b'(' => out.push_str("\\("),
            b')' => out.push_str("\\)"),
            b'\\' => out.push_str("\\\\"),
            b'\n' => out.push_str("\\n"),
            b'\r' => out.push_str("\\r"),
            b'\t' => out.push_str("\\t"),
            0x08 => out.push_str("\\b"),
            0x0c => out.push_str("\\f"),
            0x20..=0x7e => out.push(b as char),
            _ => out.push_str(&format!("\\{b:03o}")),
        }
    }
    out
}

/// A numeric operand: most arithmetic operators accept either integer or
/// real and promote as needed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Num {
    Int(i64),
    Real(f64),
}

impl Num {
    pub fn to_f64(self) -> f64 {
        match self {
            Num::Int(i) => i as f64,
            Num::Real(r) => r,
        }
    }
}

/// A PostScript dictionary. Keyed by name text only for now; the PLRM
/// allows almost any object as a key (strings are converted to names,
/// numbers used as-is), which will force a richer key type once `put`/`get`
/// on arbitrary keys land. Deliberately not generalized before then.
#[derive(Default)]
pub struct Dict {
    map: HashMap<Rc<str>, Object>,
}

impl Dict {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, key: &str) -> Option<Object> {
        self.map.get(key).cloned()
    }

    pub fn put(&mut self, key: Rc<str>, value: Object) {
        self.map.insert(key, value);
    }
}
