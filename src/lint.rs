//! Self-check / lint mode (issue #17): heuristic checks an agent driving
//! pscat can run against a finished program to catch whole classes of
//! silent failure — a blank page, a forgotten `grestore`, stuff left on
//! the stack — without eyeballing the rendered PNG.
//!
//! These are heuristics, not correctness rules: a program can
//! legitimately leave data on the operand stack, or paint nothing on
//! purpose (a library file that only defines procedures). Findings are
//! advisory — surfaced to a human/agent to interpret, never fatal on
//! their own.

use tiny_skia::Pixmap;

use crate::object::PsString;
use crate::{Interp, Object, Value};

pub struct LintFinding {
    /// A short, stable slug identifying the check — for filtering/
    /// grepping, not for display on its own.
    pub check: &'static str,
    pub message: String,
}

/// Run every lint check against a finished (or crashed — the checks are
/// still meaningful on a partial canvas) interpreter. `render_checks`
/// gates the checks that only make sense when a page was actually meant
/// to be produced — pass `false` for a pure computation (e.g. an
/// `eval`-style snippet with no `showpage`), where an empty canvas and a
/// result sitting on the operand stack are both the normal, intended
/// outcome rather than mistakes.
pub fn check(interp: &Interp, render_checks: bool) -> Vec<LintFinding> {
    check_with_pages(interp, render_checks, &DeclaredPages::None)
}

/// [`check`] plus the program's own `%%Pages:` declaration (issue #95),
/// which [`scan_declared_pages`] reads from the source.
///
/// A separate entry point rather than an extra argument on `check`:
/// both are public API, and a signature change would break any caller
/// outside this crate for a parameter almost all of them would pass
/// `None` to (Codex review, round 1).
pub fn check_with_pages(
    interp: &Interp,
    render_checks: bool,
    declared_pages: &DeclaredPages,
) -> Vec<LintFinding> {
    let mut findings = Vec::new();
    if render_checks {
        check_blank_pages(interp, &mut findings);
        check_declared_pages(interp, declared_pages, &mut findings);
    }
    check_gsave_balance(interp, &mut findings);
    check_stack_leaks(interp, render_checks, &mut findings);
    findings
}

/// The pages `--png` would write: every emitted page, plus the live
/// canvas whenever nothing has emitted it yet (`has_trailing_art`) or
/// nothing was ever emitted at all — the same rule `finish_headless`
/// uses to decide what a `--png` without a `showpage` should write.
fn pages_with_ink_flags(interp: &Interp) -> Vec<(&Pixmap, bool)> {
    let gfx = interp.gfx();
    let mut pages: Vec<(&Pixmap, bool)> = gfx
        .pages()
        .iter()
        .zip(gfx.pages_had_ink().iter().copied())
        .collect();
    if pages.is_empty() || gfx.has_trailing_art() {
        pages.push((&gfx.pixmap, gfx.has_trailing_art()));
    }
    pages
}

/// What a program's DSC header says about its own page count.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeclaredPages {
    /// No `%%Pages:` header, or the explicit `(atend)` form — DSC's
    /// legitimate "the count comes later", which declares nothing.
    None,
    Count(usize),
    /// A `%%Pages:` whose value is neither a number nor `(atend)`.
    /// Kept as a distinct case rather than folded into `None`: a
    /// typo'd count that reads as "no declaration" would silently
    /// disable the very check it was written to enable (Codex review,
    /// round 1) — precisely the failure mode issue #95 exists to close.
    Malformed(String),
}

/// The page count a program declares about itself.
///
/// DSC's own header comment, not a new convention — which is the
/// point: a program that says how many pages it produces can be
/// checked against what it actually produced.
///
/// This exists for issue #95's rendering drivers, where the rule is
/// one `showpage` per independently-checked scenario: two scenarios
/// that accidentally share a page let the second one's ink hide the
/// first one's blank-page regression, and that mistake is one deleted
/// `showpage` away in ordinary editing. A declared count turns it from
/// a convention into something checkable.
///
/// Scanning stops at `%%EndComments` or the first non-comment line,
/// mirroring `pdf::scan_document_info` — column zero alone doesn't make
/// a line a header, and a document that embeds another one further
/// down would otherwise adopt the *embedded* count and compare it
/// against the outer document's output (Codex review, round 1).
pub fn scan_declared_pages(source: &[u8]) -> DeclaredPages {
    for raw_line in source.split(|&b| b == b'\n') {
        let line = String::from_utf8_lossy(raw_line);
        let line = line.trim_end_matches('\r');
        if line.is_empty() {
            continue;
        }
        if let Some(value) = line.strip_prefix("%%Pages:") {
            let value = value.trim();
            if value.is_empty() {
                return DeclaredPages::Malformed(String::new());
            }
            // Only the first token is the count. Older DSC allows a
            // page-order operand after it (`%%Pages: 3 1`), and a lint
            // heuristic must not hard-fail on a form real files use —
            // under `--lint-strict` that would abort a run over a
            // header the check was never meant to judge, and under
            // plain `--lint` it would put a spurious finding in front
            // of every `pscat-mcp` caller (blank-context review, PR
            // #136).
            let first = value.split_whitespace().next().unwrap_or(value);
            if first == "(atend)" {
                return DeclaredPages::None;
            }
            return match first.parse() {
                Ok(n) => DeclaredPages::Count(n),
                Err(_) => DeclaredPages::Malformed(value.to_string()),
            };
        }
        if line.starts_with("%%EndComments") || !line.starts_with('%') {
            break;
        }
    }
    DeclaredPages::None
}

fn check_declared_pages(
    interp: &Interp,
    declared: &DeclaredPages,
    findings: &mut Vec<LintFinding>,
) {
    let declared = match declared {
        DeclaredPages::None => return,
        DeclaredPages::Malformed(value) => {
            findings.push(LintFinding {
                check: "page-count",
                message: format!(
                    "`%%Pages: {value}` is neither a count nor `(atend)`, so the \
                     declared-page check can't run"
                ),
            });
            return;
        }
        DeclaredPages::Count(n) => *n,
    };
    // Emitted pages only — deliberately *not* `pages_with_ink_flags`,
    // which appends the live trailing canvas. Counting that made a
    // driver whose *final* `showpage` was deleted still match its
    // declaration, so the one mistake this check exists to catch was
    // the one it missed (Codex review, round 8). The trailing canvas
    // remains the right rule for blank-page checking, which is about
    // what `--png` would write rather than about page boundaries.
    let actual = interp.gfx().pages().len();
    if actual != declared {
        findings.push(LintFinding {
            check: "page-count",
            message: format!(
                "the program declares `%%Pages: {declared}` but emitted {actual} \
                 showpage(s) — a missing or extra one"
            ),
        });
    }
}

fn check_blank_pages(interp: &Interp, findings: &mut Vec<LintFinding>) {
    let pages = pages_with_ink_flags(interp);
    let total = pages.len();
    for (i, (page, had_ink)) in pages.into_iter().enumerate() {
        // `had_ink` catches a page that's a lazy-erase repeat of the
        // previous one (two showpages with nothing painted between
        // them snapshot the identical, non-blank pixmap twice) even
        // when its pixels aren't literally blank; `is_blank` still
        // catches a first/only page that's untouched white.
        if !had_ink || is_blank(page) {
            let why = if had_ink {
                "every pixel is the untouched white background".to_string()
            } else {
                "nothing was painted since the previous page (repeats it, or the program never drew before this showpage)".to_string()
            };
            findings.push(LintFinding {
                check: "blank-page",
                message: format!("page {} of {total} has no ink — {why}", i + 1),
            });
        }
    }
}

/// Untouched device pixels are opaque white — the fill `Gfx::with_scale`
/// starts every page with. A deliberate solid fill (even solid white
/// paper drawn on purpose) isn't distinguishable from "nothing drawn"
/// this way, but that's a rare and harmless false negative; the
/// common case this catches is "the program never painted at all".
fn is_blank(pixmap: &Pixmap) -> bool {
    pixmap
        .data()
        .as_chunks::<4>()
        .0
        .iter()
        .all(|p| *p == [255, 255, 255, 255])
}

fn check_gsave_balance(interp: &Interp, findings: &mut Vec<LintFinding>) {
    let depth = interp.gfx().gsave_depth();
    if depth > 0 {
        findings.push(LintFinding {
            check: "gsave-imbalance",
            message: format!(
                "{depth} unmatched gsave(s) left open at the end of the program (missing grestore)"
            ),
        });
    }
}

/// A leaked multi-megabyte string is exactly the kind of thing worth a
/// leak warning about, so the preview can't just skip large operands —
/// but it must stay cheap regardless of how large they are.
/// `pscat-mcp`'s `render_postscript` runs this on every render, so an
/// unbounded preview becomes unbounded MCP latency/memory, not just an
/// unbounded message (Codex review, PR #59, round 2: truncating
/// `repr()`'s *output* still built the whole thing first — a 10M-byte
/// string produced a ~40MB intermediate before being cut to 80 chars).
/// Strings are previewed by slicing the bytes *before* escaping, never
/// escaping more than the limit; arrays/procedures report only their
/// length, never recursing into elements, since a short array of huge
/// strings would defeat a length-only bound on the array itself; names
/// are previewed by taking a bounded prefix of chars (round 3: a name
/// has no length limit here — `read_regular_run` just keeps consuming
/// non-delimiter bytes — and `n.to_string()`/`format!("/{n}")` would
/// otherwise copy the whole thing). Everything else (numbers, dicts,
/// ...) already has an inherently bounded `repr()`.
const OPERAND_PREVIEW_LIMIT: usize = 80;

fn preview(obj: &Object) -> String {
    match &obj.value {
        Value::String(s) => string_preview(s),
        Value::Array(a) => {
            let kind = if obj.executable { "procedure" } else { "array" };
            format!("<{kind} of {} element(s)>", a.len())
        }
        Value::Name(n) => {
            let total = n.len();
            let prefix: String = n.chars().take(OPERAND_PREVIEW_LIMIT).collect();
            if prefix.len() == total {
                obj.repr()
            } else {
                let sigil = if obj.executable { "" } else { "/" };
                format!("{sigil}{prefix}...<{total} bytes total>")
            }
        }
        _ => obj.repr(),
    }
}

fn string_preview(s: &PsString) -> String {
    let len = s.len();
    let take = OPERAND_PREVIEW_LIMIT.min(len);
    let escaped = crate::object::escape_string(&s.borrow_bytes()[..take]);
    if take < len {
        format!("({escaped}...<{len} bytes total>)")
    } else {
        format!("({escaped})")
    }
}

fn check_stack_leaks(interp: &Interp, render_checks: bool, findings: &mut Vec<LintFinding>) {
    // A snippet with no output format requested (eval-style usage) is
    // expected to leave its result on the stack — that's the entire
    // point of `pscat -e`/`eval_postscript` — so this check only
    // applies when a render was actually asked for.
    if render_checks {
        let ostack = interp.operand_stack();
        if !ostack.is_empty() {
            const SHOWN: usize = 5;
            let top: Vec<String> = ostack.iter().rev().take(SHOWN).map(preview).collect();
            let more = if ostack.len() > SHOWN {
                format!(" (top {SHOWN} of {} shown, topmost first)", ostack.len())
            } else {
                " (topmost first)".to_string()
            };
            findings.push(LintFinding {
                check: "stack-leak",
                message: format!(
                    "{} item(s) left on the operand stack at the end of the program: {}{more}",
                    ostack.len(),
                    top.join(" ")
                ),
            });
        }
    }
    // systemdict + userdict are the permanent baseline (see
    // `Interp::pop_dict`); anything deeper is an unmatched `begin` —
    // true regardless of whether a render was requested, unlike the
    // operand stack, so this isn't gated by `render_checks`.
    let dict_depth = interp.dict_stack_len();
    if dict_depth > 2 {
        findings.push(LintFinding {
            check: "dict-leak",
            message: format!(
                "{} dictionary(ies) still open at the end of the program (missing `end`)",
                dict_depth - 2
            ),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Interp {
        let mut it = Interp::with_page(50, 50).expect("page");
        let _ = it.run_str(src);
        it
    }

    fn checks(it: &Interp, render_checks: bool) -> Vec<&'static str> {
        check(it, render_checks).iter().map(|f| f.check).collect()
    }

    fn checks_with_pages(it: &Interp, declared: &DeclaredPages) -> Vec<&'static str> {
        check_with_pages(it, true, declared)
            .iter()
            .map(|f| f.check)
            .collect()
    }

    #[test]
    fn blank_canvas_is_flagged() {
        let it = run("showpage");
        assert!(checks(&it, true).contains(&"blank-page"));
    }

    #[test]
    fn a_matching_declared_page_count_is_not_flagged() {
        let it = run("0 0 40 40 rectfill showpage 5 5 30 30 rectfill showpage");
        assert!(!checks_with_pages(&it, &DeclaredPages::Count(2)).contains(&"page-count"));
    }

    #[test]
    fn a_missing_showpage_merges_two_scenarios_and_is_flagged() {
        // The failure mode this check exists for (issue #95): a
        // rendering driver declares one page per checked scenario, and
        // a deleted `showpage` silently merges two of them — after
        // which the second scenario's ink hides whether the first drew
        // anything at all.
        let it = run("0 0 40 40 rectfill 5 5 30 30 rectfill showpage");
        let found = check_with_pages(&it, true, &DeclaredPages::Count(2));
        let msg = found
            .iter()
            .find(|f| f.check == "page-count")
            .expect("page-count finding");
        assert!(msg.message.contains("emitted 1"), "{}", msg.message);
    }

    #[test]
    fn a_deleted_final_showpage_is_flagged() {
        // Regression test (Codex review, PR #136): counting the live
        // trailing canvas as a page made the *last* scenario's missing
        // `showpage` invisible — the check matched its declaration
        // anyway.
        let it = run("0 0 40 40 rectfill showpage 5 5 30 30 rectfill");
        let found = check_with_pages(&it, true, &DeclaredPages::Count(2));
        let msg = found
            .iter()
            .find(|f| f.check == "page-count")
            .expect("page-count finding");
        assert!(msg.message.contains("emitted 1"), "{}", msg.message);
    }

    #[test]
    fn an_undeclared_page_count_is_never_flagged() {
        // Most programs carry no `%%Pages:` at all; they must not
        // acquire a finding for it.
        let it = run("0 0 40 40 rectfill showpage");
        assert!(!checks_with_pages(&it, &DeclaredPages::None).contains(&"page-count"));
    }

    #[test]
    fn declared_pages_reads_a_dsc_header() {
        assert_eq!(
            scan_declared_pages(b"%!PS\n%%Pages: 7\n%%EndComments\n"),
            DeclaredPages::Count(7)
        );
    }

    #[test]
    fn declared_pages_tolerates_a_trailing_page_order_operand() {
        // Regression test (blank-context review, PR #136): `%%Pages: 3 1`
        // is a real DSC form, and reporting it as malformed would fail
        // a strict-lint run over a header the check never judges.
        assert_eq!(
            scan_declared_pages(b"%%Pages: 3 1\n"),
            DeclaredPages::Count(3)
        );
        assert_eq!(
            scan_declared_pages(b"%%Pages: 3\n"),
            DeclaredPages::Count(3)
        );
    }

    #[test]
    fn declared_pages_treats_atend_as_no_declaration() {
        // DSC's legitimate "the count comes later" form.
        assert_eq!(
            scan_declared_pages(b"%%Pages: (atend)\n"),
            DeclaredPages::None
        );
        assert_eq!(scan_declared_pages(b"%!PS\n"), DeclaredPages::None);
    }

    #[test]
    fn a_malformed_page_count_is_reported_not_ignored() {
        // Regression test (Codex review, PR #136): treating a typo'd
        // count as "no declaration" would silently disable the very
        // check it was written to enable.
        assert_eq!(
            scan_declared_pages(b"%%Pages: nine\n"),
            DeclaredPages::Malformed("nine".to_string())
        );
        let it = run("0 0 40 40 rectfill showpage");
        let found = check_with_pages(&it, true, &scan_declared_pages(b"%%Pages: nine\n"));
        let msg = found
            .iter()
            .find(|f| f.check == "page-count")
            .expect("page-count finding");
        assert!(msg.message.contains("neither a count"), "{}", msg.message);
    }

    #[test]
    fn declared_pages_stops_at_the_end_of_the_dsc_header() {
        // Regression test (Codex review, PR #136): column zero alone
        // doesn't make a line a header. A document that embeds another
        // one further down would otherwise adopt the *embedded* count
        // and compare it against the outer document's output.
        assert_eq!(
            scan_declared_pages(b"%!PS\n%%EndComments\n%%Pages: 12\n"),
            DeclaredPages::None
        );
        assert_eq!(
            scan_declared_pages(b"%!PS\n0 0 moveto\n%%Pages: 12\n"),
            DeclaredPages::None
        );
        // ...and an indented one isn't a header comment either.
        assert_eq!(scan_declared_pages(b"  %%Pages: 3\n"), DeclaredPages::None);
    }

    #[test]
    fn painted_canvas_is_not_flagged() {
        let it = run("0 0 40 40 rectfill showpage");
        assert!(!checks(&it, true).contains(&"blank-page"));
    }

    #[test]
    fn blank_page_is_skipped_when_render_checks_are_off() {
        let it = run("3 4 add pop");
        assert!(!checks(&it, false).contains(&"blank-page"));
    }

    #[test]
    fn nothing_painted_but_no_showpage_still_flags_the_trailing_canvas() {
        let it = run("");
        assert!(checks(&it, true).contains(&"blank-page"));
    }

    #[test]
    fn unbalanced_gsave_is_flagged() {
        let it = run("gsave");
        assert!(checks(&it, true).contains(&"gsave-imbalance"));
    }

    #[test]
    fn balanced_gsave_is_not_flagged() {
        let it = run("gsave grestore");
        assert!(!checks(&it, true).contains(&"gsave-imbalance"));
    }

    #[test]
    fn leftover_operands_are_flagged() {
        let it = run("1 2 3");
        let found = check(&it, true);
        let leak = found
            .iter()
            .find(|f| f.check == "stack-leak")
            .expect("stack-leak finding");
        assert!(
            leak.message.contains('3'),
            "count in message: {}",
            leak.message
        );
    }

    #[test]
    fn clean_stack_is_not_flagged() {
        let it = run("1 1 add pop");
        assert!(!checks(&it, true).contains(&"stack-leak"));
    }

    #[test]
    fn unmatched_begin_is_flagged() {
        let it = run("userdict begin");
        assert!(checks(&it, true).contains(&"dict-leak"));
    }

    #[test]
    fn matched_begin_end_is_not_flagged() {
        let it = run("userdict begin end");
        assert!(!checks(&it, true).contains(&"dict-leak"));
    }

    #[test]
    fn stack_leak_is_skipped_when_render_checks_are_off() {
        // Regression test (Codex review, PR #59): eval-style usage
        // (`pscat -e`, `eval_postscript`) leaves its result on the
        // stack on purpose — that used to still trip stack-leak because
        // only check_blank_pages was gated on render_checks.
        let it = run("3 4 add");
        assert!(!checks(&it, false).contains(&"stack-leak"));
    }

    #[test]
    fn dict_leak_still_fires_when_render_checks_are_off() {
        // Unlike stack-leak, an unmatched `begin` is a mistake in eval
        // usage too — not gated by render_checks.
        let it = run("userdict begin");
        assert!(checks(&it, false).contains(&"dict-leak"));
    }

    #[test]
    fn a_second_showpage_with_no_new_ink_is_a_logical_blank_page() {
        // Regression test (Codex review, PR #59): showpage's lazy erase
        // means two showpages with nothing painted between them
        // snapshot the identical, non-blank pixmap twice — pixel
        // content alone can't see that the second one has no ink of
        // its own.
        let it = run("0 0 40 40 rectfill showpage showpage");
        let findings = check(&it, true);
        let blanks: Vec<&str> = findings
            .iter()
            .filter(|f| f.check == "blank-page")
            .map(|f| f.message.as_str())
            .collect();
        assert_eq!(blanks.len(), 1, "exactly the second page: {blanks:?}");
        assert!(blanks[0].contains("page 2 of 2"), "{blanks:?}");
    }

    #[test]
    fn a_long_leaked_operand_is_truncated() {
        let it = run("100000 string");
        let findings = check(&it, true);
        let leak = findings
            .iter()
            .find(|f| f.check == "stack-leak")
            .expect("stack-leak finding");
        assert!(
            leak.message.len() < 1000,
            "message should be bounded, got {} bytes",
            leak.message.len()
        );
        assert!(
            leak.message.contains("bytes total"),
            "should note it was truncated: {}",
            leak.message
        );
    }

    #[test]
    fn a_huge_leaked_string_is_bounded_without_building_the_whole_repr() {
        // Regression test (Codex review round 2, PR #59): the original
        // fix truncated repr()'s *output*, which still built the whole
        // escaped string first -- a 10M-byte string produced a ~40MB
        // intermediate. This should stay fast and the message small
        // regardless of the leaked string's size.
        let it = run("10000000 string");
        let findings = check(&it, true);
        let leak = findings
            .iter()
            .find(|f| f.check == "stack-leak")
            .expect("stack-leak finding");
        assert!(
            leak.message.len() < 1000,
            "message should be bounded, got {} bytes",
            leak.message.len()
        );
        assert!(
            leak.message.contains("10000000 bytes total") || leak.message.contains("10000002"),
            "should report the real size: {}",
            leak.message
        );
    }

    #[test]
    fn a_leaked_array_reports_length_without_recursing_into_elements() {
        // A short array of one huge string must not defeat the bound
        // by recursing into repr() for each element.
        let it = run("[10000000 string]");
        let findings = check(&it, true);
        let leak = findings
            .iter()
            .find(|f| f.check == "stack-leak")
            .expect("stack-leak finding");
        assert!(
            leak.message.len() < 1000,
            "message should be bounded, got {} bytes",
            leak.message.len()
        );
        assert!(
            leak.message.contains("array of 1 element"),
            "should describe the array by length, not its contents: {}",
            leak.message
        );
    }

    #[test]
    fn a_huge_leaked_name_is_bounded() {
        // Regression test (Codex review round 3, PR #59): a literal
        // name has no length limit here (`read_regular_run` just keeps
        // consuming non-delimiter bytes), so the fallback-to-repr()
        // path missed the same unbounded-preview problem as strings.
        let src = format!("/{}", "a".repeat(1_000_000));
        let it = run(&src);
        let findings = check(&it, true);
        let leak = findings
            .iter()
            .find(|f| f.check == "stack-leak")
            .expect("stack-leak finding");
        assert!(
            leak.message.len() < 1000,
            "message should be bounded, got {} bytes",
            leak.message.len()
        );
        assert!(
            leak.message.contains("bytes total"),
            "should note it was truncated: {}",
            leak.message
        );
    }
}
