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

use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::rc::Rc;

use crate::error::PsError;
use crate::font::{ShowCtx, ShowStep};
use crate::gfx::{DEFAULT_PAGE, Gfx, GraphicsState};
use crate::image::{ImageCtx, ImageStep};
use crate::lexer::{Lexer, Token};
use crate::object::{Dict, Num, Object, PsArray, PsString, SaveHandle, Value};
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
    /// The show family: one glyph per step; Type 3 glyphs and kshow
    /// procs run as ordinary frames above this one (see `crate::font`).
    Show(Box<ShowCtx>),
    /// image/imagemask/colorimage accumulating sample data; procedure
    /// data sources run as frames above (see `crate::image`).
    Image(Box<ImageCtx>),
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

/// One undo entry in the save/restore journal: the whole backing store
/// of an array or dict as it was when first mutated at the current save
/// level (object-granularity copy-on-write — see `VM.md`). Strings are
/// exempt from restore per PLRM §3.7.3.2, so they never appear here.
enum JEntry {
    Array {
        data: Rc<RefCell<Vec<Object>>>,
        old: Vec<Object>,
    },
    Dict {
        dict: Rc<RefCell<Dict>>,
        old: Dict,
    },
}

/// The interpreter-side record behind a `save` object.
struct SaveRecord {
    handle: Rc<SaveHandle>,
    /// Journal length at save time; restore undoes back to here.
    journal_mark: usize,
    /// Backing-store pointers already journaled at this level. The
    /// journal entry keeps the `Rc` alive, so pointer reuse can't alias.
    seen: HashSet<usize>,
    /// Graphics snapshot (same mechanism as Type 3 glyph contexts):
    /// restore = grestoreall to the save point.
    gfx_depth: usize,
    gfx_state: Box<GraphicsState>,
}

pub struct Interp {
    pub(crate) ostack: Vec<Object>,
    dstack: Vec<Rc<RefCell<Dict>>>,
    estack: Vec<Frame>,
    /// Mutation journal + live save records. Empty save stack = no
    /// journaling (the common, zero-overhead path).
    journal: Vec<JEntry>,
    save_stack: Vec<SaveRecord>,
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
        let system = Rc::new(RefCell::new(system));
        let user = Rc::new(RefCell::new(Dict::new()));
        // The permanent dicts are reachable by name, per the PLRM —
        // found code writes `userdict /x get` and `systemdict begin`
        // routinely. (systemdict referencing itself is an Rc cycle, so
        // it outlives the Interp — one bounded allocation per
        // interpreter, for an object with process lifetime anyway.)
        system.borrow_mut().put(
            "systemdict".into(),
            Object::lit(Value::Dict(system.clone())),
        );
        system
            .borrow_mut()
            .put("userdict".into(), Object::lit(Value::Dict(user.clone())));
        Some(Interp {
            ostack: Vec::new(),
            dstack: vec![system, user],
            estack: Vec::new(),
            journal: Vec::new(),
            save_stack: Vec::new(),
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
        self.estack
            .push(Frame::Scanner(Lexer::main_program(src.to_vec())));
    }

    /// Execute up to `budget` objects. `Ok(true)` means work remains. On
    /// error (or `quit`) the execution stack is cleared — the program is
    /// aborted — but the operand stack and canvas are left for inspection.
    pub fn step_n(&mut self, budget: usize) -> Result<bool, PsError> {
        let result = self.step_n_inner(budget);
        if result.is_err() || self.quit_requested {
            self.unwind_all();
        }
        result
    }

    /// Abort the program: drop every frame, sealing any Type 3 glyph
    /// contexts left open (their graphics-state snapshot and paint
    /// suppression must not leak past the show that created them).
    fn unwind_all(&mut self) {
        while let Some(frame) = self.estack.pop() {
            if let Frame::Show(mut ctx) = frame {
                ctx.cleanup(&mut self.gfx);
            }
        }
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
            // $error is a VM dict; its writes roll back like any other.
            self.journal_dict(&d);
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
            PopThenPush2(Object, Object),
            Iterate(PsArray),
            IterateWith(Object, PsArray),
            IterateWith2(Object, Object, PsArray),
            /// Show frame: push operands, then execute the target with
            /// procedure-call semantics (BuildChar, kshow proc).
            ExecWith(Vec<Object>, Object),
            /// Show frame did synchronous work; go around again.
            Nothing,
            /// An eexec source finished: it carried a systemdict push
            /// that ends with it.
            PopScannerAndDict,
        }
        loop {
            let action = {
                // Split borrow: the scanner needs the dictionary stack
                // (for //immediate names), the show frame the graphics
                // state, while the frame itself is borrowed.
                let Interp {
                    estack,
                    dstack,
                    gfx,
                    ostack,
                    ..
                } = self;
                let Some(frame) = estack.last_mut() else {
                    return Ok(None);
                };
                match frame {
                    Frame::Scanner(lexer) => match lexer.next_token()? {
                        None => {
                            if lexer.pop_systemdict {
                                Action::PopScannerAndDict
                            } else {
                                Action::Pop
                            }
                        }
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
                    Frame::Show(ctx) => match ctx.step(gfx)? {
                        ShowStep::Pop => Action::Pop,
                        ShowStep::PopPushWidth(wx, wy) => {
                            Action::PopThenPush2(Object::real(wx), Object::real(wy))
                        }
                        ShowStep::Exec { operands, target } => Action::ExecWith(operands, target),
                        ShowStep::Again => Action::Nothing,
                    },
                    Frame::Image(ctx) => {
                        if ctx.waiting() {
                            // The data-source procedure's result.
                            let s = ostack.pop().ok_or(PsError::StackUnderflow)?;
                            ctx.supply(s)?;
                        }
                        match ctx.step(gfx)? {
                            ImageStep::Exec(p) => Action::ExecWith(Vec::new(), p),
                            ImageStep::Done => Action::Pop,
                        }
                    }
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
                Action::PopThenPush2(a, b) => {
                    self.estack.pop();
                    self.push(a);
                    self.push(b);
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
                Action::ExecWith(operands, target) => {
                    for o in operands {
                        self.push(o);
                    }
                    self.exec_object(target)?;
                }
                Action::Nothing => {}
                Action::PopScannerAndDict => {
                    self.estack.pop();
                    if self.dstack.len() > 2 {
                        self.dstack.pop();
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
            match &obj.value {
                Value::Operator(op) => return (op.func)(self),
                Value::Array(body) => return self.push_proc_frame(body.clone()),
                // An executable string runs as source — the `(...) cvx
                // exec` idiom found code uses constantly.
                Value::String(s) => {
                    return self.push_frame(Frame::Scanner(Lexer::new(s.to_vec())));
                }
                // An executable file runs in place, sharing its position
                // with everything else holding the handle.
                Value::File(f) => {
                    return self.push_frame(Frame::Scanner(Lexer::from_file(f.clone())));
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

    /// The `token` operator on a file: one token scanned in place — the
    /// file's shared position advances exactly past it.
    pub(crate) fn scan_token_from_file(
        &self,
        file: crate::file::FileHandle,
    ) -> Result<Option<Object>, PsError> {
        let mut lexer = Lexer::from_file(file);
        Ok(self.scan_one(&mut lexer)?.map(|(obj, _)| obj))
    }

    /// The `token` operator: scan one token from raw bytes, returning the
    /// object and how many bytes the scanner consumed.
    pub(crate) fn scan_token_from(
        &self,
        bytes: Vec<u8>,
    ) -> Result<Option<(Object, usize)>, PsError> {
        let mut lexer = Lexer::new(bytes);
        self.scan_one(&mut lexer)
    }

    fn scan_one(&self, lexer: &mut Lexer) -> Result<Option<(Object, usize)>, PsError> {
        let obj = match lexer.next_token()? {
            None => return Ok(None),
            Some(Token::RBrace) => {
                return Err(PsError::Syntax("'}' with no matching '{'".to_string()));
            }
            Some(Token::LBrace) => scan_procedure(lexer, 0, &self.dstack)?,
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

    /// Queue a show (any of the family) as an execution-stack frame.
    pub(crate) fn begin_show(&mut self, ctx: ShowCtx) -> Result<(), PsError> {
        self.push_frame(Frame::Show(Box::new(ctx)))
    }

    /// Queue an image (image/imagemask/colorimage) frame.
    pub(crate) fn begin_image(&mut self, ctx: ImageCtx) -> Result<(), PsError> {
        self.push_frame(Frame::Image(Box::new(ctx)))
    }

    /// `currentfile`: the topmost *file* being executed. Executable
    /// strings scan through the same machinery but aren't files, per
    /// the PLRM, so they're skipped.
    pub(crate) fn current_file(&self) -> Option<crate::file::FileHandle> {
        self.estack.iter().rev().find_map(|f| match f {
            Frame::Scanner(lx) => lx.file().cloned(),
            _ => None,
        })
    }

    /// `eexec`: run a decrypting view of `file` as program source, with
    /// systemdict pushed for its duration (Type 1 fonts assume it).
    pub(crate) fn begin_eexec(&mut self, file: crate::file::FileHandle) -> Result<(), PsError> {
        let eexec_file = crate::file::PsFile::filtered(file, crate::file::Decoder::eexec());
        let mut lexer = Lexer::from_file(eexec_file);
        lexer.pop_systemdict = true;
        self.push_dict(self.dstack[0].clone());
        self.push_frame(Frame::Scanner(lexer))
    }

    /// setcachedevice/setcharwidth: hand the width to the innermost show
    /// whose glyph procedure is actually running. False if there is none
    /// — the operators are meaningless outside BuildChar.
    pub(crate) fn set_type3_glyph_width(&mut self, w: (f64, f64)) -> bool {
        for frame in self.estack.iter_mut().rev() {
            if let Frame::Show(ctx) = frame
                && ctx.has_pending()
            {
                ctx.set_glyph_width(w);
                return true;
            }
        }
        false
    }

    /// Unwind to (and including) the nearest StopMark. Returns whether a
    /// context existed; with none, the whole program is aborted, which is
    /// what the PLRM's outermost job-server `stopped` would do.
    pub(crate) fn do_stop(&mut self) -> bool {
        while let Some(frame) = self.estack.pop() {
            match frame {
                Frame::StopMark => return true,
                // Unwinding across an in-flight show must seal its glyph
                // context (see unwind_all).
                Frame::Show(mut ctx) => ctx.cleanup(&mut self.gfx),
                _ => {}
            }
        }
        false
    }

    /// `exit`: unwind to (and including) the innermost loop frame.
    /// Stops at a source boundary, a `stopped` boundary, or a show in
    /// progress (a BuildChar/kshow proc can't `exit` the show) — per the
    /// PLRM's `invalidexit`.
    pub(crate) fn exit_loop(&mut self) -> Result<(), PsError> {
        loop {
            match self.estack.last() {
                None
                | Some(Frame::Scanner(_) | Frame::StopMark | Frame::Show(_) | Frame::Image(_)) => {
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
        if let Some(top) = self.dstack.last().cloned() {
            self.journal_dict(&top);
            top.borrow_mut().put(name.into(), obj);
        }
    }

    // --- save/restore (design: VM.md) -----------------------------------

    /// Copy-on-write barrier: call *before* mutating an array's contents.
    /// First touch at the current save level snapshots the whole backing
    /// store; later touches are one hash lookup. No-op with no live save.
    pub(crate) fn journal_array(&mut self, a: &PsArray) {
        let Some(rec) = self.save_stack.last_mut() else {
            return;
        };
        let data = a.data_rc();
        if rec.seen.insert(Rc::as_ptr(data) as usize) {
            self.journal.push(JEntry::Array {
                data: data.clone(),
                old: data.borrow().clone(),
            });
        }
    }

    /// The dict counterpart of [`journal_array`].
    pub(crate) fn journal_dict(&mut self, d: &Rc<RefCell<Dict>>) {
        let Some(rec) = self.save_stack.last_mut() else {
            return;
        };
        if rec.seen.insert(Rc::as_ptr(d) as usize) {
            self.journal.push(JEntry::Dict {
                dict: d.clone(),
                old: d.borrow().clone(),
            });
        }
    }

    pub(crate) fn do_save(&mut self) -> Object {
        let (gfx_depth, gfx_state) = self.gfx.glyph_snapshot();
        let handle = Rc::new(SaveHandle {
            valid: Cell::new(true),
        });
        self.save_stack.push(SaveRecord {
            handle: handle.clone(),
            journal_mark: self.journal.len(),
            seen: HashSet::new(),
            gfx_depth,
            gfx_state,
        });
        Object::lit(Value::Save(handle))
    }

    pub(crate) fn do_restore(&mut self, obj: &Object) -> Result<(), PsError> {
        let Value::Save(h) = &obj.value else {
            return Err(PsError::Typecheck);
        };
        if !h.valid.get() {
            return Err(PsError::InvalidRestore);
        }
        let idx = self
            .save_stack
            .iter()
            .position(|r| Rc::ptr_eq(&r.handle, h))
            .ok_or(PsError::InvalidRestore)?;
        // Restoring this save discards any newer ones; their objects go
        // stale (a later restore on them is invalidrestore, per PLRM).
        for rec in &self.save_stack[idx..] {
            rec.handle.valid.set(false);
        }
        self.save_stack.truncate(idx + 1);
        let rec = self.save_stack.pop().expect("record at idx exists");
        while self.journal.len() > rec.journal_mark {
            match self.journal.pop().expect("length checked") {
                // Replacing contents while holding the borrow is safe
                // even for self-referential structures: the displaced
                // objects' Drop skips anything still multiply-owned,
                // and this cell is kept alive by `data`/`dict` itself.
                JEntry::Array { data, old } => *data.borrow_mut() = old,
                JEntry::Dict { dict, old } => *dict.borrow_mut() = old,
            }
        }
        // grestoreall to the save point, then drop the saved state —
        // pinned against gs (`save gsave gsave 5 setlinewidth restore`).
        self.gfx
            .restore_glyph_snapshot(rec.gfx_depth, rec.gfx_state);
        Ok(())
    }

    /// `vmstatus`'s save-nesting level.
    pub(crate) fn save_level(&self) -> usize {
        self.save_stack.len()
    }

    /// `grestoreall`: pop to the innermost save's boundary (keeping the
    /// boundary state available for its restore), or to the bottom.
    pub(crate) fn do_grestoreall(&mut self) {
        match self.save_stack.last() {
            Some(rec) => {
                let state = rec.gfx_state.clone();
                let depth = rec.gfx_depth;
                self.gfx.restore_glyph_snapshot(depth, state);
            }
            None => self.gfx.grestore_all_bottom(),
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
