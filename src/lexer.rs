//! The tokenizer ("scanner" in PLRM terms).
//!
//! Operates on bytes, not `char`s: PostScript is a byte-oriented language
//! (strings are byte arrays, and real-world files are not reliably UTF-8).
//!
//! One rule worth calling out because it shapes the whole number parser:
//! anything that *looks* like it might be a number but fails to parse as
//! one is a **name**, not an error. `1.2.3`, `16#zz`, and `-` are all
//! valid executable names per the PLRM.

use crate::error::PsError;

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    Integer(i64),
    Real(f64),
    /// Contents of a `(...)` or `<...>` string, escapes already processed.
    String(Vec<u8>),
    /// An executable name — including the self-delimiting `[`, `]`, `<<`,
    /// `>>`, which the PLRM defines as ordinary names looked up in the
    /// dictionary stack.
    Name(String),
    /// `/name`
    LiteralName(String),
    /// `//name` — replaced by its current value at scan time.
    ImmediateName(String),
    LBrace,
    RBrace,
}

pub struct Lexer {
    src: Vec<u8>,
    pos: usize,
}

const fn is_whitespace(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n' | b'\x0c' | b'\0')
}

const fn is_delimiter(b: u8) -> bool {
    matches!(
        b,
        b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%'
    )
}

const fn is_regular(b: u8) -> bool {
    !is_whitespace(b) && !is_delimiter(b)
}

fn syntax(msg: &str) -> PsError {
    PsError::Syntax(msg.to_string())
}

impl Lexer {
    pub fn new(src: Vec<u8>) -> Self {
        Lexer { src, pos: 0 }
    }

    /// Bytes consumed so far — the `token` operator's remainder split.
    pub(crate) fn pos(&self) -> usize {
        self.pos
    }

    fn peek(&self) -> Option<u8> {
        self.src.get(self.pos).copied()
    }

    fn bump(&mut self) -> Option<u8> {
        let b = self.peek();
        if b.is_some() {
            self.pos += 1;
        }
        b
    }

    pub fn next_token(&mut self) -> Result<Option<Token>, PsError> {
        loop {
            let Some(b) = self.bump() else {
                return Ok(None);
            };
            match b {
                _ if is_whitespace(b) => {}
                b'%' => self.skip_comment(),
                b'(' => return Ok(Some(Token::String(self.read_string()?))),
                b')' => return Err(syntax("')' with no matching '('")),
                b'<' => match self.peek() {
                    Some(b'<') => {
                        self.pos += 1;
                        return Ok(Some(Token::Name("<<".to_string())));
                    }
                    Some(b'~') => {
                        self.pos += 1;
                        return Ok(Some(Token::String(self.read_ascii85_string()?)));
                    }
                    _ => return Ok(Some(Token::String(self.read_hex_string()?))),
                },
                b'>' => match self.peek() {
                    Some(b'>') => {
                        self.pos += 1;
                        return Ok(Some(Token::Name(">>".to_string())));
                    }
                    _ => return Err(syntax("'>' outside a hex string")),
                },
                b'[' | b']' => return Ok(Some(Token::Name((b as char).to_string()))),
                b'{' => return Ok(Some(Token::LBrace)),
                b'}' => return Ok(Some(Token::RBrace)),
                b'/' => {
                    if self.peek() == Some(b'/') {
                        self.pos += 1;
                        let word = self.read_regular_run();
                        return Ok(Some(Token::ImmediateName(
                            String::from_utf8_lossy(&word).into_owned(),
                        )));
                    }
                    let word = self.read_regular_run();
                    return Ok(Some(Token::LiteralName(
                        String::from_utf8_lossy(&word).into_owned(),
                    )));
                }
                _ => {
                    self.pos -= 1;
                    let word = self.read_regular_run();
                    return Ok(Some(classify_word(&word)));
                }
            }
        }
    }

    fn skip_comment(&mut self) {
        while let Some(b) = self.peek() {
            if b == b'\n' || b == b'\r' {
                break;
            }
            self.pos += 1;
        }
    }

    fn read_regular_run(&mut self) -> Vec<u8> {
        let start = self.pos;
        while let Some(b) = self.peek() {
            if !is_regular(b) {
                break;
            }
            self.pos += 1;
        }
        self.src[start..self.pos].to_vec()
    }

    fn read_string(&mut self) -> Result<Vec<u8>, PsError> {
        let mut out = Vec::new();
        let mut depth = 1usize;
        loop {
            let Some(b) = self.bump() else {
                return Err(syntax("unterminated string"));
            };
            match b {
                b'\\' => {
                    let Some(e) = self.bump() else {
                        return Err(syntax("unterminated string"));
                    };
                    match e {
                        b'n' => out.push(b'\n'),
                        b'r' => out.push(b'\r'),
                        b't' => out.push(b'\t'),
                        b'b' => out.push(0x08),
                        b'f' => out.push(0x0c),
                        b'0'..=b'7' => {
                            let mut v = u32::from(e - b'0');
                            for _ in 0..2 {
                                match self.peek() {
                                    Some(d @ b'0'..=b'7') => {
                                        v = v * 8 + u32::from(d - b'0');
                                        self.pos += 1;
                                    }
                                    _ => break,
                                }
                            }
                            // Per PLRM, high-order overflow in \ddd is ignored.
                            out.push((v & 0xff) as u8);
                        }
                        // \<newline> is a line continuation: no character.
                        b'\n' => {}
                        b'\r' => {
                            if self.peek() == Some(b'\n') {
                                self.pos += 1;
                            }
                        }
                        // Includes \\, \(, \): the escaped character itself.
                        other => out.push(other),
                    }
                }
                b'(' => {
                    depth += 1;
                    out.push(b);
                }
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        return Ok(out);
                    }
                    out.push(b);
                }
                // The PLRM normalizes CR and CRLF inside strings to LF.
                b'\r' => {
                    if self.peek() == Some(b'\n') {
                        self.pos += 1;
                    }
                    out.push(b'\n');
                }
                _ => out.push(b),
            }
        }
    }

    /// `<~ ... ~>` ASCII85: 5 chars ('!'..'u') encode 4 bytes; 'z' is a
    /// shorthand for four zero bytes; a final partial group of n chars
    /// yields n-1 bytes.
    fn read_ascii85_string(&mut self) -> Result<Vec<u8>, PsError> {
        let mut out = Vec::new();
        let mut group = [0u8; 5];
        let mut n = 0usize;
        loop {
            let Some(b) = self.bump() else {
                return Err(syntax("unterminated ASCII85 string"));
            };
            match b {
                b'~' => {
                    if self.bump() != Some(b'>') {
                        return Err(syntax("'~' without '>' in ASCII85 string"));
                    }
                    break;
                }
                _ if is_whitespace(b) => {}
                b'z' if n == 0 => out.extend_from_slice(&[0, 0, 0, 0]),
                b'!'..=b'u' => {
                    group[n] = b - b'!';
                    n += 1;
                    if n == 5 {
                        let v = group.iter().fold(0u32, |acc, d| {
                            acc.wrapping_mul(85).wrapping_add(u32::from(*d))
                        });
                        out.extend_from_slice(&v.to_be_bytes());
                        n = 0;
                    }
                }
                _ => return Err(syntax("invalid character in ASCII85 string")),
            }
        }
        if n == 1 {
            return Err(syntax("truncated ASCII85 group"));
        }
        if n > 1 {
            // Pad with 'u' (84) and keep n-1 output bytes.
            for slot in group.iter_mut().skip(n) {
                *slot = 84;
            }
            let v = group.iter().fold(0u32, |acc, d| {
                acc.wrapping_mul(85).wrapping_add(u32::from(*d))
            });
            out.extend_from_slice(&v.to_be_bytes()[..n - 1]);
        }
        Ok(out)
    }

    fn read_hex_string(&mut self) -> Result<Vec<u8>, PsError> {
        let mut nibbles = Vec::new();
        loop {
            let Some(b) = self.bump() else {
                return Err(syntax("unterminated hex string"));
            };
            match b {
                b'>' => break,
                _ if is_whitespace(b) => {}
                _ => match (b as char).to_digit(16) {
                    Some(v) => nibbles.push(v as u8),
                    None => return Err(syntax("invalid character in hex string")),
                },
            }
        }
        let mut out = Vec::with_capacity(nibbles.len().div_ceil(2));
        for pair in nibbles.chunks(2) {
            // An odd final nibble is padded with zero, per the PLRM.
            let lo = pair.get(1).copied().unwrap_or(0);
            out.push((pair[0] << 4) | lo);
        }
        Ok(out)
    }
}

fn classify_word(word: &[u8]) -> Token {
    let Ok(s) = std::str::from_utf8(word) else {
        return Token::Name(String::from_utf8_lossy(word).into_owned());
    };
    parse_number(s).unwrap_or_else(|| Token::Name(s.to_string()))
}

/// Also used by `cvi`/`cvr`, which accept full scanner number syntax.
pub(crate) fn parse_number(s: &str) -> Option<Token> {
    // Radix numbers: base#digits, base 2..=36. Anything malformed falls
    // through to being a name.
    if let Some((base, digits)) = s.split_once('#') {
        let base: u32 = base.parse().ok()?;
        if !(2..=36).contains(&base)
            || digits.is_empty()
            || !digits.bytes().all(|b| b.is_ascii_alphanumeric())
        {
            return None;
        }
        return i64::from_str_radix(digits, base).ok().map(Token::Integer);
    }

    let bytes = s.as_bytes();
    if !bytes.iter().any(u8::is_ascii_digit) {
        return None;
    }
    if bytes
        .iter()
        .all(|b| b.is_ascii_digit() || matches!(b, b'+' | b'-'))
    {
        if let Ok(i) = s.parse::<i64>() {
            return Some(Token::Integer(i));
        }
        // Integer literals too large for the integer type become reals —
        // that's the PLRM scanner's rule, not an invention.
        return s
            .parse::<f64>()
            .ok()
            .filter(|r| r.is_finite())
            .map(Token::Real);
    }
    // The charset pre-check keeps f64::from_str from accepting things
    // PostScript doesn't consider numbers ("inf", "NaN").
    if bytes
        .iter()
        .all(|b| b.is_ascii_digit() || matches!(b, b'+' | b'-' | b'.' | b'e' | b'E'))
    {
        return s
            .parse::<f64>()
            .ok()
            .filter(|r| r.is_finite())
            .map(Token::Real);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lex(src: &str) -> Vec<Token> {
        let mut lx = Lexer::new(src.as_bytes().to_vec());
        let mut out = Vec::new();
        while let Some(t) = lx.next_token().expect("unexpected lex error") {
            out.push(t);
        }
        out
    }

    fn lex_err(src: &str) -> PsError {
        let mut lx = Lexer::new(src.as_bytes().to_vec());
        loop {
            match lx.next_token() {
                Ok(Some(_)) => {}
                Ok(None) => panic!("expected a lex error in {src:?}"),
                Err(e) => return e,
            }
        }
    }

    #[test]
    fn integers() {
        assert_eq!(
            lex("1 +2 -3 0"),
            vec![
                Token::Integer(1),
                Token::Integer(2),
                Token::Integer(-3),
                Token::Integer(0)
            ]
        );
    }

    #[test]
    fn reals() {
        assert_eq!(
            lex("1.5 -.5 6. 1e3 1E-3 2.5e2"),
            vec![
                Token::Real(1.5),
                Token::Real(-0.5),
                Token::Real(6.0),
                Token::Real(1000.0),
                Token::Real(0.001),
                Token::Real(250.0)
            ]
        );
    }

    #[test]
    fn huge_integer_literal_becomes_real() {
        assert_eq!(
            lex("9223372036854775808"),
            vec![Token::Real(9.223372036854776e18)]
        );
    }

    #[test]
    fn radix_numbers() {
        assert_eq!(
            lex("16#ff 8#777 2#1010 36#z"),
            vec![
                Token::Integer(255),
                Token::Integer(511),
                Token::Integer(10),
                Token::Integer(35)
            ]
        );
    }

    #[test]
    fn number_lookalikes_are_names() {
        for src in [
            "1.2.3", "1e", "16#zz", "16#", "1#0", "-", "+", "1-2", "inf", "NaN",
        ] {
            assert_eq!(
                lex(src),
                vec![Token::Name(src.to_string())],
                "input {src:?}"
            );
        }
    }

    #[test]
    fn literal_names() {
        assert_eq!(
            lex("/foo /"),
            vec![
                Token::LiteralName("foo".to_string()),
                Token::LiteralName(String::new())
            ]
        );
    }

    #[test]
    fn self_delimiting_tokens() {
        assert_eq!(
            lex("1[2]3"),
            vec![
                Token::Integer(1),
                Token::Name("[".to_string()),
                Token::Integer(2),
                Token::Name("]".to_string()),
                Token::Integer(3)
            ]
        );
        assert_eq!(
            lex("10{dup}"),
            vec![
                Token::Integer(10),
                Token::LBrace,
                Token::Name("dup".to_string()),
                Token::RBrace
            ]
        );
        assert_eq!(
            lex("<< >>"),
            vec![Token::Name("<<".to_string()), Token::Name(">>".to_string())]
        );
    }

    #[test]
    fn comments() {
        assert_eq!(
            lex("1 % junk ( { \n2"),
            vec![Token::Integer(1), Token::Integer(2)]
        );
        assert_eq!(lex("%!PS-Adobe-3.0\n42"), vec![Token::Integer(42)]);
    }

    #[test]
    fn strings() {
        assert_eq!(lex("(hello)"), vec![Token::String(b"hello".to_vec())]);
        assert_eq!(lex("(a(b)c)"), vec![Token::String(b"a(b)c".to_vec())]);
        assert_eq!(lex(r"(a\)b)"), vec![Token::String(b"a)b".to_vec())]);
        assert_eq!(lex(r"(a\nb)"), vec![Token::String(b"a\nb".to_vec())]);
        assert_eq!(lex(r"(\101\12\5)"), vec![Token::String(vec![65, 10, 5])]);
        assert_eq!(lex(r"(\q)"), vec![Token::String(b"q".to_vec())]);
        // Backslash-newline is a line continuation.
        assert_eq!(lex("(a\\\nb)"), vec![Token::String(b"ab".to_vec())]);
        // Raw CR and CRLF normalize to LF.
        assert_eq!(lex("(a\r\nb)"), vec![Token::String(b"a\nb".to_vec())]);
    }

    #[test]
    fn hex_strings() {
        assert_eq!(lex("<48656c6c6f>"), vec![Token::String(b"Hello".to_vec())]);
        assert_eq!(lex("<48 65>"), vec![Token::String(b"He".to_vec())]);
        assert_eq!(lex("<7>"), vec![Token::String(vec![0x70])]);
    }

    #[test]
    fn syntax_errors() {
        for src in ["(abc", ")", ">", "<zz>", "<12"] {
            assert!(matches!(lex_err(src), PsError::Syntax(_)), "input {src:?}");
        }
    }

    #[test]
    fn immediate_names() {
        assert_eq!(lex("//foo"), vec![Token::ImmediateName("foo".to_string())]);
        assert_eq!(
            lex("/a//b"),
            vec![
                Token::LiteralName("a".to_string()),
                Token::ImmediateName("b".to_string())
            ]
        );
    }

    #[test]
    fn ascii85_strings() {
        // "BOu!r" is the canonical encoding of "hell".
        assert_eq!(lex("<~BOu!r~>"), vec![Token::String(b"hell".to_vec())]);
        // Partial final group: 2 chars -> 1 byte.
        assert_eq!(lex("<~@/~>"), vec![Token::String(b"a".to_vec())]);
        // 'z' is four zero bytes; whitespace is ignored.
        assert_eq!(lex("<~ z ~>"), vec![Token::String(vec![0, 0, 0, 0])]);
        assert_eq!(lex("<~~>"), vec![Token::String(Vec::new())]);
        for src in ["<~BOu!r", "<~v~>", "<~!~>"] {
            assert!(matches!(lex_err(src), PsError::Syntax(_)), "input {src:?}");
        }
    }
}
