//! CI perf/memory regression gate — issue #25. Unlike `perf.rs` (a
//! dev-loop tripwire run by hand against one build) and `vs_gs.rs`
//! (pscat vs Ghostscript), this bench compares two *already-built*
//! `pscat` binaries — the PR's HEAD and the PR's merge base — on the
//! same runner, in the same job, so runner-to-runner hardware/thermal
//! noise (a real risk on shared GitHub-hosted macOS runners) cancels
//! out instead of contaminating the comparison. `.github/workflows/
//! ci.yml`'s `perf-regression` job builds both binaries (from two
//! separate checkouts) and invokes this bench — which is compiled
//! from the HEAD checkout only — pointed at both paths. Workload `.ps`
//! files always come from the HEAD checkout, for both sides: this
//! means a PR that only edits `gallery/fern.ps` correctly shows up as
//! a workload change, not a false "interpreter regression."
//!
//! Run with `cargo bench --bench regression -- --head <path-to-pscat>
//! --base <path-to-pscat> [--runs N]`. Prints a markdown report to
//! stdout (posted verbatim to the PR/issue by ci.yml, mirroring
//! `scripts/ci_test_summary.sh`'s delivery shape) and exits non-zero
//! if any workload regressed past `FAIL_PCT`, or if peak-RSS
//! measurement failed outright (an infra problem, not a real
//! signal — must not silently read as "no regression").

use std::process::Command;
use std::time::Instant;

/// Below this, a delta is noise — don't even mention it.
const WARN_PCT: f64 = 20.0;
/// Above this, a delta is implausible as pure CI noise for these
/// workloads (see NOTES.md Stage 8/11 for stable same-hardware
/// numbers) — fail the job. Not wired into `required_status_checks`
/// yet (see NOTES.md and the PR this bench shipped in); a failing job
/// here is a strong signal, not an enforced merge block.
const FAIL_PCT: f64 = 50.0;

struct Workload {
    name: &'static str,
    source: Source,
    page: Option<(u32, u32)>,
}

enum Source {
    Snippet(&'static str),
    File(&'static str),
}

const FIB: &str =
    "/fib { dup 2 lt { pop 1 } { dup 1 sub fib exch 2 sub fib add } ifelse } def 27 fib pop";
const DEFLOOP: &str = "/x 0 def 200000 { /x x 1 add def } repeat";

fn workloads() -> [Workload; 4] {
    [
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
            name: "sierpinski",
            source: Source::File("examples/sierpinski.ps"),
            page: Some((500, 500)),
        },
        Workload {
            name: "fern",
            source: Source::File("gallery/fern.ps"),
            page: Some((620, 800)),
        },
    ]
}

fn pscat_cmd(binary: &str, w: &Workload) -> Command {
    let mut c = Command::new(binary);
    if let Some((pw, ph)) = w.page {
        c.arg("--page").arg(format!("{pw}x{ph}"));
    }
    match &w.source {
        Source::Snippet(s) => {
            c.arg("-e").arg(s);
        }
        Source::File(f) => {
            c.arg("--headless").arg(f);
        }
    }
    c
}

struct Sample {
    secs: f64,
    rss: Option<u64>,
}

/// One timed + RSS-measured invocation via `/usr/bin/time -l`
/// (macOS). `perf.rs`/`vs_gs.rs` established this as the working
/// peak-RSS source on this repo's actual CI platform.
fn sample(binary: &str, w: &Workload) -> Sample {
    let inner = pscat_cmd(binary, w);
    let mut timed = Command::new("/usr/bin/time");
    timed.arg("-l").arg(inner.get_program());
    timed.args(inner.get_args());
    timed.stdout(std::process::Stdio::null());

    let t = Instant::now();
    let out = timed
        .output()
        .unwrap_or_else(|e| panic!("failed to launch /usr/bin/time for {}: {e}", w.name));
    let secs = t.elapsed().as_secs_f64();
    if !out.status.success() {
        panic!(
            "workload {} exited non-zero under binary {binary}:\n{}",
            w.name,
            String::from_utf8_lossy(&out.stderr)
        );
    }
    let text = String::from_utf8_lossy(&out.stderr);
    let rss = text
        .lines()
        .find(|l| l.contains("maximum resident set size"))
        .and_then(|l| l.split_whitespace().next())
        .and_then(|n| n.parse::<u64>().ok());
    Sample { secs, rss }
}

struct Measurement {
    /// Best-of-N wall time.
    ms: f64,
    /// Median of successfully captured peak-RSS samples.
    rss_bytes: u64,
}

fn pct_delta(base: f64, head: f64) -> f64 {
    if base == 0.0 {
        0.0
    } else {
        (head - base) / base * 100.0
    }
}

fn status_icon(pct: f64) -> &'static str {
    if pct.abs() >= FAIL_PCT {
        "\u{274c}" // ❌
    } else if pct.abs() >= WARN_PCT {
        "\u{26a0}\u{fe0f}" // ⚠️
    } else {
        "\u{2705}" // ✅
    }
}

fn parse_args() -> (String, String, u32) {
    let args: Vec<String> = std::env::args().collect();
    let mut head = None;
    let mut base = None;
    let mut runs = 5u32;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--head" => {
                head = args.get(i + 1).cloned();
                i += 2;
            }
            "--base" => {
                base = args.get(i + 1).cloned();
                i += 2;
            }
            "--runs" => {
                runs = args.get(i + 1).and_then(|s| s.parse().ok()).unwrap_or(runs);
                i += 2;
            }
            _ => {
                i += 1;
            }
        }
    }
    let head = head.unwrap_or_else(|| panic!("--head <path-to-pscat-binary> is required"));
    let base = base.unwrap_or_else(|| panic!("--base <path-to-pscat-binary> is required"));
    (head, base, runs)
}

fn main() {
    let (head, base, runs) = parse_args();

    println!("## Perf/memory regression check");
    println!();
    println!(
        "Comparing HEAD against merge base, {runs} interleaved runs per workload \
         on this runner (same-job A/B — not a stored cross-run baseline, to cancel \
         out runner-to-runner noise). \u{2705} < {WARN_PCT:.0}% delta (noise), \
         \u{26a0}\u{fe0f} {WARN_PCT:.0}\u{2013}{FAIL_PCT:.0}%, \u{274c} \u{2265}{FAIL_PCT:.0}% \
         (fails this job). **Not currently a required status check** — a red \u{274c} \
         here is a strong signal for a human/agent to look at, not an enforced merge block."
    );
    println!();
    println!("| workload | metric | base | head | \u{394} | |");
    println!("|---|---|---|---|---|---|");

    let mut worst_pct = 0.0f64;
    for w in workloads() {
        // Interleave which side runs first each iteration so drift
        // over the loop doesn't all land on one side.
        let mut head_ms = Vec::new();
        let mut head_rss = Vec::new();
        let mut base_ms = Vec::new();
        let mut base_rss = Vec::new();
        for run_idx in 0..runs {
            if run_idx % 2 == 0 {
                let h = sample(&head, &w);
                let b = sample(&base, &w);
                head_ms.push(h.secs);
                if let Some(r) = h.rss {
                    head_rss.push(r);
                }
                base_ms.push(b.secs);
                if let Some(r) = b.rss {
                    base_rss.push(r);
                }
            } else {
                let b = sample(&base, &w);
                let h = sample(&head, &w);
                base_ms.push(b.secs);
                if let Some(r) = b.rss {
                    base_rss.push(r);
                }
                head_ms.push(h.secs);
                if let Some(r) = h.rss {
                    head_rss.push(r);
                }
            }
        }

        let finish = |mut ms: Vec<f64>, mut rss: Vec<u64>, side: &str, name: &str| {
            if rss.is_empty() {
                panic!(
                    "workload {name}: /usr/bin/time -l never reported peak RSS for {side} \
                     across {runs} runs — treat as a broken measurement environment"
                );
            }
            ms.sort_by(|a, b| a.partial_cmp(b).unwrap());
            rss.sort_unstable();
            let ms_best = ms[0] * 1000.0;
            let rss_med = rss[rss.len() / 2];
            Measurement {
                ms: ms_best,
                rss_bytes: rss_med,
            }
        };
        let head_m = finish(head_ms, head_rss, "head", w.name);
        let base_m = finish(base_ms, base_rss, "base", w.name);

        let ms_pct = pct_delta(base_m.ms, head_m.ms);
        let rss_pct = pct_delta(base_m.rss_bytes as f64, head_m.rss_bytes as f64);
        worst_pct = worst_pct.max(ms_pct.abs()).max(rss_pct.abs());

        println!(
            "| {} | time | {:.0}ms | {:.0}ms | {:+.1}% | {} |",
            w.name,
            base_m.ms,
            head_m.ms,
            ms_pct,
            status_icon(ms_pct)
        );
        println!(
            "| {} | peak RSS | {:.1}MB | {:.1}MB | {:+.1}% | {} |",
            w.name,
            base_m.rss_bytes as f64 / (1024.0 * 1024.0),
            head_m.rss_bytes as f64 / (1024.0 * 1024.0),
            rss_pct,
            status_icon(rss_pct)
        );
    }

    println!();
    if worst_pct >= FAIL_PCT {
        println!(
            "**Result: \u{274c} regression \u{2265}{FAIL_PCT:.0}% detected** — see rows above."
        );
        std::process::exit(1);
    } else if worst_pct >= WARN_PCT {
        println!(
            "**Result: \u{26a0}\u{fe0f} within noise-to-{FAIL_PCT:.0}% band** — worth a human \
             glance, not blocking."
        );
    } else {
        println!("**Result: \u{2705} no workload moved past the {WARN_PCT:.0}% noise band.**");
    }
}
