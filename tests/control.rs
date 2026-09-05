//! Stage 3 end-to-end tests: control flow, procedures, recursion,
//! dictionaries, comparisons.

use pscat::{Interp, PsError};

fn eval(src: &str) -> Vec<String> {
    let mut it = Interp::new();
    it.run_str(src)
        .unwrap_or_else(|e| panic!("eval of {src:?} failed: {e}"));
    it.operand_stack().iter().map(|o| o.repr()).collect()
}

fn eval_err(src: &str) -> PsError {
    let mut it = Interp::new();
    match it.run_str(src) {
        Ok(()) => panic!("expected {src:?} to fail, stack: {:?}", it.operand_stack()),
        Err(e) => e,
    }
}

#[test]
fn comparisons() {
    assert_eq!(eval("1 2 eq 2 2 eq 2 2.0 eq"), ["false", "true", "true"]);
    assert_eq!(eval("(abc) (abc) eq (abc) (abd) eq"), ["true", "false"]);
    // A name and a string with the same text are equal, per the PLRM.
    assert_eq!(eval("/abc (abc) eq"), ["true"]);
    assert_eq!(eval("1 2 ne"), ["true"]);
    assert_eq!(
        eval("1 2 lt 2 2 le 3 2 gt 2 3 ge"),
        ["true", "true", "true", "false"]
    );
    assert_eq!(eval("(abc) (abd) lt"), ["true"]);
    assert_eq!(eval("1 1.5 lt"), ["true"]);
    // Composites compare by identity, not content.
    assert_eq!(eval("[1 2] [1 2] eq"), ["false"]);
    assert_eq!(eval("[1 2] dup eq"), ["true"]);
}

#[test]
fn boolean_and_bitwise() {
    assert_eq!(
        eval("true false and true false or true false xor"),
        ["false", "true", "true"]
    );
    assert_eq!(eval("6 3 and 6 3 or 6 3 xor"), ["2", "7", "5"]);
    assert_eq!(eval("true not 5 not"), ["false", "-6"]);
    assert_eq!(eval_err("true 3 and"), PsError::Typecheck);
}

#[test]
fn if_and_ifelse() {
    assert_eq!(eval("true {1} if"), ["1"]);
    assert_eq!(eval("false {1} if"), Vec::<String>::new());
    assert_eq!(eval("true {1} {2} ifelse"), ["1"]);
    assert_eq!(eval("false {1} {2} ifelse"), ["2"]);
    assert_eq!(eval("1 2 lt {(yes)} {(no)} ifelse"), ["(yes)"]);
    assert_eq!(eval_err("1 {2} if"), PsError::Typecheck);
}

#[test]
fn exec_runs_things() {
    assert_eq!(eval("{1 2 add} exec"), ["3"]);
    // exec of a literal pushes it back.
    assert_eq!(eval("5 exec"), ["5"]);
    assert_eq!(eval("/dup exec"), ["/dup"]);
}

#[test]
fn repeat_loop_for() {
    assert_eq!(eval("4 {1} repeat"), ["1", "1", "1", "1"]);
    assert_eq!(eval("0 {1} repeat"), Vec::<String>::new());
    assert_eq!(eval("3 {} repeat"), Vec::<String>::new());
    assert_eq!(eval("1 1 5 {} for"), ["1", "2", "3", "4", "5"]);
    assert_eq!(eval("0 2 6 {} for"), ["0", "2", "4", "6"]);
    assert_eq!(eval("5 -1 1 {} for"), ["5", "4", "3", "2", "1"]);
    assert_eq!(eval("0 0.5 1 {} for"), ["0.0", "0.5", "1.0"]);
    // Body runs with the control value on the stack.
    assert_eq!(eval("0 1 1 4 {add} for"), ["10"]);
    assert_eq!(eval("0 { 1 add dup 5 eq {exit} if } loop"), ["5"]);
    // exit terminates repeat early: counts 1, 2, 3 and bails at 3.
    assert_eq!(eval("0 4 { 1 add dup 2 gt {exit} if } repeat"), ["3"]);
}

#[test]
fn exit_outside_a_loop_is_invalidexit() {
    assert_eq!(eval_err("exit"), PsError::InvalidExit);
    // exit inside a plain procedure, but no loop anywhere.
    assert_eq!(eval_err("{exit} exec"), PsError::InvalidExit);
}

/// Issue #142. A `stop` that reaches the top level used to end
/// execution silently, losing an error that was still pending in
/// `$error`. That made the standard cleanup idiom
/// `{ userproc } stopped { restore-my-state stop } if` unusable: a
/// library wrapping a caller's procedure had to choose between leaking
/// its own state and swallowing the failure outright.
///
/// All three cases were checked against Ghostscript directly, including
/// the last one -- a plain control-flow `stop` after an earlier
/// *handled* error reports that stale error there too.
#[test]
fn a_top_level_stop_reports_the_pending_error() {
    // Re-raised after a catch: the original error, not silence.
    assert_eq!(
        eval_err("{ nosuchname } stopped { stop } if"),
        PsError::Undefined("nosuchname".to_string())
    );
    // The payload survives, so `OffendingCommand` still names the cause.
    assert_eq!(
        eval_err("{ othername } stopped { stop } if"),
        PsError::Undefined("othername".to_string())
    );
    // Cleanup between the catch and the re-raise still happens.
    assert_eq!(
        eval("/cleaned false def { nosuchname } stopped { /cleaned true def } if cleaned"),
        ["true"]
    );
}

/// ...but a `stop` with no error ever recorded stays silent, matching
/// Ghostscript. Gating on `$error /newerror` is what separates the two,
/// and without it every control-flow `stop` would become an error.
#[test]
fn a_top_level_stop_with_no_pending_error_is_silent() {
    let mut it = Interp::new();
    it.run_str("1 2 stop 99")
        .expect("a bare stop with no pending error must not raise");
    // It still ends the program: the 99 never runs.
    assert_eq!(
        it.operand_stack()
            .iter()
            .map(|o| o.repr())
            .collect::<Vec<_>>(),
        ["1", "2"]
    );
}

#[test]
fn def_and_dictionaries() {
    assert_eq!(eval("/x 5 def x"), ["5"]);
    assert_eq!(eval("/x 5 def /x 6 def x"), ["6"]);
    assert_eq!(eval("/sq {dup mul} def 7 sq"), ["49"]);
    // A local dict shadows, and end unshadows.
    assert_eq!(eval("/x 1 def 2 dict begin /x 2 def x end x"), ["2", "1"]);
    // Defs inside begin/end don't leak.
    assert_eq!(
        eval_err("2 dict begin /y 9 def end y"),
        PsError::Undefined("y".to_string())
    );
    assert_eq!(eval_err("end"), PsError::DictStackUnderflow);
    assert_eq!(eval_err("5 begin"), PsError::Typecheck);
    assert_eq!(eval("/v 3 def /v load"), ["3"]);
}

#[test]
fn recursion() {
    assert_eq!(
        eval("/fact { dup 1 le { pop 1 } { dup 1 sub fact mul } ifelse } def 10 fact"),
        ["3628800"]
    );
    assert_eq!(
        eval("/fib { dup 2 lt { } { dup 1 sub fib exch 2 sub fib add } ifelse } def 15 fib"),
        ["610"]
    );
}

#[test]
fn tail_recursion_runs_in_constant_exec_stack() {
    // 200k self-calls in tail position: only survivable because finished
    // frames pop before their last element executes.
    assert_eq!(
        eval("/count { dup 0 gt { 1 sub count } if } def 200000 count"),
        ["0"]
    );
}

#[test]
fn runaway_recursion_is_an_error_not_a_crash() {
    // Non-tail recursion (the `add` afterwards keeps each frame alive)
    // must die as execstackoverflow, not a process abort.
    assert_eq!(eval_err("/f { f 1 add } def f"), PsError::ExecStackOverflow);
}

#[test]
fn bind_resolves_operators_early() {
    assert_eq!(eval("/double {2 mul} bind def 5 double"), ["10"]);
    // The bound proc survives a hostile redefinition of mul...
    assert_eq!(
        eval("/double {2 mul} bind def /mul {pop pop 0} def 5 double"),
        ["10"]
    );
    // ...while an unbound one picks it up.
    assert_eq!(
        eval("/double {2 mul} def /mul {pop pop 0} def 5 double"),
        ["0"]
    );
    // bind recurses into nested procedures.
    assert_eq!(
        eval("/f { true {3 add} if } bind def /add {pop pop 99} def 4 f"),
        ["7"]
    );
    assert_eq!(eval_err("5 bind"), PsError::Typecheck);
}

#[test]
fn nested_loops_and_exit_only_leaves_the_innermost() {
    assert_eq!(
        eval("1 1 3 { 10 mul 1 1 3 { add dup 100 eq {exit} if } for } for"),
        // Inner loop: (i*10)+1+2+3 for i=1..3; 100 never hits, exit never
        // fires; verifies the nesting bookkeeping.
        ["16", "26", "36"]
    );
    assert_eq!(
        eval("0 3 { 3 { 1 add dup 4 eq { exit } if } repeat } repeat"),
        // Inner exit fires once (at 4); outer repeat keeps going.
        ["7"]
    );
}
