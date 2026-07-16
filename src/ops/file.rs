//! File and filter operators. Files share their read position with the
//! scanner (see `src/file.rs`) — that's what `currentfile` is for.

use crate::error::PsError;
use crate::file::{Decoder, FileHandle, PsFile};
use crate::interp::Interp;
use crate::object::{Dict, Object, PsString, Value};

pub fn install(dict: &mut Dict) {
    use super::op;
    op(dict, "file", file);
    op(dict, "currentfile", currentfile);
    op(dict, "read", read);
    op(dict, "readstring", readstring);
    op(dict, "readhexstring", readhexstring);
    op(dict, "readline", readline);
    op(dict, "closefile", closefile);
    op(dict, "bytesavailable", bytesavailable);
    op(dict, "flushfile", flushfile);
    op(dict, "status", status);
    op(dict, "filter", filter);
    op(dict, "eexec", eexec);
    op(dict, "run", run);
}

fn pop_file(it: &mut Interp) -> Result<FileHandle, PsError> {
    match &it.pop()?.value {
        Value::File(f) => Ok(f.clone()),
        _ => Err(PsError::Typecheck),
    }
}

fn pop_ps_string(it: &mut Interp) -> Result<PsString, PsError> {
    match &it.pop()?.value {
        Value::String(s) => Ok(s.clone()),
        _ => Err(PsError::Typecheck),
    }
}

/// filename access file → file. Read-only access to real files (how a
/// program pulls in data or another program); write modes are refused.
fn file(it: &mut Interp) -> Result<(), PsError> {
    let access = pop_ps_string(it)?.to_vec();
    let name = pop_ps_string(it)?.text();
    if access != b"r" {
        return Err(PsError::InvalidFileAccess);
    }
    let data = std::fs::read(&name).map_err(|_| PsError::UndefinedFilename)?;
    it.push(Object::lit(Value::File(PsFile::from_bytes(data))));
    Ok(())
}

/// The file the interpreter is currently executing — or the PLRM's
/// "invalid file object" (a closed file) when there isn't one.
fn currentfile(it: &mut Interp) -> Result<(), PsError> {
    let f = it.current_file().unwrap_or_else(PsFile::closed_file);
    it.push(Object::lit(Value::File(f)));
    Ok(())
}

/// file read → byte true | false
fn read(it: &mut Interp) -> Result<(), PsError> {
    let f = pop_file(it)?;
    match f.borrow_mut().read_byte()? {
        Some(b) => {
            it.push(Object::int(i64::from(b)));
            it.push(Object::bool(true));
        }
        None => it.push(Object::bool(false)),
    }
    Ok(())
}

/// file string readstring → substring filled — the workhorse of image
/// data loops. The bool reports whether the string filled completely.
fn readstring(it: &mut Interp) -> Result<(), PsError> {
    let s = pop_ps_string(it)?;
    let f = pop_file(it)?;
    let mut n = 0;
    while n < s.len() {
        match f.borrow_mut().read_byte()? {
            Some(b) => {
                s.put_byte(n, b)?;
                n += 1;
            }
            None => break,
        }
    }
    it.push(Object::lit(Value::String(s.interval(0, n)?)));
    it.push(Object::bool(n > 0 && n == s.len()));
    Ok(())
}

/// file string readhexstring → substring filled. Non-hex bytes are
/// ignored, per the PLRM.
fn readhexstring(it: &mut Interp) -> Result<(), PsError> {
    let s = pop_ps_string(it)?;
    let f = pop_file(it)?;
    let mut n = 0;
    let mut hi: Option<u8> = None;
    loop {
        if n >= s.len() {
            break;
        }
        let Some(b) = f.borrow_mut().read_byte()? else {
            break;
        };
        if let Some(v) = (b as char).to_digit(16) {
            match hi.take() {
                Some(h) => {
                    s.put_byte(n, (h << 4) | v as u8)?;
                    n += 1;
                }
                None => hi = Some(v as u8),
            }
        }
    }
    it.push(Object::lit(Value::String(s.interval(0, n)?)));
    it.push(Object::bool(n > 0 && n == s.len()));
    Ok(())
}

/// file string readline → substring found — up to (not including) the
/// newline; rangecheck if the line doesn't fit.
fn readline(it: &mut Interp) -> Result<(), PsError> {
    let s = pop_ps_string(it)?;
    let f = pop_file(it)?;
    let mut n = 0;
    let found = loop {
        let Some(b) = f.borrow_mut().read_byte()? else {
            break false;
        };
        match b {
            b'\n' => break true,
            b'\r' => {
                if f.borrow_mut().peek_byte()? == Some(b'\n') {
                    f.borrow_mut().read_byte()?;
                }
                break true;
            }
            _ => {
                if n >= s.len() {
                    return Err(PsError::Rangecheck);
                }
                s.put_byte(n, b)?;
                n += 1;
            }
        }
    };
    it.push(Object::lit(Value::String(s.interval(0, n)?)));
    it.push(Object::bool(found));
    Ok(())
}

fn closefile(it: &mut Interp) -> Result<(), PsError> {
    let f = pop_file(it)?;
    f.borrow_mut().close();
    Ok(())
}

fn bytesavailable(it: &mut Interp) -> Result<(), PsError> {
    let f = pop_file(it)?;
    let n = f.borrow().bytes_available();
    it.push(Object::int(n.map_or(-1, |n| n as i64)));
    Ok(())
}

/// For an input file, flushfile reads and discards to EOF, per the PLRM.
fn flushfile(it: &mut Interp) -> Result<(), PsError> {
    let f = pop_file(it)?;
    while f.borrow_mut().read_byte()?.is_some() {}
    Ok(())
}

/// file status → bool | filename status → false (no filesystem stats).
fn status(it: &mut Interp) -> Result<(), PsError> {
    let top = it.pop()?;
    match &top.value {
        Value::File(f) => it.push(Object::bool(f.borrow().is_open())),
        Value::String(_) => it.push(Object::bool(false)),
        _ => return Err(PsError::Typecheck),
    }
    Ok(())
}

/// source [dict] name filter → file. Decode filters only; a parameter
/// dict operand is accepted and ignored (none of the supported filters
/// take parameters we honor yet). Sources: file or string.
fn filter(it: &mut Interp) -> Result<(), PsError> {
    let name = match &it.pop()?.value {
        Value::Name(n) => n.clone(),
        _ => return Err(PsError::Typecheck),
    };
    let dec = match &*name {
        "ASCIIHexDecode" => Decoder::ascii_hex(),
        "ASCII85Decode" => Decoder::ascii85(),
        "RunLengthDecode" => Decoder::RunLength,
        "FlateDecode" => Decoder::flate(),
        "LZWDecode" => Decoder::lzw(),
        "DCTDecode" => Decoder::dct(),
        // Encode filters (and the one decoder we don't have,
        // CCITTFaxDecode) are undefined.
        _ => return Err(PsError::Undefined(name.to_string())),
    };
    let mut src = it.pop()?;
    if matches!(src.value, Value::Dict(_)) {
        // A parameter dict between source and name: accept, ignore.
        src = it.pop()?;
    }
    let source = match &src.value {
        Value::File(f) => f.clone(),
        // A string source is a snapshot of its current contents.
        Value::String(s) => PsFile::from_bytes(s.to_vec()),
        _ => return Err(PsError::Typecheck),
    };
    it.push(Object::lit(Value::File(PsFile::filtered(source, dec))));
    Ok(())
}

/// file eexec — execute the encrypted region as program source. The
/// canonical caller is an embedded Type 1 font: `currentfile eexec`.
fn eexec(it: &mut Interp) -> Result<(), PsError> {
    let src = it.pop()?;
    let file = match &src.value {
        Value::File(f) => f.clone(),
        // eexec on a string is legal (fonts are sometimes wrapped).
        Value::String(s) => PsFile::from_bytes(s.to_vec()),
        _ => return Err(PsError::Typecheck),
    };
    it.begin_eexec(file)
}

/// filename run — execute a program file from disk.
fn run(it: &mut Interp) -> Result<(), PsError> {
    let name = pop_ps_string(it)?.text();
    let data = std::fs::read(&name).map_err(|_| PsError::UndefinedFilename)?;
    let f = PsFile::from_bytes(data);
    it.exec_object(Object::exec(Value::File(f)))
}
