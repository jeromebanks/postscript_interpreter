//! `%%SelfTest` blocks and `pscat --selftest` (issue #95, Phase A
//! mechanism 1 of `docs/PS_LIBRARY_COUPLING.md`'s "Touchpoint 2").
//!
//! A PostScript library can carry its own validation-guard regression
//! tests as doc comments, so checking a PS-only change no longer
//! requires writing Rust:
//!
//! ```text
//! %%SelfTest: pkribbon-rejects-a-non-procedure-pressure
//! %   { newpath 0 0 moveto 40 0 lineto << /Pressure 5 >> pkribbon }
//! %   /pkribbon-pressure-must-be-a-procedure
//! %   (a bare number as /Pressure) mustguard
//! %%EndSelfTest
//! ```
//!
//! The block body is comment text, so the library stays completely
//! inert when a normal program `run`s it — the tests cost nothing at
//! load time and live next to the code they check.
//!
//! ## Why every failure assertion names the error it expects
//!
//! The obvious design — `{ ... } stopped not { fail } if`, "assert
//! *something* raised" — is unsound as a regression test: a typo in
//! the test body (a misspelled operator, a renamed procedure) also
//! raises, so the assertion passes green forever while testing
//! nothing. That is the exact silent-drift failure this whole
//! mechanism exists to prevent, so there is deliberately no
//! "any error will do" assertion in the vocabulary.
//!
//! [`mustguard`](PRELUDE) is the primary form, and it fits how this
//! repo's libraries actually report malformed input: they invoke a
//! self-documenting undefined name (`pkribbon-width-must-not-be-a-
//! procedure`), which raises `undefined` with that name recorded in
//! `$error`'s `/command`. Matching both `/errorname` *and* `/command`
//! means a typo — which raises `undefined` under some *other* name —
//! fails the assertion instead of satisfying it.
//!
//! ## Isolation
//!
//! Each block runs in its own freshly built [`Interp`], with the
//! prelude, the `% @requires:` chain, and the file under test loaded
//! from scratch. A block that leaves the machine in a strange state
//! therefore cannot affect any other block. Within a block,
//! `mustguard`/`mustfail`/`mustpass` restore the operand and
//! dictionary stacks after a caught error so one assertion's debris
//! can't corrupt the next; the graphics state has no PostScript-level
//! depth query, so instead the runner checks `gsave` balance after the
//! block and reports an imbalance as a failure — a leak becomes loud
//! rather than silent.

use std::path::{Path, PathBuf};

use crate::object::Value;
use crate::{Interp, Object};

/// Opening marker of a self-test block. Must start at column 0 — a
/// `%%`-prefixed DSC-style comment, deliberately *not* the `% @tag:`
/// shape `build.rs`'s capability-catalog parser owns, so the two
/// conventions can't be confused for one another.
const BEGIN: &str = "%%SelfTest:";
const END: &str = "%%EndSelfTest";

/// The file-level tag naming the prerequisite `run` chain. Reused from
/// `build.rs`'s capability-catalog grammar rather than invented again
/// here: a second, independent way to spell "this file needs artkit
/// loaded first" is a second thing to keep in sync, and the two would
/// drift.
const REQUIRES_TAG: &str = "@requires:";

/// Cap on how many individual assertion failures one block reports.
/// The array is preallocated in PostScript (no growable array in the
/// language), so this is a real limit — the count past it is still
/// reported accurately, only the details are dropped.
const MAX_FAILURES: usize = 128;

/// One `%%SelfTest:` block, with the `%` comment markers stripped from
/// its body so it can be executed as PostScript.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelfTestBlock {
    pub name: String,
    /// 1-indexed line of the `%%SelfTest:` marker, for error messages.
    pub line: usize,
    pub body: String,
}

/// One failed assertion, as the PostScript prelude recorded it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure {
    pub label: String,
    pub why: String,
    /// The error actually raised, or `--none--` when the assertion
    /// failed for a reason other than the wrong error (nothing raised
    /// at all, a false `mustbe`).
    pub errorname: String,
    pub command: String,
}

/// The outcome of running one block.
#[derive(Debug, Clone)]
pub struct BlockResult {
    pub name: String,
    pub line: usize,
    pub failures: Vec<Failure>,
    /// How many failures the block actually recorded, which can exceed
    /// `failures.len()` once [`MAX_FAILURES`] is hit.
    pub failure_count: usize,
    /// An error that escaped the block body itself (as opposed to one
    /// an assertion caught and classified), already formatted by
    /// [`Interp::error_report`].
    pub error: Option<String>,
    /// Unmatched `gsave`s left open by the block.
    pub gsave_depth: usize,
}

impl BlockResult {
    pub fn ok(&self) -> bool {
        self.failure_count == 0 && self.error.is_none() && self.gsave_depth == 0
    }
}

/// Everything `--selftest` found and ran for one file.
#[derive(Debug, Clone)]
pub struct Report {
    pub path: String,
    pub blocks: Vec<BlockResult>,
}

impl Report {
    pub fn ok(&self) -> bool {
        self.blocks.iter().all(BlockResult::ok)
    }
}

/// The assertion vocabulary, defined fresh in every block's
/// interpreter before anything else is loaded.
///
/// Everything internal is `pscat_st_`-prefixed so it can't collide
/// with a library's own names; only the four assertion operators are
/// unprefixed, since they're the surface a block author writes.
pub const PRELUDE: &str = r"
% --- pscat --selftest prelude (injected; not part of any library) ---
/pscat_st_fails 128 array def
/pscat_st_nfail 0 def
/pscat_st_depth 0 def
/pscat_st_ddepth 0 def

% (label) (why) errorname command  ->  -
/pscat_st_note {
    4 array astore
    pscat_st_nfail 128 lt
        { pscat_st_fails pscat_st_nfail 3 -1 roll put }
        { pop }
    ifelse
    /pscat_st_nfail pscat_st_nfail 1 add def
} def

% A stale $error from a previous assertion must never be able to
% satisfy the next one, so every assertion clears it first.
/pscat_st_clearerr {
    $error /newerror false put
    $error /errorname /--none-- put
    $error /command /--none-- put
} def

% Record where the stacks stood, with the proc under test still on
% top (hence `count 1 sub`: `stopped` is about to consume it).
/pscat_st_mark {
    count 1 sub /pscat_st_depth exch def
    countdictstack /pscat_st_ddepth exch def
} def

% Undo whatever a caught error left behind. Dictionaries first: `end`
% doesn't touch the operand stack, whereas popping operands first
% would be undone by nothing.
/pscat_st_reset {
    { countdictstack pscat_st_ddepth le { exit } if end } loop
    { count pscat_st_depth le { exit } if pop } loop
} def

% {proc} /guardname (label) mustguard  ->  -
% The proc must raise `undefined` on exactly /guardname -- this repo's
% libraries reject malformed input by invoking a self-documenting
% undefined name, and matching that name is what keeps a typo in the
% test body from passing as a caught guard.
/mustguard {
    /pscat_st_label exch def
    /pscat_st_want exch def
    pscat_st_mark
    pscat_st_clearerr
    stopped {
        pscat_st_reset
        $error /errorname get /undefined eq
        $error /command get pscat_st_want eq
        and not {
            pscat_st_label (raised a different error than the expected guard)
            $error /errorname get $error /command get pscat_st_note
        } if
    } {
        pscat_st_reset
        pscat_st_label (completed without raising the expected guard)
        /--none-- /--none-- pscat_st_note
    } ifelse
} def

% {proc} /errorname (label) mustfail  ->  -
% For guards that raise a real interpreter error (typecheck,
% rangecheck, ...) rather than an undefined guard name.
/mustfail {
    /pscat_st_label exch def
    /pscat_st_want exch def
    pscat_st_mark
    pscat_st_clearerr
    stopped {
        pscat_st_reset
        $error /errorname get pscat_st_want ne {
            pscat_st_label (raised a different error than expected)
            $error /errorname get $error /command get pscat_st_note
        } if
    } {
        pscat_st_reset
        pscat_st_label (completed without raising the expected error)
        /--none-- /--none-- pscat_st_note
    } ifelse
} def

% {proc} (label) mustpass  ->  -
/mustpass {
    /pscat_st_label exch def
    pscat_st_mark
    pscat_st_clearerr
    stopped {
        pscat_st_reset
        pscat_st_label (raised an error but was expected to succeed)
        $error /errorname get $error /command get pscat_st_note
    } {
        pscat_st_reset
    } ifelse
} def

% bool (label) mustbe  ->  -
/mustbe {
    exch dup type /booleantype ne {
        pop pop (mustbe) (was given a non-boolean)
        /--none-- /--none-- pscat_st_note
    } {
        not { (assertion was false) /--none-- /--none-- pscat_st_note } { pop } ifelse
    } ifelse
} def
% --- end prelude ---
";

/// Parse every `%%SelfTest:` block out of a library's source.
///
/// Every malformation is a hard error rather than a skipped block: a
/// self-test that silently doesn't run is worse than no self-test at
/// all, since it reads as coverage that isn't there.
pub fn parse_blocks(source: &str) -> Result<Vec<SelfTestBlock>, String> {
    let mut blocks: Vec<SelfTestBlock> = Vec::new();
    let mut lines = source.lines().enumerate();
    while let Some((i, line)) = lines.next() {
        if !line.starts_with(BEGIN) {
            if line.starts_with(END) {
                return Err(format!(
                    "line {}: `{END}` with no matching `{BEGIN}`",
                    i + 1
                ));
            }
            continue;
        }
        let name = line[BEGIN.len()..].trim();
        if name.is_empty() {
            return Err(format!("line {}: `{BEGIN}` needs a name", i + 1));
        }
        if name.split_whitespace().count() != 1 {
            return Err(format!(
                "line {}: self-test name `{name}` must be a single word",
                i + 1
            ));
        }
        if let Some(prev) = blocks.iter().find(|b| b.name == name) {
            return Err(format!(
                "line {}: duplicate self-test name `{name}` (already used on line {})",
                i + 1,
                prev.line
            ));
        }
        let mut body = String::new();
        let mut closed = false;
        for (j, body_line) in lines.by_ref() {
            if body_line.starts_with(END) {
                closed = true;
                break;
            }
            if body_line.starts_with(BEGIN) {
                return Err(format!(
                    "line {}: `{BEGIN}` inside the block opened on line {} \
                     (blocks don't nest -- is a `{END}` missing?)",
                    j + 1,
                    i + 1
                ));
            }
            let Some(rest) = body_line.strip_prefix('%') else {
                return Err(format!(
                    "line {}: every line of self-test `{name}` must be a `%` comment, \
                     so the library stays inert when it's run normally",
                    j + 1
                ));
            };
            // One optional space after the `%` is the comment marker,
            // not indentation -- everything past it is preserved so a
            // block author can indent PostScript freely.
            body.push_str(rest.strip_prefix(' ').unwrap_or(rest));
            body.push('\n');
        }
        if !closed {
            return Err(format!(
                "line {}: self-test `{name}` is never closed with `{END}`",
                i + 1
            ));
        }
        if body.trim().is_empty() {
            return Err(format!(
                "line {}: self-test `{name}` has an empty body",
                i + 1
            ));
        }
        blocks.push(SelfTestBlock {
            name: name.to_string(),
            line: i + 1,
            body,
        });
    }
    Ok(blocks)
}

/// The relative paths named by a file's `% @requires:` tag, in load
/// order.
///
/// The tag's value is a literal PostScript snippet (`(lib/artkit.ps)
/// run`), and this reads the paths back out of it rather than
/// executing it, so resolution can be made independent of the current
/// directory (see [`resolve_required`]). Anything but a run of
/// `(path) run` pairs is rejected: quietly ignoring a shape this
/// doesn't understand would load the wrong prerequisites and blame the
/// resulting failures on the library.
pub fn parse_requires(source: &str) -> Result<Vec<String>, String> {
    let mut found: Option<&str> = None;
    for (i, line) in source.lines().enumerate() {
        let Some(rest) = line.trim_start().strip_prefix('%') else {
            continue;
        };
        let rest = rest.trim_start();
        let Some(value) = rest.strip_prefix(REQUIRES_TAG) else {
            continue;
        };
        if found.is_some() {
            return Err(format!("line {}: duplicate `% {REQUIRES_TAG}`", i + 1));
        }
        found = Some(value.trim());
    }
    let Some(value) = found else {
        return Ok(Vec::new());
    };
    let mut paths = Vec::new();
    let mut rest = value;
    loop {
        rest = rest.trim_start();
        if rest.is_empty() {
            return Ok(paths);
        }
        let Some(after_open) = rest.strip_prefix('(') else {
            return Err(format!(
                "`% {REQUIRES_TAG}` must be a sequence of `(path) run`; \
                 got `{value}` (stuck at `{rest}`)"
            ));
        };
        let Some(close) = after_open.find(')') else {
            return Err(format!("`% {REQUIRES_TAG}`: unterminated `(` in `{value}`"));
        };
        paths.push(after_open[..close].to_string());
        rest = after_open[close + 1..].trim_start();
        let Some(after_run) = rest.strip_prefix("run") else {
            return Err(format!(
                "`% {REQUIRES_TAG}`: expected `run` after `({})` in `{value}`",
                &after_open[..close]
            ));
        };
        rest = after_run;
    }
}

/// Where a `% @requires:` path actually lives.
///
/// The tag's paths are written repo-root-relative (`lib/artkit.ps`),
/// so they're resolved against the ancestors of the file under test
/// before falling back to [`crate::paths::program_file`]'s CWD and
/// install-layout candidates. Ancestors come first deliberately: a
/// self-test on `<worktree>/lib/paintkit.ps` must load
/// `<worktree>/lib/artkit.ps`, never the copy in whatever directory
/// the command happened to be run from.
fn resolve_required(under_test: &Path, rel: &str) -> Option<PathBuf> {
    let start = under_test.parent()?;
    for ancestor in start.ancestors() {
        let candidate = ancestor.join(rel);
        if candidate.exists() {
            return Some(candidate);
        }
    }
    crate::paths::program_file(rel)
}

/// Run every `%%SelfTest:` block in `path`.
///
/// `Err` means the file itself couldn't be read or its blocks couldn't
/// be parsed; a `Report` that isn't [`Report::ok`] means the blocks ran
/// and something in them failed. Both are non-zero exits for the CLI,
/// but only the second is a statement about the library under test.
pub fn run_file(path: &Path) -> Result<Report, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    // Blocks and tags are ASCII comment text; scanning a lossy view is
    // safe, but the file is *executed* as its original bytes (a lib
    // may legitimately carry non-UTF-8 string data).
    let text = String::from_utf8_lossy(&bytes);
    let blocks = parse_blocks(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    let requires = parse_requires(&text).map_err(|e| format!("{}: {e}", path.display()))?;

    let mut prerequisites: Vec<(String, Vec<u8>)> = Vec::new();
    for rel in &requires {
        let resolved = resolve_required(path, rel).ok_or_else(|| {
            format!(
                "{}: `% {REQUIRES_TAG}` names `{rel}`, which doesn't exist \
                 relative to the file or any of its parent directories",
                path.display()
            )
        })?;
        let body = std::fs::read(&resolved)
            .map_err(|e| format!("cannot read {}: {e}", resolved.display()))?;
        prerequisites.push((resolved.display().to_string(), body));
    }

    let mut results = Vec::new();
    for block in blocks {
        results.push(run_block(path, &bytes, &prerequisites, &block)?);
    }
    Ok(Report {
        path: path.display().to_string(),
        blocks: results,
    })
}

/// A page big enough that a block which actually paints something has
/// somewhere to paint it. Self-tests are about validation guards, not
/// pixels (that's Phase B's pixel-sample operator), so nothing here
/// reads the canvas back — but a stroke that silently clipped to a
/// 1x1 device would be a confusing way to fail.
const PAGE: (u32, u32) = (612, 792);

fn run_block(
    path: &Path,
    under_test: &[u8],
    prerequisites: &[(String, Vec<u8>)],
    block: &SelfTestBlock,
) -> Result<BlockResult, String> {
    let mut interp = Interp::with_page(PAGE.0, PAGE.1)
        .ok_or_else(|| format!("cannot build a {}x{} page", PAGE.0, PAGE.1))?;

    // A failure in the prelude or in the library itself isn't a
    // statement about this block, so it aborts the whole run rather
    // than being reported as one block's failure.
    if let Err(e) = interp.run_str(PRELUDE) {
        return Err(format!(
            "selftest prelude failed: {}",
            interp.error_report(&e)
        ));
    }
    for (name, body) in prerequisites {
        if let Err(e) = interp.run_source(body) {
            return Err(format!(
                "prerequisite {name} failed to load: {}",
                interp.error_report(&e)
            ));
        }
    }
    if let Err(e) = interp.run_source(under_test) {
        return Err(format!(
            "{} failed to load: {}",
            path.display(),
            interp.error_report(&e)
        ));
    }

    let error = match interp.run_source(block.body.as_bytes()) {
        Ok(()) => None,
        Err(e) => Some(interp.error_report(&e)),
    };
    let (failure_count, failures) = collect_failures(&interp);
    Ok(BlockResult {
        name: block.name.clone(),
        line: block.line,
        failures,
        failure_count,
        error,
        gsave_depth: interp.gfx().gsave_depth(),
    })
}

/// Read the prelude's failure log back out of the interpreter.
///
/// A block that clobbered `pscat_st_nfail`/`pscat_st_fails` with
/// something of the wrong shape reports as a synthetic failure rather
/// than as a pass — the alternative (treating an unreadable log as an
/// empty one) would turn a corrupted run green.
fn collect_failures(interp: &Interp) -> (usize, Vec<Failure>) {
    let unreadable = |why: &str| {
        (
            1,
            vec![Failure {
                label: "(harness)".to_string(),
                why: why.to_string(),
                errorname: "--none--".to_string(),
                command: "--none--".to_string(),
            }],
        )
    };
    let Some(count) = interp.load("pscat_st_nfail") else {
        return unreadable("the block removed the harness's own failure counter");
    };
    let count = match count.value {
        Value::Integer(n) if n >= 0 => n as usize,
        _ => return unreadable("the block overwrote the harness's failure counter"),
    };
    if count == 0 {
        return (0, Vec::new());
    }
    let Some(log) = interp.load("pscat_st_fails") else {
        return unreadable("the block removed the harness's own failure log");
    };
    let Value::Array(log) = &log.value else {
        return unreadable("the block overwrote the harness's failure log");
    };
    let mut failures = Vec::new();
    for i in 0..count.min(MAX_FAILURES).min(log.len()) {
        let entry = match log.get(i).map(|o| o.value) {
            Some(Value::Array(a)) if a.len() == 4 => a,
            _ => {
                return unreadable("the harness's failure log holds unexpected entries");
            }
        };
        let field = |n: usize| entry.get(n).map(|o: Object| o.text()).unwrap_or_default();
        failures.push(Failure {
            label: field(0),
            why: field(1),
            errorname: field(2),
            command: field(3),
        });
    }
    (count, failures)
}

/// Human-readable `--selftest` output: one line per block, then the
/// detail of anything that failed.
pub fn format_report(report: &Report) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    if report.blocks.is_empty() {
        let _ = writeln!(
            out,
            "pscat: selftest: {}: no %%SelfTest blocks",
            report.path
        );
        return out;
    }
    for block in &report.blocks {
        let status = if block.ok() { "ok  " } else { "FAIL" };
        let _ = writeln!(
            out,
            "pscat: selftest: {status} {}:{} {}",
            report.path, block.line, block.name
        );
        for f in &block.failures {
            let _ = writeln!(out, "         {}: {}", f.label, f.why);
            if f.errorname != "--none--" {
                let _ = writeln!(
                    out,
                    "           got errorname /{} on /{}",
                    f.errorname, f.command
                );
            }
        }
        if block.failure_count > block.failures.len() {
            let _ = writeln!(
                out,
                "         ... and {} more failure(s) not shown",
                block.failure_count - block.failures.len()
            );
        }
        if let Some(e) = &block.error {
            let _ = writeln!(out, "         the block itself raised: {e}");
        }
        if block.gsave_depth > 0 {
            let _ = writeln!(
                out,
                "         {} unmatched gsave(s) left open by the block",
                block.gsave_depth
            );
        }
    }
    let failed = report.blocks.iter().filter(|b| !b.ok()).count();
    let _ = if failed == 0 {
        writeln!(
            out,
            "pscat: selftest: {} block(s) passed",
            report.blocks.len()
        )
    } else {
        writeln!(
            out,
            "pscat: selftest: {failed} of {} block(s) failed",
            report.blocks.len()
        )
    };
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_block_and_strips_one_leading_space() {
        let src = "%%SelfTest: alpha\n%   1 2 add\n%%EndSelfTest\n";
        let blocks = parse_blocks(src).expect("parses");
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].name, "alpha");
        assert_eq!(blocks[0].line, 1);
        assert_eq!(blocks[0].body, "  1 2 add\n");
    }

    #[test]
    fn ignores_ordinary_comments_and_code_between_blocks() {
        let src = "% just a comment\n/foo { } def\n%%SelfTest: a\n% 1\n%%EndSelfTest\n";
        assert_eq!(parse_blocks(src).expect("parses").len(), 1);
    }

    #[test]
    fn a_non_comment_line_inside_a_block_is_an_error() {
        // The whole point of the convention is that the library stays
        // inert on a normal `run`; a bare code line inside a block
        // would execute at load time.
        let src = "%%SelfTest: a\n1 2 add\n%%EndSelfTest\n";
        let err = parse_blocks(src).expect_err("rejected");
        assert!(err.contains("must be a `%` comment"), "{err}");
    }

    #[test]
    fn an_unterminated_block_is_an_error() {
        let src = "%%SelfTest: a\n% 1 2 add\n";
        let err = parse_blocks(src).expect_err("rejected");
        assert!(err.contains("never closed"), "{err}");
    }

    #[test]
    fn a_duplicate_name_is_an_error() {
        let src = "%%SelfTest: a\n% 1\n%%EndSelfTest\n%%SelfTest: a\n% 2\n%%EndSelfTest\n";
        let err = parse_blocks(src).expect_err("rejected");
        assert!(err.contains("duplicate"), "{err}");
    }

    #[test]
    fn an_empty_body_is_an_error() {
        let src = "%%SelfTest: a\n%\n%%EndSelfTest\n";
        let err = parse_blocks(src).expect_err("rejected");
        assert!(err.contains("empty body"), "{err}");
    }

    #[test]
    fn a_nameless_block_is_an_error() {
        let err = parse_blocks("%%SelfTest:\n% 1\n%%EndSelfTest\n").expect_err("rejected");
        assert!(err.contains("needs a name"), "{err}");
    }

    #[test]
    fn a_stray_end_marker_is_an_error() {
        let err = parse_blocks("%%EndSelfTest\n").expect_err("rejected");
        assert!(err.contains("no matching"), "{err}");
    }

    #[test]
    fn a_nested_begin_marker_is_an_error() {
        let src = "%%SelfTest: a\n% 1\n%%SelfTest: b\n% 2\n%%EndSelfTest\n";
        let err = parse_blocks(src).expect_err("rejected");
        assert!(err.contains("don't nest"), "{err}");
    }

    #[test]
    fn requires_reads_the_capability_catalog_tag() {
        let src = "% @requires: (lib/artkit.ps) run (lib/hatchkit.ps) run\n";
        assert_eq!(
            parse_requires(src).expect("parses"),
            vec!["lib/artkit.ps".to_string(), "lib/hatchkit.ps".to_string()]
        );
    }

    #[test]
    fn no_requires_tag_means_no_prerequisites() {
        assert!(
            parse_requires("% ordinary comment\n")
                .expect("parses")
                .is_empty()
        );
    }

    #[test]
    fn a_requires_shape_this_cant_read_is_an_error() {
        // Silently loading nothing would blame the library for
        // failures caused by a missing prerequisite.
        let err = parse_requires("% @requires: lib/artkit.ps\n").expect_err("rejected");
        assert!(err.contains("sequence of"), "{err}");
    }

    #[test]
    fn a_requires_path_without_run_is_an_error() {
        let err = parse_requires("% @requires: (lib/artkit.ps)\n").expect_err("rejected");
        assert!(err.contains("expected `run`"), "{err}");
    }
}
