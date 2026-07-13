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
use crate::object::{Dict, Num, Object, PsArray, PsString, Value};
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
    Proc { body: PsArray, pc: usize },
    /// `repeat`: run the body `remaining` more times.
    Repeat { body: PsArray, remaining: i64 },
    /// `loop`: run the body until `exit` (or an error) unwinds it.
    Loop { body: PsArray },
    /// `for`: push the control value, run the body, advance, repeat.
    For { body: PsArray, control: ForControl },
    /// `forall`: push element(s), run the body, advance.
    Forall { body: PsArray, src: ForallSrc },
    /// The boundary a `stopped` context plants: reached normally, it pops
    /// and pushes `false`; `stop` (or a recoverable error) unwinds to it
    /// and pushes `true`.
    StopMark,
}

pub(crate) enum ForallSrc {
    /// Elements are fetched live, so mutation mid-loop is visible —
    /// matching PostScript.
    Array(PsArray, usize),
    /// Bytes pushed as integers.
    Str(PsString, usize),
    /// Dict pairs are snapshotted when `forall` starts; the PLRM leaves
    /// order and mid-loop mutation behavior unspecified.
    Pairs(Vec<(Object, Object)>, usize),
}

pub(crate) struct ForControl {
    current: f64,
    increment: f64,
    limit: f64,
    /// All three operands were integers, so the pushed control values are
    /// integers too, per the PLRM.
    integral: bool,
}

impl ForControl {
    pub(crate) fn new(initial: Num, increment: Num, limit: Num) -> Self {
        let integral = matches!(
            (initial, increment, limit),
            (Num::Int(_), Num::Int(_), Num::Int(_))
        );
        ForControl {
            current: initial.to_f64(),
            increment: increment.to_f64(),
            limit: limit.to_f64(),
            integral,
        }
    }

    /// A zero increment never finishes — that matches PostScript, where
    /// such a loop runs until `exit`.
    fn finished(&self) -> bool {
        if self.increment >= 0.0 {
            self.current > self.limit
        } else {
            self.current < self.limit
        }
    }

    fn value(&self) -> Object {
        if self.integral {
            Object::int(self.current as i64)
        } else {
            Object::real(self.current)
        }
    }
}

pub struct Interp {
    pub(crate) ostack: Vec<Object>,
    dstack: Vec<Rc<RefCell<Dict>>>,
    estack: Vec<Frame>,
    pub(crate) quit_requested: bool,
    /// The most recently executed name, for `OffendingCommand` in error
    /// reports.
    last_name: Option<Rc<str>>,
    /// State for `rand`/`srand`/`rrand`. Deterministic by default —
    /// reproducible art is a feature here, not a bug.
    pub(crate) rand_state: i64,
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
        // Error machinery: $error is where recovered errors are recorded;
        // errordict exists for programs that expect it (custom handlers
        // in it are not yet consulted — see NOTES.md).
        system.put(
            "errordict".into(),
            Object::lit(Value::Dict(Rc::new(RefCell::new(Dict::new())))),
        );
        let mut error_state = Dict::new();
        error_state.put("newerror".into(), Object::bool(false));
        system.put(
            "$error".into(),
            Object::lit(Value::Dict(Rc::new(RefCell::new(error_state)))),
        );
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
            rand_state: 1,
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
            let obj = match self.next_item() {
                Ok(None) => return Ok(false),
                Ok(Some(o)) => o,
                Err(e) => {
                    self.recover(e)?;
                    continue;
                }
            };
            if let Err(e) = self.execute_element(obj) {
                self.recover(e)?;
            }
        }
        Ok(!self.estack.is_empty())
    }

    /// If a `stopped` context encloses the error, record it in `$error`,
    /// unwind to the context, and push `true` — errors are catchable.
    /// Otherwise propagate; the front end reports it.
    fn recover(&mut self, e: PsError) -> Result<(), PsError> {
        if !self.estack.iter().any(|f| matches!(f, Frame::StopMark)) {
            return Err(e);
        }
        self.record_error(&e);
        self.do_stop();
        self.push(Object::bool(true));
        Ok(())
    }

    fn record_error(&mut self, e: &PsError) {
        let command = match e {
            PsError::Undefined(n) => Some(Rc::from(n.as_str())),
            _ => self.last_name.clone(),
        };
        let error_dict = match self.load("$error") {
            Some(obj) => match &obj.value {
                Value::Dict(d) => Some(d.clone()),
                _ => None,
            },
            None => None,
        };
        if let Some(d) = error_dict {
            let mut d = d.borrow_mut();
            d.put("newerror".into(), Object::bool(true));
            d.put(
                "errorname".into(),
                Object::lit(Value::Name(e.name().into())),
            );
            if let Some(c) = command {
                d.put("command".into(), Object::exec(Value::Name(c)));
            }
        }
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
    /// exhausted frames and iterating loop frames as needed.
    fn next_item(&mut self) -> Result<Option<Object>, PsError> {
        // Frame inspection and stack mutation can't overlap borrows, so
        // each pass decides on an action first, then applies it.
        enum Action {
            Yield(Object),
            PopThenYield(Object),
            Pop,
            PopThenPush(Object),
            Iterate(PsArray),
            IterateWith(Object, PsArray),
            IterateWith2(Object, Object, PsArray),
        }
        loop {
            let action = {
                // Split borrow: the scanner needs the dictionary stack
                // (for //immediate names) while the frame is borrowed.
                let Interp { estack, dstack, .. } = self;
                let Some(frame) = estack.last_mut() else {
                    return Ok(None);
                };
                match frame {
                    Frame::Scanner(lexer) => match lexer.next_token()? {
                        None => Action::Pop,
                        Some(Token::RBrace) => {
                            return Err(PsError::Syntax("'}' with no matching '{'".to_string()));
                        }
                        Some(Token::LBrace) => Action::Yield(scan_procedure(lexer, 0, dstack)?),
                        Some(Token::ImmediateName(n)) => {
                            Action::Yield(resolve_immediate(&n, dstack)?)
                        }
                        Some(tok) => Action::Yield(token_to_object(tok)),
                    },
                    Frame::Proc { body, pc } => match body.get(*pc) {
                        None => Action::Pop,
                        Some(o) => {
                            *pc += 1;
                            // Tail call: the frame is finished the moment
                            // it yields its last element, so drop it
                            // before executing.
                            if *pc >= body.len() {
                                Action::PopThenYield(o)
                            } else {
                                Action::Yield(o)
                            }
                        }
                    },
                    Frame::Repeat { body, remaining } => {
                        if *remaining <= 0 {
                            Action::Pop
                        } else {
                            *remaining -= 1;
                            Action::Iterate(body.clone())
                        }
                    }
                    Frame::Loop { body } => Action::Iterate(body.clone()),
                    Frame::For { body, control } => {
                        if control.finished() {
                            Action::Pop
                        } else {
                            let v = control.value();
                            control.current += control.increment;
                            Action::IterateWith(v, body.clone())
                        }
                    }
                    Frame::Forall { body, src } => match src {
                        ForallSrc::Array(a, i) => match a.get(*i) {
                            None => Action::Pop,
                            Some(el) => {
                                *i += 1;
                                Action::IterateWith(el, body.clone())
                            }
                        },
                        ForallSrc::Str(s, i) => match s.byte(*i) {
                            None => Action::Pop,
                            Some(b) => {
                                *i += 1;
                                Action::IterateWith(Object::int(i64::from(b)), body.clone())
                            }
                        },
                        ForallSrc::Pairs(pairs, i) => match pairs.get(*i) {
                            None => Action::Pop,
                            Some((k, v)) => {
                                let (k, v) = (k.clone(), v.clone());
                                *i += 1;
                                Action::IterateWith2(k, v, body.clone())
                            }
                        },
                    },
                    // Reached from above means the stopped procedure ran
                    // to completion without stopping.
                    Frame::StopMark => Action::PopThenPush(Object::bool(false)),
                }
            };
            match action {
                Action::Yield(o) => return Ok(Some(o)),
                Action::PopThenYield(o) => {
                    self.estack.pop();
                    return Ok(Some(o));
                }
                Action::Pop => {
                    self.estack.pop();
                }
                Action::PopThenPush(o) => {
                    self.estack.pop();
                    self.push(o);
                }
                Action::Iterate(body) => self.push_proc_frame(body)?,
                Action::IterateWith(v, body) => {
                    self.push(v);
                    self.push_proc_frame(body)?;
                }
                Action::IterateWith2(a, b, body) => {
                    self.push(a);
                    self.push(b);
                    self.push_proc_frame(body)?;
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
            match &obj.value {
                Value::Operator(op) => return (op.func)(self),
                Value::Array(body) => return self.push_proc_frame(body.clone()),
                // An executable string runs as source — the `(...) cvx
                // exec` idiom found code uses constantly.
                Value::String(s) => {
                    return self.push_frame(Frame::Scanner(Lexer::new(s.to_vec())));
                }
                Value::Name(n) => {
                    hops += 1;
                    if hops > 100 {
                        return Err(PsError::Limitcheck);
                    }
                    let n = n.clone();
                    self.last_name = Some(n.clone());
                    obj = self
                        .load(&n)
                        .ok_or_else(|| PsError::Undefined(n.to_string()))?;
                }
                _ => {
                    self.push(obj);
                    return Ok(());
                }
            }
        }
    }

    fn push_proc_frame(&mut self, body: PsArray) -> Result<(), PsError> {
        if body.is_empty() {
            return Ok(());
        }
        if self.estack.len() >= EXEC_STACK_LIMIT {
            return Err(PsError::ExecStackOverflow);
        }
        self.estack.push(Frame::Proc { body, pc: 0 });
        Ok(())
    }

    fn push_frame(&mut self, frame: Frame) -> Result<(), PsError> {
        if self.estack.len() >= EXEC_STACK_LIMIT {
            return Err(PsError::ExecStackOverflow);
        }
        self.estack.push(frame);
        Ok(())
    }

    /// `exec` and the conditional operators: run an object with full
    /// executable semantics (procedures run rather than being pushed).
    pub(crate) fn exec_object(&mut self, obj: Object) -> Result<(), PsError> {
        self.execute_resolved(obj)
    }

    /// The `token` operator: scan one token from raw bytes, returning the
    /// object and how many bytes the scanner consumed.
    pub(crate) fn scan_token_from(
        &self,
        bytes: Vec<u8>,
    ) -> Result<Option<(Object, usize)>, PsError> {
        let mut lexer = Lexer::new(bytes);
        let obj = match lexer.next_token()? {
            None => return Ok(None),
            Some(Token::RBrace) => {
                return Err(PsError::Syntax("'}' with no matching '{'".to_string()));
            }
            Some(Token::LBrace) => scan_procedure(&mut lexer, 0, &self.dstack)?,
            Some(Token::ImmediateName(n)) => resolve_immediate(&n, &self.dstack)?,
            Some(tok) => token_to_object(tok),
        };
        Ok(Some((obj, lexer.pos())))
    }

    pub(crate) fn begin_repeat(&mut self, body: PsArray, count: i64) -> Result<(), PsError> {
        self.push_frame(Frame::Repeat {
            body,
            remaining: count,
        })
    }

    pub(crate) fn begin_loop(&mut self, body: PsArray) -> Result<(), PsError> {
        self.push_frame(Frame::Loop { body })
    }

    pub(crate) fn begin_for(&mut self, body: PsArray, control: ForControl) -> Result<(), PsError> {
        self.push_frame(Frame::For { body, control })
    }

    pub(crate) fn begin_forall(&mut self, body: PsArray, src: ForallSrc) -> Result<(), PsError> {
        self.push_frame(Frame::Forall { body, src })
    }

    /// Plant the boundary for a `stopped` context.
    pub(crate) fn begin_stopped(&mut self) -> Result<(), PsError> {
        self.push_frame(Frame::StopMark)
    }

    /// Unwind to (and including) the nearest StopMark. Returns whether a
    /// context existed; with none, the whole program is aborted, which is
    /// what the PLRM's outermost job-server `stopped` would do.
    pub(crate) fn do_stop(&mut self) -> bool {
        while let Some(frame) = self.estack.pop() {
            if matches!(frame, Frame::StopMark) {
                return true;
            }
        }
        false
    }

    /// `exit`: unwind to (and including) the innermost loop frame.
    /// Stops at a source boundary or a `stopped` boundary — `exit` can't
    /// jump out of either, per the PLRM's `invalidexit`.
    pub(crate) fn exit_loop(&mut self) -> Result<(), PsError> {
        loop {
            match self.estack.last() {
                None | Some(Frame::Scanner(_) | Frame::StopMark) => {
                    return Err(PsError::InvalidExit);
                }
                Some(
                    Frame::Repeat { .. }
                    | Frame::Loop { .. }
                    | Frame::For { .. }
                    | Frame::Forall { .. },
                ) => {
                    self.estack.pop();
                    return Ok(());
                }
                Some(Frame::Proc { .. }) => {
                    self.estack.pop();
                }
            }
        }
    }

    // --- dictionary-stack access for the dict operators -----------------

    pub(crate) fn current_dict(&self) -> Rc<RefCell<Dict>> {
        // The dict stack never drops below systemdict/userdict.
        self.dstack
            .last()
            .cloned()
            .unwrap_or_else(|| unreachable!("dict stack is never empty"))
    }

    pub(crate) fn dict_stack_len(&self) -> usize {
        self.dstack.len()
    }

    pub(crate) fn clear_dict_stack(&mut self) {
        self.dstack.truncate(2);
    }

    /// `where`: the topmost dictionary defining `key`, if any.
    pub(crate) fn find_defining_dict(
        &self,
        key: &Object,
    ) -> Result<Option<Rc<RefCell<Dict>>>, PsError> {
        for d in self.dstack.iter().rev() {
            if d.borrow().known(key)? {
                return Ok(Some(d.clone()));
            }
        }
        Ok(None)
    }

    pub(crate) fn push_dict(&mut self, dict: Rc<RefCell<Dict>>) {
        self.dstack.push(dict);
    }

    /// `end`: systemdict and userdict are permanent, per the PLRM.
    pub(crate) fn pop_dict(&mut self) -> Result<(), PsError> {
        if self.dstack.len() <= 2 {
            return Err(PsError::DictStackUnderflow);
        }
        self.dstack.pop();
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
        // All callers consume brace and immediate-name tokens before
        // converting, so these cannot reach here.
        Token::LBrace | Token::RBrace | Token::ImmediateName(_) => {
            unreachable!("structural tokens handled by the scanner")
        }
    }
}

/// `//name`: substituted with its current value at scan time — the value
/// is *not* executed, even if it's a procedure.
fn resolve_immediate(name: &str, dicts: &[Rc<RefCell<Dict>>]) -> Result<Object, PsError> {
    for d in dicts.iter().rev() {
        if let Some(v) = d.borrow().get(name) {
            return Ok(v);
        }
    }
    Err(PsError::Undefined(name.to_string()))
}

/// Collect tokens up to the matching `}` into a procedure object. Called
/// with the opening `{` already consumed.
fn scan_procedure(
    lexer: &mut Lexer,
    depth: usize,
    dicts: &[Rc<RefCell<Dict>>],
) -> Result<Object, PsError> {
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
            Some(Token::LBrace) => items.push(scan_procedure(lexer, depth + 1, dicts)?),
            Some(Token::ImmediateName(n)) => items.push(resolve_immediate(&n, dicts)?),
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
