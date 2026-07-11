use std::env;
use std::io::{self, BufRead, Write};
use std::process::ExitCode;

use pscat::window::{WindowOptions, run_windowed};
use pscat::{Interp, gfx};

struct Options {
    file: Option<String>,
    eval: Option<String>,
    headless: bool,
    png: Option<String>,
    steps_per_frame: usize,
    page: (u32, u32),
}

fn main() -> ExitCode {
    let options = match parse_args() {
        Ok(o) => o,
        Err(msg) => {
            eprintln!("pscat: {msg}");
            print_usage();
            return ExitCode::FAILURE;
        }
    };

    let Some(mut interp) = Interp::with_page(options.page.0, options.page.1) else {
        eprintln!(
            "pscat: unusable page size {}x{}",
            options.page.0, options.page.1
        );
        return ExitCode::FAILURE;
    };

    if let Some(expr) = &options.eval {
        return finish_headless(
            run_headless(&mut interp, expr.as_bytes()),
            &interp,
            &options,
        );
    }
    let Some(path) = &options.file else {
        return repl(&mut interp);
    };
    let source = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) => {
            eprintln!("pscat: cannot read {path}: {e}");
            return ExitCode::FAILURE;
        }
    };

    if options.headless || options.png.is_some() {
        return finish_headless(run_headless(&mut interp, &source), &interp, &options);
    }

    interp.begin_source(&source);
    let window_options = WindowOptions {
        title: format!("pscat — {path}"),
        steps_per_frame: options.steps_per_frame,
    };
    match run_windowed(interp, window_options) {
        Ok(()) => ExitCode::SUCCESS,
        Err(msg) => {
            eprintln!("pscat: {msg}");
            ExitCode::FAILURE
        }
    }
}

fn run_headless(interp: &mut Interp, source: &[u8]) -> bool {
    match interp.run_source(source) {
        Ok(()) => true,
        Err(e) => {
            eprintln!("{}", interp.error_report(&e));
            false
        }
    }
}

/// Write the PNG (if requested) even after an error — a partial canvas is
/// exactly what you want to see when debugging a program that died.
fn finish_headless(ok: bool, interp: &Interp, options: &Options) -> ExitCode {
    if let Some(path) = &options.png {
        if let Err(e) = interp.gfx().pixmap.save_png(path) {
            eprintln!("pscat: cannot write {path}: {e}");
            return ExitCode::FAILURE;
        }
        println!("pscat: wrote {path}");
    }
    if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn parse_args() -> Result<Options, String> {
    let mut options = Options {
        file: None,
        eval: None,
        headless: false,
        png: None,
        steps_per_frame: 100,
        page: gfx::DEFAULT_PAGE,
    };
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                print_usage();
                std::process::exit(0);
            }
            "-e" | "--eval" => {
                options.eval = Some(args.next().ok_or("missing expression after -e")?);
            }
            "--headless" => options.headless = true,
            "--png" => {
                options.png = Some(args.next().ok_or("missing path after --png")?);
            }
            "--speed" => {
                let n = args.next().ok_or("missing value after --speed")?;
                options.steps_per_frame = n
                    .parse()
                    .ok()
                    .filter(|&n| n > 0)
                    .ok_or_else(|| format!("invalid --speed value: {n}"))?;
            }
            "--page" => {
                let spec = args.next().ok_or("missing WxH after --page")?;
                let (w, h) = spec
                    .split_once('x')
                    .and_then(|(w, h)| Some((w.parse().ok()?, h.parse().ok()?)))
                    .ok_or_else(|| format!("invalid --page value: {spec} (expected WxH)"))?;
                options.page = (w, h);
            }
            _ if arg.starts_with('-') => return Err(format!("unknown option: {arg}")),
            _ => {
                if options.file.is_some() {
                    return Err("more than one input file given".to_string());
                }
                options.file = Some(arg);
            }
        }
    }
    Ok(options)
}

fn print_usage() {
    println!("usage: pscat [options] [file.ps]");
    println!();
    println!("Runs file.ps in a live window (watch it draw), or a REPL if no file.");
    println!();
    println!("  -e, --eval 'code'   evaluate a snippet headlessly and exit");
    println!("      --headless      run the file without a window");
    println!("      --png PATH      write the final canvas as a PNG (implies --headless)");
    println!("      --speed N       interpreter steps per frame (default 100)");
    println!("      --page WxH      canvas size in points (default 612x792, US Letter)");
}

fn repl(interp: &mut Interp) -> ExitCode {
    println!(
        "pscat {} — PostScript interpreter (headless canvas; graphics ops draw off-screen)",
        env!("CARGO_PKG_VERSION")
    );
    println!("Type PostScript; 'quit' or Ctrl-D exits. The prompt shows operand-stack depth.");
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
                    eprintln!("{}", interp.error_report(&e));
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
