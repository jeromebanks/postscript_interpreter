//! Line accumulation for interactive input — shared by the terminal
//! REPL and the `--interactive` window (Stage 8's last sliver).
//!
//! The one real decision lives here: is what the user has typed so far
//! executable, or are they mid-procedure / mid-string and the front end
//! should keep reading? Keeping it in a module of its own makes the
//! policy testable without a terminal or a window.

use crate::error::PsError;
use crate::lexer::{Lexer, Token};

/// Accumulates typed lines into executable chunks.
#[derive(Default)]
pub struct LineBuffer {
    pending: String,
}

impl LineBuffer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed one line (trailing newline optional). Returns the full
    /// buffered source once it forms a complete program; `None` means
    /// keep reading (show a continuation prompt).
    pub fn push_line(&mut self, line: &str) -> Option<String> {
        self.pending.push_str(line);
        if !self.pending.ends_with('\n') {
            self.pending.push('\n');
        }
        if source_is_complete(&self.pending) {
            Some(std::mem::take(&mut self.pending))
        } else {
            None
        }
    }

    /// Whether a continuation is in progress (`...>` rather than `PS>`).
    pub fn is_mid_input(&self) -> bool {
        !self.pending.is_empty()
    }
}

/// Whether `src` can be executed as-is, or is mid-procedure / mid-string
/// and the caller should keep reading lines. Errors other than "ran off
/// the end" count as complete — the interpreter will report them properly.
pub fn source_is_complete(src: &str) -> bool {
    let mut lexer = Lexer::new(src.as_bytes().to_vec());
    let mut brace_depth = 0i64;
    loop {
        match lexer.next_token() {
            Ok(None) => return brace_depth <= 0,
            Ok(Some(Token::LBrace)) => brace_depth += 1,
            Ok(Some(Token::RBrace)) => brace_depth -= 1,
            Ok(Some(_)) => {}
            // An unterminated string (or hex string) means "keep typing";
            // anything else is a real syntax error to surface now.
            Err(PsError::Syntax(m)) if m.starts_with("unterminated") => return false,
            Err(_) => return true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_lines_are_complete() {
        assert!(source_is_complete("1 2 add =\n"));
        assert!(source_is_complete(""));
    }

    #[test]
    fn open_procedures_and_strings_continue() {
        assert!(!source_is_complete("/f {\n"));
        assert!(!source_is_complete("/f { 1 { 2 } if\n"));
        assert!(!source_is_complete("(two\nlines"));
        assert!(!source_is_complete("<AB CD"));
        assert!(source_is_complete("/f { 1 } def\n"));
        assert!(source_is_complete("(two\nlines)\n"));
    }

    #[test]
    fn real_syntax_errors_count_as_complete() {
        // Excess closers and malformed tokens go to the interpreter,
        // which reports them as proper PostScript errors.
        assert!(source_is_complete("} }\n"));
        assert!(source_is_complete("<zz>\n"));
    }

    #[test]
    fn buffer_joins_lines_until_complete() {
        let mut buf = LineBuffer::new();
        assert_eq!(buf.push_line("/box {"), None);
        assert!(buf.is_mid_input());
        assert_eq!(buf.push_line("  0 0 10 10 rectstroke"), None);
        let chunk = buf.push_line("} def").expect("complete now");
        assert_eq!(chunk, "/box {\n  0 0 10 10 rectstroke\n} def\n");
        assert!(!buf.is_mid_input());
        // And the buffer is ready for the next chunk.
        assert_eq!(buf.push_line("1 1 add ="), Some("1 1 add =\n".into()));
    }
}
