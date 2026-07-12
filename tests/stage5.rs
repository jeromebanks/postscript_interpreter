//! Stage 5: data structures, conversions, and error recovery — the
//! operator set found PostScript leans on.

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
fn length_get_put() {
    assert_eq!(eval("[1 2 3] length"), ["3"]);
    assert_eq!(eval("(hello) length"), ["5"]);
    assert_eq!(eval("/name length"), ["4"]);
    assert_eq!(eval("2 dict dup /a 1 put length"), ["1"]);
    assert_eq!(eval("[10 20 30] 1 get"), ["20"]);
    assert_eq!(eval("(abc) 1 get"), ["98"]);
    assert_eq!(eval("[1 2 3] dup 1 99 put 1 get"), ["99"]);
    assert_eq!(eval("(abc) dup 0 88 put"), ["(Xbc)"]);
    assert_eq!(eval_err("[1 2] 5 get"), PsError::Rangecheck);
    assert_eq!(eval_err("[1 2] -1 get"), PsError::Rangecheck);
    assert_eq!(
        eval_err("2 dict /missing get"),
        PsError::Undefined("missing".to_string())
    );
}

#[test]
fn intervals_are_views_not_copies() {
    // Mutating the parent shows through the interval...
    assert_eq!(
        eval("/a [1 2 3 4 5] def /v a 1 3 getinterval def a 2 99 put v 1 get"),
        ["99"]
    );
    // ...and mutating through the view writes the parent.
    assert_eq!(
        eval("/s (hello) def /t s 1 3 getinterval def t 0 69 put s"),
        ["(hello)".replace("ell", "Ell")] // (hEllo)... e -> E at index 1
    );
    assert_eq!(eval("(hello) 1 3 getinterval"), ["(ell)"]);
    assert_eq!(eval("[1 2 3 4] 1 2 getinterval length"), ["2"]);
    assert_eq!(eval_err("[1 2 3] 2 5 getinterval"), PsError::Rangecheck);
}

#[test]
fn putinterval_and_copy() {
    assert_eq!(eval("/s (aaaaa) def s 1 (bc) putinterval s"), ["(abcaa)"]);
    assert_eq!(
        eval("/a [0 0 0 0] def a 2 [7 8] putinterval a 2 get a 3 get"),
        ["7", "8"]
    );
    // Composite copy returns the filled prefix as a view of the target.
    assert_eq!(eval("[1 2 3] 5 array copy length"), ["3"]);
    assert_eq!(eval("(ab) (xyz) copy"), ["(ab)"]);
    assert_eq!(eval("/d 2 dict def d /k 5 put d 3 dict copy /k get"), ["5"]);
    // Plain stack copy still works through the polymorphic dispatch.
    assert_eq!(eval("1 2 3 2 copy"), ["1", "2", "3", "2", "3"]);
    assert_eq!(eval_err("[1 2 3] 2 array copy"), PsError::Rangecheck);
}

#[test]
fn forall_over_everything() {
    assert_eq!(eval("0 [1 2 3 4] {add} forall"), ["10"]);
    assert_eq!(eval("0 (abc) {add} forall"), ["294"]); // 97+98+99
    assert_eq!(
        eval("0 2 dict dup /a 1 put dup /b 2 put {exch pop add} forall"),
        ["3"]
    );
    // exit works from forall like any loop.
    assert_eq!(
        eval("[1 2 3 4] {dup 3 eq {exit} if} forall"),
        ["1", "2", "3"]
    );
    // Empty containers run the body zero times.
    assert_eq!(eval("[] {99} forall"), Vec::<String>::new());
}

#[test]
fn array_string_aload_astore() {
    assert_eq!(eval("3 array"), ["[null null null]"]);
    assert_eq!(eval("3 string length"), ["3"]);
    assert_eq!(eval("[1 2 3] aload"), ["1", "2", "3", "[1 2 3]"]);
    assert_eq!(eval("10 20 30 3 array astore"), ["[10 20 30]"]);
    assert_eq!(eval_err("-1 array"), PsError::Rangecheck);
    assert_eq!(eval_err("999999999 string"), PsError::Limitcheck);
}

#[test]
fn dict_literals_and_conveniences() {
    assert_eq!(eval("<< /a 1 /b 2 >> /b get"), ["2"]);
    assert_eq!(eval("<< >> length"), ["0"]);
    assert_eq!(eval_err("<< /a >>"), PsError::Rangecheck);
    assert_eq!(eval("2 dict dup /x 1 put /x known"), ["true"]);
    assert_eq!(eval("2 dict /nope known"), ["false"]);
    assert_eq!(eval("/x 7 def /x where {/x get} {0} ifelse"), ["7"]);
    assert_eq!(eval("/zzz_undefined where"), ["false"]);
    // store replaces in the defining dict, not the current one.
    assert_eq!(eval("/x 1 def 2 dict begin /x 2 store end x"), ["2"]);
    assert_eq!(eval("countdictstack"), ["2"]);
    assert_eq!(
        eval("2 dict begin 2 dict begin cleardictstack countdictstack"),
        ["2"]
    );
    assert_eq!(eval("2 dict dup /a 1 put dup /a undef /a known"), ["false"]);
    assert_eq!(eval("currentdict type"), ["dicttype"]);
}

#[test]
fn exotic_dict_keys() {
    // Integer keys, and real keys normalize onto them per the PLRM.
    assert_eq!(eval("2 dict dup 3 (x) put 3.0 get"), ["(x)"]);
    // String keys convert to names.
    assert_eq!(eval("2 dict dup (k) 9 put /k get"), ["9"]);
    // Booleans as keys, why not.
    assert_eq!(eval("2 dict dup true 1 put true get"), ["1"]);
    assert_eq!(eval_err("2 dict null 1 put"), PsError::Typecheck);
}

#[test]
fn conversions() {
    assert_eq!(eval("3.7 cvi -3.7 cvi"), ["3", "-3"]);
    assert_eq!(eval("(42) cvi (16#ff) cvi"), ["42", "255"]);
    assert_eq!(eval("5 cvr"), ["5.0"]);
    assert_eq!(eval("(2.5e2) cvr"), ["250.0"]);
    assert_eq!(eval("(abc) cvn"), ["/abc"]);
    assert_eq!(eval("(abc) cvx cvn xcheck"), ["true"]);
    assert_eq!(eval("123 10 string cvs"), ["(123)"]);
    assert_eq!(eval("/foo 10 string cvs"), ["(foo)"]);
    assert_eq!(eval("true 10 string cvs length"), ["4"]);
    assert_eq!(eval("255 16 5 string cvrs"), ["(FF)"]);
    assert_eq!(eval("5 type"), ["integertype"]);
    assert_eq!(
        eval("(a) type 5.0 type [1] type"),
        ["stringtype", "realtype", "arraytype"]
    );
    assert_eq!(eval("{dup} xcheck [1] xcheck"), ["true", "false"]);
    assert_eq!(eval("/x cvx xcheck"), ["true"]);
    assert_eq!(
        eval_err("(1e400) cvr"),
        PsError::Syntax("not a number: 1e400".to_string())
    );
    assert_eq!(eval_err("123 2 string cvs"), PsError::Rangecheck);
}

#[test]
fn executable_strings_run_as_source() {
    assert_eq!(eval("(3 4 add) cvx exec"), ["7"]);
    assert_eq!(eval("(/x 5 def x 1 add) cvx exec"), ["6"]);
}

#[test]
fn stopped_catches_stop_and_errors() {
    assert_eq!(eval("{1 2 add} stopped"), ["3", "false"]);
    assert_eq!(eval("{1 stop 2} stopped"), ["1", "true"]);
    // A genuine error is caught the same way...
    assert_eq!(eval("{1 0 div} stopped"), ["true"]);
    assert_eq!(eval("{undefined_thing} stopped"), ["true"]);
    // ...and execution continues afterward.
    assert_eq!(eval("{frob} stopped pop 40 2 add"), ["42"]);
    // Nested stopped contexts unwind one level only.
    assert_eq!(eval("{ {stop} stopped } stopped"), ["true", "false"]);
    // $error records what happened.
    assert_eq!(
        eval("{1 0 div} stopped pop $error /errorname get"),
        ["/undefinedresult"]
    );
    assert_eq!(
        eval("{frobnicate} stopped pop $error /command get"),
        ["frobnicate"]
    );
    // newerror flips only when an error is actually caught.
    assert_eq!(
        eval("$error /newerror get {1 0 div} stopped pop $error /newerror get"),
        ["false", "true"]
    );
}

#[test]
fn rand_is_deterministic_and_seedable() {
    let a = eval("rand rand rand");
    let b = eval("rand rand rand");
    assert_eq!(a, b, "same seed, same sequence");
    assert_eq!(eval("42 srand rrand"), ["42"]);
    // All values in [0, 2^31).
    let mut it = Interp::new();
    it.run_str("100 { rand dup 0 lt exch 2147483647 gt or { (bad) } if } repeat")
        .expect("rand range");
    assert!(it.operand_stack().is_empty());
}

#[test]
fn bitshift() {
    assert_eq!(eval("7 3 bitshift"), ["56"]);
    assert_eq!(eval("142 -3 bitshift"), ["17"]);
    assert_eq!(eval("1 100 bitshift"), ["0"]);
}

#[test]
fn prolog_idioms_from_the_wild() {
    // The classic shortcut-prolog pattern found in real files.
    assert_eq!(
        eval("/bd {bind def} bind def /sq {dup mul} bd /m /moveto load def 7 sq"),
        ["49"]
    );
    // Dispatch table via dict + exec.
    assert_eq!(
        eval("/table << /double {2 mul} /square {dup mul} >> def 7 table /square get exec"),
        ["49"]
    );
    // Building a string incrementally with cvs + putinterval.
    assert_eq!(
        eval(
            "/buf 16 string def buf 0 (x=) putinterval buf 2 42 5 string cvs putinterval buf 0 4 getinterval"
        ),
        ["(x=42)"]
    );
}
