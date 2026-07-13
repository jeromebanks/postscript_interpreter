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
//! single-threaded (live rendering steps the machine rather than sharing
//! it across threads), so `Rc` rather than `Arc`.
//!
//! Arrays and strings are **views** ([`PsArray`], [`PsString`]): a shared
//! backing store plus offset/length. That's what makes `getinterval`
//! return a window onto the same storage — mutate the parent and the
//! interval sees it — which real PostScript code relies on (`cvs`
//! returning a prefix of its buffer, prologs slicing tables).

use std::cell::{Ref, RefCell, RefMut};
use std::collections::HashMap;
use std::fmt;
use std::rc::Rc;

use crate::error::PsError;
use crate::file::FileHandle;
use crate::interp::Interp;

pub type OpFn = fn(&mut Interp) -> Result<(), PsError>;

/// A built-in operator: its systemdict name (kept for error reporting and
/// `==` output) and the function that implements it.
#[derive(Clone, Copy)]
pub struct Operator {
    pub name: &'static str,
    pub func: OpFn,
}

/// A view into shared array storage.
#[derive(Clone)]
pub struct PsArray {
    data: Rc<RefCell<Vec<Object>>>,
    off: usize,
    len: usize,
}

impl PsArray {
    pub fn from_vec(items: Vec<Object>) -> Self {
        let len = items.len();
        PsArray {
            data: Rc::new(RefCell::new(items)),
            off: 0,
            len,
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn get(&self, i: usize) -> Option<Object> {
        if i < self.len {
            Some(self.data.borrow()[self.off + i].clone())
        } else {
            None
        }
    }

    pub fn put(&self, i: usize, obj: Object) -> Result<(), PsError> {
        if i >= self.len {
            return Err(PsError::Rangecheck);
        }
        self.data.borrow_mut()[self.off + i] = obj;
        Ok(())
    }

    /// A sub-view sharing this view's storage.
    pub fn interval(&self, off: usize, len: usize) -> Result<PsArray, PsError> {
        if off > self.len || len > self.len - off {
            return Err(PsError::Rangecheck);
        }
        Ok(PsArray {
            data: self.data.clone(),
            off: self.off + off,
            len,
        })
    }

    /// PostScript `eq`: composites are equal only as identical objects —
    /// same storage *and* same window onto it.
    pub fn same_object(&self, other: &PsArray) -> bool {
        Rc::ptr_eq(&self.data, &other.data) && self.off == other.off && self.len == other.len
    }

    pub fn snapshot(&self) -> Vec<Object> {
        let b = self.data.borrow();
        b[self.off..self.off + self.len].to_vec()
    }

    /// Run `f` over each element in place; returns false (doing nothing)
    /// if the storage is already borrowed — the self-referential case.
    pub fn for_each_mut(&self, mut f: impl FnMut(&mut Object)) -> bool {
        let Ok(mut data) = self.data.try_borrow_mut() else {
            return false;
        };
        for el in &mut data[self.off..self.off + self.len] {
            f(el);
        }
        true
    }

    fn identity(&self) -> (usize, usize, usize) {
        (Rc::as_ptr(&self.data) as usize, self.off, self.len)
    }

    pub(crate) fn data_rc(&self) -> &Rc<RefCell<Vec<Object>>> {
        &self.data
    }
}

/// A view into shared string (byte) storage.
#[derive(Clone)]
pub struct PsString {
    data: Rc<RefCell<Vec<u8>>>,
    off: usize,
    len: usize,
}

impl PsString {
    pub fn from_bytes(bytes: Vec<u8>) -> Self {
        let len = bytes.len();
        PsString {
            data: Rc::new(RefCell::new(bytes)),
            off: 0,
            len,
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn borrow_bytes(&self) -> Ref<'_, [u8]> {
        Ref::map(self.data.borrow(), |v| &v[self.off..self.off + self.len])
    }

    pub fn borrow_bytes_mut(&self) -> RefMut<'_, [u8]> {
        RefMut::map(self.data.borrow_mut(), |v| {
            &mut v[self.off..self.off + self.len]
        })
    }

    pub fn to_vec(&self) -> Vec<u8> {
        self.borrow_bytes().to_vec()
    }

    pub fn byte(&self, i: usize) -> Option<u8> {
        if i < self.len {
            Some(self.data.borrow()[self.off + i])
        } else {
            None
        }
    }

    pub fn put_byte(&self, i: usize, b: u8) -> Result<(), PsError> {
        if i >= self.len {
            return Err(PsError::Rangecheck);
        }
        self.data.borrow_mut()[self.off + i] = b;
        Ok(())
    }

    pub fn interval(&self, off: usize, len: usize) -> Result<PsString, PsError> {
        if off > self.len || len > self.len - off {
            return Err(PsError::Rangecheck);
        }
        Ok(PsString {
            data: self.data.clone(),
            off: self.off + off,
            len,
        })
    }

    /// Copy `bytes` into this view starting at `at`; rangecheck if they
    /// don't fit.
    pub fn write_at(&self, at: usize, bytes: &[u8]) -> Result<(), PsError> {
        if at > self.len || bytes.len() > self.len - at {
            return Err(PsError::Rangecheck);
        }
        self.borrow_bytes_mut()[at..at + bytes.len()].copy_from_slice(bytes);
        Ok(())
    }

    pub fn same_object(&self, other: &PsString) -> bool {
        Rc::ptr_eq(&self.data, &other.data) && self.off == other.off && self.len == other.len
    }

    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.borrow_bytes()).into_owned()
    }
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
    String(PsString),
    Array(PsArray),
    Dict(Rc<RefCell<Dict>>),
    Operator(Operator),
    File(FileHandle),
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

    pub fn bool(b: bool) -> Self {
        Self::lit(Value::Boolean(b))
    }

    pub fn name(n: &str) -> Self {
        Self::lit(Value::Name(n.into()))
    }

    pub fn array(items: Vec<Object>) -> Self {
        Self::lit(Value::Array(PsArray::from_vec(items)))
    }

    pub fn procedure(items: Vec<Object>) -> Self {
        Self::exec(Value::Array(PsArray::from_vec(items)))
    }

    pub fn string(bytes: Vec<u8>) -> Self {
        Self::lit(Value::String(PsString::from_bytes(bytes)))
    }

    /// The `type` operator's name for this object.
    pub fn type_name(&self) -> &'static str {
        match &self.value {
            Value::Integer(_) => "integertype",
            Value::Real(_) => "realtype",
            Value::Boolean(_) => "booleantype",
            Value::Mark => "marktype",
            Value::Null => "nulltype",
            Value::Name(_) => "nametype",
            Value::String(_) => "stringtype",
            Value::Array(_) => "arraytype",
            Value::Dict(_) => "dicttype",
            Value::Operator(_) => "operatortype",
            Value::File(_) => "filetype",
        }
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
            Value::String(s) => s.text(),
            Value::Mark
            | Value::Null
            | Value::Array(_)
            | Value::Dict(_)
            | Value::Operator(_)
            | Value::File(_) => "--nostringval--".to_string(),
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
            Value::String(s) => format!("({})", escape_string(&s.borrow_bytes())),
            Value::Array(a) => {
                if depth >= MAX_REPR_DEPTH {
                    return "...".to_string();
                }
                let inner: Vec<String> = (0..a.len())
                    .map(|i| {
                        a.get(i)
                            .map(|o| o.repr_depth(depth + 1))
                            .unwrap_or_default()
                    })
                    .collect();
                if self.executable {
                    format!("{{{}}}", inner.join(" "))
                } else {
                    format!("[{}]", inner.join(" "))
                }
            }
            Value::Dict(_) => "-dict-".to_string(),
            Value::Operator(op) => format!("--{}--", op.name),
            Value::File(_) => "-file-".to_string(),
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
/// last handle to an array's backing store, tear it down iteratively
/// instead, reusing the store's own element vector as the worklist.
impl Drop for Object {
    fn drop(&mut self) {
        let Value::Array(a) = &self.value else {
            return;
        };
        if Rc::strong_count(a.data_rc()) != 1 {
            return;
        }
        let Value::Array(a) = std::mem::replace(&mut self.value, Value::Null) else {
            return;
        };
        let Ok(cell) = Rc::try_unwrap(a.data) else {
            return;
        };
        let mut pending = cell.into_inner();
        while let Some(mut obj) = pending.pop() {
            if let Value::Array(inner) = &obj.value
                && Rc::strong_count(inner.data_rc()) == 1
                && let Value::Array(inner) = std::mem::replace(&mut obj.value, Value::Null)
                && let Ok(cell) = Rc::try_unwrap(inner.data)
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

/// Non-name dictionary keys. Names (and strings, which convert to names
/// per the PLRM) stay in a dedicated name-keyed map — that's the hot path
/// for every operator lookup — while numbers, booleans, marks, and
/// composites-by-identity land here.
#[derive(Clone, PartialEq, Eq, Hash)]
enum ExoticKey {
    Int(i64),
    /// Reals with no exact integer value, keyed by bit pattern. Reals
    /// that *are* whole numbers normalize to `Int` so `3` and `3.0` are
    /// the same key, as the PLRM requires.
    RealBits(u64),
    Bool(bool),
    Mark,
    /// Arrays/strings-as-array-likes by identity: (storage ptr, off, len).
    ArrayId(usize, usize, usize),
    DictId(usize),
    OperatorId(usize),
    FileId(usize),
}

enum KeyClass {
    Name(Rc<str>),
    Exotic(ExoticKey),
}

fn classify_key(key: &Object) -> Result<KeyClass, PsError> {
    Ok(match &key.value {
        Value::Name(n) => KeyClass::Name(n.clone()),
        // Strings convert to names when used as keys, per the PLRM.
        Value::String(s) => KeyClass::Name(s.text().into()),
        Value::Integer(i) => KeyClass::Exotic(ExoticKey::Int(*i)),
        Value::Real(r) => {
            if r.fract() == 0.0 && (i64::MIN as f64..=i64::MAX as f64).contains(r) {
                KeyClass::Exotic(ExoticKey::Int(*r as i64))
            } else {
                KeyClass::Exotic(ExoticKey::RealBits(r.to_bits()))
            }
        }
        Value::Boolean(b) => KeyClass::Exotic(ExoticKey::Bool(*b)),
        Value::Mark => KeyClass::Exotic(ExoticKey::Mark),
        Value::Array(a) => {
            let (p, o, l) = a.identity();
            KeyClass::Exotic(ExoticKey::ArrayId(p, o, l))
        }
        Value::Dict(d) => KeyClass::Exotic(ExoticKey::DictId(Rc::as_ptr(d) as usize)),
        Value::Operator(op) => KeyClass::Exotic(ExoticKey::OperatorId(op.func as usize)),
        Value::File(f) => KeyClass::Exotic(ExoticKey::FileId(Rc::as_ptr(f) as usize)),
        // The one type the PLRM forbids as a key.
        Value::Null => return Err(PsError::Typecheck),
    })
}

/// A PostScript dictionary.
#[derive(Default)]
pub struct Dict {
    names: HashMap<Rc<str>, Object>,
    /// Exotic entries keep the original key object so `forall` and
    /// `copy` can hand it back.
    exotic: HashMap<ExoticKey, (Object, Object)>,
}

impl Dict {
    pub fn new() -> Self {
        Self::default()
    }

    /// Name-keyed fast path (operator lookup, `def`, `load`).
    pub fn get(&self, key: &str) -> Option<Object> {
        self.names.get(key).cloned()
    }

    pub fn put(&mut self, key: Rc<str>, value: Object) {
        self.names.insert(key, value);
    }

    /// General lookup with any PostScript key object.
    pub fn get_obj(&self, key: &Object) -> Result<Option<Object>, PsError> {
        Ok(match classify_key(key)? {
            KeyClass::Name(n) => self.names.get(&*n).cloned(),
            KeyClass::Exotic(k) => self.exotic.get(&k).map(|(_, v)| v.clone()),
        })
    }

    pub fn put_obj(&mut self, key: &Object, value: Object) -> Result<(), PsError> {
        match classify_key(key)? {
            KeyClass::Name(n) => {
                self.names.insert(n, value);
            }
            KeyClass::Exotic(k) => {
                self.exotic.insert(k, (key.clone(), value));
            }
        }
        Ok(())
    }

    pub fn known(&self, key: &Object) -> Result<bool, PsError> {
        Ok(match classify_key(key)? {
            KeyClass::Name(n) => self.names.contains_key(&*n),
            KeyClass::Exotic(k) => self.exotic.contains_key(&k),
        })
    }

    pub fn undef(&mut self, key: &Object) -> Result<(), PsError> {
        match classify_key(key)? {
            KeyClass::Name(n) => {
                self.names.remove(&*n);
            }
            KeyClass::Exotic(k) => {
                self.exotic.remove(&k);
            }
        }
        Ok(())
    }

    pub fn len(&self) -> usize {
        self.names.len() + self.exotic.len()
    }

    pub fn is_empty(&self) -> bool {
        self.names.is_empty() && self.exotic.is_empty()
    }

    /// Snapshot of (key, value) pairs for `forall`/`copy`. Name keys come
    /// back as literal name objects. Order is unspecified, as the PLRM
    /// allows.
    pub fn pairs(&self) -> Vec<(Object, Object)> {
        let mut out: Vec<(Object, Object)> = self
            .names
            .iter()
            .map(|(k, v)| (Object::lit(Value::Name(k.clone())), v.clone()))
            .collect();
        out.extend(self.exotic.values().cloned());
        out
    }
}
