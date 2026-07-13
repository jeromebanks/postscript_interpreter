//! Type 1 fonts, end to end: the test builds a complete PFA font
//! program in memory — eexec-encrypted region, charstring-encrypted
//! glyphs delivered through the `N RD <binary>` idiom, the canonical
//! Private/CharStrings/definefont sequence, zeros, cleartomark — and
//! runs it through the interpreter like any found file. The same bytes
//! are also handed to Ghostscript when available, as proof the font is
//! genuine Type 1 and the renders agree.

use std::process::Command;

use pscat::{Interp, Value};

// --- Type 1 encryption (the inverse of src/file.rs's decoders) ---------

fn crypt_step(p: u8, r: &mut u16) -> u8 {
    let c = p ^ (*r >> 8) as u8;
    *r = (u16::from(c).wrapping_add(*r))
        .wrapping_mul(52845)
        .wrapping_add(22719);
    c
}

fn eexec_encrypt(plain: &[u8]) -> Vec<u8> {
    let mut r: u16 = 55665;
    b"XXXX"
        .iter()
        .chain(plain)
        .map(|&p| crypt_step(p, &mut r))
        .collect()
}

fn charstring_encrypt(plain: &[u8]) -> Vec<u8> {
    let mut r: u16 = 4330;
    b"XXXX"
        .iter()
        .chain(plain)
        .map(|&p| crypt_step(p, &mut r))
        .collect()
}

// --- charstring assembly ------------------------------------------------

fn cs_num(out: &mut Vec<u8>, n: i32) {
    match n {
        -107..=107 => out.push((n + 139) as u8),
        108..=1131 => {
            let v = n - 108;
            out.push((v / 256 + 247) as u8);
            out.push((v % 256) as u8);
        }
        -1131..=-108 => {
            let v = -n - 108;
            out.push((v / 256 + 251) as u8);
            out.push((v % 256) as u8);
        }
        _ => {
            out.push(255);
            out.extend(n.to_be_bytes());
        }
    }
}

fn charstring(items: &[CsItem]) -> Vec<u8> {
    let mut out = Vec::new();
    for item in items {
        match item {
            CsItem::N(n) => cs_num(&mut out, *n),
            CsItem::Op(op) => out.push(*op),
            CsItem::Esc(op) => out.extend([12, *op]),
        }
    }
    charstring_encrypt(&out)
}

enum CsItem {
    N(i32),
    Op(u8),
    Esc(u8),
}
use CsItem::{Esc, N, Op};

const HSBW: u8 = 13;
const RMOVETO: u8 = 21;
const RLINETO: u8 = 5;
const HLINETO: u8 = 6;
const VLINETO: u8 = 7;
const CLOSEPATH: u8 = 9;
const CALLSUBR: u8 = 10;
const RETURN: u8 = 11;
const ENDCHAR: u8 = 14;
const DIV: u8 = 12; // escape 12

// --- the font ------------------------------------------------------------

/// A complete PFA: /square (a 400x700 box, width 600), /slant (uses a
/// Subr and div), /.notdef (empty, width 600).
fn mini_font() -> Vec<u8> {
    let square = charstring(&[
        N(50),
        N(600),
        Op(HSBW),
        N(50),
        N(0),
        Op(RMOVETO),
        N(400),
        Op(HLINETO),
        N(700),
        Op(VLINETO),
        N(-400),
        Op(HLINETO),
        Op(CLOSEPATH),
        Op(ENDCHAR),
    ]);
    // Subr 0 draws the closing edge; div exercises the escape path:
    // 800 2 div = 400 wide base.
    let slant = charstring(&[
        N(50),
        N(600),
        Op(HSBW),
        N(50),
        N(0),
        Op(RMOVETO),
        N(800),
        N(2),
        Esc(DIV),
        Op(HLINETO),
        N(100),
        N(600),
        Op(RLINETO),
        N(0),
        Op(CALLSUBR),
        Op(CLOSEPATH),
        Op(ENDCHAR),
    ]);
    let subr0 = charstring(&[N(-300), N(50), Op(RLINETO), Op(RETURN)]);
    let notdef = charstring(&[N(0), N(600), Op(HSBW), Op(ENDCHAR)]);

    let mut encrypted = Vec::new();
    encrypted.extend(b"dup /Private 8 dict dup begin\n".as_slice());
    encrypted.extend(b"/RD {string currentfile exch readstring pop} executeonly def\n".as_slice());
    encrypted.extend(b"/ND {noaccess def} executeonly def\n".as_slice());
    encrypted.extend(b"/NP {noaccess put} executeonly def\n".as_slice());
    encrypted.extend(b"/lenIV 4 def\n".as_slice());
    encrypted.extend(b"/password 5839 def\n".as_slice());
    encrypted.extend(b"/BlueValues [] def\n".as_slice());
    encrypted.extend(b"/Subrs 1 array\n".as_slice());
    encrypted.extend(format!("dup 0 {} RD ", subr0.len()).into_bytes());
    encrypted.extend(&subr0);
    encrypted.extend(b" NP\nND\n".as_slice());
    encrypted.extend(b"2 index /CharStrings 4 dict dup begin\n".as_slice());
    for (name, cs) in [(".notdef", &notdef), ("square", &square), ("slant", &slant)] {
        encrypted.extend(format!("/{name} {} RD ", cs.len()).into_bytes());
        encrypted.extend(cs);
        encrypted.extend(b" ND\n".as_slice());
    }
    encrypted.extend(b"end\nend\nreadonly put\nnoaccess put\n".as_slice());
    encrypted.extend(b"dup /FontName get exch definefont pop\n".as_slice());
    encrypted.extend(b"mark currentfile closefile\n".as_slice());

    let mut pfa = Vec::new();
    pfa.extend(b"%!PS-AdobeFont-1.0: MiniSquare 001.000\n".as_slice());
    pfa.extend(b"11 dict begin\n".as_slice());
    pfa.extend(b"/FontName /MiniSquare def\n".as_slice());
    pfa.extend(b"/FontType 1 def\n".as_slice());
    pfa.extend(b"/FontMatrix [0.001 0 0 0.001 0 0] readonly def\n".as_slice());
    pfa.extend(b"/FontBBox {0 0 600 700} readonly def\n".as_slice());
    pfa.extend(b"/PaintType 0 def\n".as_slice());
    pfa.extend(b"/Encoding 256 array\n0 1 255 {1 index exch /.notdef put} for\n".as_slice());
    pfa.extend(b"dup 97 /square put\ndup 98 /slant put\nreadonly def\n".as_slice());
    pfa.extend(b"currentdict end\ncurrentfile eexec\n".as_slice());
    pfa.extend(eexec_encrypt(&encrypted));
    pfa.extend(b"\n".as_slice());
    for _ in 0..8 {
        pfa.extend(
            b"0000000000000000000000000000000000000000000000000000000000000000\n".as_slice(),
        );
    }
    pfa.extend(b"cleartomark\n".as_slice());
    pfa
}

fn run_with_font(extra: &str) -> Interp {
    let mut src = mini_font();
    src.extend(extra.as_bytes());
    let mut it = Interp::with_page(100, 100).expect("test page");
    it.run_source(&src)
        .unwrap_or_else(|e| panic!("run failed: {}", it.error_report(&e)));
    it
}

fn pop_f64(it: &mut Interp) -> f64 {
    match it.pop().expect("operand").value {
        Value::Integer(i) => i as f64,
        Value::Real(r) => r,
        _ => panic!("expected number"),
    }
}

fn ink_count(it: &Interp) -> usize {
    it.gfx()
        .pixmap
        .pixels()
        .iter()
        .filter(|p| p.red() < 128)
        .count()
}

// --- tests ----------------------------------------------------------------

#[test]
fn font_program_parses_and_registers() {
    let mut it = run_with_font(
        "count FontDirectory /MiniSquare known
         /MiniSquare findfont /CharStrings get length",
    );
    assert_eq!(pop_f64(&mut it), 3.0, "three charstrings");
    assert_eq!(it.pop().expect("known").repr(), "true");
    assert_eq!(pop_f64(&mut it), 0.0, "operand stack clean after font");
}

#[test]
fn charstrings_paint_and_advance() {
    let mut it = run_with_font(
        "/MiniSquare findfont 20 scalefont setfont
         10 10 moveto (aa) show currentpoint pop",
    );
    let x = pop_f64(&mut it);
    // Two glyphs at width 600/1000 em, 20pt: 2 x 12.
    assert!((x - 34.0).abs() < 1e-3, "advance {x}");
    assert!(ink_count(&it) > 100, "square glyphs painted");
}

#[test]
fn stringwidth_measures_without_ink() {
    let mut it = run_with_font("/MiniSquare findfont 10 scalefont setfont (aab) stringwidth");
    let wy = pop_f64(&mut it);
    let wx = pop_f64(&mut it);
    assert_eq!(wy, 0.0);
    assert!((wx - 18.0).abs() < 1e-9, "3 x 6pt: {wx}");
    assert_eq!(ink_count(&it), 0);
}

#[test]
fn charpath_yields_the_outline_geometry() {
    let mut it = run_with_font(
        "/MiniSquare findfont 20 scalefont setfont
         newpath 10 10 moveto (a) false charpath pathbbox",
    );
    let uy = pop_f64(&mut it);
    let ux = pop_f64(&mut it);
    let ly = pop_f64(&mut it);
    let lx = pop_f64(&mut it);
    // Square spans x 100..500, y 0..700 in glyph space, x0.02 + (10,10).
    // The initial moveto (10,10) and the advanced current point
    // (10 + 600x0.02 = 22, 10) are part of the path as well.
    assert!((lx - 10.0).abs() < 0.1, "lx {lx}");
    assert!((ly - 10.0).abs() < 0.1, "ly {ly}");
    assert!((ux - 22.0).abs() < 0.1, "ux {ux}");
    assert!((uy - 24.0).abs() < 0.1, "uy {uy}");
}

#[test]
fn subrs_and_div_execute() {
    let mut it = run_with_font(
        "/MiniSquare findfont 30 scalefont setfont
         20 20 moveto (b) show currentpoint pop",
    );
    let x = pop_f64(&mut it);
    assert!((x - 38.0).abs() < 1e-3, "advance {x}");
    assert!(ink_count(&it) > 50, "slant glyph painted via subr");
}

#[test]
fn unencoded_bytes_fall_back_to_notdef_width() {
    let mut it = run_with_font(
        "/MiniSquare findfont 10 scalefont setfont
         5 5 moveto (z) show currentpoint pop",
    );
    let x = pop_f64(&mut it);
    assert!((x - 11.0).abs() < 1e-3, ".notdef advances 600: {x}");
    assert_eq!(ink_count(&it), 0, ".notdef paints nothing");
}

#[test]
fn ghostscript_accepts_the_same_bytes() {
    // The strongest possible check on the generator and on our
    // semantics at once: gs must parse the identical font program and
    // report the same stringwidth.
    let gs_ok = Command::new("gs")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success());
    if !gs_ok {
        eprintln!("skipping gs cross-check: ghostscript not installed");
        return;
    }
    let dir = std::env::temp_dir();
    let pfa_path = dir.join("pscat_minisquare.pfa");
    std::fs::write(&pfa_path, mini_font()).expect("write pfa");
    let script = format!(
        "({}) run /MiniSquare findfont 20 scalefont setfont (ab) stringwidth pop == quit",
        pfa_path.display()
    );
    let out = Command::new("gs")
        .args(["-q", "-dNODISPLAY", "-dNOSAFER", "-c", &script])
        .output()
        .expect("run gs");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let gs_width: f64 = stdout.trim().parse().unwrap_or_else(|_| {
        panic!(
            "gs did not print a width: stdout={stdout:?} stderr={:?}",
            String::from_utf8_lossy(&out.stderr)
        )
    });
    let _ = std::fs::remove_file(&pfa_path);
    assert!(
        (gs_width - 24.0).abs() < 0.01,
        "gs agrees on 2 x 600/1000 x 20pt: {gs_width}"
    );
}
