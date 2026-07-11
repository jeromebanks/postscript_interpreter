//! The execution machine: operand stack, dictionary stack, and an explicit
//! execution stack.
//!
//! Execution uses an explicit stack of frames rather than host recursion,
//! for three reasons that all pay off in later stages:
//!
//! 1. Deeply recursive PostScript programs must not consume Rust call
//!    stack — depth is bounded by our own limit, and overflow becomes a
//!    catchable `execstackoverflow` instead of a process abort.
//! 2. The machine can be stepped one object at a time, which is the hook
//!    Stage 2's live rendering will drive (run N steps, present a frame).
//! 3. A procedure frame that has yielded its last element is popped
//!    *before* that element executes, so tail-recursive PostScript runs in
//!    constant execution-stack space for free.

use std::cell::RefCell;
use std::rc::Rc;

use crate::error::PsError;
use crate::gfx::{DEFAULT_PAGE, Gfx};
use crate::lexer::{Lexer, Token};
use crate::object::{Dict, Num, Object, Value};
use crate::ops;

/// Generous enough for any sane program (procedure calls in tail position
/// don't consume frames at all), small enough that runaway recursion is an
/// error rather than unbounded memory growth.
const EXEC_STACK_LIMIT: usize = 10_000;

/// Nesting cap for `{ { { ... } } }` at scan time; the scanner recurses per
/// nesting level, so this bounds Rust stack use on malicious input.
const PROC_NESTING_LIMIT: usize = 500;

enum Frame {
    /// Tokens being scanned incrementally from program source.
    Scanner(Lexer),
    /// A procedure (executable array) being executed element by element.
    Proc {
        body: Rc<RefCell<Vec<Object>>>,
        pc: usize,
    },
}

pub struct Interp {
    pub(crate) ostack: Vec<Object>,
    dstack: Vec<Rc<RefCell<Dict>>>,
    estack: Vec<Frame>,
    pub(crate) quit_requested: bool,
    /// The most recently executed name, for `OffendingCommand` in error
    /// reports.
    last_name: Option<Rc<str>>,
    pub(crate) gfx: Gfx,
}

impl Interp {
    pub fn new() -> Self {
        // The default page size is a nonzero constant; this cannot fail.
        Self::with_page(DEFAULT_PAGE.0, DEFAULT_PAGE.1).expect("default page size is valid")
    }

    /// `None` if the page dimensions are unusable (zero or too large for
    /// a pixmap).
    pub fn with_page(width: u32, height: u32) -> Option<Self> {
        let gfx = Gfx::new(width, height)?;
        let mut system = Dict::new();
        ops::install_all(&mut system);
        Some(Interp {
            ostack: Vec::new(),
            dstack: vec![
                Rc::new(RefCell::new(system)),
                // userdict; empty until `def` lands in Stage 3, but having
                // the two-dict stack from day one means name resolution
                // semantics don't change later.
                Rc::new(RefCell::new(Dict::new())),
            ],
            estack: Vec::new(),
            quit_requested: false,
            last_name: None,
            gfx,
        })
    }

    pub fn run_str(&mut self, src: &str) -> Result<(), PsError> {
        self.run_source(src.as_bytes())
    }

    pub fn run_source(&mut self, src: &[u8]) -> Result<(), PsError> {
        self.begin_source(src);
        while self.step_n(4096)? {}
        Ok(())
    }

    /// Queue a program for execution without running it; drive it with
    /// [`step_n`](Self::step_n). This is the live-rendering entry point.
    pub fn begin_source(&mut self, src: &[u8]) {
        self.quit_requested = false;
        self.last_name = None;
        self.estack.push(Frame::Scanner(Lexer::new(src.to_vec())));
    }

    /// Execute up to `budget` objects. `Ok(true)` means work remains. On
    /// error (or `quit`) the execution stack is cleared — the program is
    /// aborted — but the operand stack and canvas are left for inspection.
    pub fn step_n(&mut self, budget: usize) -> Result<bool, PsError> {
        let result = self.step_n_inner(budget);
        if result.is_err() || self.quit_requested {
            self.estack.clear();
        }
        result
    }

    fn step_n_inner(&mut self, budget: usize) -> Result<bool, PsError> {
        for _ in 0..budget {
            if self.quit_requested {
                return Ok(false);
            }
            let Some(obj) = self.next_item()? else {
                return Ok(false);
            };
            self.execute_element(obj)?;
        }
        Ok(!self.estack.is_empty())
    }

    /// LaserWriter-style error report, e.g.
    /// `%%[ Error: undefined; OffendingCommand: frobnicate ]%%`.
    pub fn error_report(&self, err: &PsError) -> String {
        let kind = match err {
            PsError::Syntax(detail) => format!("syntaxerror ({detail})"),
            _ => err.name().to_string(),
        };
        let command = match err {
            PsError::Undefined(name) => name.clone(),
            _ => self.last_executed_name().unwrap_or("--none--").to_string(),
        };
        format!("%%[ Error: {kind}; OffendingCommand: {command} ]%%")
    }

    /// Pull the next object to execute off the execution stack, popping
    /// exhausted frames as they empty.
    fn next_item(&mut self) -> Result<Option<Object>, PsError> {
        loop {
            let Some(frame) = self.estack.last_mut() else {
                return Ok(None);
            };
            match frame {
                Frame::Scanner(lexer) => match lexer.next_token()? {
                    None => {
                        self.estack.pop();
                    }
                    Some(Token::RBrace) => {
                        return Err(PsError::Syntax("'}' with no matching '{'".to_string()));
                    }
                    Some(Token::LBrace) => return Ok(Some(scan_procedure(lexer, 0)?)),
                    Some(tok) => return Ok(Some(token_to_object(tok))),
                },
                Frame::Proc { body, pc } => {
                    let (obj, done) = {
                        let b = body.borrow();
                        if *pc >= b.len() {
                            (None, true)
                        } else {
                            let o = b[*pc].clone();
                            *pc += 1;
                            (Some(o), *pc >= b.len())
                        }
                    };
                    // Tail call: the frame is finished the moment it yields
                    // its last element, so drop it before executing.
                    if done {
                        self.estack.pop();
                    }
                    if let Some(o) = obj {
                        return Ok(Some(o));
                    }
                }
            }
        }
    }

    /// Execute one object encountered in a token stream or procedure body.
    /// Only executable names and operators actually execute here; an
    /// executable array in this position is *pushed* (that's what makes
    /// `{...}` a deferred procedure), as is everything literal.
    fn execute_element(&mut self, obj: Object) -> Result<(), PsError> {
        match (&obj.value, obj.executable) {
            (Value::Name(n), true) => {
                let name = n.clone();
                self.last_name = Some(name.clone());
                let resolved = self
                    .load(&name)
                    .ok_or_else(|| PsError::Undefined(name.to_string()))?;
                self.execute_resolved(resolved)
            }
            (Value::Operator(op), true) => (op.func)(self),
            _ => {
                self.push(obj);
                Ok(())
            }
        }
    }

    /// Execute the object a name resolved to. Unlike `execute_element`,
    /// an executable array here *runs* — this is a procedure call.
    fn execute_resolved(&mut self, mut obj: Object) -> Result<(), PsError> {
        // A name may resolve to another executable name; follow the chain
        // with a budget so `a -> b -> a` cycles error out instead of
        // spinning forever.
        let mut hops = 0;
        loop {
            if !obj.executable {
                self.push(obj);
                return Ok(());
            }
            match obj.value {
                Value::Operator(op) => return (op.func)(self),
                Value::Array(body) => return self.push_proc_frame(body),
                Value::Name(ref n) => {
                    hops += 1;
                    if hops > 100 {
                        return Err(PsError::Limitcheck);
                    }
                    self.last_name = Some(n.clone());
                    obj = self
                        .load(n)
                        .ok_or_else(|| PsError::Undefined(n.to_string()))?;
                }
                _ => {
                    self.push(obj);
                    return Ok(());
                }
            }
        }
    }

    fn push_proc_frame(&mut self, body: Rc<RefCell<Vec<Object>>>) -> Result<(), PsError> {
        if body.borrow().is_empty() {
            return Ok(());
        }
        if self.estack.len() >= EXEC_STACK_LIMIT {
            return Err(PsError::ExecStackOverflow);
        }
        self.estack.push(Frame::Proc { body, pc: 0 });
        Ok(())
    }

    /// Look a name up through the dictionary stack, top down.
    pub fn load(&self, name: &str) -> Option<Object> {
        self.dstack.iter().rev().find_map(|d| d.borrow().get(name))
    }

    /// Define a name in the topmost dictionary (what `def` will do once
    /// control flow lands; already used by tests and available to embedders).
    pub fn define(&mut self, name: &str, obj: Object) {
        if let Some(top) = self.dstack.last() {
            top.borrow_mut().put(name.into(), obj);
        }
    }

    pub fn operand_stack(&self) -> &[Object] {
        &self.ostack
    }

    pub fn quit_requested(&self) -> bool {
        self.quit_requested
    }

    pub fn last_executed_name(&self) -> Option<&str> {
        self.last_name.as_deref()
    }

    pub fn push(&mut self, obj: Object) {
        self.ostack.push(obj);
    }

    pub fn pop(&mut self) -> Result<Object, PsError> {
        self.ostack.pop().ok_or(PsError::StackUnderflow)
    }

    pub fn pop_num(&mut self) -> Result<Num, PsError> {
        match self.pop()?.value {
            Value::Integer(i) => Ok(Num::Int(i)),
            Value::Real(r) => Ok(Num::Real(r)),
            _ => Err(PsError::Typecheck),
        }
    }

    pub fn pop_int(&mut self) -> Result<i64, PsError> {
        match self.pop()?.value {
            Value::Integer(i) => Ok(i),
            _ => Err(PsError::Typecheck),
        }
    }

    pub fn pop_f64(&mut self) -> Result<f64, PsError> {
        Ok(self.pop_num()?.to_f64())
    }

    pub fn gfx(&self) -> &Gfx {
        &self.gfx
    }

    pub fn gfx_mut(&mut self) -> &mut Gfx {
        &mut self.gfx
    }
}

impl Default for Interp {
    fn default() -> Self {
        Self::new()
    }
}

fn token_to_object(tok: Token) -> Object {
    match tok {
        Token::Integer(i) => Object::int(i),
        Token::Real(r) => Object::real(r),
        Token::String(bytes) => Object::string(bytes),
        Token::Name(n) => Object::exec(Value::Name(n.into())),
        Token::LiteralName(n) => Object::lit(Value::Name(n.into())),
        // Both callers (next_item and scan_procedure) consume brace tokens
        // before converting, so these cannot reach here.
        Token::LBrace | Token::RBrace => unreachable!("brace tokens handled by the scanner"),
    }
}

/// Collect tokens up to the matching `}` into a procedure object. Called
/// with the opening `{` already consumed.
fn scan_procedure(lexer: &mut Lexer, depth: usize) -> Result<Object, PsError> {
    if depth >= PROC_NESTING_LIMIT {
        return Err(PsError::Limitcheck);
    }
    let mut items = Vec::new();
    loop {
        match lexer.next_token()? {
            None => {
                return Err(PsError::Syntax(
                    "unterminated procedure: missing '}'".to_string(),
                ));
            }
            Some(Token::RBrace) => break,
            Some(Token::LBrace) => items.push(scan_procedure(lexer, depth + 1)?),
            Some(tok) => items.push(token_to_object(tok)),
        }
    }
    Ok(Object::procedure(items))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn procedure_is_deferred_and_calls_work_via_names() {
        let mut it = Interp::new();
        it.run_str("{2 mul}").expect("scan proc");
        assert_eq!(it.operand_stack().len(), 1);
        let proc = it.pop().expect("proc on stack");
        assert_eq!(proc.repr(), "{2 mul}");

        it.define("double", proc);
        it.run_str("5 double double").expect("call proc");
        assert_eq!(it.pop().expect("result").repr(), "20");
    }

    #[test]
    fn name_resolution_cycles_are_limitcheck() {
        let mut it = Interp::new();
        it.define("a", Object::exec(Value::Name("b".into())));
        it.define("b", Object::exec(Value::Name("a".into())));
        assert_eq!(it.run_str("a"), Err(PsError::Limitcheck));
    }

    #[test]
    fn deep_brace_nesting_is_limitcheck_not_stack_overflow() {
        let src = "{".repeat(2000);
        let mut it = Interp::new();
        assert_eq!(it.run_str(&src), Err(PsError::Limitcheck));
    }

    #[test]
    fn literal_name_pushes_itself() {
        let mut it = Interp::new();
        it.run_str("/foo").expect("literal name");
        assert_eq!(it.pop().expect("name").repr(), "/foo");
    }
}
