//! File objects, filters, and eexec — the Stage 7 task 1/2 machinery.
//! The load-bearing property throughout: the scanner and data reads
//! share one file position.

use pscat::{Interp, PsError};

fn run(src: &str) -> Interp {
    let mut it = Interp::with_page(100, 100).expect("test page");
    it.run_str(src)
        .unwrap_or_else(|e| panic!("run failed: {e}"));
    it
}

fn run_bytes(src: &[u8]) -> Interp {
    let mut it = Interp::with_page(100, 100).expect("test page");
    it.run_source(src)
        .unwrap_or_else(|e| panic!("run failed: {e}"));
    it
}

fn top_repr(it: &mut Interp) -> String {
    it.pop().expect("operand").repr()
}

#[test]
fn currentfile_reads_data_from_the_program() {
    // The single space after `readstring` is the token's delimiter and
    // is consumed with it (PLRM; pinned against gs) — data starts at
    // 'A'. The scanner then resumes with " pop" as program text.
    let mut it = run("currentfile 5 string readstring ABCDE pop");
    assert_eq!(top_repr(&mut it), "(ABCDE)");
}

#[test]
fn currentfile_readhexstring_ignores_delimiters() {
    let mut it = run("currentfile 3 string readhexstring 41 42 43 pop");
    assert_eq!(top_repr(&mut it), "(ABC)");
}

#[test]
fn read_single_bytes() {
    // The delimiter space went with the token; read takes 'X'.
    let mut it = run("currentfile read X");
    assert_eq!(top_repr(&mut it), "true");
    assert_eq!(top_repr(&mut it), "88");
}

#[test]
fn readline_splits_on_newlines() {
    let mut it = run_bytes(b"currentfile 20 string readline data here\npop");
    assert_eq!(top_repr(&mut it), "(data here)");
}

#[test]
fn exec_and_token_on_files() {
    // A filtered file is a program source like any other:
    // "3420332061646420" decodes to "4 3 add ".
    let mut it = run("(3420332061646420) /ASCIIHexDecode filter cvx exec");
    assert_eq!(top_repr(&mut it), "7");

    // token on a file advances the shared position past each token.
    let mut it = run("(3420332061646420) /ASCIIHexDecode filter dup token pop exch token pop");
    assert_eq!(top_repr(&mut it), "3");
    assert_eq!(top_repr(&mut it), "4");
}

#[test]
fn filter_chain_stacks() {
    // RunLength inside ASCIIHex: 03 61626364 = literal "abcd", 80 = EOD.
    let mut it = run(
        "(0361626364 80 3e) /ASCIIHexDecode filter /RunLengthDecode filter
         5 string readstring",
    );
    assert_eq!(top_repr(&mut it), "false");
    assert_eq!(top_repr(&mut it), "(abcd)");
}

#[test]
fn bytesavailable_status_and_close() {
    let mut it = run("(deadbeef) /ASCIIHexDecode filter bytesavailable
         currentfile bytesavailable");
    let program = top_repr(&mut it).parse::<i64>().expect("int");
    let filtered = top_repr(&mut it);
    assert_eq!(filtered, "-1", "filters don't know their length");
    assert!(program >= 0, "byte-backed files know theirs");

    // Closing a *filter* file: status flips, reads hit EOF (no error).
    let mut it = run("(4142) /ASCIIHexDecode filter
         dup status exch dup closefile dup status exch read");
    assert_eq!(top_repr(&mut it), "false", "read at EOF");
    assert_eq!(top_repr(&mut it), "false", "closed status");
    assert_eq!(top_repr(&mut it), "true", "open status");
}

#[test]
fn closing_the_file_being_executed_stops_it() {
    // The Type 1 convention in miniature: after closefile, nothing
    // further from that file runs — undefined-name junk included.
    let mut it = run("1 2 currentfile closefile total garbage +++");
    assert_eq!(top_repr(&mut it), "2");
    assert_eq!(top_repr(&mut it), "1");
}

/// eexec encryption (the inverse of what src/file.rs decodes).
fn eexec_encrypt(plain: &[u8]) -> Vec<u8> {
    let mut r: u16 = 55665;
    let mut out = Vec::new();
    for &p in b"XXXX".iter().chain(plain) {
        let c = p ^ (r >> 8) as u8;
        r = (u16::from(c).wrapping_add(r))
            .wrapping_mul(52845)
            .wrapping_add(22719);
        out.push(c);
    }
    out
}

#[test]
fn eexec_executes_and_hands_back_cleanly() {
    // The full Type 1 embedding shape: an encrypted region that ends
    // with `mark currentfile closefile`, then plaintext zeros, then
    // cleartomark. closefile must stop the eexec scanner at exactly the
    // right byte so the outer scanner sees the zeros as its own tokens,
    // and cleartomark cleans them off with the mark.
    let mut src = b"currentfile eexec".to_vec();
    src.push(b' ');
    src.extend(eexec_encrypt(
        b"userdict /answer 42 put mark currentfile closefile ",
    ));
    src.extend(b"\n0000000000000000000000000000000000000000000000000000000000000000\n");
    src.extend(b"cleartomark answer count");
    let mut it = run_bytes(&src);
    assert_eq!(top_repr(&mut it), "1", "stack clean apart from answer");
    assert_eq!(top_repr(&mut it), "42");
}

#[test]
fn eexec_hex_form_works_too() {
    let enc = eexec_encrypt(b"userdict /hexed true put currentfile closefile ");
    let hex: String = enc.iter().map(|b| format!("{b:02x}")).collect();
    let src = format!("currentfile eexec {hex}\nhexed");
    let mut it = run(&src);
    assert_eq!(top_repr(&mut it), "true");
}

#[test]
fn eexec_pushes_systemdict_for_its_duration() {
    let mut src = b"countdictstack currentfile eexec".to_vec();
    src.push(b' ');
    src.extend(eexec_encrypt(
        b"userdict /inside countdictstack put mark currentfile closefile ",
    ));
    src.extend(b"\n00000000\ncleartomark inside countdictstack");
    let mut it = run_bytes(&src);
    let after = top_repr(&mut it).parse::<i64>().expect("int");
    let inside = top_repr(&mut it).parse::<i64>().expect("int");
    let before = top_repr(&mut it).parse::<i64>().expect("int");
    assert_eq!(inside, before + 1, "systemdict pushed inside eexec");
    assert_eq!(after, before, "and popped when it ended");
}

#[test]
fn flate_decode_roundtrip() {
    use flate2::{Compression, write::ZlibEncoder};
    use std::io::Write;
    let mut enc = ZlibEncoder::new(Vec::new(), Compression::default());
    enc.write_all(b"compressed postscript payload")
        .expect("compress");
    let z = enc.finish().expect("finish");
    let hex: String = z.iter().map(|b| format!("{b:02x}")).collect();
    let mut it = run(&format!(
        "({hex}) /ASCIIHexDecode filter /FlateDecode filter
         64 string readstring"
    ));
    assert_eq!(top_repr(&mut it), "false");
    assert_eq!(top_repr(&mut it), "(compressed postscript payload)");
}

#[test]
fn lzw_decode_known_vector() {
    // Hand-packed 9-bit codes for "aaaa":
    // Clear(256) 97 258 97 EOD(257) -> 80 18 60 46 18 08.
    let mut it = run("(801860461808) /ASCIIHexDecode filter /LZWDecode filter
         8 string readstring");
    assert_eq!(top_repr(&mut it), "false");
    assert_eq!(top_repr(&mut it), "(aaaa)");
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

// Fixtures: flat-color JPEGs produced by gs (jpeggray / jpeg devices),
// APP/COM segments stripped. Flat color survives JPEG nearly exactly
// (DC coefficient only), so tight tolerances hold.

#[test]
fn dct_decode_gray_jpeg() {
    // 8x8 grayscale, 0.5 setgray: 64 one-byte samples near 128.
    let jpeg = hex(include_bytes!("data/gray8.jpg"));
    let mut it = run(&format!(
        "({jpeg}) /ASCIIHexDecode filter /DCTDecode filter
         100 string readstring pop dup length exch 0 get"
    ));
    let first: i64 = top_repr(&mut it).parse().expect("sample");
    let len: i64 = top_repr(&mut it).parse().expect("length");
    assert_eq!(len, 64, "one byte per grayscale sample");
    assert!((first - 128).abs() <= 8, "mid-gray sample, got {first}");
}

#[test]
fn dct_decode_color_jpeg_yields_rgb() {
    // 4x4 solid red: 48 bytes of interleaved RGB.
    let jpeg = hex(include_bytes!("data/red4.jpg"));
    let mut it = run(&format!(
        "({jpeg}) /ASCIIHexDecode filter /DCTDecode filter
         100 string readstring pop
         dup length exch dup 0 get exch dup 1 get exch 2 get"
    ));
    let b: i64 = top_repr(&mut it).parse().expect("b");
    let g: i64 = top_repr(&mut it).parse().expect("g");
    let r: i64 = top_repr(&mut it).parse().expect("r");
    let len: i64 = top_repr(&mut it).parse().expect("length");
    assert_eq!(len, 48, "three bytes per pixel");
    assert!(r >= 240 && g <= 15 && b <= 15, "red pixel, got {r} {g} {b}");
}

#[test]
fn dct_consumes_exactly_through_eoi() {
    // Bytes after the JPEG's EOI marker stay unread in the source —
    // the shared-cursor contract every decoder keeps.
    let jpeg = hex(include_bytes!("data/gray8.jpg"));
    let mut it = run(&format!(
        "({jpeg}414243>) /ASCIIHexDecode filter dup /DCTDecode filter
         100 string readstring pop pop
         3 string readstring"
    ));
    assert_eq!(top_repr(&mut it), "true");
    assert_eq!(top_repr(&mut it), "(ABC)");
}

#[test]
fn file_errors() {
    let mut it = Interp::with_page(100, 100).expect("page");
    assert_eq!(
        it.run_str("(/nonexistent-path-xyz) (r) file"),
        Err(PsError::UndefinedFilename)
    );
    assert_eq!(it.run_str("(x) (w) file"), Err(PsError::InvalidFileAccess));
    assert!(matches!(
        it.run_str("(x) /NoSuchDecode filter"),
        Err(PsError::Undefined(_))
    ));
}

/// A relative-path fixture under `target/`, cleaned up on drop even
/// if the test panics — `target/` is already gitignored, and the
/// crate root is `cargo test`'s CWD, so a relative path to it is the
/// realistic case `run`/`file` resolve for programs that `(lib/foo)
/// run` un-anchored from that CWD (see `src/ops/file.rs::resolve`
/// and `src/paths.rs`). Named with the process id *and* a per-process
/// counter — the pid alone isn't enough, since every test in this
/// binary shares one process and `cargo test` runs them concurrently
/// by default (an earlier version of this fixture collided exactly
/// that way: two tests raced on one filename and each other's
/// content leaked across the read).
struct RelFixture {
    rel_path: String,
}

impl RelFixture {
    fn new(contents: &str) -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let rel_path = format!("target/pscat-test-fixture-{}-{n}.ps", std::process::id());
        std::fs::write(&rel_path, contents).expect("write fixture");
        Self { rel_path }
    }
}

impl Drop for RelFixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.rel_path);
    }
}

#[test]
fn run_resolves_a_relative_path_against_cwd() {
    // The common case, unchanged by adding fallback resolution: a
    // relative `run` that already resolves against CWD must keep
    // working exactly as it did before `src/paths.rs` existed. This
    // is the PS-level `run` operator specifically — the gap flagged
    // during review, since the existing artkit/styles/fontlib tests
    // all load `lib/*.ps` at the Rust level instead.
    let fixture = RelFixture::new("41 42 add");
    let mut it = Interp::with_page(100, 100).expect("page");
    it.run_str(&format!("({}) run", fixture.rel_path))
        .unwrap_or_else(|e| panic!("run failed: {e}"));
    assert_eq!(top_repr(&mut it), "83");
}

#[test]
fn file_resolves_a_relative_path_against_cwd() {
    let fixture = RelFixture::new("hello");
    let mut it = Interp::with_page(100, 100).expect("page");
    it.run_str(&format!(
        "({}) (r) file 5 string readstring",
        fixture.rel_path
    ))
    .unwrap_or_else(|e| panic!("run failed: {e}"));
    assert_eq!(top_repr(&mut it), "true");
    assert_eq!(top_repr(&mut it), "(hello)");
}

#[test]
fn currentfile_skips_executable_strings() {
    // Inside an executable string, currentfile falls through to the
    // main program file (strings aren't files, per the PLRM).
    let mut it = run("(currentfile status) cvx exec");
    assert_eq!(top_repr(&mut it), "true");
}

#[test]
fn value_file_is_a_real_object() {
    let mut it = run("currentfile dup type exch dup xcheck exch currentfile eq");
    assert_eq!(top_repr(&mut it), "true", "currentfile twice is eq");
    assert_eq!(top_repr(&mut it), "false", "literal attribute");
    // `type` returns an executable name (matches gs), printed bare.
    assert_eq!(top_repr(&mut it), "filetype");
}
