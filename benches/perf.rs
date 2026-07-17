//! Perf yardstick — Stage 8 task 3. Not statistical benchmarking: a
//! handful of representative workloads timed wall-clock (best of
//! three), so interpreter-speed work (name interning, and whatever
//! comes after) has before/after numbers and a regression tripwire.
//!
//! Run with `cargo bench` (compiles under the release-derived bench
//! profile; the numbers below are meaningless in debug builds).
//!
//! The workloads, chosen to stress different subsystems:
//! - fib 27 — procedure calls + dict lookups, zero graphics. The
//!   standing gs comparison (`gs -dNODISPLAY -q`, minus its ~20ms
//!   startup): ~30ms vs our 214ms at Stage 8 opening on an M-series.
//!   Name interning (task 2) brought it to ~137ms; the remaining gap
//!   is execution machinery (per-element RefCell borrow + Object
//!   clone in the frame loop), the next lever if it ever matters.
//! - defloop — 200k iterations of def/load on one name: the
//!   dict-write path.
//! - sierpinski — examples/sierpinski.ps: recursion + small fills.
//! - fern — gallery/fern.ps: 48k random affine hops, one tiny
//!   arc-fill each; rasterization-heavy.

use std::time::Instant;

use pscat::Interp;

const FIB: &[u8] = b"/fib { dup 2 lt { pop 1 } { dup 1 sub fib exch 2 sub fib add } ifelse } def
     27 fib pop";

const DEFLOOP: &[u8] =
    b"/x 0 def 200000 { /x x 1 add def } repeat x 200000 eq not { limitcheck } if";

fn run(source: &[u8], size: u32) -> f64 {
    let mut best = f64::INFINITY;
    for _ in 0..3 {
        let mut it = Interp::with_page(size, size).expect("page");
        let t = Instant::now();
        it.run_source(source)
            .unwrap_or_else(|e| panic!("workload failed: {}", it.error_report(&e)));
        best = best.min(t.elapsed().as_secs_f64());
    }
    best
}

fn main() {
    let file = |path: &str| std::fs::read(path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    let workloads: [(&str, Vec<u8>, u32); 4] = [
        ("fib 27", FIB.to_vec(), 8),
        ("defloop 200k", DEFLOOP.to_vec(), 8),
        ("sierpinski", file("examples/sierpinski.ps"), 500),
        ("fern", file("gallery/fern.ps"), 500),
    ];
    println!("{:<14} {:>9}", "workload", "best-of-3");
    for (name, src, size) in workloads {
        let secs = run(&src, size);
        println!("{name:<14} {:>8.0}ms", secs * 1000.0);
    }
}
