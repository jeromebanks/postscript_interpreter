//! File objects and decode filters.
//!
//! A `PsFile` is a byte source with a *shared position*: the scanner,
//! `currentfile`, `read`/`readstring`/`readhexstring`, and filters all
//! pull from the same cursor. That sharing is the whole point — it is
//! what makes `currentfile eexec` (embedded Type 1 fonts), inline image
//! data, and `currentfile ... readhexstring` prologs work: the program
//! reads data from the middle of its own source, and the interpreter
//! resumes scanning exactly where the data ended.
//!
//! Filters are files layered on files. Decoding is **on demand**, one
//! pull at a time, so a filter consumes only as many source bytes as
//! its consumer actually needed — the property eexec's
//! `currentfile closefile` convention depends on (the plaintext zeros
//! after the encrypted region must remain unread until the outer
//! scanner gets there).

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use crate::error::PsError;

pub type FileHandle = Rc<RefCell<PsFile>>;

pub struct PsFile {
    kind: Kind,
    closed: bool,
    /// Bytes handed to consumers — the `token` operator's remainder
    /// arithmetic and diagnostics.
    consumed: usize,
}

enum Kind {
    Bytes {
        data: Vec<u8>,
        pos: usize,
    },
    Filter {
        source: FileHandle,
        dec: Decoder,
        out: VecDeque<u8>,
        eod: bool,
    },
}

impl PsFile {
    pub fn from_bytes(data: Vec<u8>) -> FileHandle {
        Rc::new(RefCell::new(PsFile {
            kind: Kind::Bytes { data, pos: 0 },
            closed: false,
            consumed: 0,
        }))
    }

    /// The PLRM's "invalid file object" — what `currentfile` returns
    /// when no file is being executed.
    pub fn closed_file() -> FileHandle {
        let f = Self::from_bytes(Vec::new());
        f.borrow_mut().closed = true;
        f
    }

    pub fn filtered(source: FileHandle, dec: Decoder) -> FileHandle {
        Rc::new(RefCell::new(PsFile {
            kind: Kind::Filter {
                source,
                dec,
                out: VecDeque::new(),
                eod: false,
            },
            closed: false,
            consumed: 0,
        }))
    }

    pub fn read_byte(&mut self) -> Result<Option<u8>, PsError> {
        if self.closed {
            return Ok(None);
        }
        let b = match &mut self.kind {
            Kind::Bytes { data, pos } => {
                let b = data.get(*pos).copied();
                if b.is_some() {
                    *pos += 1;
                }
                b
            }
            Kind::Filter {
                source,
                dec,
                out,
                eod,
            } => {
                Self::ensure_output(source, dec, out, eod)?;
                out.pop_front()
            }
        };
        if b.is_some() {
            self.consumed += 1;
        }
        Ok(b)
    }

    pub fn peek_byte(&mut self) -> Result<Option<u8>, PsError> {
        if self.closed {
            return Ok(None);
        }
        Ok(match &mut self.kind {
            Kind::Bytes { data, pos } => data.get(*pos).copied(),
            Kind::Filter {
                source,
                dec,
                out,
                eod,
            } => {
                Self::ensure_output(source, dec, out, eod)?;
                out.front().copied()
            }
        })
    }

    fn ensure_output(
        source: &FileHandle,
        dec: &mut Decoder,
        out: &mut VecDeque<u8>,
        eod: &mut bool,
    ) -> Result<(), PsError> {
        while out.is_empty() && !*eod {
            dec.pull(source, out, eod)?;
        }
        Ok(())
    }

    pub fn close(&mut self) {
        self.closed = true;
    }

    pub fn is_open(&self) -> bool {
        !self.closed
    }

    /// `bytesavailable`: None means unknown (filters), per the PLRM's -1.
    pub fn bytes_available(&self) -> Option<usize> {
        if self.closed {
            return Some(0);
        }
        match &self.kind {
            Kind::Bytes { data, pos } => Some(data.len() - pos),
            Kind::Filter { .. } => None,
        }
    }

    pub fn pos(&self) -> usize {
        self.consumed
    }

    pub fn same_object(a: &FileHandle, b: &FileHandle) -> bool {
        Rc::ptr_eq(a, b)
    }
}

/// Streaming decoders. Each `pull` produces at least one output byte or
/// sets `eod` (end of data — the filter's own marker, or source
/// exhaustion, which the PLRM treats as premature-but-terminal).
pub enum Decoder {
    AsciiHex {
        hi: Option<u8>,
    },
    Ascii85 {
        group: [u8; 5],
        n: usize,
    },
    RunLength,
    /// The eexec / Type 1 charstring cipher. Sniffs hexadecimal vs
    /// binary form from the first four bytes, per the Type 1 spec, and
    /// discards `skip` leading plaintext bytes.
    Eexec {
        r: u16,
        skip: usize,
        mode: EexecMode,
    },
}

pub enum EexecMode {
    Unknown,
    Binary,
    /// Buffered pending high nibble of a hex pair.
    Hex(Option<u8>),
}

impl Decoder {
    pub fn ascii_hex() -> Self {
        Decoder::AsciiHex { hi: None }
    }

    pub fn ascii85() -> Self {
        Decoder::Ascii85 {
            group: [0; 5],
            n: 0,
        }
    }

    pub fn eexec() -> Self {
        Decoder::Eexec {
            r: 55665,
            skip: 4,
            mode: EexecMode::Unknown,
        }
    }

    fn pull(
        &mut self,
        source: &FileHandle,
        out: &mut VecDeque<u8>,
        eod: &mut bool,
    ) -> Result<(), PsError> {
        let next = || source.borrow_mut().read_byte();
        match self {
            Decoder::AsciiHex { hi } => loop {
                let Some(b) = next()? else {
                    if let Some(h) = hi.take() {
                        out.push_back(h << 4);
                    }
                    *eod = true;
                    return Ok(());
                };
                match b {
                    b'>' => {
                        // An odd final digit is padded with zero.
                        if let Some(h) = hi.take() {
                            out.push_back(h << 4);
                        }
                        *eod = true;
                        return Ok(());
                    }
                    _ if is_ps_whitespace(b) => {}
                    _ => match (b as char).to_digit(16) {
                        Some(v) => match hi.take() {
                            Some(h) => {
                                out.push_back((h << 4) | v as u8);
                                return Ok(());
                            }
                            None => *hi = Some(v as u8),
                        },
                        None => return Err(PsError::Io),
                    },
                }
            },
            Decoder::Ascii85 { group, n } => loop {
                let Some(b) = next()? else {
                    *eod = true;
                    return flush_a85(group, *n, out);
                };
                match b {
                    b'~' => {
                        // The '>' of the '~>' EOD; tolerate EOF after '~'.
                        let _ = next()?;
                        *eod = true;
                        return flush_a85(group, *n, out);
                    }
                    _ if is_ps_whitespace(b) => {}
                    b'z' if *n == 0 => {
                        out.extend([0, 0, 0, 0]);
                        return Ok(());
                    }
                    b'!'..=b'u' => {
                        group[*n] = b - b'!';
                        *n += 1;
                        if *n == 5 {
                            let v = group.iter().fold(0u32, |acc, d| {
                                acc.wrapping_mul(85).wrapping_add(u32::from(*d))
                            });
                            out.extend(v.to_be_bytes());
                            *n = 0;
                            return Ok(());
                        }
                    }
                    _ => return Err(PsError::Io),
                }
            },
            Decoder::RunLength => {
                let Some(len) = next()? else {
                    *eod = true;
                    return Ok(());
                };
                match len {
                    128 => *eod = true,
                    0..=127 => {
                        for _ in 0..=len {
                            let Some(b) = next()? else {
                                *eod = true;
                                return Ok(());
                            };
                            out.push_back(b);
                        }
                    }
                    _ => {
                        let Some(b) = next()? else {
                            *eod = true;
                            return Ok(());
                        };
                        for _ in 0..257 - usize::from(len) {
                            out.push_back(b);
                        }
                    }
                }
                Ok(())
            }
            Decoder::Eexec { r, skip, mode } => {
                if matches!(mode, EexecMode::Unknown) {
                    // Skip leading whitespace, then sniff the first four
                    // bytes: all-hex means the hexadecimal form.
                    let mut first4 = [0u8; 4];
                    let mut i = 0;
                    while i < 4 {
                        let Some(b) = next()? else {
                            *eod = true;
                            return Ok(());
                        };
                        if i == 0 && is_ps_whitespace(b) {
                            continue;
                        }
                        first4[i] = b;
                        i += 1;
                    }
                    let hex = first4.iter().all(u8::is_ascii_hexdigit);
                    *mode = if hex {
                        EexecMode::Hex(None)
                    } else {
                        EexecMode::Binary
                    };
                    for b in first4 {
                        eexec_byte(b, r, skip, mode, out)?;
                    }
                    return Ok(());
                }
                loop {
                    let Some(b) = next()? else {
                        *eod = true;
                        return Ok(());
                    };
                    let before = out.len();
                    eexec_byte(b, r, skip, mode, out)?;
                    if out.len() > before {
                        return Ok(());
                    }
                }
            }
        }
    }
}

/// One input byte of an eexec stream (already past mode sniffing):
/// assemble hex pairs if applicable, decrypt, and emit unless still in
/// the 4-byte plaintext lead-in.
fn eexec_byte(
    b: u8,
    r: &mut u16,
    skip: &mut usize,
    mode: &mut EexecMode,
    out: &mut VecDeque<u8>,
) -> Result<(), PsError> {
    let cipher = match mode {
        EexecMode::Binary => b,
        EexecMode::Hex(hi) => {
            if is_ps_whitespace(b) {
                return Ok(());
            }
            let v = (b as char).to_digit(16).ok_or(PsError::Io)? as u8;
            match hi.take() {
                Some(h) => (h << 4) | v,
                None => {
                    *hi = Some(v);
                    return Ok(());
                }
            }
        }
        EexecMode::Unknown => unreachable!("sniffed before decrypting"),
    };
    let plain = decrypt_byte(cipher, r);
    if *skip > 0 {
        *skip -= 1;
    } else {
        out.push_back(plain);
    }
    Ok(())
}

/// The shared eexec/charstring cipher step (Type 1 spec, C1/C2).
pub fn decrypt_byte(cipher: u8, r: &mut u16) -> u8 {
    let plain = cipher ^ (*r >> 8) as u8;
    *r = (u16::from(cipher).wrapping_add(*r))
        .wrapping_mul(52845)
        .wrapping_add(22719);
    plain
}

/// Charstring decryption: whole-buffer form, dropping `len_iv` leading
/// plaintext bytes (Private dict /lenIV, default 4).
pub fn decrypt_charstring(data: &[u8], len_iv: usize) -> Vec<u8> {
    let mut r: u16 = 4330;
    let mut out = Vec::with_capacity(data.len().saturating_sub(len_iv));
    for (i, &c) in data.iter().enumerate() {
        let p = decrypt_byte(c, &mut r);
        if i >= len_iv {
            out.push(p);
        }
    }
    out
}

fn flush_a85(group: &mut [u8; 5], n: usize, out: &mut VecDeque<u8>) -> Result<(), PsError> {
    match n {
        0 => Ok(()),
        1 => Err(PsError::Io),
        _ => {
            for slot in group.iter_mut().skip(n) {
                *slot = 84;
            }
            let v = group.iter().fold(0u32, |acc, d| {
                acc.wrapping_mul(85).wrapping_add(u32::from(*d))
            });
            out.extend(v.to_be_bytes().into_iter().take(n - 1));
            Ok(())
        }
    }
}

fn is_ps_whitespace(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n' | b'\x0c' | b'\0')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drain(f: &FileHandle) -> Vec<u8> {
        let mut out = Vec::new();
        while let Some(b) = f.borrow_mut().read_byte().expect("read") {
            out.push(b);
        }
        out
    }

    #[test]
    fn ascii_hex_decodes_and_stops_at_eod() {
        let src = PsFile::from_bytes(b"48 65 6c6C6f>tail".to_vec());
        let f = PsFile::filtered(src.clone(), Decoder::ascii_hex());
        assert_eq!(drain(&f), b"Hello");
        // The filter consumed exactly through '>' — 'tail' remains.
        assert_eq!(src.borrow_mut().read_byte().expect("read"), Some(b't'));
    }

    #[test]
    fn ascii85_matches_the_lexer() {
        let src = PsFile::from_bytes(b"BOu!r~>rest".to_vec());
        let f = PsFile::filtered(src.clone(), Decoder::ascii85());
        assert_eq!(drain(&f), b"hell");
        assert_eq!(src.borrow_mut().read_byte().expect("read"), Some(b'r'));
    }

    #[test]
    fn run_length_roundtrip() {
        // literal "ab", then 'c' x 4, then EOD.
        let src = PsFile::from_bytes(vec![1, b'a', b'b', 253, b'c', 128, b'X']);
        let f = PsFile::filtered(src.clone(), Decoder::RunLength);
        assert_eq!(drain(&f), b"abcccc");
        assert_eq!(src.borrow_mut().read_byte().expect("read"), Some(b'X'));
    }

    #[test]
    fn eexec_binary_roundtrip() {
        // Encrypt "42 =" with the eexec cipher (4-byte lead), decrypt back.
        let mut r: u16 = 55665;
        let mut enc = Vec::new();
        for &p in b"\0\0\0\0" {
            let c = p ^ (r >> 8) as u8;
            r = (u16::from(c).wrapping_add(r))
                .wrapping_mul(52845)
                .wrapping_add(22719);
            enc.push(c);
        }
        for &p in b"42 =" {
            let c = p ^ (r >> 8) as u8;
            r = (u16::from(c).wrapping_add(r))
                .wrapping_mul(52845)
                .wrapping_add(22719);
            enc.push(c);
        }
        let f = PsFile::filtered(PsFile::from_bytes(enc), Decoder::eexec());
        assert_eq!(drain(&f), b"42 =");
    }

    #[test]
    fn charstring_decrypt_roundtrip() {
        let mut r: u16 = 4330;
        let mut enc = Vec::new();
        for &p in &[0u8, 0, 0, 0, 13, 14] {
            let c = p ^ (r >> 8) as u8;
            r = (u16::from(c).wrapping_add(r))
                .wrapping_mul(52845)
                .wrapping_add(22719);
            enc.push(c);
        }
        assert_eq!(decrypt_charstring(&enc, 4), vec![13, 14]);
    }
}
