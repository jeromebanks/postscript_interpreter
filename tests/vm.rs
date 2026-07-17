//! save/restore — Stage 8 task 1. Every behavioral choice here is
//! pinned against gs (the snippets and their gs output are in VM.md);
//! deviations are tested as deviations, deliberately.

use pscat::{Interp, PsError};

fn run(src: &str) -> Interp {
    let mut it = Interp::with_page(100, 100).expect("test page");
    it.run_str(src)
        .unwrap_or_else(|e| panic!("run failed: {}", it.error_report(&e)));
    it
}

fn top_repr(it: &mut Interp) -> String {
    it.pop().expect("operand").repr()
}

#[test]
fn arrays_roll_back() {
    let mut it = run("/a [1 2 3] def save a 0 99 put restore a");
    assert_eq!(top_repr(&mut it), "[1 2 3]");
}

#[test]
fn dicts_roll_back() {
    let mut it = run("/d 2 dict def save d /k 1 put restore d /k known");
    assert_eq!(top_repr(&mut it), "false");
}

#[test]
fn strings_are_exempt() {
    // PLRM §3.7.3.2 excepts strings from restore; gs: (Xbc).
    let mut it = run("/s (abc) def save s 0 88 put restore s");
    assert_eq!(top_repr(&mut it), "(Xbc)");
}

#[test]
fn defs_after_save_roll_back() {
    let mut it = run("save /zzz 1 def restore userdict /zzz known");
    assert_eq!(top_repr(&mut it), "false");
}

#[test]
fn redefinitions_roll_back_to_old_value() {
    let mut it = run("/x 7 def save /x 8 def /x 9 store restore x");
    assert_eq!(top_repr(&mut it), "7");
}

#[test]
fn undef_rolls_back() {
    let mut it = run("/d 2 dict def d /k 5 put save d /k undef restore d /k get");
    assert_eq!(top_repr(&mut it), "5");
}

#[test]
fn graphics_state_rolls_back() {
    let mut it = run("save 5 setlinewidth restore currentlinewidth");
    assert_eq!(top_repr(&mut it), "1.0");
}

#[test]
fn restore_is_grestoreall_to_the_save_point() {
    // Pinned against gs: unbalanced gsaves inside the save context are
    // popped and the pre-save state wins.
    let mut it = run("save gsave gsave 5 setlinewidth restore currentlinewidth");
    assert_eq!(top_repr(&mut it), "1.0");
}

#[test]
fn nested_saves_restore_lifo() {
    let mut it = run("/a [1] def
         save a 0 2 put
         save a 0 3 put
         restore a 0 get
         exch restore a 0 get");
    assert_eq!(top_repr(&mut it), "1", "outer restore → original");
    assert_eq!(top_repr(&mut it), "2", "inner restore → outer's value");
}

#[test]
fn nested_touch_only_after_inner_save_rolls_fully_back() {
    // First mutation of `a` happens after the inner save; the outer
    // level never journaled it. Outer restore must still recover the
    // original (the inner entry restored it; nothing re-dirtied it).
    let mut it = run("/a [1] def save save a 0 9 put restore a 0 get exch restore a 0 get");
    assert_eq!(top_repr(&mut it), "1", "after outer restore");
    assert_eq!(top_repr(&mut it), "1", "after inner restore");
}

#[test]
fn restoring_outer_invalidates_inner() {
    let mut it = Interp::with_page(100, 100).expect("page");
    assert_eq!(
        it.run_str("save save exch restore restore"),
        Err(PsError::InvalidRestore),
        "inner save object is stale once the outer restored"
    );
}

#[test]
fn double_restore_is_invalid() {
    let mut it = Interp::with_page(100, 100).expect("page");
    assert_eq!(
        it.run_str("save dup restore restore"),
        Err(PsError::InvalidRestore)
    );
}

#[test]
fn save_objects_have_savetype_and_identity() {
    let mut it = run("save dup type exch dup eq");
    assert_eq!(top_repr(&mut it), "true");
    // `type` returns an executable name, hence no slash in the repr.
    assert_eq!(top_repr(&mut it), "savetype");
}

#[test]
fn self_referential_array_restores_safely() {
    let mut it = run("/a [1 2 3] def save a 0 a put restore a 0 get");
    assert_eq!(top_repr(&mut it), "1");
}

#[test]
fn bind_rolls_back() {
    // bind mutates the procedure's VM contents, so restore un-binds.
    // (Not observable in gs: its invalidrestore stack scan fires on
    // the post-save proc before the rollback could be inspected.)
    let mut it = run("save {add} dup bind pop dup 0 get type 3 1 roll exch restore 0 get type");
    assert_eq!(top_repr(&mut it), "nametype", "unbound after restore");
    assert_eq!(top_repr(&mut it), "operatortype", "bound inside save");
}

#[test]
fn matrix_writes_roll_back() {
    let mut it = run("/m [0 0 0 0 0 0] def save m currentmatrix pop restore m 0 get");
    assert_eq!(top_repr(&mut it), "0");
}

#[test]
fn vmstatus_reports_the_save_level() {
    let mut it = run("vmstatus pop pop save vmstatus pop pop exch restore");
    assert_eq!(top_repr(&mut it), "1", "inside one save");
    assert_eq!(top_repr(&mut it), "0", "outside");
}

#[test]
fn grestoreall_respects_the_save_boundary() {
    let mut it = run("2 setlinewidth save gsave gsave 5 setlinewidth
         grestoreall currentlinewidth exch restore currentlinewidth");
    assert_eq!(top_repr(&mut it), "2.0", "restore still lands pre-save");
    assert_eq!(top_repr(&mut it), "2.0", "grestoreall → save-time state");
}

#[test]
fn grestoreall_without_save_pops_to_bottom() {
    // PLRM bottom-of-stack semantics. gs prints 1.0 here, but only
    // because its job server wraps every program in an implicit save
    // whose boundary state is the default graphics state.
    let mut it = run(
        "3 setlinewidth gsave 5 setlinewidth gsave 7 setlinewidth grestoreall currentlinewidth",
    );
    assert_eq!(top_repr(&mut it), "3.0");
}

// --- documented deviations (VM.md) ----------------------------------

#[test]
fn no_invalidrestore_scan_of_the_stacks() {
    // gs raises invalidrestore here (a post-save composite is on the
    // operand stack). We deliberately let it run: under Rc the object
    // stays valid — found files should render, not error.
    let mut it = run("save (newstring) exch restore");
    assert_eq!(top_repr(&mut it), "(newstring)");
}

#[test]
fn post_save_dict_on_dict_stack_is_tolerated() {
    // gs: invalidrestore. Here the dict simply stays on the stack.
    let mut it = run("save 3 dict begin restore countdictstack");
    assert_eq!(top_repr(&mut it), "3");
}

#[test]
fn stopped_error_inside_save_context_recovers() {
    // The journal must stay consistent when errors unwind mid-context:
    // the failed put still rolls back, and $error's writes roll back
    // with it (they're VM writes like any other).
    let mut it = run("/a [1] def
         save
         { a 0 2 put a 99 3 put } stopped
         exch restore a 0 get exch");
    assert_eq!(top_repr(&mut it), "true", "inner error was caught");
    assert_eq!(top_repr(&mut it), "1", "successful put rolled back too");
}
