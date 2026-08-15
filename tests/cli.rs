//! CLI contract tests (Stage 14): the behaviors scripts and agents
//! rely on — `-` reads stdin, exit codes are honest, artifacts are
//! announced on stdout and errors go to stderr.

use std::io::Write;
use std::process::{Command, Stdio};

use tiny_skia::Pixmap;

const BIN: &str = env!("CARGO_BIN_EXE_pscat");

fn tmp(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("pscat-cli-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir.join(name)
}

/// Run pscat with `args`, feeding `stdin`, returning (status, stdout, stderr).
fn run(args: &[&str], stdin: &str) -> (bool, String, String) {
    let mut child = Command::new(BIN)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn pscat");
    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(stdin.as_bytes())
        .expect("write stdin");
    let out = child.wait_with_output().expect("wait");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn dash_reads_the_program_from_stdin() {
    let png = tmp("stdin.png");
    let (ok, stdout, stderr) = run(
        &["--page", "50x50", "--png", png.to_str().unwrap(), "-"],
        "0 0 50 50 rectfill",
    );
    assert!(ok, "stderr: {stderr}");
    assert!(stdout.contains("wrote"), "artifact announced: {stdout}");
    let bytes = std::fs::read(&png).expect("png written");
    assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
}

#[test]
fn a_postscript_error_fails_the_exit_code() {
    let (ok, _out, stderr) = run(&["--headless", "-"], "1 0 div");
    assert!(!ok, "undefinedresult must exit nonzero");
    assert!(
        stderr.contains("undefinedresult"),
        "error named on stderr: {stderr}"
    );
}

#[test]
fn eval_prints_on_stdout_and_exits_clean() {
    let (ok, stdout, stderr) = run(&["-e", "3 4 add ="], "");
    assert!(ok, "stderr: {stderr}");
    assert!(stdout.contains('7'), "= output on stdout: {stdout}");
}

#[test]
fn stdin_with_interactive_is_rejected() {
    let (ok, _out, stderr) = run(&["--interactive", "-"], "");
    assert!(!ok);
    assert!(stderr.contains("stdin"), "explains the conflict: {stderr}");
}

#[test]
fn lint_flags_an_unbalanced_gsave() {
    let (ok, _out, stderr) = run(&["--lint", "-"], "gsave");
    assert!(ok, "a lint finding isn't a program failure: {stderr}");
    assert!(
        stderr.contains("pscat: lint: [gsave-imbalance]"),
        "stderr: {stderr}"
    );
}

#[test]
fn lint_is_clean_when_nothing_is_wrong() {
    let png = tmp("lint-clean.png");
    let (ok, _out, stderr) = run(
        &[
            "--page",
            "40x40",
            "--png",
            png.to_str().unwrap(),
            "--lint",
            "-",
        ],
        "0 0 40 40 rectfill",
    );
    assert!(ok, "stderr: {stderr}");
    assert!(stderr.contains("pscat: lint: clean"), "stderr: {stderr}");
}

#[test]
fn lint_skips_the_blank_page_check_for_eval_style_usage() {
    // -e is the "leave a result on the stack, don't necessarily draw
    // anything" exception -- an untouched canvas isn't a mistake there.
    let (ok, _out, stderr) = run(&["--lint", "-e", "3 4 add pop"], "");
    assert!(ok, "stderr: {stderr}");
    assert!(!stderr.contains("blank-page"), "stderr: {stderr}");
}

#[test]
fn lint_flags_a_blank_page_on_a_plain_file_run_with_no_output_format() {
    // Regression test (Codex review round 2, PR #59): render_checks
    // used to key off whether --png/--svg/--pdf was given, so a plain
    // `pscat --lint file.ps` (a real program run, just not asked to
    // save an artifact) silently skipped blank-page/stack-leak. Only
    // -e should skip them.
    let (ok, _out, stderr) = run(&["--lint", "-"], "showpage");
    assert!(ok, "stderr: {stderr}");
    assert!(
        stderr.contains("pscat: lint: [blank-page]"),
        "stderr: {stderr}"
    );
}

#[test]
fn lint_flags_a_blank_page_for_eval_style_usage_when_an_output_format_is_also_requested() {
    // Regression test (Codex review round 7, PR #59): render_checks
    // gated purely on `eval.is_none()`, so `-e` paired with an
    // explicit --png/--svg/--pdf -- a real render request, not just a
    // "leave a result on the stack" calculator snippet -- silently
    // skipped blank-page/stack-leak too. An output format flag means
    // rendering was actually asked for, regardless of the source.
    let png = tmp("lint-eval-blank.png");
    let (ok, _out, stderr) = run(
        &[
            "--page",
            "40x40",
            "--png",
            png.to_str().unwrap(),
            "--lint",
            "-e",
            "showpage",
        ],
        "",
    );
    assert!(ok, "stderr: {stderr}");
    assert!(
        stderr.contains("pscat: lint: [blank-page]"),
        "stderr: {stderr}"
    );
}

#[test]
fn lint_flags_a_blank_page_when_an_output_format_is_requested() {
    let png = tmp("lint-blank.png");
    let (ok, _out, stderr) = run(
        &[
            "--page",
            "40x40",
            "--png",
            png.to_str().unwrap(),
            "--lint",
            "-",
        ],
        "showpage",
    );
    assert!(ok, "stderr: {stderr}");
    assert!(
        stderr.contains("pscat: lint: [blank-page]"),
        "stderr: {stderr}"
    );
}

#[test]
fn lint_is_clean_on_real_example_and_gallery_pieces() {
    // Toy programs can't catch what a real, library-loading piece
    // does — confirmed the hard way: the first version of this feature
    // passed every unit test while --lint against these exact files
    // found two real, previously undetected bugs (a `search` "not
    // found" leak in artkit's tfdrawline /justify, and a missing `and`
    // in etching.ps's et-hatch bounds check) — both fixed alongside
    // this test. Regression coverage for "the lint pass itself doesn't
    // cry wolf on ordinary library usage."
    for path in [
        "examples/paragraph_layout.ps",
        "examples/etching_demo.ps",
        "gallery/compositors_proof.ps",
    ] {
        let png = tmp(&format!("lint-corpus-{}.png", path.replace('/', "_")));
        let (ok, _out, stderr) = run(&["--png", png.to_str().unwrap(), "--lint", path], "");
        assert!(ok, "{path} stderr: {stderr}");
        assert!(
            stderr.contains("pscat: lint: clean"),
            "{path} should lint clean, stderr: {stderr}"
        );
    }
}

#[test]
fn error_report_includes_a_line_number() {
    let (ok, _out, stderr) = run(&["--headless", "-"], "1 0 div\n");
    assert!(!ok);
    assert!(stderr.contains("Line: 1"), "stderr: {stderr}");
}

// --- issue #21: sweep / contact-sheet ---------------------------------

/// The one test that catches "the override never fires": a script that
/// hardcodes its own `N srand` still produces genuinely different
/// pixels per swept seed, because `--sweep-seed` overrides `srand`'s
/// operand rather than requiring the source to cooperate.
#[test]
fn sweep_seed_produces_different_frames_from_a_hardcoded_srand() {
    let out = tmp("sweep-differ.png");
    let source = "7 srand 1 1 1 setrgbcolor 0 0 20 20 rectfill \
                  rand 255 mod 255 div rand 255 mod 255 div rand 255 mod 255 div setrgbcolor \
                  0 0 20 20 rectfill";
    let (ok, _out_text, stderr) = run(
        &[
            "--page",
            "20x20",
            "--sweep-seed",
            "1,2",
            "--png",
            out.to_str().unwrap(),
            "-",
        ],
        source,
    );
    assert!(ok, "stderr: {stderr}");
    let a = std::fs::read(out.with_file_name("sweep-differ-001.png")).unwrap();
    let b = std::fs::read(out.with_file_name("sweep-differ-002.png")).unwrap();
    assert_ne!(a, b, "different seeds must render different pixels");
}

/// The reproducible-art doctrine, through a sweep: the same seed
/// listed twice must render byte-identical frames.
#[test]
fn sweep_seed_repeated_value_is_byte_identical() {
    let out = tmp("sweep-same.png");
    let source = "7 srand rand 255 mod 255 div rand 255 mod 255 div rand 255 mod 255 div setrgbcolor \
         0 0 20 20 rectfill";
    let (ok, _out_text, stderr) = run(
        &[
            "--page",
            "20x20",
            "--sweep-seed",
            "5,5",
            "--png",
            out.to_str().unwrap(),
            "-",
        ],
        source,
    );
    assert!(ok, "stderr: {stderr}");
    let a = std::fs::read(out.with_file_name("sweep-same-001.png")).unwrap();
    let b = std::fs::read(out.with_file_name("sweep-same-002.png")).unwrap();
    assert_eq!(a, b, "same seed twice must render byte-identical frames");
}

#[test]
fn sweep_seed_warns_when_source_never_calls_srand() {
    let out = tmp("sweep-no-srand.png");
    let (ok, _out_text, stderr) = run(
        &[
            "--page",
            "20x20",
            "--sweep-seed",
            "1,2",
            "--png",
            out.to_str().unwrap(),
            "-",
        ],
        "0 0 20 20 rectfill",
    );
    assert!(ok, "a missing srand call isn't a program failure: {stderr}");
    assert!(stderr.contains("never called srand"), "stderr: {stderr}");
}

/// The generic named-parameter sweep, and that a contact sheet lays
/// cells out left-to-right in sweep order: three frames each painted
/// a distinct solid color keyed off the swept value, probed at each
/// cell's center after compositing.
#[test]
fn sweep_param_orders_contact_sheet_cells_left_to_right() {
    let sheet = tmp("sweep-hue-sheet.png");
    let source = "/Shade where { pop Shade } { 0 } ifelse dup dup setrgbcolor 0 0 10 10 rectfill";
    let (ok, _out_text, stderr) = run(
        &[
            "--page",
            "10x10",
            "--sweep",
            "Shade=0,0.5,1",
            "--contact-sheet",
            sheet.to_str().unwrap(),
            "--grid",
            "3x1",
            "-",
        ],
        source,
    );
    assert!(ok, "stderr: {stderr}");
    let pixmap = Pixmap::load_png(&sheet).expect("decode contact sheet");
    // 3 cells of 10px + 2 gaps of 4px (contact_sheet::GAP) = 38.
    assert_eq!(pixmap.width(), 38);
    let sample = |x: u32, y: u32| -> [u8; 4] {
        let i = ((y * pixmap.width() + x) * 4) as usize;
        pixmap.data()[i..i + 4].try_into().unwrap()
    };
    let cell0 = sample(5, 5);
    let cell1 = sample(19, 5); // 10 + 4(gap) + 5
    let cell2 = sample(33, 5); // 20 + 8(gap) + 5
    assert_eq!(cell0, [0, 0, 0, 255], "Shade=0 -> black");
    assert_eq!(cell2, [255, 255, 255, 255], "Shade=1 -> white");
    assert!(
        cell1[0] == cell1[1] && cell1[1] == cell1[2] && cell1[0] > 0 && cell1[0] < 255,
        "Shade=0.5 -> a mid gray strictly between black and white, got {cell1:?}"
    );
}

#[test]
fn sweep_requires_an_output_flag() {
    let (ok, _out, stderr) = run(&["--sweep-seed", "1,2", "-"], "");
    assert!(!ok);
    assert!(stderr.contains("--png and/or --contact-sheet"), "{stderr}");
}

#[test]
fn sweep_rejects_incompatible_flags() {
    let (ok, _out, stderr) = run(&["--sweep-seed", "1,2", "--svg", "out.svg", "-"], "");
    assert!(!ok);
    assert!(stderr.contains("--svg"), "{stderr}");
}

#[test]
fn sweep_rejects_both_axes_at_once() {
    let (ok, _out, stderr) = run(
        &[
            "--sweep-seed",
            "1,2",
            "--sweep",
            "Foo=1,2",
            "--png",
            "out.png",
            "-",
        ],
        "",
    );
    assert!(!ok);
    assert!(stderr.contains("mutually exclusive"), "{stderr}");
}

#[test]
fn contact_sheet_grid_too_small_errors() {
    let sheet = tmp("sweep-grid-too-small.png");
    let (ok, _out, stderr) = run(
        &[
            "--sweep-seed",
            "1,2,3",
            "--contact-sheet",
            sheet.to_str().unwrap(),
            "--grid",
            "1x1",
            "-",
        ],
        "0 0 10 10 rectfill",
    );
    assert!(!ok);
    assert!(stderr.contains("--grid 1x1"), "{stderr}");
}

/// A frame that errors doesn't abort the sweep: later frames still
/// render, and the partial canvas from the failed frame is still
/// written (this CLI's existing partial-render-on-error philosophy),
/// but the process exits nonzero so a caller knows something failed.
#[test]
fn sweep_frame_error_continues_the_sweep_and_reports_nonzero() {
    let out = tmp("sweep-error.png");
    let source = "/Div where { pop Div } { 1 } ifelse 10 exch div pop 0 0 10 10 rectfill";
    let (ok, _out_text, stderr) = run(
        &[
            "--page",
            "10x10",
            "--sweep",
            "Div=0,1",
            "--png",
            out.to_str().unwrap(),
            "-",
        ],
        source,
    );
    assert!(!ok, "a failed frame must exit nonzero");
    assert!(
        stderr.contains("frame 1/2") && stderr.contains("undefinedresult"),
        "stderr: {stderr}"
    );
    assert!(
        out.with_file_name("sweep-error-001.png").exists(),
        "the failed frame's partial canvas is still written"
    );
    assert!(out.with_file_name("sweep-error-002.png").exists());
}
