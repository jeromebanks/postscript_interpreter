//! pscat vs Ghostscript — Stage 11 task 1. Runs identical workloads
//! through both *binaries* (process launch included, so startup cost
//! is visible rather than hidden), measuring wall time (best of
//! three) and peak RSS via `/usr/bin/time -l` (macOS; on other
//! platforms RSS prints as `-`). An empty-program baseline row shows
//! each interpreter's fixed startup cost so the per-workload numbers
//! can be read net of it.
//!
//! Run with `cargo bench --bench vs_gs`. Skips gs columns when gs
//! isn't installed. Findings and the optimizations they justified are
//! recorded in NOTES.md (Stage 11).

use std::process::Command;
use std::time::Instant;

struct Workload {
    name: &'static str,
    /// Inline snippet (both sides) or a .ps file path.
    source: Source,
    /// Page size for render workloads (points, 72dpi).
    page: Option<(u32, u32)>,
}

enum Source {
    Snippet(&'static str),
    File(&'static str),
}

const FIB: &str =
    "/fib { dup 2 lt { pop 1 } { dup 1 sub fib exch 2 sub fib add } ifelse } def 27 fib pop";
const DEFLOOP: &str = "/x 0 def 200000 { /x x 1 add def } repeat";
const SAVELOOP: &str = "/a [1 2 3] def 2000 { save a 0 9 put a 1 8 put restore } repeat";

fn workloads() -> Vec<Workload> {
    vec![
        Workload {
            name: "startup",
            source: Source::Snippet(""),
            page: None,
        },
        Workload {
            name: "fib 27",
            source: Source::Snippet(FIB),
            page: None,
        },
        Workload {
            name: "defloop 200k",
            source: Source::Snippet(DEFLOOP),
            page: None,
        },
        Workload {
            name: "saveloop 2k",
            source: Source::Snippet(SAVELOOP),
            page: None,
        },
        Workload {
            name: "sierpinski",
            source: Source::File("examples/sierpinski.ps"),
            page: Some((500, 500)),
        },
        Workload {
            name: "fern",
            source: Source::File("gallery/fern.ps"),
            page: Some((620, 800)),
        },
        Workload {
            name: "specimen (text)",
            source: Source::File("examples/specimen.ps"),
            page: Some((500, 500)),
        },
        Workload {
            name: "postcard (img)",
            source: Source::File("examples/postcard.ps"),
            page: Some((500, 500)),
        },
    ]
}

fn pscat_cmd(w: &Workload) -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_pscat"));
    if let Some((pw, ph)) = w.page {
        c.arg("--page").arg(format!("{pw}x{ph}"));
    }
    match &w.source {
        Source::Snippet(s) => {
            c.arg("-e").arg(if s.is_empty() { " " } else { s });
        }
        Source::File(f) => {
            c.arg("--headless").arg(f);
        }
    }
    c
}

fn gs_cmd(w: &Workload) -> Command {
    let mut c = Command::new("gs");
    c.args(["-dNOPAUSE", "-dBATCH", "-q"]);
    match &w.source {
        Source::Snippet(s) => {
            c.arg("-dNODISPLAY");
            c.arg("-c").arg(if s.is_empty() { " " } else { s });
        }
        Source::File(f) => {
            let (pw, ph) = w.page.unwrap_or((500, 500));
            c.args([
                "-sDEVICE=png16m",
                &format!("-g{pw}x{ph}"),
                "-r72",
                "-o/dev/null",
            ]);
            c.arg(f);
        }
    }
    c
}

struct Measurement {
    /// Best-of-three wall time.
    secs: f64,
    /// Peak RSS in bytes, if `/usr/bin/time -l` could report it.
    max_rss: Option<u64>,
}

/// Run `make()`'s command three times for wall time, once under
/// `/usr/bin/time -l` for peak RSS.
fn measure(make: impl Fn() -> Command) -> Option<Measurement> {
    let mut best = f64::INFINITY;
    for _ in 0..3 {
        let mut cmd = make();
        cmd.stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        let t = Instant::now();
        let status = cmd.status().ok()?;
        let dt = t.elapsed().as_secs_f64();
        if !status.success() {
            return None;
        }
        best = best.min(dt);
    }

    let inner = make();
    let mut timed = Command::new("/usr/bin/time");
    timed.arg("-l");
    timed.arg(inner.get_program());
    timed.args(inner.get_args());
    timed.stdout(std::process::Stdio::null());
    let max_rss = timed.output().ok().and_then(|out| {
        let text = String::from_utf8_lossy(&out.stderr);
        text.lines()
            .find(|l| l.contains("maximum resident set size"))
            .and_then(|l| l.split_whitespace().next())
            .and_then(|n| n.parse::<u64>().ok())
    });
    Some(Measurement {
        secs: best,
        max_rss,
    })
}

fn fmt_ms(m: &Option<Measurement>) -> String {
    match m {
        Some(m) => format!("{:.0}ms", m.secs * 1000.0),
        None => "-".to_string(),
    }
}

fn fmt_rss(m: &Option<Measurement>) -> String {
    match m.as_ref().and_then(|m| m.max_rss) {
        Some(b) => format!("{:.1}MB", b as f64 / (1024.0 * 1024.0)),
        None => "-".to_string(),
    }
}

fn main() {
    let gs_present = Command::new("gs")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success());
    if !gs_present {
        eprintln!("note: gs not installed; gs columns will be empty");
    }
    println!(
        "{:<16} {:>10} {:>10} | {:>10} {:>10}",
        "workload", "pscat", "pscat RSS", "gs", "gs RSS"
    );
    for w in workloads() {
        let ours = measure(|| pscat_cmd(&w));
        let theirs = if gs_present {
            measure(|| gs_cmd(&w))
        } else {
            None
        };
        println!(
            "{:<16} {:>10} {:>10} | {:>10} {:>10}",
            w.name,
            fmt_ms(&ours),
            fmt_rss(&ours),
            fmt_ms(&theirs),
            fmt_rss(&theirs),
        );
    }
    println!(
        "\n(process launch included; subtract each side's startup row to
compare interpretation alone. gs render rows write PNG to /dev/null;
pscat render rows are --headless without an output encode.)"
    );
}
