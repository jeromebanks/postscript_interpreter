//! Proof that issue #95's Phase A mechanisms actually catch the defect
//! classes they claim, rather than merely being wired up.
//!
//! `docs/PS_LIBRARY_COUPLING.md`'s Touchpoint 2 table classifies all 18
//! real defects cross-model review found on PR #76 and says which
//! mechanism would have caught each. Asserting that mapping is easy;
//! the acceptance criterion asks for something harder — a regression
//! test that **reintroduces the defect and shows the mechanism goes
//! red**. That's what most of this file does.
//!
//! Each mutation proof asserts three states, not two:
//!
//!   1. the unmutated library passes its own self-tests,
//!   2. with the guard removed, the malformed call behaves *exactly*
//!      the way the defect it closes behaved — and in particular never
//!      raises the guard's own name,
//!   3. the mutated library fails `--selftest`, naming the block that
//!      covers that guard.
//!
//! Step 2 is what keeps the proof from being vacuous. Without it,
//! "mutated → self-test fails" is satisfied just as well by a mutation
//! that broke the library outright, which would prove the assertion
//! catches noise rather than that the guard is load-bearing.
//!
//! Measuring step 2 rather than assuming it turned up something worth
//! recording: four of these five guards close a *silent* defect (the
//! library accepts malformed input and draws the wrong thing), but
//! `pkribbon`'s `/Pressure` guard does not. Removing it produces a
//! `typecheck` on `def`, from deep inside the implementation, with no
//! hint of which option was wrong — which is precisely what PR #76's
//! round 2 described: the unexecuted value "leaks extra operands into
//! every downstream computation instead of raising a clean error".
//! [`Unguarded`] makes each row state which shape it is, so a change
//! in that behavior fails here instead of passing unnoticed.

use std::path::{Path, PathBuf};
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_pscat");

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// A scratch directory that cleans itself up.
///
/// Hand-rolled rather than pulling in a `tempdir` crate: the rest of
/// this suite already builds scratch paths out of `env::temp_dir()`
/// plus the process id (see `tests/artkit.rs`), and one more dev
/// dependency for eight lines isn't worth it. `name` keeps concurrent
/// tests in the same binary from colliding, which the pid alone
/// wouldn't.
struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Scratch {
        let dir =
            std::env::temp_dir().join(format!("pscat-selftest-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch dir");
        Scratch(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A scratch copy of `lib/` with one guard removed from one file.
///
/// The whole `lib/` directory is copied, not just the file under test:
/// `% @requires:` resolution walks up from the file being checked, so
/// the mutated `paintkit.ps` has to find an `artkit.ps` beside it —
/// and it must be the *unmutated* one, or the proof would be about two
/// changes at once.
struct MutatedLib {
    dir: Scratch,
    file: PathBuf,
}

impl MutatedLib {
    fn new(name: &str, rel: &str, remove: &str) -> MutatedLib {
        let root = repo_root();
        let dir = Scratch::new(name);
        let lib_out = dir.path().join("lib");
        std::fs::create_dir_all(&lib_out).expect("mkdir lib");
        for entry in std::fs::read_dir(root.join("lib")).expect("read lib/") {
            let entry = entry.expect("dir entry");
            if entry.path().extension().is_some_and(|e| e == "ps") {
                std::fs::copy(entry.path(), lib_out.join(entry.file_name())).expect("copy");
            }
        }
        let file = dir.path().join(rel);
        let text = std::fs::read_to_string(&file).expect("read the file to mutate");
        // Assert the text is there before removing it: a renamed guard
        // must fail this test loudly rather than silently "mutating"
        // nothing and then passing because the unmutated library still
        // works.
        assert!(
            text.contains(remove),
            "the guard this proof mutates is gone from {rel} — if it moved, update the \
             mutation; if it was deleted, that is the regression this test exists to catch:\n\
             {remove}"
        );
        assert_eq!(
            text.matches(remove).count(),
            1,
            "the mutation target must be unique in {rel}, or the proof removes more than \
             it means to:\n{remove}"
        );
        std::fs::write(&file, text.replace(remove, "")).expect("write the mutated file");
        MutatedLib { dir, file }
    }

    fn path(&self) -> &Path {
        &self.file
    }

    /// Run a snippet against this mutated library: `(ok, stderr)`.
    fn run(&self, snippet: &str) -> (bool, String) {
        let program = format!(
            "({}) run ({}) run\n{snippet}\n",
            self.dir.path().join("lib/artkit.ps").display(),
            self.file.display(),
        );
        let out = Command::new(BIN)
            .args(["--headless", "-e", &program])
            .output()
            .expect("run pscat");
        (
            out.status.success(),
            String::from_utf8_lossy(&out.stderr).into_owned(),
        )
    }

    /// A call that is valid for the library under test, used to
    /// confirm the mutation didn't break anything but the guard.
    fn well_formed_call(&self, rel: &str) -> String {
        match rel {
            "lib/artkit.ps" => {
                "0 0 100 100 screct << /Mark { pop pop pop pop } /Count 3 >> scatter".to_string()
            }
            _ => "newpath 0 0 moveto 40 0 lineto << /Width 8 >> pkribbon".to_string(),
        }
    }
}

fn selftest(path: &Path) -> std::process::Output {
    Command::new(BIN)
        .arg("--selftest")
        .arg(path)
        .output()
        .expect("run pscat --selftest")
}

/// What a malformed call does once its guard is removed — the thing
/// that makes the guard load-bearing in the first place.
#[derive(Clone, Copy)]
enum Unguarded {
    /// Nothing raises: the library accepts malformed input and draws
    /// something wrong. The majority case here.
    Accepted,
    /// Something raises, but not the guard — a confusing error from
    /// deep inside the implementation instead of a named contract
    /// violation attributable to the call site.
    RaisesSomethingElse(&'static str),
}

/// The full three-state proof for one guard.
///
/// `snippet` is the same malformed call the `%%SelfTest` block makes,
/// written out here independently. The duplication is deliberate: if
/// the two ever drift, step 2 still measures what the *defect* does,
/// which is the property the proof actually depends on.
fn proves_the_guard_is_load_bearing(
    name: &str,
    rel: &str,
    remove: &str,
    snippet: &str,
    unguarded: Unguarded,
    block: &str,
) {
    let pristine = repo_root().join(rel);
    let before = selftest(&pristine);
    assert!(
        before.status.success(),
        "the unmutated {rel} must pass its own self-tests first:\n{}",
        String::from_utf8_lossy(&before.stderr)
    );

    let mutated = MutatedLib::new(name, rel, remove);

    // The mutation has to be surgical: a library that no longer loads,
    // or no longer works at all, would fail --selftest for reasons
    // that say nothing about this guard.
    let (well_formed_ok, _) = mutated.run(&mutated.well_formed_call(rel));
    assert!(
        well_formed_ok,
        "removing the guard must leave the library otherwise working, or step 3 proves \
         nothing about the guard"
    );

    let (ok, stderr) = mutated.run(snippet);
    let guard_name = remove
        .split_once('{')
        .and_then(|(_, r)| r.split_whitespace().next())
        .expect("the mutation text names the guard it removes");
    assert!(
        !stderr.contains(guard_name),
        "the guard is supposed to be gone, but {guard_name} still fired:\n{stderr}"
    );
    match unguarded {
        Unguarded::Accepted => assert!(
            ok,
            "this guard is documented as closing a *silent* defect, but the unguarded \
             call raised something:\n{stderr}"
        ),
        Unguarded::RaisesSomethingElse(errorname) => {
            assert!(
                !ok,
                "expected the unguarded call to raise {errorname}:\n{stderr}"
            );
            assert!(
                stderr.contains(errorname),
                "expected {errorname} from the unguarded call:\n{stderr}"
            );
        }
    }

    let after = selftest(mutated.path());
    assert!(
        !after.status.success(),
        "--selftest must fail once the guard is gone, and didn't"
    );
    let stderr = String::from_utf8_lossy(&after.stderr);
    assert!(
        stderr.contains(block),
        "the failure must name the block that covers this guard ({block}):\n{stderr}"
    );
}

// --- Phase A mechanism 1: `%%SelfTest` + `--selftest` ----------------
//
// Rows 5, 6, 10, 15 and 18 of the Touchpoint 2 defect table, each
// proved by removing the guard that closed it.

#[test]
fn row_5_and_10_a_non_callable_pressure_is_caught() {
    // Rows 5 (round 2, a bare number) and 10 (round 3, `xcheck` alone
    // is insufficient) are the same guard: the `type` test is what
    // distinguishes them, and removing it reopens both.
    proves_the_guard_is_load_bearing(
        "row5",
        "lib/paintkit.ps",
        "    not { pkribbon-pressure-must-be-a-procedure } if\n",
        "newpath 0 0 moveto 40 0 lineto << /Pressure 5 >> pkribbon",
        // Not silent: an unexecuted /Pressure value leaks onto the
        // stack and `def` eventually chokes on it, far from the call
        // site and naming nothing useful.
        Unguarded::RaisesSomethingElse("typecheck"),
        "pkribbon-rejects-a-non-callable-pressure",
    );
}

#[test]
fn row_6_an_undocumented_cap_value_is_caught() {
    proves_the_guard_is_load_bearing(
        "row6",
        "lib/paintkit.ps",
        "    pkstartcap /round eq pkstartcap /flat eq or pkstartcap /pointed eq or not\n        { pkribbon-startcap-must-be-round-flat-or-pointed } if\n",
        "newpath 0 0 moveto 40 0 lineto << /StartCap /square >> pkribbon",
        Unguarded::Accepted,
        "pkribbon-rejects-undocumented-cap-values",
    );
}

#[test]
fn row_15_an_out_of_range_taper_is_caught() {
    proves_the_guard_is_load_bearing(
        "row15",
        "lib/paintkit.ps",
        "    pkstarttaper 0 lt pkstarttaper 1 gt or\n        { pkribbon-starttaper-must-be-0-to-1 } if\n",
        "newpath 0 0 moveto 40 0 lineto << /StartTaper 2 >> pkribbon",
        Unguarded::Accepted,
        "pkribbon-rejects-out-of-range-tapers",
    );
}

#[test]
fn row_18_an_executable_value_option_is_caught() {
    proves_the_guard_is_load_bearing(
        "row18",
        "lib/paintkit.ps",
        "    /pkwidth load xcheck { pkribbon-width-must-not-be-a-procedure } if\n",
        "newpath 0 0 moveto 40 0 lineto << /Width { 10 } >> pkribbon",
        Unguarded::Accepted,
        "pkribbon-rejects-executable-value-options",
    );
}

#[test]
fn artkits_own_guards_are_covered_the_same_way() {
    proves_the_guard_is_load_bearing(
        "artkit-scatter",
        "lib/artkit.ps",
        "    /scmark load sccallable not { scatter-mark-must-be-a-procedure } if\n",
        "0 0 100 100 screct << /Mark 5 >> scatter",
        Unguarded::Accepted,
        "scatter-validates-its-option-dict",
    );
}

// --- The mechanism's own soundness ----------------------------------

#[test]
fn a_typo_in_a_test_body_does_not_satisfy_a_guard_assertion() {
    // The property the whole design rests on. `{ ... } stopped not
    // { fail } if` would pass here, which is exactly why that form
    // isn't in the vocabulary: a renamed procedure would keep the
    // self-test green forever while testing nothing.
    let dir = Scratch::new("typo");
    let file = dir.path().join("typo.ps");
    std::fs::write(
        &file,
        "%%SelfTest: a-typo-must-not-pass\n\
         %   { nosuchoperator }\n\
         %   /pkribbon-width-must-be-positive\n\
         %   (a typo) mustguard\n\
         %%EndSelfTest\n",
    )
    .expect("write");
    let out = selftest(&file);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "a typo must fail:\n{stderr}");
    assert!(
        stderr.contains("nosuchoperator"),
        "the report must name what actually raised:\n{stderr}"
    );
}

#[test]
fn a_block_that_leaks_a_gsave_fails_even_with_no_failed_assertion() {
    // PostScript has no gsave-depth query, so `mustguard` can't undo a
    // graphics-state leak the way it undoes an operand-stack one. The
    // runner checks the balance itself so the leak is loud.
    let dir = Scratch::new("gsave-leak");
    let file = dir.path().join("leak.ps");
    std::fs::write(&file, "%%SelfTest: leaks\n%   gsave\n%%EndSelfTest\n").expect("write");
    let out = selftest(&file);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "{stderr}");
    assert!(stderr.contains("unmatched gsave"), "{stderr}");
}

#[test]
fn a_block_that_leaks_a_dictionary_fails_even_with_no_failed_assertion() {
    // Same reasoning as the gsave check: PostScript can undo its own
    // operand debris, but a leaked `begin` would otherwise be carried
    // silently to the end of the block.
    let dir = Scratch::new("dict-leak");
    let file = dir.path().join("leak.ps");
    std::fs::write(
        &file,
        "%%SelfTest: leaks\n%   userdict begin\n%%EndSelfTest\n",
    )
    .expect("write");
    let out = selftest(&file);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "{stderr}");
    assert!(stderr.contains("dictionary(ies) still open"), "{stderr}");
}

#[test]
fn a_proc_that_shadows_the_harnesss_own_names_cannot_defeat_the_cleanup() {
    // The assertions restore the operand and dictionary stacks after a
    // caught error. If the depths they restore to were read back by
    // name, a proc that opened a dictionary defining those same names
    // would turn the whole cleanup into a no-op — verified: it left the
    // dictionary open and the debris in place, with nothing reporting
    // it. They travel on the operand stack instead (`mark`/
    // `cleartomark` for the operands, a captured `countdictstack` for
    // the dictionaries), which no `begin` can shadow.
    let dir = Scratch::new("shadowing");
    let file = dir.path().join("shadow.ps");
    std::fs::write(
        &file,
        "%%SelfTest: shadowing-cannot-defeat-the-cleanup\n\
         %   { 1 dict begin\n\
         %     /pscat_st_ddepth 99 def /pscat_st_depth -1 def\n\
         %     11 22 33 undefinedname }\n\
         %   /undefinedname\n\
         %   (a proc shadowing every harness name it can) mustguard\n\
         %   count 0 eq (the debris was cleaned up anyway) mustbe\n\
         %   countdictstack 2 eq (the dictionary was closed anyway) mustbe\n\
         %%EndSelfTest\n",
    )
    .expect("write");
    let out = selftest(&file);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn a_later_assertion_still_works_after_an_earlier_one_caught_an_error() {
    // Within-block isolation: `stopped` leaves debris, and the next
    // assertion has to be unaffected by it.
    let dir = Scratch::new("sequencing");
    let file = dir.path().join("seq.ps");
    std::fs::write(
        &file,
        "/goodguard { my-guard-fired } def\n\
         %%SelfTest: a-caught-error-does-not-affect-the-next-assertion\n\
         %   77 88\n\
         %   { 1 2 3 undefinedname } /undefinedname (first) mustguard\n\
         %   { goodguard } /my-guard-fired (second) mustguard\n\
         %   count 2 eq (the block's own operands survived) mustbe\n\
         %%EndSelfTest\n",
    )
    .expect("write");
    let out = selftest(&file);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn a_block_that_wraps_itself_in_a_dictionary_still_reports_its_failures() {
    // The failure log is written through userdict by name. A plain
    // `def` would land in the block's own dictionary and be discarded
    // at its `end`, so a failing block would report clean — the worst
    // possible direction for this mechanism to be wrong in.
    let dir = Scratch::new("wrapped-block");
    let file = dir.path().join("wrapped.ps");
    std::fs::write(
        &file,
        "%%SelfTest: failures-survive-the-blocks-own-dictionary\n\
         %   10 dict begin\n\
         %     false (this must be reported, not swallowed) mustbe\n\
         %   end\n\
         %%EndSelfTest\n",
    )
    .expect("write");
    let out = selftest(&file);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "the failure was lost:\n{stderr}");
    assert!(stderr.contains("this must be reported"), "{stderr}");
}

#[test]
fn a_malformed_block_fails_instead_of_being_skipped() {
    // A self-test that silently doesn't run reads as coverage that
    // isn't there — worse than no self-test at all.
    let dir = Scratch::new("malformed");
    let file = dir.path().join("bad.ps");
    std::fs::write(&file, "%%SelfTest: unclosed\n%   1 2 add\n").expect("write");
    let out = selftest(&file);
    assert!(!out.status.success());
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("never closed"),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn every_library_that_carries_blocks_passes_them() {
    // What scripts/selftest.sh runs, run again under `cargo test` so a
    // regression shows up in CI's existing gate too.
    let mut checked = 0;
    for entry in std::fs::read_dir(repo_root().join("lib")).expect("read lib/") {
        let path = entry.expect("dir entry").path();
        if path.extension().is_none_or(|e| e != "ps") {
            continue;
        }
        let text = std::fs::read_to_string(&path).expect("read");
        if !text.contains("\n%%SelfTest:") && !text.starts_with("%%SelfTest:") {
            continue;
        }
        checked += 1;
        let out = selftest(&path);
        assert!(
            out.status.success(),
            "{} failed its own self-tests:\n{}",
            path.display(),
            String::from_utf8_lossy(&out.stderr)
        );
    }
    assert!(checked >= 2, "expected at least two migrated libraries");
}

// --- Phase A mechanism 2: strict `--lint` over rendering drivers -----

/// The `%%SelfTestPage: WxH` header `scripts/selftest.sh` reads.
fn driver_page(text: &str) -> String {
    text.lines()
        .find_map(|l| l.strip_prefix("%%SelfTestPage:"))
        .unwrap_or_else(|| panic!("driver has no %%SelfTestPage: header"))
        .trim()
        .to_string()
}

fn lint_strict(path: &Path, page: &str) -> std::process::Output {
    Command::new(BIN)
        .arg("--page")
        .arg(page)
        .arg("--lint-strict")
        .arg(path)
        .current_dir(repo_root())
        .output()
        .expect("run pscat --lint-strict")
}

fn drivers() -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = std::fs::read_dir(repo_root().join("selftest/drivers"))
        .expect("read selftest/drivers")
        .map(|e| e.expect("dir entry").path())
        .filter(|p| p.extension().is_some_and(|e| e == "ps"))
        .collect();
    found.sort();
    assert!(!found.is_empty(), "no rendering drivers found");
    found
}

#[test]
fn every_driver_passes_strict_lint() {
    for driver in drivers() {
        let text = std::fs::read_to_string(&driver).expect("read driver");
        let out = lint_strict(&driver, &driver_page(&text));
        assert!(
            out.status.success(),
            "{} failed strict lint:\n{}",
            driver.display(),
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

#[test]
fn rows_9_11_12_a_scenario_that_renders_blank_fails_strict_lint() {
    // Rows 9, 11 and 12 of the defect table are all "the call succeeds
    // and the page is empty". Proved by neutering the first scenario
    // of the paintkit driver — the short pointed stroke, which is row
    // 9's own geometry — and showing strict lint goes red.
    let src = repo_root().join("selftest/drivers/paintkit.ps");
    let text = std::fs::read_to_string(&src).expect("read driver");
    let target = "<< /Width 14 /StartCap /pointed /EndCap /pointed >> pkribbon";
    assert!(text.contains(target), "the row-9 scenario moved");
    let dir = Scratch::new("blank-driver");
    let file = dir.path().join("paintkit.ps");
    // `pop` consumes the option dict and paints nothing: the same
    // shape a genuinely blank-rendering defect produces.
    std::fs::write(&file, text.replace(target, "pop")).expect("write");

    let out = lint_strict(&file, &driver_page(&text));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "strict lint must fail:\n{stderr}");
    assert!(stderr.contains("[blank-page] page 1"), "{stderr}");
}

#[test]
fn a_deleted_showpage_fails_instead_of_silently_merging_two_scenarios() {
    // The ninth-round gap the one-showpage-per-scenario rule exists to
    // close: two scenarios sharing a page let the second one's ink
    // hide whether the first drew anything. Author discipline alone
    // can't enforce that, so `%%Pages:` is checked against reality.
    let src = repo_root().join("selftest/drivers/paintkit.ps");
    let text = std::fs::read_to_string(&src).expect("read driver");
    let dir = Scratch::new("merged-driver");
    let file = dir.path().join("paintkit.ps");
    let merged = text.replacen("\nshowpage\n", "\n", 1);
    assert_ne!(merged, text, "no showpage to remove");
    std::fs::write(&file, &merged).expect("write");

    let out = lint_strict(&file, &driver_page(&text));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "strict lint must fail:\n{stderr}");
    assert!(stderr.contains("[page-count]"), "{stderr}");
}

#[test]
fn plain_lint_stays_advisory() {
    // Issue #17's contract is unchanged: findings are advisory unless
    // `--lint-strict` is given. A blank page must still exit 0 under
    // plain `--lint`, or every existing caller changes behavior.
    // A real source file, not `-e`: an eval snippet legitimately isn't
    // expected to draw anything, so report_lint skips the render
    // checks for it (issue #17's own contract).
    let dir = Scratch::new("advisory");
    let blank = dir.path().join("blank.ps");
    std::fs::write(&blank, "showpage\n").expect("write");

    let out = Command::new(BIN)
        .arg("--lint")
        .arg(&blank)
        .output()
        .expect("run pscat");
    assert!(out.status.success(), "plain --lint must not fail the run");
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("[blank-page]"),
        "...but it must still report the finding: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let strict = Command::new(BIN)
        .arg("--lint-strict")
        .arg(&blank)
        .output()
        .expect("run pscat");
    assert!(!strict.status.success(), "--lint-strict must fail it");
}
