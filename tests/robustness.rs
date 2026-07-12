//! Adversarial-input tests: whatever we feed the interpreter, it must
//! return an error or a result — never panic, never hang (execution is
//! budgeted with step_n), never blow the Rust stack.

use pscat::Interp;

/// Deterministic xorshift so failures reproduce.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
}

/// Run bounded: a random program is quite capable of `{...} loop`.
fn run_bounded(source: &[u8]) {
    let mut it = Interp::with_page(64, 64).expect("page");
    it.begin_source(source);
    for _ in 0..50 {
        match it.step_n(10_000) {
            Ok(true) => {}
            Ok(false) | Err(_) => return,
        }
    }
}

#[test]
fn random_bytes_never_panic() {
    let mut rng = Rng(0x5eed);
    for _ in 0..300 {
        let len = (rng.next() % 200) as usize;
        let bytes: Vec<u8> = (0..len).map(|_| (rng.next() & 0xff) as u8).collect();
        run_bounded(&bytes);
    }
}

#[test]
fn random_token_soup_never_panics() {
    // Structured garbage reaches much deeper than byte noise: real
    // operators with wrong arities, types, and nesting.
    const POOL: &[&str] = &[
        "0",
        "1",
        "-1",
        "2.5",
        "1e300",
        "9223372036854775807",
        "(str)",
        "/n",
        "x",
        "add",
        "sub",
        "mul",
        "div",
        "idiv",
        "mod",
        "neg",
        "sqrt",
        "atan",
        "exp",
        "dup",
        "pop",
        "exch",
        "copy",
        "index",
        "roll",
        "clear",
        "count",
        "mark",
        "counttomark",
        "cleartomark",
        "[",
        "]",
        "{",
        "}",
        "if",
        "ifelse",
        "exec",
        "repeat",
        "loop",
        "for",
        "exit",
        "def",
        "dict",
        "begin",
        "end",
        "load",
        "bind",
        "eq",
        "ne",
        "lt",
        "gt",
        "and",
        "or",
        "not",
        "true",
        "false",
        "null",
        "moveto",
        "lineto",
        "curveto",
        "arc",
        "arcn",
        "closepath",
        "newpath",
        "fill",
        "stroke",
        "gsave",
        "grestore",
        "translate",
        "rotate",
        "scale",
        "setrgbcolor",
        "setgray",
        "setlinewidth",
        "currentpoint",
        "showpage",
    ];
    let mut rng = Rng(0xf00d);
    for _ in 0..300 {
        let n = (rng.next() % 40) as usize;
        let src: Vec<&str> = (0..n)
            .map(|_| POOL[(rng.next() as usize) % POOL.len()])
            .collect();
        run_bounded(src.join(" ").as_bytes());
    }
}

#[test]
fn pathological_nesting_is_bounded() {
    let mut it = Interp::new();
    // Deep braces: limitcheck, not a Rust stack overflow.
    assert!(it.run_str(&"{".repeat(100_000)).is_err());
    // Deep bracket nesting builds a deep array; printing it must not
    // recurse to death either (repr is depth-capped).
    let mut it = Interp::new();
    let n = 10_000;
    let src = format!("{}{}", "[".repeat(n), "]".repeat(n));
    it.run_str(&src).expect("nested arrays are legal");
    let repr = it.operand_stack()[0].repr();
    assert!(repr.contains("..."), "deep repr should elide");
}

#[test]
fn degenerate_graphics_are_errors_or_noops_never_panics() {
    for src in [
        "0 0 scale 10 10 moveto currentpoint", // singular CTM
        "1e300 1e300 moveto 1e300 neg 1e300 lineto stroke", // huge coords
        "newpath 0 0 moveto 0 0 lineto fill",  // empty-area fill
        "newpath closepath closepath fill",    // close with no subpath
        "0 setlinewidth newpath 1 1 moveto 60 60 lineto stroke", // hairline
        "-5 5 -100 0 360 arc fill",            // negative radius
        "50 50 5 90 90 arc stroke",            // zero sweep
        "grestore grestore grestore",          // pops past bottom
    ] {
        let mut it = Interp::with_page(64, 64).expect("page");
        let _ = it.run_str(src); // error or success both fine; panic is not
    }
}

#[test]
fn interpreter_survives_errors_and_keeps_working() {
    let mut it = Interp::new();
    for bad in ["pop", "1 0 div", "}", "frob", "(a", "end", "exit"] {
        let _ = it.run_str(bad);
    }
    it.run_str("2 3 add").expect("still functional");
    assert_eq!(
        it.operand_stack().last().map(|o| o.repr()).as_deref(),
        Some("5")
    );
}
