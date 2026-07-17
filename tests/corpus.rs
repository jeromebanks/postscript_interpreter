//! Found-file corpus, round 2 (Stage 8): classic PostScript art from
//! the Ghostscript examples collection, rendered by pscat and by gs
//! and block-compared like tests/golden.rs.
//!
//! The files are **downloaded on first run** into tests/corpus/
//! (gitignored — they're AGPL-distributed, so we fetch rather than
//! vendor) and cached; without network and without a cached copy the
//! affected file is skipped with a note, same philosophy as skipping
//! golden when gs is absent.
//!
//! Per-file status lives in the EXPECT table — flip an entry to Pass
//! when a gap closes. Files marked Pass are hard assertions; Known
//! failures print their current error so drift is visible.

use std::process::Command;

use pscat::Interp;
use tiny_skia::Pixmap;

const SIZE: u32 = 500;
const BLOCK: u32 = 10;
const BASE: &str = "https://raw.githubusercontent.com/ArtifexSoftware/ghostpdl/master/examples";

#[derive(PartialEq)]
enum Expect {
    /// Renders and matches gs within the golden tolerances.
    Pass,
    /// Runs to completion but rendering differs from gs (reason).
    RunsDiffers(&'static str),
}

// waterfal.ps is deliberately absent: it reads gs's QUIET
// command-line define, a gs-environment dependency, not a language
// gap. Everything below is plain PostScript.
const EXPECT: &[(&str, Expect)] = &[
    ("tiger.eps", Expect::Pass),
    ("golfer.eps", Expect::Pass),
    ("escher.ps", Expect::Pass),
    ("colorcir.ps", Expect::Pass),
    ("doretree.ps", Expect::Pass),
    // Both run to completion and render structurally right, but their
    // art is rand-driven and rand sequences are implementation-
    // specific (same reason type3_ransom sits outside the golden
    // suite) — pixel comparison with gs is meaningless.
    ("snowflak.ps", Expect::RunsDiffers("rand-driven art")),
    ("vasarely.ps", Expect::RunsDiffers("rand-driven art")),
];

fn gs_available() -> bool {
    Command::new("gs")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

fn fetch(name: &str) -> Option<Vec<u8>> {
    let dir = std::path::Path::new("tests/corpus");
    let path = dir.join(name);
    if let Ok(data) = std::fs::read(&path) {
        return Some(data);
    }
    let _ = std::fs::create_dir_all(dir);
    let ok = Command::new("curl")
        .args(["-sf", "-m", "20", "-o"])
        .arg(&path)
        .arg(format!("{BASE}/{name}"))
        .status()
        .is_ok_and(|s| s.success());
    if ok { std::fs::read(&path).ok() } else { None }
}

fn render_pscat(src: &[u8]) -> Result<Pixmap, String> {
    let mut it = Interp::with_page(SIZE, SIZE).expect("page");
    it.run_source(src).map_err(|e| it.error_report(&e))?;
    Ok(it.gfx().pixmap.clone())
}

fn render_gs(name: &str) -> Pixmap {
    let out = std::env::temp_dir().join(format!("pscat_corpus_{}.png", name.replace('.', "_")));
    let status = Command::new("gs")
        .args([
            "-q",
            "-dNOPAUSE",
            "-dBATCH",
            "-sDEVICE=png16m",
            // We antialias; gs's png16m doesn't by default. These files
            // are edge-dense enough that AA policy would swamp the
            // geometry/color signal the comparison is after.
            "-dGraphicsAlphaBits=4",
            "-dTextAlphaBits=4",
            &format!("-g{SIZE}x{SIZE}"),
            "-r72",
            &format!("-o{}", out.display()),
            &format!("tests/corpus/{name}"),
        ])
        .status()
        .expect("run gs");
    assert!(status.success(), "gs failed on {name}");
    let data = std::fs::read(&out).expect("read gs output");
    let _ = std::fs::remove_file(&out);
    Pixmap::decode_png(&data).expect("decode gs png")
}

fn downsample(pm: &Pixmap) -> Vec<[f64; 3]> {
    let (w, h) = (pm.width(), pm.height());
    let (bw, bh) = (w / BLOCK, h / BLOCK);
    let mut cells = vec![[0f64; 3]; (bw * bh) as usize];
    let data = pm.data();
    for y in 0..h {
        for x in 0..w {
            let i = ((y * w + x) * 4) as usize;
            let cell = &mut cells[((y / BLOCK) * bw + x / BLOCK) as usize];
            cell[0] += f64::from(data[i]);
            cell[1] += f64::from(data[i + 1]);
            cell[2] += f64::from(data[i + 2]);
        }
    }
    let n = f64::from(BLOCK * BLOCK);
    for c in &mut cells {
        c[0] /= n;
        c[1] /= n;
        c[2] /= n;
    }
    cells
}

fn compare(name: &str, ours: &Pixmap, theirs: &Pixmap) -> Result<(), String> {
    let (a, b) = (downsample(ours), downsample(theirs));
    let mut total = 0f64;
    for (i, (ca, cb)) in a.iter().zip(&b).enumerate() {
        let (la, lb) = ((ca[0] + ca[1] + ca[2]) / 3.0, (cb[0] + cb[1] + cb[2]) / 3.0);
        // Found art is denser than our hand-made examples; use the
        // text-width ink bands from golden.rs.
        if (la < 200.0 && lb > 253.0) || (lb < 200.0 && la > 253.0) {
            return Err(format!(
                "{name}: block {i} inked in one render only ({la:.0} vs {lb:.0})"
            ));
        }
        for ch in 0..3 {
            total += (ca[ch] - cb[ch]).abs();
        }
    }
    let mean = total / (a.len() * 3) as f64;
    if mean >= 8.0 {
        return Err(format!("{name}: mean block diff {mean:.2}"));
    }
    Ok(())
}

#[test]
fn corpus_round2_matches_ghostscript() {
    if !gs_available() {
        eprintln!("skipping corpus tests: ghostscript (gs) not installed");
        return;
    }
    let mut failures = Vec::new();
    for (name, expect) in EXPECT {
        let Some(src) = fetch(name) else {
            eprintln!("corpus: {name} unavailable (offline?) — skipped");
            continue;
        };
        // Every listed file must at least run — RunsDiffers only
        // waives the pixel comparison, never an interpreter error.
        let ours = match render_pscat(&src) {
            Ok(p) => p,
            Err(e) => {
                failures.push(format!("{name}: {e}"));
                continue;
            }
        };
        let result = compare(name, &ours, &render_gs(name));
        match (expect, result) {
            (Expect::Pass, Ok(())) => eprintln!("corpus: {name} PASS"),
            (Expect::Pass, Err(e)) => failures.push(e),
            (Expect::RunsDiffers(why), Err(e)) => {
                eprintln!("corpus: {name} known-differs ({why}): {e}");
            }
            (Expect::RunsDiffers(why), Ok(())) => {
                failures.push(format!(
                    "{name}: marked RunsDiffers({why}) but now matches — promote to Pass"
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
