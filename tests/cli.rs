//! CLI contract tests (Stage 14): the behaviors scripts and agents
//! rely on — `-` reads stdin, exit codes are honest, artifacts are
//! announced on stdout and errors go to stderr.

use std::io::Write;
use std::process::{Command, Stdio};

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
