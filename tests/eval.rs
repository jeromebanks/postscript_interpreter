//! End-to-end interpreter tests: PostScript source in, operand stack out.
//! Stack contents are compared via `==`-style representations, which keeps
//! expected values readable and exercises the printer as a bonus.

use pscat::{Interp, PsError, Value};

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

/// For math whose exact bit pattern we shouldn't depend on.
fn eval_approx(src: &str, expected: f64) {
    let mut it = Interp::new();
    it.run_str(src)
        .unwrap_or_else(|e| panic!("eval of {src:?} failed: {e}"));
    let stack = it.operand_stack();
    assert_eq!(stack.len(), 1, "eval of {src:?} left {stack:?}");
    let got = match stack[0].value {
        Value::Real(r) => r,
        Value::Integer(i) => i as f64,
        _ => panic!("eval of {src:?} left non-number {stack:?}"),
    };
    assert!(
        (got - expected).abs() < 1e-9,
        "eval of {src:?}: got {got}, expected {expected}"
    );
}

#[test]
fn integer_arithmetic() {
    assert_eq!(eval("3 4 add"), ["7"]);
    assert_eq!(eval("3 4 sub"), ["-1"]);
    assert_eq!(eval("3 4 mul"), ["12"]);
    assert_eq!(eval("7 2 idiv"), ["3"]);
    assert_eq!(eval("-7 2 idiv"), ["-3"]);
    assert_eq!(eval("7 3 mod"), ["1"]);
    assert_eq!(eval("-7 3 mod"), ["-1"]);
    assert_eq!(eval("5 neg"), ["-5"]);
    assert_eq!(eval("-5 abs"), ["5"]);
}

#[test]
fn mixed_and_real_arithmetic() {
    assert_eq!(eval("3 4.0 add"), ["7.0"]);
    assert_eq!(eval("7 2 div"), ["3.5"]);
    assert_eq!(eval("6 2 div"), ["3.0"]);
    assert_eq!(eval("1.5 neg"), ["-1.5"]);
}

#[test]
fn integer_overflow_promotes_to_real() {
    assert_eq!(eval("9223372036854775807 1 add"), ["9.223372036854776e18"]);
    assert_eq!(eval("-9223372036854775808 neg"), ["9.223372036854776e18"]);
    assert_eq!(eval("-9223372036854775808 abs"), ["9.223372036854776e18"]);
}

#[test]
fn rounding_operators() {
    assert_eq!(eval("3.2 ceiling"), ["4.0"]);
    assert_eq!(eval("3.8 floor"), ["3.0"]);
    assert_eq!(eval("3.7 truncate"), ["3.0"]);
    assert_eq!(eval("-3.7 truncate"), ["-3.0"]);
    assert_eq!(eval("2.5 round"), ["3.0"]);
    // PLRM: ties round to the greater value.
    assert_eq!(eval("-2.5 round"), ["-2.0"]);
    // Rounding an integer is the identity, and stays an integer.
    assert_eq!(
        eval("7 ceiling 7 floor 7 round 7 truncate"),
        ["7", "7", "7", "7"]
    );
}

#[test]
fn math_functions() {
    assert_eq!(eval("9 sqrt"), ["3.0"]);
    assert_eq!(eval("0 sin"), ["0.0"]);
    assert_eq!(eval("0 cos"), ["1.0"]);
    eval_approx("30 sin", 0.5);
    eval_approx("60 cos", 0.5);
    eval_approx("1 0 atan", 90.0);
    eval_approx("-1 -1 atan", 225.0);
    eval_approx("2 8 exp", 256.0);
    eval_approx("100 log", 2.0);
    eval_approx("1 ln", 0.0);
}

#[test]
fn stack_manipulation() {
    assert_eq!(eval("1 2 3 pop"), ["1", "2"]);
    assert_eq!(eval("1 2 exch"), ["2", "1"]);
    assert_eq!(eval("1 dup"), ["1", "1"]);
    assert_eq!(eval("1 2 3 2 copy"), ["1", "2", "3", "2", "3"]);
    assert_eq!(eval("1 2 3 0 copy"), ["1", "2", "3"]);
    assert_eq!(eval("1 2 3 0 index"), ["1", "2", "3", "3"]);
    assert_eq!(eval("1 2 3 2 index"), ["1", "2", "3", "1"]);
    assert_eq!(eval("1 2 clear"), Vec::<String>::new());
    assert_eq!(eval("1 2 count"), ["1", "2", "2"]);
}

#[test]
fn roll() {
    // The PLRM's own examples.
    assert_eq!(eval("1 2 3 3 -1 roll"), ["2", "3", "1"]);
    assert_eq!(eval("1 2 3 3 1 roll"), ["3", "1", "2"]);
    assert_eq!(eval("1 2 3 3 0 roll"), ["1", "2", "3"]);
    // j larger than n wraps.
    assert_eq!(eval("1 2 3 3 4 roll"), ["3", "1", "2"]);
    assert_eq!(eval("1 2 3 4 5 5 -2 roll"), ["3", "4", "5", "1", "2"]);
}

#[test]
fn marks() {
    assert_eq!(eval("mark 1 2 counttomark"), ["-mark-", "1", "2", "2"]);
    assert_eq!(eval("1 mark 2 3 cleartomark"), ["1"]);
    assert_eq!(eval("[1 2 3]"), ["[1 2 3]"]);
    assert_eq!(eval("[1 [2] 3]"), ["[1 [2] 3]"]);
    assert_eq!(eval("[]"), ["[]"]);
}

#[test]
fn literals_and_procedures() {
    assert_eq!(eval("true false null"), ["true", "false", "null"]);
    assert_eq!(eval("/foo"), ["/foo"]);
    assert_eq!(eval("(hi)"), ["(hi)"]);
    assert_eq!(eval(r"(a\)b)"), [r"(a\)b)"]);
    assert_eq!(eval("16#ff 8#777"), ["255", "511"]);
    // A procedure is pushed, not executed.
    assert_eq!(eval("{1 2 add}"), ["{1 2 add}"]);
    assert_eq!(eval("{dup {mul} pop}"), ["{dup {mul} pop}"]);
}

#[test]
fn errors() {
    assert_eq!(eval_err("1 0 div"), PsError::UndefinedResult);
    assert_eq!(eval_err("8 0 mod"), PsError::UndefinedResult);
    assert_eq!(eval_err("8 0 idiv"), PsError::UndefinedResult);
    assert_eq!(eval_err("0 0 atan"), PsError::UndefinedResult);
    assert_eq!(eval_err("pop"), PsError::StackUnderflow);
    assert_eq!(eval_err("1 exch"), PsError::StackUnderflow);
    assert_eq!(eval_err("1 2 3 4 2 roll"), PsError::StackUnderflow);
    assert_eq!(eval_err("(a) 1 add"), PsError::Typecheck);
    assert_eq!(eval_err("1.5 2 idiv"), PsError::Typecheck);
    assert_eq!(eval_err("-1 copy"), PsError::Rangecheck);
    assert_eq!(eval_err("-1 sqrt"), PsError::Rangecheck);
    assert_eq!(eval_err("0 ln"), PsError::Rangecheck);
    assert_eq!(eval_err("-5 log"), PsError::Rangecheck);
    assert_eq!(eval_err("1 2 counttomark"), PsError::UnmatchedMark);
    assert_eq!(eval_err("1 2 cleartomark"), PsError::UnmatchedMark);
    assert_eq!(
        eval_err("nonsuch_operator"),
        PsError::Undefined("nonsuch_operator".to_string())
    );
    // Number lookalikes are names, and undefined ones say so.
    assert_eq!(eval_err("1.2.3"), PsError::Undefined("1.2.3".to_string()));
    assert!(matches!(eval_err("}"), PsError::Syntax(_)));
    assert!(matches!(eval_err("{1 2"), PsError::Syntax(_)));
    assert!(matches!(eval_err("(abc"), PsError::Syntax(_)));
}

#[test]
fn errors_leave_operand_stack_intact() {
    let mut it = Interp::new();
    assert!(it.run_str("1 2 nonsuch 99").is_err());
    // Work already done stays visible; nothing after the error ran.
    let reprs: Vec<String> = it.operand_stack().iter().map(|o| o.repr()).collect();
    assert_eq!(reprs, ["1", "2"]);
    // The interpreter is reusable after an error.
    it.run_str("add").expect("recover after error");
    assert_eq!(it.operand_stack()[0].repr(), "3");
}

#[test]
fn dsc_comments_are_ignored() {
    assert_eq!(
        eval("%!PS-Adobe-3.0 EPSF-3.0\n%%BoundingBox: 0 0 500 500\n1 2 add"),
        ["3"]
    );
}
