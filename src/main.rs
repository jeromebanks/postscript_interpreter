use std::env;
use std::io::{self, BufRead, Write};
use std::process::ExitCode;

use pscat::{Interp, PsError};

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    match args.as_slice() {
        [] => repl(),
        [flag] if flag == "-h" || flag == "--help" => {
            print_usage();
            ExitCode::SUCCESS
        }
        [flag, expr] if flag == "-e" || flag == "--eval" => run(expr.as_bytes()),
        [path] => match std::fs::read(path) {
            Ok(bytes) => run(&bytes),
            Err(e) => {
                eprintln!("pscat: cannot read {path}: {e}");
                ExitCode::FAILURE
            }
        },
        _ => {
            print_usage();
            ExitCode::FAILURE
        }
    }
}

fn print_usage() {
    println!("usage: pscat [file.ps]         run a PostScript program");
    println!("       pscat -e 'code'         evaluate a snippet");
    println!("       pscat                   interactive REPL");
}

fn run(src: &[u8]) -> ExitCode {
    let mut interp = Interp::new();
    match interp.run_source(src) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{}", error_report(&interp, &e));
            ExitCode::FAILURE
        }
    }
}

/// Format errors the way a LaserWriter would have sent them back over the
/// serial line — a small nostalgia nod, and unambiguous about which
/// operator failed.
fn error_report(interp: &Interp, err: &PsError) -> String {
    let kind = match err {
        PsError::Syntax(detail) => format!("syntaxerror ({detail})"),
        _ => err.name().to_string(),
    };
    let command = match err {
        PsError::Undefined(name) => name.clone(),
        _ => interp
            .last_executed_name()
            .unwrap_or("--none--")
            .to_string(),
    };
    format!("%%[ Error: {kind}; OffendingCommand: {command} ]%%")
}

fn repl() -> ExitCode {
    println!(
        "pscat {} — PostScript interpreter, Stage 1 (stack & arithmetic)",
        env!("CARGO_PKG_VERSION")
    );
    println!("Type PostScript; 'quit' or Ctrl-D exits. The prompt shows operand-stack depth.");
    let mut interp = Interp::new();
    let stdin = io::stdin();
    loop {
        let depth = interp.operand_stack().len();
        if depth == 0 {
            print!("PS> ");
        } else {
            print!("PS<{depth}> ");
        }
        let _ = io::stdout().flush();

        let mut line = String::new();
        match stdin.lock().read_line(&mut line) {
            Ok(0) => {
                println!();
                return ExitCode::SUCCESS;
            }
            Ok(_) => {
                if let Err(e) = interp.run_str(&line) {
                    eprintln!("{}", error_report(&interp, &e));
                }
                if interp.quit_requested() {
                    return ExitCode::SUCCESS;
                }
            }
            Err(e) => {
                eprintln!("pscat: read error: {e}");
                return ExitCode::FAILURE;
            }
        }
    }
}
