//! Interned names — Stage 8 task 2.
//!
//! PostScript programs execute names constantly (every operator call
//! is a name lookup), and hashing the *text* of `Rc<str>` keys on
//! every dictionary probe was the measured bottleneck (`benches/`:
//! fib 27 at ~7× gs before this module). A [`PsName`] is the text
//! plus a process-wide (per-thread) integer id assigned on first
//! sight; dictionaries key their name entries by id, so a lookup
//! hashes one `u32` instead of a string.
//!
//! The interner is thread-local: the interpreter is single-threaded
//! by design (`Rc` composites make `Interp: !Send`), and multiple
//! `Interp`s on one thread share ids harmlessly. Names live for the
//! life of the thread, matching PostScript's own name semantics —
//! the table only grows, and that's correct.

use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::ops::Deref;
use std::rc::Rc;

/// An interned name: id for identity/hashing, text for display and
/// the many call sites that match on `&*name`.
#[derive(Clone)]
pub struct PsName {
    id: u32,
    text: Rc<str>,
}

impl PsName {
    pub fn id(&self) -> u32 {
        self.id
    }

    /// The shared text handle (cheap clone).
    pub fn text_rc(&self) -> Rc<str> {
        self.text.clone()
    }
}

impl Deref for PsName {
    type Target = str;
    fn deref(&self) -> &str {
        &self.text
    }
}

impl PartialEq for PsName {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}
impl Eq for PsName {}

impl Hash for PsName {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

impl fmt::Display for PsName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text)
    }
}

impl fmt::Debug for PsName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "/{}", self.text)
    }
}

impl From<&str> for PsName {
    fn from(s: &str) -> Self {
        intern(s)
    }
}

#[derive(Default)]
struct Interner {
    by_text: HashMap<Rc<str>, u32>,
    by_id: Vec<Rc<str>>,
}

thread_local! {
    static INTERNER: RefCell<Interner> = RefCell::new(Interner::default());
}

pub fn intern(s: &str) -> PsName {
    INTERNER.with(|i| {
        let mut i = i.borrow_mut();
        if let Some(&id) = i.by_text.get(s) {
            return PsName {
                id,
                text: i.by_id[id as usize].clone(),
            };
        }
        let text: Rc<str> = s.into();
        let id = i.by_id.len() as u32;
        i.by_id.push(text.clone());
        i.by_text.insert(text.clone(), id);
        PsName { id, text }
    })
}

/// Intern reusing an existing allocation.
pub fn intern_rc(s: Rc<str>) -> PsName {
    INTERNER.with(|i| {
        let mut i = i.borrow_mut();
        if let Some(&id) = i.by_text.get(&*s) {
            return PsName {
                id,
                text: i.by_id[id as usize].clone(),
            };
        }
        let id = i.by_id.len() as u32;
        i.by_id.push(s.clone());
        i.by_text.insert(s.clone(), id);
        PsName { id, text: s }
    })
}

/// Id lookup **without** growing the table — the read-only probes
/// (`Dict::get(&str)`) must not accumulate garbage names.
pub fn lookup(s: &str) -> Option<u32> {
    INTERNER.with(|i| i.borrow().by_text.get(s).copied())
}

/// Resolve an id back to a full name (for `Dict::pairs`).
pub fn resolve(id: u32) -> PsName {
    INTERNER.with(|i| PsName {
        id,
        text: i.borrow().by_id[id as usize].clone(),
    })
}

/// `BuildHasher` for id-keyed maps: one multiply instead of SipHash.
/// (Ids are small and sequential; the golden-ratio multiply spreads
/// them across the table.)
#[derive(Default, Clone)]
pub struct IdHashBuilder;

pub struct IdHasher(u64);

impl std::hash::BuildHasher for IdHashBuilder {
    type Hasher = IdHasher;
    fn build_hasher(&self) -> IdHasher {
        IdHasher(0)
    }
}

impl Hasher for IdHasher {
    fn write(&mut self, _bytes: &[u8]) {
        unreachable!("IdHasher is for u32 name ids only");
    }
    fn write_u32(&mut self, n: u32) {
        self.0 = u64::from(n).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    }
    fn finish(&self) -> u64 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_text_same_id() {
        let a = intern("moveto");
        let b = intern("moveto");
        assert_eq!(a, b);
        assert_eq!(a.id(), b.id());
        assert!(Rc::ptr_eq(&a.text_rc(), &b.text_rc()), "one allocation");
    }

    #[test]
    fn lookup_does_not_grow() {
        assert_eq!(lookup("never-interned-xyzzy"), None);
        assert_eq!(lookup("never-interned-xyzzy"), None);
    }

    #[test]
    fn deref_matches_text() {
        let n = intern("show");
        assert_eq!(&*n, "show");
        assert_eq!(resolve(n.id()).text_rc(), n.text_rc());
    }
}
