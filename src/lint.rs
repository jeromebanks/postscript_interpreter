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

use crate::{Interp, Object};

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
    let mut findings = Vec::new();
    if render_checks {
        check_blank_pages(interp, &mut findings);
    }
    check_gsave_balance(interp, &mut findings);
    check_stack_leaks(interp, render_checks, &mut findings);
    findings
}

fn check_blank_pages(interp: &Interp, findings: &mut Vec<LintFinding>) {
    let gfx = interp.gfx();
    let mut pages: Vec<(&Pixmap, bool)> = gfx
        .pages()
        .iter()
        .zip(gfx.pages_had_ink().iter().copied())
        .collect();
    // The live canvas counts as a trailing page whenever nothing has
    // emitted it yet (has_trailing_art) or nothing was ever emitted at
    // all (pages is empty) — the same rule `finish_headless` uses to
    // decide what a `--png` without a `showpage` should write.
    if pages.is_empty() || gfx.has_trailing_art() {
        pages.push((&gfx.pixmap, gfx.has_trailing_art()));
    }
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
        .chunks_exact(4)
        .all(|p| p == [255, 255, 255, 255])
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

/// A `repr()` can be enormous (a leaked multi-megabyte string is
/// exactly the kind of thing worth a leak warning about) — truncate so
/// one finding can't balloon the diagnostic output past what it's
/// reporting on. `pscat-mcp`'s `render_postscript` forwards this text
/// verbatim into its response, so an unbounded repr becomes an
/// unbounded MCP payload.
const OPERAND_PREVIEW_LIMIT: usize = 80;

fn preview(obj: &Object) -> String {
    let r = obj.repr();
    // Byte-slicing a raw index risks landing mid-character on a
    // non-ASCII name; counting/collecting chars instead is always
    // boundary-safe, at the cost of "chars" not quite meaning "bytes"
    // for the truncation point — an acceptable tradeoff for a preview.
    if r.chars().count() <= OPERAND_PREVIEW_LIMIT {
        r
    } else {
        let truncated: String = r.chars().take(OPERAND_PREVIEW_LIMIT).collect();
        format!("{truncated}...<{} bytes total>", r.len())
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

    #[test]
    fn blank_canvas_is_flagged() {
        let it = run("showpage");
        assert!(checks(&it, true).contains(&"blank-page"));
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
}
