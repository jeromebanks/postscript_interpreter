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
    /// `pathforall`: one path element per step — push its user-space
    /// coordinates, run the matching proc (move/line/curve/close).
    /// The path is snapshotted at operator time; holds no external
    /// state, so unwinding needs no cleanup.
    PathForall {
        elems: Vec<PathForallEl>,
        idx: usize,
        /// moveto, lineto, curveto, closepath procs, in that order.
        procs: Box<[Object; 4]>,
    },
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
    /// A one-shot continuation: an operator that must run a PostScript
    /// procedure mid-flight (a Separation tint transform) parks its
    /// completion here and pushes the procedure above; when the
    /// procedure finishes, the continuation consumes its results.
    /// Holds no external state, so unwinding needs no cleanup.
    PostOp(PostOp),
}

pub(crate) enum PostOp {
    /// A Separation tint transform finished: pop the alt-space
    /// components it produced and make them the current color (the
    /// color space itself stays Separation).
    SeparationColor { alt_ncomp: u32 },
}

/// One `pathforall` element, coordinates already in user space.
pub(crate) enum PathForallEl {
    Move(f64, f64),
    Line(f64, f64),
    Curve([f64; 6]),
    Close,
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
    /// The most recently executed name (interned id — resolving to
    /// text only happens at error time), for `OffendingCommand`.
    last_name: Option<u32>,
    /// The line most recently scanned directly from the main program
    /// source (issue #17) — `None` when no line is known, or the most
    /// recent scanning happened in a `run`-loaded file, eexec stream, or
    /// executable string (see `Lexer::line`). Sticky across procedure
    /// calls: an error deep inside a previously-defined procedure is
    /// attributed to the line that most recently touched real source
    /// text, not the procedure's original definition site — this
    /// interpreter doesn't tag objects with a source position.
    last_line: Option<usize>,
    /// State for `rand`/`srand`/`rrand`. Deterministic by default —
    /// reproducible art is a feature here, not a bug.
    pub(crate) rand_state: i64,
    /// `setpacking` flag — tracked, but packed arrays are ordinary
    /// arrays here (ops/level2.rs).
    pub(crate) packing: bool,
    /// `usertime`'s zero point.
    pub(crate) clock: crate::clock::Clock,
    /// Non-Font resource categories (ops/level2.rs). Font shares
    /// FontDirectory instead.
    resources: crate::ops::level2::CategoryMap,
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
        Self::with_page_scaled(width, height, 1.0)
    }

    /// A page of `width`x`height` *points* rendered at `scale` device
    /// pixels per point (`--dpi` divides by 72 to get this).
    pub fn with_page_scaled(width: u32, height: u32, scale: f32) -> Option<Self> {
        let px = |points: u32| ((points as f32 * scale).round() as u32).max(1);
        let (pw, ph) = (px(width), px(height));
        // Same ceiling as --page: keeps a huge dpi from attempting a
        // multi-gigabyte canvas.
        if pw > 8000 || ph > 8000 {
            return None;
        }
        let gfx = Gfx::with_scale(pw, ph, scale)?;
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
            last_line: None,
            rand_state: 1,
            packing: false,
            clock: crate::clock::Clock::start(),
            resources: Default::default(),
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
        self.last_line = None;
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

    /// Per-frame teardown shared by every unwind path — `unwind_all`
    /// (abort: an unhandled error or `quit`) and `do_stop` (recoverable:
    /// `stop`, or an error caught by an enclosing `stopped`). Any frame
    /// kind that holds state needing explicit cleanup when it's dropped
    /// mid-flight belongs here, not duplicated in both callers — missing
    /// one path is exactly how the eexec case (below) went unnoticed
    /// until issue #17's lint mode made it visible (round 5 of that
    /// PR's cross-model review: the first fix only covered
    /// `unwind_all`, missing that `stopped` catching an error/`stop`
    /// inside an eexec stream goes through `do_stop` instead — worse
    /// than the abort case, since execution *continues* afterward with
    /// a phantom systemdict on top of the dict stack, so a subsequent
    /// `def` lands in the wrong dictionary).
    fn cleanup_unwound_frame(&mut self, frame: Frame) {
        match frame {
            // Type 3 glyph contexts' graphics-state snapshot and paint
            // suppression must not leak past the show that created them.
            Frame::Show(mut ctx) => ctx.cleanup(&mut self.gfx),
            // `begin_eexec` pushes systemdict for the encrypted stream's
            // duration, popped on normal completion via
            // `Action::PopScannerAndDict`; a scanner dropped mid-flight
            // needs the same pop or systemdict ends up pushed twice.
            // Only when the *exact* dict `begin_eexec` pushed (by
            // identity, not merely "whatever's on top") is still there,
            // though: PostScript running inside the encrypted stream is
            // free to manage the dict stack itself (a Type 1 font's own
            // `currentdict end ... Private begin` is exactly this) —
            // `end`-ing the injected copy and `begin`-ning a
            // program-owned dict of its own before stopping/erroring
            // must not have that dict popped out from under it here
            // (round 6 of PR #59's review: an unconditional pop did).
            Frame::Scanner(lexer) if lexer.pop_systemdict && self.dstack.len() > 2 => {
                if let Some(top) = self.dstack.last()
                    && Rc::ptr_eq(top, &self.dstack[0])
                {
                    self.dstack.pop();
                }
            }
            _ => {}
        }
    }

    /// Abort the program: drop every frame (see `cleanup_unwound_frame`
    /// for what each kind needs).
    fn unwind_all(&mut self) {
        while let Some(frame) = self.estack.pop() {
            self.cleanup_unwound_frame(frame);
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
                    // A scan/syntax error short-circuits out of
                    // next_item before any Action ran, so the failing
                    // Scanner frame is still on top with its line set.
                    self.sync_scan_line();
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
            _ => self.last_name.map(|id| crate::name::resolve(id).text_rc()),
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
                d.put(
                    "command".into(),
                    Object::exec(Value::Name(crate::name::intern_rc(c))),
                );
            }
        }
    }

    /// Refresh `last_line` from the innermost frame if it's a
    /// main-program `Scanner` — a no-op (line stays sticky) otherwise.
    /// Called both when a token is successfully yielded *and* when
    /// scanning it failed (`next_token` sets its line before dispatch
    /// succeeds or fails, so the frame reflects the failing token's
    /// line either way — it just hasn't been popped yet, since the
    /// error propagated before any `Action` was applied).
    fn sync_scan_line(&mut self) {
        if let Some(Frame::Scanner(lexer)) = self.estack.last()
            && let Some(line) = lexer.line()
        {
            self.last_line = Some(line);
        }
    }

    /// LaserWriter-style error report, e.g.
    /// `%%[ Error: undefined; OffendingCommand: frobnicate ]%%`. Appends
    /// `; Line: N` (issue #17) when the most recent token scanned
    /// directly from the submitted program source is known — see
    /// `last_line`'s doc comment for what "known" excludes.
    pub fn error_report(&self, err: &PsError) -> String {
        let kind = match err {
            PsError::Syntax(detail) => format!("syntaxerror ({detail})"),
            _ => err.name().to_string(),
        };
        let command = match err {
            PsError::Undefined(name) => name.clone(),
            _ => self
                .last_executed_name()
                .unwrap_or_else(|| "--none--".to_string()),
        };
        match self.last_line {
            Some(line) => {
                format!("%%[ Error: {kind}; OffendingCommand: {command}; Line: {line} ]%%")
            }
            None => format!("%%[ Error: {kind}; OffendingCommand: {command} ]%%"),
        }
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
                    Frame::PathForall { elems, idx, procs } => match elems.get(*idx) {
                        None => Action::Pop,
                        Some(el) => {
                            let (coords, proc) = match el {
                                PathForallEl::Move(x, y) => {
                                    (vec![Object::real(*x), Object::real(*y)], procs[0].clone())
                                }
                                PathForallEl::Line(x, y) => {
                                    (vec![Object::real(*x), Object::real(*y)], procs[1].clone())
                                }
                                PathForallEl::Curve(c) => (
                                    c.iter().map(|v| Object::real(*v)).collect(),
                                    procs[2].clone(),
                                ),
                                PathForallEl::Close => (Vec::new(), procs[3].clone()),
                            };
                            *idx += 1;
                            Action::ExecWith(coords, proc)
                        }
                    },
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
                    Frame::PostOp(op) => {
                        match op {
                            PostOp::SeparationColor { alt_ncomp } => {
                                let n = *alt_ncomp as usize;
                                let mut comps = [0f64; 4];
                                for slot in comps[..n].iter_mut().rev() {
                                    let o = ostack.pop().ok_or(PsError::StackUnderflow)?;
                                    *slot = match o.value {
                                        Value::Integer(i) => i as f64,
                                        Value::Real(r) => r,
                                        _ => return Err(PsError::Typecheck),
                                    };
                                }
                                match n {
                                    1 => gfx.set_rgb(comps[0], comps[0], comps[0]),
                                    3 => gfx.set_rgb(comps[0], comps[1], comps[2]),
                                    _ => gfx.set_cmyk(comps[0], comps[1], comps[2], comps[3]),
                                }
                            }
                        }
                        Action::Pop
                    }
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
                Action::Yield(o) => {
                    self.sync_scan_line();
                    return Ok(Some(o));
                }
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
                    if let Some(frame) = self.estack.pop() {
                        self.cleanup_unwound_frame(frame);
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
                let id = n.id();
                self.last_name = Some(id);
                let resolved = self
                    .load_id(id)
                    .ok_or_else(|| PsError::Undefined(n.to_string()))?;
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
                    let id = n.id();
                    self.last_name = Some(id);
                    let next = self
                        .load_id(id)
                        .ok_or_else(|| PsError::Undefined(n.to_string()))?;
                    obj = next;
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

    /// Park `post` as a continuation, put `operands` on the operand
    /// stack, and run `proc` above it — the pattern for any operator
    /// that needs a PostScript procedure's result (never recurse).
    pub(crate) fn begin_postop(
        &mut self,
        post: PostOp,
        operands: Vec<Object>,
        proc: &Object,
    ) -> Result<(), PsError> {
        let Value::Array(body) = &proc.value else {
            return Err(PsError::Typecheck);
        };
        if !proc.executable {
            return Err(PsError::Typecheck);
        }
        self.push_frame(Frame::PostOp(post))?;
        for o in operands {
            self.push(o);
        }
        self.push_proc_frame(body.clone())
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

    pub(crate) fn begin_pathforall(
        &mut self,
        elems: Vec<PathForallEl>,
        procs: [Object; 4],
    ) -> Result<(), PsError> {
        self.push_frame(Frame::PathForall {
            elems,
            idx: 0,
            procs: Box::new(procs),
        })
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
            if matches!(frame, Frame::StopMark) {
                return true;
            }
            self.cleanup_unwound_frame(frame);
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
                | Some(
                    Frame::Scanner(_)
                    | Frame::StopMark
                    | Frame::Show(_)
                    | Frame::Image(_)
                    | Frame::PostOp(_),
                ) => {
                    return Err(PsError::InvalidExit);
                }
                Some(
                    Frame::Repeat { .. }
                    | Frame::Loop { .. }
                    | Frame::For { .. }
                    | Frame::Forall { .. }
                    | Frame::PathForall { .. },
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

    /// The instance dict for a non-Font resource category, created on
    /// first touch.
    pub(crate) fn resource_category(&mut self, category: &Rc<str>) -> Rc<RefCell<Dict>> {
        self.resources
            .entry(category.clone())
            .or_insert_with(|| Rc::new(RefCell::new(Dict::new())))
            .clone()
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

    /// The interned fast path for the same walk — what name execution
    /// uses (one integer hash per dictionary probed, no Rc traffic).
    #[inline]
    pub(crate) fn load_id(&self, id: u32) -> Option<Object> {
        self.dstack.iter().rev().find_map(|d| d.borrow().get_id(id))
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

    /// Self-check/lint mode (issue #17): heuristic checks for common
    /// silent-failure mistakes — see `crate::lint` for what's checked
    /// and why `render_checks` exists.
    pub fn lint(&self, render_checks: bool) -> Vec<crate::lint::LintFinding> {
        crate::lint::check(self, render_checks)
    }

    pub fn quit_requested(&self) -> bool {
        self.quit_requested
    }

    pub fn last_executed_name(&self) -> Option<String> {
        self.last_name
            .map(|id| crate::name::resolve(id).to_string())
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
        Token::Name(n) => Object::exec(Value::Name(crate::name::intern(&n))),
        Token::LiteralName(n) => Object::lit(Value::Name(crate::name::intern(&n))),
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

    #[test]
    fn error_report_names_the_line_the_error_happened_on() {
        let mut it = Interp::new();
        // A trailing newline is the case that catches an off-by-one: the
        // token `div` finishes and consumes that newline as its
        // delimiter (see lexer.rs's `eat_token_delimiter`) before `div`
        // actually runs and fails, so a naive "read the file's current
        // line" would report line 2 for an error that happened on line 1.
        let err = it.run_str("1 0 div\n").unwrap_err();
        let report = it.error_report(&err);
        assert!(report.contains("Line: 1"), "report: {report}");
    }

    #[test]
    fn error_report_line_advances_across_newlines() {
        let mut it = Interp::new();
        let err = it.run_str("1 1 add pop\n1 0 div\n").unwrap_err();
        let report = it.error_report(&err);
        assert!(report.contains("Line: 2"), "report: {report}");
    }

    #[test]
    fn error_report_line_stays_with_the_main_program_across_a_string() {
        let mut it = Interp::new();
        // "exec" is the last token scanned directly from the main
        // program (line 2); the executable string's own "div" isn't
        // main-program source and must not overwrite that with a
        // spurious line of its own (it would always be line 1).
        let err = it.run_str("42 pop\n(1 0 div) cvx exec").unwrap_err();
        let report = it.error_report(&err);
        assert!(report.contains("Line: 2"), "report: {report}");
    }

    #[test]
    fn error_report_lines_a_syntax_error_correctly() {
        // Regression test (Codex review, PR #59): a scan/syntax error
        // propagates out of next_item via `?` before any Action ever
        // runs, so the old Yield-only line sync never saw it — the
        // report kept whatever line the previous good token was on.
        // "1 pop" is line 1; the stray ')' that has no matching '(' is
        // on line 2.
        let mut it = Interp::new();
        let err = it.run_str("1 pop\n)").unwrap_err();
        let report = it.error_report(&err);
        assert!(report.contains("Line: 2"), "report: {report}");
    }

    #[test]
    fn error_report_counts_lone_cr_as_a_line_ending() {
        // Regression test (Codex review, PR #59): the file-level line
        // counter only advanced on '\n', missing old-Mac-style
        // lone-CR line endings (PostScript accepts CR, LF, and CRLF).
        let mut it = Interp::new();
        let err = it.run_str("1 pop\r1 0 div\r").unwrap_err();
        let report = it.error_report(&err);
        assert!(report.contains("Line: 2"), "report: {report}");
    }

    #[test]
    fn error_report_counts_a_crlf_pair_as_one_line_ending() {
        let mut it = Interp::new();
        let err = it.run_str("1 pop\r\n1 0 div\r\n").unwrap_err();
        let report = it.error_report(&err);
        assert!(report.contains("Line: 2"), "report: {report}");
    }

    #[test]
    fn error_report_lines_a_malformed_first_token() {
        // Same gap, worst case: the very first token is malformed, so
        // there's no earlier successfully-scanned token to fall back
        // on at all.
        let mut it = Interp::new();
        let err = it.run_str(")").unwrap_err();
        let report = it.error_report(&err);
        assert!(report.contains("Line: 1"), "report: {report}");
    }

    /// The eexec cipher (Type 1 spec, C1) — the inverse of
    /// `crate::file`'s decoder, duplicated here (as `tests/type1.rs`
    /// already does) rather than shared, since it's ten lines and this
    /// is the only other place that needs to construct an encrypted
    /// stream rather than decode one.
    fn eexec_encrypt(plain: &[u8]) -> Vec<u8> {
        let mut r: u16 = 55665;
        b"XXXX"
            .iter()
            .chain(plain)
            .map(|&p| {
                let c = p ^ (r >> 8) as u8;
                r = (u16::from(c).wrapping_add(r))
                    .wrapping_mul(52845)
                    .wrapping_add(22719);
                c
            })
            .collect()
    }

    #[test]
    fn an_error_inside_eexec_does_not_leave_a_phantom_dict_stack_entry() {
        // Regression test (Codex review round 4, PR #59): begin_eexec
        // pushes systemdict for the encrypted stream's duration,
        // popped when the scanner exhausts normally
        // (Action::PopScannerAndDict) -- but unwind_all (what runs on
        // an unhandled error or quit) used to drop the scanner frame
        // without the matching dict-stack pop, leaving systemdict
        // pushed a second time. Nothing about that is visible from
        // outside the crate except dict_stack_len(), which issue #17's
        // lint mode is now the first real consumer of -- it used to
        // misreport this as a "missing end" dict-leak.
        let mut it = Interp::new();
        let encrypted = eexec_encrypt(b"1 0 div");
        let mut src = b"currentfile eexec ".to_vec();
        src.extend(encrypted);
        it.run_source(&src).expect_err("1 0 div still errors");
        assert_eq!(
            it.dict_stack_len(),
            2,
            "systemdict/userdict only -- eexec's push must not survive an aborted run"
        );
    }

    #[test]
    fn a_caught_stop_inside_eexec_does_not_leave_a_phantom_dict_stack_entry() {
        // Regression test (Codex review round 5, PR #59): round 4 only
        // fixed unwind_all (the abort path); `stop` inside an eexec
        // stream -- or an error caught by an *enclosing* `stopped` --
        // unwinds via do_stop instead, which had the identical gap.
        // Worse than the abort case: execution continues afterward, so
        // a leftover systemdict push doesn't just mislead a diagnostic,
        // it silently redirects the next `def`.
        let mut it = Interp::new();
        // Two things this can't be built from: a `{...}` procedure
        // literal (parsed as an ordinary token stream up front, before
        // `eexec` ever runs, so the raw encrypted bytes would be
        // (mis)scanned as literal source) or an executable *string*
        // (`currentfile` explicitly skips non-file scanners per the
        // PLRM, so it would resolve to whatever real file happens to be
        // running further down the stack instead of this one). A `Value
        // ::File` pushed and executed directly — what `run` builds
        // internally — is a real file frame, so `currentfile` inside it
        // resolves to itself, same as a `run`-loaded program would see.
        //
        // A trailing "\n" in the plaintext, not just "stop": the scanner
        // peeks one byte past a token to consume its delimiter
        // (`eat_token_delimiter`) *before* the token is even returned
        // for execution, so without real plaintext there to satisfy
        // that peek it decodes whatever raw byte follows next using the
        // ongoing cipher stream — garbage, not this test's concern.
        // Real Type 1 fonts avoid this the same way, via trailing zero
        // padding after the encrypted section.
        let encrypted = eexec_encrypt(b"stop\n");
        let mut inner = b"currentfile eexec ".to_vec();
        inner.extend(encrypted);
        it.push(Object::exec(Value::File(crate::file::PsFile::from_bytes(
            inner,
        ))));
        it.run_str("stopped pop /probe 1 def")
            .expect("stopped catches it; overall run succeeds");
        assert_eq!(
            it.dict_stack_len(),
            2,
            "systemdict/userdict only -- eexec's push must not survive a caught stop"
        );
        assert_eq!(
            it.load("probe").expect("defined").repr(),
            "1",
            "def after the stopped block must land in userdict, not a phantom systemdict"
        );
    }

    #[test]
    fn a_program_owned_dict_left_open_by_eexec_survives_the_cleanup() {
        // Regression test (Codex review round 6, PR #59): round 5's fix
        // popped whatever was on top of the dict stack unconditionally,
        // assuming it must be eexec's injected systemdict copy — but
        // PostScript inside the encrypted stream can `end` that copy
        // itself and `begin` a dict of its own (a real Type 1 font's
        // `currentdict end ... Private begin` does exactly this) before
        // stopping. That program-owned dict must not be popped out from
        // under it just because *a* pop_systemdict scanner unwound —
        // only the exact dict `begin_eexec` pushed, by identity, should
        // ever be removed here.
        let mut it = Interp::new();
        let encrypted = eexec_encrypt(b"end 10 dict begin stop\n");
        let mut inner = b"currentfile eexec ".to_vec();
        inner.extend(encrypted);
        it.push(Object::exec(Value::File(crate::file::PsFile::from_bytes(
            inner,
        ))));
        it.run_str("stopped pop")
            .expect("stopped catches it; overall run succeeds");
        assert_eq!(
            it.dict_stack_len(),
            3,
            "systemdict/userdict + the program's own still-open dict, per the PLRM's \
             stopped-doesn't-restore-the-dict-stack rule -- not eexec's copy, which the \
             program's own `end` already removed, and not popped again by cleanup"
        );
    }

    #[test]
    fn eexec_completing_normally_pops_its_injected_dict_by_identity_not_position() {
        // Follow-up to round 6: that fix only landed in
        // cleanup_unwound_frame, the abort/stop path. The far more
        // common path -- an eexec stream simply running out of bytes
        // and completing normally -- goes through
        // Action::PopScannerAndDict instead, which still had the
        // original unconditional "pop whatever's on top" logic. A real
        // Type 1 font's `currentdict end ... Private begin` before
        // falling off the end of its encrypted section would lose its
        // own dict on the single most common eexec path of all.
        // Action::PopScannerAndDict now delegates to
        // cleanup_unwound_frame so both paths share one identity check.
        let mut it = Interp::new();
        let encrypted = eexec_encrypt(b"end 10 dict begin\n");
        let mut src = b"currentfile eexec ".to_vec();
        src.extend(encrypted);
        it.run_source(&src)
            .expect("plain completion, nothing errors");
        assert_eq!(
            it.dict_stack_len(),
            3,
            "systemdict/userdict + the program's own dict left open across eexec's natural \
             end -- not eexec's injected copy, which the program's own `end` already removed"
        );
    }
}
