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
//! can't corrupt the next — the depths they restore to travel on the
//! operand stack rather than in named variables, so a proc that opens
//! a dictionary shadowing a harness name can't turn the cleanup into a
//! no-op. The graphics and dictionary stacks are checked from Rust
//! after each block as well, so anything that does survive the
//! cleanup is reported rather than silently carried forward.

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
    /// The block's PostScript, `%` markers stripped. Bytes rather than
    /// `String`: a `.ps` file may legitimately carry non-UTF-8 string
    /// data, and a lossy conversion would execute something other than
    /// what the author wrote.
    pub body: Vec<u8>,
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
    /// Unmatched `begin`s left open by the block — `systemdict` and
    /// `userdict` are the permanent baseline, so anything past 2.
    pub dict_depth: usize,
    /// How the operand stack moved during the block, relative to what
    /// loading the library left. Signed on purpose: a block that
    /// *consumed* library-owned operands hasn't left the stack as it
    /// found it either, and clamping that to zero let it report ok
    /// (Codex review, round 4).
    pub operand_delta: i64,
    /// How many assertions actually ran. Zero means the block tested
    /// nothing, which is a failure, not a pass.
    pub assertions: usize,
}

impl BlockResult {
    /// Note the *positive* condition: a block has to have run at least
    /// one assertion. Without it, `ok` was a set of purely negative
    /// checks — nothing failed, nothing leaked — which a block that
    /// asserts nothing at all satisfies trivially. Deleting an
    /// assertion line and leaving its `{ ... }` proc literal behind
    /// (an ordinary careless edit) turned a real guard's coverage off
    /// and still reported green (blank-context review, PR #136).
    pub fn ok(&self) -> bool {
        self.failure_count == 0
            && self.error.is_none()
            && self.gsave_depth == 0
            && self.dict_depth == 0
            && self.operand_delta == 0
            && self.assertions > 0
    }
}

/// Everything `--selftest` found and ran for one file.
#[derive(Debug, Clone)]
pub struct Report {
    pub path: String,
    pub blocks: Vec<BlockResult>,
}

impl Report {
    /// A file explicitly handed to `--selftest` that yields no blocks
    /// is a failure, not a vacuous pass: deleting a library's last
    /// block would otherwise drop its coverage to zero with nothing
    /// reporting it, which is the same silent-drift failure the
    /// parser's hard errors exist to prevent (blank-context review,
    /// PR #136).
    pub fn ok(&self) -> bool {
        !self.blocks.is_empty() && self.blocks.iter().all(BlockResult::ok)
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
% The failure log. Its *entries* are a VM array and roll back under
% `restore` like any other composite -- which is fine, because the
% counters that decide pass/fail do not (see below), so a rolled-back
% entry reports as an unreadable log rather than as a pass.
userdict /pscat_st_fails 128 array put

% The counters, held in a *string* rather than as dictionary entries,
% because PostScript strings are exempt from `restore` (PLRM 3.7.3.2 --
% verified against this interpreter, not just read). Dictionary entries
% are not: with the counters in userdict, a block that ran one real
% assertion and then did `save ... failing assertions ... restore`
% rolled the failure count back to zero and reported green
% (blank-context review, PR #136). Wrapping a test in `save`/`restore`
% to isolate VM state is the same instinct as `gsave`/`grestore`, so
% that is a trap worth closing rather than documenting.
%
%   bytes 0-1  failures recorded      (big-endian, saturating at 65535)
%   bytes 2-3  assertions run
%   bytes 4-5  1 while an assertion is in flight, else 0
userdict /pscat_st_ctr 8 string put

% offset -> n
/pscat_st_ctrget {
    userdict /pscat_st_ctr get exch
    2 copy get 256 mul
    3 1 roll 1 add get
    add
} def

% offset n -> -
/pscat_st_ctrput {
    dup 65535 gt { pop 65535 } if
    2 copy 256 idiv
    userdict /pscat_st_ctr get 3 1 roll put
    256 mod
    exch 1 add exch
    userdict /pscat_st_ctr get 3 1 roll put
} def

% offset -> -   (add one, saturating)
/pscat_st_ctrbump {
    dup pscat_st_ctrget 1 add pscat_st_ctrput
} def

% (label) (why) errorname command  ->  -
%
% Reads and writes the log through `userdict` explicitly rather than
% through the dict stack. A plain `def` lands in whatever dictionary is
% on top, so a block that wraps its assertions in `10 dict begin ...
% end` would record its failures into that dictionary and lose every
% one of them at the `end` -- the run would then report clean. Naming
% userdict pins the log to one place regardless of what the block has
% open, and makes it unshadowable on the way back out.
/pscat_st_note {
    4 array astore
    0 pscat_st_ctrget 128 lt
        {
            userdict /pscat_st_fails get
            0 pscat_st_ctrget
            3 -1 roll put
        }
        { pop }
    ifelse
    0 pscat_st_ctrbump
} def

% A stale $error from a previous assertion must never be able to
% satisfy the next one, so every assertion clears it first.
/pscat_st_clearerr {
    $error /newerror false put
    $error /errorname /--none-- put
    $error /command /--none-- put
} def

% Arm an assertion: record where the stacks stood, with the proc under
% test still on top (hence `count 1 sub`: `stopped` is about to consume
% it), and count the assertion as having run.
%
% Both depths are written and read through `userdict` by name, never
% through the dict stack and never on the operand stack. Two earlier
% shapes were wrong in opposite directions, both verified rather than
% argued:
%
%   - A plain `def` + bare lookup let the proc under test shadow them
%     (`1 dict begin /pscat_st_ddepth 99 def`), turning the cleanup
%     below into a silent no-op -- it left the dictionary open and the
%     debris in place with nothing reporting it.
%   - Carrying them on the operand stack under a `mark` fixed that, but
%     broke when the proc pushed a mark of *its own*: `cleartomark`
%     then cleared to the proc's mark instead, and the cleanup went on
%     to read whatever the proc had left as if it were the saved depth
%     (blank-context review, PR #136 -- one variant silently leaked,
%     the other raised `dictstackunderflow` while popping dictionaries
%     it never opened).
%
% Naming `userdict` explicitly sidesteps both: no dict-stack lookup to
% shadow, and no operand-stack position to disturb.
/pscat_st_arm {
    % `count` has to run before anything else is pushed, or it counts
    % `userdict` and the key too -- which silently overstates the
    % baseline and leaves the proc's debris in place. Caught by the
    % operand-depth check added alongside this, which is the point of
    % having it.
    count 1 sub
    userdict exch /pscat_st_depth exch put
    countdictstack
    userdict exch /pscat_st_ddepth exch put
    % One saved depth per harness, not a stack: an assertion whose proc
    % runs another assertion would overwrite the outer one's baseline,
    % and the outer cleanup would then restore to the wrong place. No
    % block nests today and nesting buys nothing, so it is reported
    % rather than supported -- the alternative is a misattributed
    % failure under the inner label, which is what happened before this
    % check existed (blank-context review, PR #136).
    4 pscat_st_ctrget 0 ne {
        (harness) (assertions cannot be nested: one assertion's proc ran another)
        /--none-- /--none-- pscat_st_note
    } if
    4 1 pscat_st_ctrput
    2 pscat_st_ctrbump
} def

% Undo whatever a caught error left behind. Dictionaries first: `end`
% doesn't touch the operand stack, whereas popping operands first would
% be undone by nothing.
/pscat_st_reset {
    { countdictstack userdict /pscat_st_ddepth get le { exit } if end } loop
    { count userdict /pscat_st_depth get le { exit } if pop } loop
    4 0 pscat_st_ctrput
} def

% {proc} /guardname (label) mustguard  ->  -
% The proc must raise `undefined` on exactly /guardname -- this repo's
% libraries reject malformed input by invoking a self-documenting
% undefined name, and matching that name is what keeps a typo in the
% test body from passing as a caught guard.
%
% Every branch runs pscat_st_reset *before* reading pscat_st_label or
% pscat_st_want: the reset is what pops a dictionary the proc left
% open, so a name lookup after it can't hit a shadowing definition.
/mustguard {
    /pscat_st_label exch def
    /pscat_st_want exch def
    pscat_st_clearerr
    pscat_st_arm
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
    pscat_st_clearerr
    pscat_st_arm
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
    pscat_st_clearerr
    pscat_st_arm
    stopped {
        pscat_st_reset
        pscat_st_label (raised an error but was expected to succeed)
        $error /errorname get $error /command get pscat_st_note
    } {
        pscat_st_reset
    } ifelse
} def

% bool (label) mustbe  ->  -
% A non-boolean is reported rather than coerced: `not` on an integer is
% a bitwise complement in PostScript, so a mistyped operand would
% otherwise quietly test something else entirely.
/mustbe {
    2 pscat_st_ctrbump
    exch dup type /booleantype ne {
        pop (was given a non-boolean, so it tests nothing)
        /--none-- /--none-- pscat_st_note
    } {
        not { (assertion was false) /--none-- /--none-- pscat_st_note } { pop } ifelse
    } ifelse
} def
% --- end prelude ---
";

/// A library's source split into lines, with any `\r` before the `\n`
/// dropped — a CRLF-checked-out file must parse the same as an LF one,
/// or `%%EndSelfTest\r` stops matching its own marker.
///
/// Bytes, not `str`, throughout: a `.ps` file is allowed to hold
/// non-UTF-8 string data, and running a lossy conversion of a block
/// body would silently replace those bytes with U+FFFD and execute
/// something the author didn't write.
/// Per-line: does this line begin already inside an unterminated
/// `(...)` string carried over from an earlier one?
///
/// A `%` only starts a comment *outside* a string, so a multiline
/// PostScript string whose continuation line happens to read
/// `%%SelfTest:` or `% @requires:` is string content, not metadata.
/// Without this, such a line would be parsed as a real block — a
/// phantom passing test — or reject a perfectly valid library (Codex
/// review, round 1).
///
/// Deliberately the same rule `build.rs::lines_starting_in_string`
/// uses, because the two scanners have to agree about which lines are
/// metadata: `build.rs` already filters self-test regions this way, so
/// a `src/selftest.rs` that didn't would disagree with it about the
/// same file. (Neither tracks ASCII85 `<~...~>` strings — a real gap in
/// both, tracked as issue #104, not introduced here.)
fn lines_starting_in_string(lines: &[&[u8]]) -> Vec<bool> {
    let mut out = Vec::with_capacity(lines.len());
    let mut depth: i32 = 0;
    for line in lines {
        out.push(depth > 0);
        let mut chars = line.iter().copied();
        while let Some(c) = chars.next() {
            if depth > 0 {
                match c {
                    b'\\' => {
                        chars.next();
                    }
                    b'(' => depth += 1,
                    b')' => depth -= 1,
                    _ => {}
                }
                continue;
            }
            if c == b'%' {
                break;
            }
            if c == b'(' {
                depth += 1;
            }
        }
    }
    out
}

/// Per-line: is this line inside a `%%SelfTest:`...`%%EndSelfTest`
/// region (markers included)?
///
/// A block body is PostScript the harness owns, not file-level
/// metadata — so a body line reading `% @requires: ...` is part of the
/// test, not a prerequisite declaration. `build.rs::selftest_lines`
/// masks its own scans exactly this way, and the doc comment on
/// [`lines_starting_in_string`] claims the two scanners agree about
/// which lines are metadata; without this they didn't (blank-context
/// review, PR #136).
///
/// Unterminated regions are left unmasked here rather than treated as
/// running to end of file: `parse_blocks` reports that as an error with
/// a line number, which is the better message.
fn selftest_regions(lines: &[&[u8]], in_string: &[bool]) -> Vec<bool> {
    let mut mask = vec![false; lines.len()];
    let mut open: Option<usize> = None;
    for (i, line) in lines.iter().enumerate() {
        if in_string[i] {
            continue;
        }
        if open.is_some() {
            mask[i] = true;
            if line.starts_with(END.as_bytes()) {
                open = None;
            }
        } else if line.starts_with(BEGIN.as_bytes()) {
            mask[i] = true;
            open = Some(i);
        }
    }
    if let Some(start) = open {
        for m in mask.iter_mut().skip(start) {
            *m = false;
        }
    }
    mask
}

fn lines(source: &[u8]) -> Vec<&[u8]> {
    if source.is_empty() {
        return Vec::new();
    }
    let mut out: Vec<&[u8]> = source
        .split(|&b| b == b'\n')
        .map(|l| l.strip_suffix(b"\r").unwrap_or(l))
        .collect();
    // `split` yields a trailing empty element for a source ending in a
    // newline; `str::lines()` doesn't, and the difference isn't
    // cosmetic here — that phantom line isn't a `%` comment, so an
    // unterminated block at end of file would be reported as "this
    // line isn't a comment" rather than "never closed".
    if source.ends_with(b"\n") {
        out.pop();
    }
    out
}

/// Parse every `%%SelfTest:` block out of a library's source.
///
/// Every malformation is a hard error rather than a skipped block: a
/// self-test that silently doesn't run is worse than no self-test at
/// all, since it reads as coverage that isn't there.
pub fn parse_blocks(source: &[u8]) -> Result<Vec<SelfTestBlock>, String> {
    let mut blocks: Vec<SelfTestBlock> = Vec::new();
    let all = lines(source);
    let in_string = lines_starting_in_string(&all);
    let mut i = 0;
    while i < all.len() {
        let line = all[i];
        if in_string[i] {
            i += 1;
            continue;
        }
        if !line.starts_with(BEGIN.as_bytes()) {
            if line.starts_with(END.as_bytes()) {
                return Err(format!(
                    "line {}: `{END}` with no matching `{BEGIN}`",
                    i + 1
                ));
            }
            i += 1;
            continue;
        }
        // The name is an identifier, so it has to be text even in a
        // file whose string data isn't.
        let name = std::str::from_utf8(&line[BEGIN.len()..])
            .map_err(|_| format!("line {}: self-test name is not valid UTF-8", i + 1))?
            .trim();
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
        let mut body: Vec<u8> = Vec::new();
        let mut closed = false;
        let mut j = i + 1;
        while j < all.len() {
            let body_line = all[j];
            if body_line.starts_with(END.as_bytes()) {
                closed = true;
                break;
            }
            if body_line.starts_with(BEGIN.as_bytes()) {
                return Err(format!(
                    "line {}: `{BEGIN}` inside the block opened on line {} \
                     (blocks don't nest -- is a `{END}` missing?)",
                    j + 1,
                    i + 1
                ));
            }
            let Some(rest) = body_line.strip_prefix(b"%") else {
                return Err(format!(
                    "line {}: every line of self-test `{name}` must be a `%` comment, \
                     so the library stays inert when it's run normally",
                    j + 1
                ));
            };
            // One optional space after the `%` is the comment marker,
            // not indentation -- everything past it is preserved so a
            // block author can indent PostScript freely.
            body.extend_from_slice(rest.strip_prefix(b" ").unwrap_or(rest));
            body.push(b'\n');
            j += 1;
        }
        if !closed {
            return Err(format!(
                "line {}: self-test `{name}` is never closed with `{END}`",
                i + 1
            ));
        }
        if body.iter().all(|b| b.is_ascii_whitespace()) {
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
        // Resume after the `%%EndSelfTest` line, never inside the
        // block just consumed.
        i = j + 1;
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
pub fn parse_requires(source: &[u8]) -> Result<Vec<String>, String> {
    let mut found: Option<&str> = None;
    let all = lines(source);
    let in_string = lines_starting_in_string(&all);
    let in_selftest = selftest_regions(&all, &in_string);
    for (i, line) in all.iter().enumerate() {
        // Content of a multiline string is not a comment, however
        // `% @requires:`-shaped it looks; and a self-test block's body
        // is the harness's PostScript, not file-level metadata. Both
        // filters mirror the capability scanner in build.rs, which has
        // to agree with this one about which lines are metadata.
        if in_string[i] || in_selftest[i] {
            continue;
        }
        // A tag line is plain ASCII by construction; anything else on
        // that line isn't one, so a non-UTF-8 line is skipped rather
        // than failing the whole file.
        let Ok(line) = std::str::from_utf8(line) else {
            continue;
        };
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

/// The names of `path`'s `%%SelfTest` blocks, in file order.
///
/// Exists so callers deciding *which* files carry blocks ask the same
/// parser that will later run them. `scripts/selftest.sh` used a
/// `grep '^%%SelfTest:'` for this, which can't tell a real marker from
/// one inside a multiline PostScript string — the parser ignores those,
/// so the two disagreed and CI rejected a file shape the parser
/// deliberately supports (Codex review, round 4).
pub fn list_blocks(path: &Path) -> Result<Vec<String>, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let blocks = parse_blocks(&bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(blocks.into_iter().map(|b| b.name).collect())
}

/// Run every `%%SelfTest:` block in `path`.
///
/// `Err` means the file itself couldn't be read or its blocks couldn't
/// be parsed; a `Report` that isn't [`Report::ok`] means the blocks ran
/// and something in them failed. Both are non-zero exits for the CLI,
/// but only the second is a statement about the library under test.
pub fn run_file(path: &Path) -> Result<Report, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let blocks = parse_blocks(&bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    let requires = parse_requires(&bytes).map_err(|e| format!("{}: {e}", path.display()))?;

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
    // Every early return has to break the cycle too, not just the
    // success path — an `Interp` dropped intact keeps its whole loaded
    // library graph alive until the process exits (blank-context
    // review, PR #136). `setup_failure` takes `interp` by value so
    // forgetting is a compile error rather than a quiet leak.
    let setup_failure = |interp: Interp, msg: String| -> String {
        interp.break_permanent_dict_cycle();
        msg
    };
    if let Err(e) = interp.run_str(PRELUDE) {
        let msg = format!("selftest prelude failed: {}", interp.error_report(&e));
        return Err(setup_failure(interp, msg));
    }
    for (name, body) in prerequisites {
        if let Err(e) = interp.run_source(body) {
            let msg = format!(
                "prerequisite {name} failed to load: {}",
                interp.error_report(&e)
            );
            return Err(setup_failure(interp, msg));
        }
    }
    if let Err(e) = interp.run_source(under_test) {
        let msg = format!(
            "{} failed to load: {}",
            path.display(),
            interp.error_report(&e)
        );
        return Err(setup_failure(interp, msg));
    }

    // Operands the *library* left behind at load time are not this
    // block's doing; measuring absolutely would blame every block in
    // the file for them (blank-context review, PR #136). Same relative
    // rule the dict-stack check uses.
    let operand_baseline = interp.operand_stack().len();

    let error = match interp.run_source(&block.body) {
        Ok(()) => None,
        Err(e) => Some(interp.error_report(&e)),
    };
    let (failure_count, failures) = collect_failures(&interp);
    let result = BlockResult {
        name: block.name.clone(),
        line: block.line,
        failures,
        failure_count,
        error,
        gsave_depth: interp.gfx().gsave_depth(),
        // systemdict + userdict are the permanent baseline (see
        // `Interp::pop_dict`), same rule `lint`'s dict-leak check uses.
        dict_depth: interp.dict_stack_len().saturating_sub(2),
        operand_delta: interp.operand_stack().len() as i64 - operand_baseline as i64,
        assertions: assertion_count(&interp),
    };
    // One `Interp` per block, each holding a fully loaded copy of the
    // library under test. Dropping it isn't enough: systemdict and
    // userdict reference each other, so the whole graph — several
    // hundred KB of library dictionaries per block — would stay alive
    // until the process exits, growing with the block count (Codex
    // review, round 1). This is the same reason `--sweep` and `--spool`
    // call it. Every field the caller needs is already extracted above,
    // and taking `self` makes "nothing runs on it again" a compile-time
    // guarantee.
    interp.break_permanent_dict_cycle();
    Ok(result)
}

/// Read the prelude's failure log back out of the interpreter.
///
/// A block that clobbered the counters or the log with something of
/// the wrong shape reports as a synthetic failure rather than as a
/// pass — the alternative (treating an unreadable log as an empty one)
/// would turn a corrupted run green.
/// One of the prelude's big-endian counters, read out of its
/// restore-exempt string. `None` means the block replaced or truncated
/// the string, which every caller treats as the failing direction.
fn counter(interp: &Interp, offset: usize) -> Option<usize> {
    let obj = interp.load("pscat_st_ctr")?;
    let Value::String(s) = &obj.value else {
        return None;
    };
    let bytes = s.borrow_bytes();
    let hi = *bytes.get(offset)? as usize;
    let lo = *bytes.get(offset + 1)? as usize;
    Some(hi * 256 + lo)
}

/// How many assertions ran. A block that clobbered the counter reads
/// as zero, i.e. as having tested nothing, so it fails rather than
/// passes.
fn assertion_count(interp: &Interp) -> usize {
    counter(interp, 2).unwrap_or(0)
}

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
    let Some(count) = counter(interp, 0) else {
        return unreadable("the block overwrote the harness's failure counter");
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
            "pscat: selftest: {}: no %%SelfTest blocks \u{2014} nothing was checked",
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
        if block.dict_depth > 0 {
            let _ = writeln!(
                out,
                "         {} dictionary(ies) still open at the end of the block",
                block.dict_depth
            );
        }
        match block.operand_delta.cmp(&0) {
            std::cmp::Ordering::Greater => {
                let _ = writeln!(
                    out,
                    "         {} operand(s) left on the stack by the block",
                    block.operand_delta
                );
            }
            std::cmp::Ordering::Less => {
                let _ = writeln!(
                    out,
                    "         {} operand(s) the block consumed but did not push",
                    -block.operand_delta
                );
            }
            std::cmp::Ordering::Equal => {}
        }
        if block.assertions == 0 {
            let _ = writeln!(
                out,
                "         the block ran no assertions at all, so it tests nothing"
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
        let blocks = parse_blocks(src.as_bytes()).expect("parses");
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].name, "alpha");
        assert_eq!(blocks[0].line, 1);
        assert_eq!(blocks[0].body, b"  1 2 add\n");
    }

    #[test]
    fn ignores_ordinary_comments_and_code_between_blocks() {
        let src = "% just a comment\n/foo { } def\n%%SelfTest: a\n% 1\n%%EndSelfTest\n";
        assert_eq!(parse_blocks(src.as_bytes()).expect("parses").len(), 1);
    }

    #[test]
    fn a_non_comment_line_inside_a_block_is_an_error() {
        // The whole point of the convention is that the library stays
        // inert on a normal `run`; a bare code line inside a block
        // would execute at load time.
        let src = "%%SelfTest: a\n1 2 add\n%%EndSelfTest\n";
        let err = parse_blocks(src.as_bytes()).expect_err("rejected");
        assert!(err.contains("must be a `%` comment"), "{err}");
    }

    #[test]
    fn an_unterminated_block_is_an_error() {
        let src = "%%SelfTest: a\n% 1 2 add\n";
        let err = parse_blocks(src.as_bytes()).expect_err("rejected");
        assert!(err.contains("never closed"), "{err}");
    }

    #[test]
    fn a_duplicate_name_is_an_error() {
        let src = "%%SelfTest: a\n% 1\n%%EndSelfTest\n%%SelfTest: a\n% 2\n%%EndSelfTest\n";
        let err = parse_blocks(src.as_bytes()).expect_err("rejected");
        assert!(err.contains("duplicate"), "{err}");
    }

    #[test]
    fn an_empty_body_is_an_error() {
        let src = "%%SelfTest: a\n%\n%%EndSelfTest\n";
        let err = parse_blocks(src.as_bytes()).expect_err("rejected");
        assert!(err.contains("empty body"), "{err}");
    }

    #[test]
    fn a_nameless_block_is_an_error() {
        let err = parse_blocks(b"%%SelfTest:\n% 1\n%%EndSelfTest\n").expect_err("rejected");
        assert!(err.contains("needs a name"), "{err}");
    }

    #[test]
    fn a_stray_end_marker_is_an_error() {
        let err = parse_blocks(b"%%EndSelfTest\n").expect_err("rejected");
        assert!(err.contains("no matching"), "{err}");
    }

    #[test]
    fn a_nested_begin_marker_is_an_error() {
        let src = "%%SelfTest: a\n% 1\n%%SelfTest: b\n% 2\n%%EndSelfTest\n";
        let err = parse_blocks(src.as_bytes()).expect_err("rejected");
        assert!(err.contains("don't nest"), "{err}");
    }

    #[test]
    fn crlf_line_endings_parse_the_same_as_lf() {
        // A CRLF checkout would otherwise leave `%%EndSelfTest\r`
        // failing to match its own marker, and the block would read as
        // unterminated.
        let src = b"%%SelfTest: alpha\r\n%   1 2 add\r\n%%EndSelfTest\r\n";
        let blocks = parse_blocks(src).expect("parses");
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].body, b"  1 2 add\n");
    }

    #[test]
    fn a_non_utf8_body_is_preserved_byte_for_byte() {
        // `.ps` files may carry non-UTF-8 string data. A lossy read
        // would replace these bytes with U+FFFD and execute something
        // the author didn't write.
        let mut src = b"%%SelfTest: alpha\n% (".to_vec();
        src.extend_from_slice(&[0xff, 0xfe]);
        src.extend_from_slice(b") pop\n%%EndSelfTest\n");
        let blocks = parse_blocks(&src).expect("parses");
        assert_eq!(blocks[0].body, b"(\xff\xfe) pop\n");
    }

    #[test]
    fn parsing_resumes_after_a_block_not_inside_it() {
        let src =
            b"%%SelfTest: a\n% 1\n%%EndSelfTest\n/x { } def\n%%SelfTest: b\n% 2\n%%EndSelfTest\n";
        let blocks = parse_blocks(src).expect("parses");
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[1].name, "b");
        assert_eq!(blocks[1].line, 5);
    }

    #[test]
    fn the_prelude_and_rust_agree_on_the_failure_cap() {
        // The cap lives in two places by necessity — PostScript has no
        // growable array, so the prelude preallocates — and a mismatch
        // would silently drop or misreport failures.
        let cap = MAX_FAILURES.to_string();
        assert_eq!(
            PRELUDE.matches(&cap).count(),
            2,
            "PRELUDE must allocate and bound at exactly MAX_FAILURES ({cap})"
        );
    }

    #[test]
    fn a_marker_inside_a_multiline_string_is_not_a_block() {
        // Regression test (Codex review, PR #136): `%` starts a comment
        // only *outside* a string, so a multiline string whose
        // continuation line reads `%%SelfTest:` is string content.
        // Parsing it as a block would invent a phantom passing test, or
        // reject a perfectly valid library. build.rs already applies
        // this rule; the two scanners have to agree.
        let src = b"/banner (line one\n%%SelfTest: not-a-real-block\n% 1 2 add\n%%EndSelfTest\nline five) def\n";
        assert!(parse_blocks(src).expect("parses").is_empty());
    }

    #[test]
    fn a_requires_tag_inside_a_multiline_string_is_not_a_tag() {
        // Same rule, same reason (Codex review, PR #136): loading a
        // prerequisite named by string content would either pull in an
        // unintended file or fail over one that doesn't exist.
        let src = b"/banner (line one\n% @requires: (lib/nonexistent.ps) run\nline three) def\n";
        assert!(parse_requires(src).expect("parses").is_empty());
    }

    #[test]
    fn a_marker_after_a_closed_string_is_still_a_block() {
        // The filter must not swallow real blocks: string state has to
        // actually close.
        let src = b"/banner (one line) def\n%%SelfTest: real\n% 1 2 add\n%%EndSelfTest\n";
        assert_eq!(parse_blocks(src).expect("parses").len(), 1);
    }

    #[test]
    fn an_escaped_paren_does_not_leave_the_scanner_inside_a_string() {
        // `\\(` is a literal paren, not a nesting one — getting this
        // wrong would make every later line look like string content
        // and silently hide every block in the file.
        let src = b"/banner (a \\( paren) def\n%%SelfTest: real\n% 1 2 add\n%%EndSelfTest\n";
        assert_eq!(parse_blocks(src).expect("parses").len(), 1);
    }

    #[test]
    fn requires_reads_the_capability_catalog_tag() {
        let src = "% @requires: (lib/artkit.ps) run (lib/hatchkit.ps) run\n";
        assert_eq!(
            parse_requires(src.as_bytes()).expect("parses"),
            vec!["lib/artkit.ps".to_string(), "lib/hatchkit.ps".to_string()]
        );
    }

    #[test]
    fn no_requires_tag_means_no_prerequisites() {
        assert!(
            parse_requires(b"% ordinary comment\n")
                .expect("parses")
                .is_empty()
        );
    }

    #[test]
    fn a_requires_tag_inside_a_self_test_body_is_not_a_prerequisite() {
        // Regression test (blank-context review, PR #136): a block body
        // is the harness's PostScript, not file-level metadata.
        // `build.rs` already masks self-test regions from its own tag
        // scan, and these two have to agree about the same file.
        let src = b"% @requires: (lib/artkit.ps) run\n\
                    %%SelfTest: a\n\
                    % @requires: (lib/nope.ps) run\n\
                    % 1 2 add\n\
                    %%EndSelfTest\n";
        assert_eq!(
            parse_requires(src).expect("parses"),
            vec!["lib/artkit.ps".to_string()]
        );
    }

    #[test]
    fn a_requires_shape_this_cant_read_is_an_error() {
        // Silently loading nothing would blame the library for
        // failures caused by a missing prerequisite.
        let err = parse_requires(b"% @requires: lib/artkit.ps\n").expect_err("rejected");
        assert!(err.contains("sequence of"), "{err}");
    }

    #[test]
    fn a_requires_path_without_run_is_an_error() {
        let err = parse_requires(b"% @requires: (lib/artkit.ps)\n").expect_err("rejected");
        assert!(err.contains("expected `run`"), "{err}");
    }
}
