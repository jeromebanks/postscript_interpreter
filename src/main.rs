use std::env;
use std::io::{self, BufRead, Write};
use std::process::ExitCode;

use pscat::repl::LineBuffer;
use pscat::spool::Watcher;
use pscat::window::{WindowOptions, run_interactive, run_spool, run_windowed};
use pscat::{Interp, gfx};

struct Options {
    file: Option<String>,
    eval: Option<String>,
    headless: bool,
    png: Option<String>,
    steps_per_frame: usize,
    page: (u32, u32),
    /// Device resolution; 72 means one pixel per point.
    dpi: f32,
    /// Write the page(s) as SVG (implies headless).
    svg: Option<String>,
    /// Write the document as PDF (implies headless).
    pdf: Option<String>,
    /// Print the operand stack after an error (REPL and headless).
    pstack_on_error: bool,
    /// Watch a directory and render whatever lands there (Stage 10).
    spool: Option<String>,
    /// Screen raster output like a mono laser printer (Stage 10).
    halftone: bool,
    /// The windowed REPL: type PostScript, watch it draw (Stage 8's
    /// last sliver).
    interactive: bool,
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

    if let Some(dir) = &options.spool {
        if options.file.is_some()
            || options.eval.is_some()
            || options.headless
            || options.interactive
            || options.png.is_some()
            || options.svg.is_some()
            || options.pdf.is_some()
        {
            eprintln!("pscat: --spool runs alone (no file argument or other mode flags)");
            return ExitCode::FAILURE;
        }
        let watcher = match Watcher::new(std::path::Path::new(dir)) {
            Ok(w) => w,
            Err(e) => {
                eprintln!("pscat: cannot watch {dir}: {e}");
                return ExitCode::FAILURE;
            }
        };
        let window_options = WindowOptions {
            title: format!("pscat spool — {dir}"),
            steps_per_frame: options.steps_per_frame,
            halftone: options.halftone,
        };
        return match run_spool(watcher, options.page, options.dpi / 72.0, window_options) {
            Ok(()) => ExitCode::SUCCESS,
            Err(msg) => {
                eprintln!("pscat: {msg}");
                ExitCode::FAILURE
            }
        };
    }

    let Some(mut interp) =
        Interp::with_page_scaled(options.page.0, options.page.1, options.dpi / 72.0)
    else {
        eprintln!(
            "pscat: unusable page size {}x{}",
            options.page.0, options.page.1
        );
        return ExitCode::FAILURE;
    };

    if options.svg.is_some() {
        interp.gfx_mut().enable_svg();
    }
    if options.pdf.is_some() {
        interp.gfx_mut().enable_pdf();
    }
    if let Some(expr) = &options.eval {
        return finish_headless(
            run_headless(&mut interp, expr.as_bytes(), &options),
            &interp,
            &options,
        );
    }
    if options.interactive {
        if options.headless
            || options.png.is_some()
            || options.svg.is_some()
            || options.pdf.is_some()
        {
            eprintln!("pscat: --interactive needs a window (drop the headless/output flags)");
            return ExitCode::FAILURE;
        }
        // A file given alongside --interactive runs as a prelude —
        // load a library, then explore it by hand.
        let prelude = match &options.file {
            None => None,
            Some(path) => match std::fs::read(path) {
                Ok(bytes) => Some(bytes),
                Err(e) => {
                    eprintln!("pscat: cannot read {path}: {e}");
                    return ExitCode::FAILURE;
                }
            },
        };
        let window_options = WindowOptions {
            title: "pscat — interactive".to_string(),
            steps_per_frame: options.steps_per_frame,
            halftone: options.halftone,
        };
        return match run_interactive(interp, window_options, prelude) {
            Ok(()) => ExitCode::SUCCESS,
            Err(msg) => {
                eprintln!("pscat: {msg}");
                ExitCode::FAILURE
            }
        };
    }
    let Some(path) = &options.file else {
        return repl(&mut interp, &options);
    };
    let source = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) => {
            eprintln!("pscat: cannot read {path}: {e}");
            return ExitCode::FAILURE;
        }
    };

    if options.headless || options.png.is_some() || options.svg.is_some() || options.pdf.is_some() {
        return finish_headless(
            run_headless(&mut interp, &source, &options),
            &interp,
            &options,
        );
    }

    interp.begin_source(&source);
    let window_options = WindowOptions {
        title: format!("pscat — {path}"),
        steps_per_frame: options.steps_per_frame,
        halftone: options.halftone,
    };
    match run_windowed(interp, window_options) {
        Ok(()) => ExitCode::SUCCESS,
        Err(msg) => {
            eprintln!("pscat: {msg}");
            ExitCode::FAILURE
        }
    }
}

fn run_headless(interp: &mut Interp, source: &[u8], options: &Options) -> bool {
    match interp.run_source(source) {
        Ok(()) => true,
        Err(e) => {
            eprintln!("{}", interp.error_report(&e));
            if options.pstack_on_error {
                print_pstack(interp);
            }
            false
        }
    }
}

/// gs-style post-mortem: the operand stack, bottom to top, in `==`
/// syntax — exactly what you want when a found file dies mid-run.
fn print_pstack(interp: &Interp) {
    let stack = interp.operand_stack();
    eprint!("Operand stack ({}):", stack.len());
    for obj in stack {
        eprint!(" {}", obj.repr());
    }
    eprintln!();
}

/// Write the PNG(s) (if requested) even after an error — a partial
/// canvas is exactly what you want when debugging a program that died.
/// One page writes the exact path given; multi-page documents write
/// out-001.png, out-002.png, ...
fn finish_headless(ok: bool, interp: &Interp, options: &Options) -> ExitCode {
    if let Some(path) = &options.png {
        let gfx = interp.gfx();
        let mut pages: Vec<&tiny_skia::Pixmap> = gfx.pages().iter().collect();
        if pages.is_empty() || gfx.has_trailing_art() {
            pages.push(&gfx.pixmap);
        }
        // --halftone screens the raster on the way out, like a mono
        // printer's RIP; the vector targets below stay contone.
        let save = |page: &tiny_skia::Pixmap, path: &str| -> bool {
            let screened;
            let page = if options.halftone {
                screened = pscat::halftone::screen(page);
                &screened
            } else {
                page
            };
            if let Err(e) = page.save_png(path) {
                eprintln!("pscat: cannot write {path}: {e}");
                false
            } else {
                println!("pscat: wrote {path}");
                true
            }
        };
        if pages.len() == 1 {
            if !save(pages[0], path) {
                return ExitCode::FAILURE;
            }
        } else {
            for (i, page) in pages.iter().enumerate() {
                if !save(page, &numbered_path(path, i + 1)) {
                    return ExitCode::FAILURE;
                }
            }
        }
    }
    if let Some(path) = &options.svg
        && let Some(pages) = interp.gfx().svg_pages()
    {
        let write = |p: &str, body: &str| -> bool {
            if let Err(e) = std::fs::write(p, body) {
                eprintln!("pscat: cannot write {p}: {e}");
                false
            } else {
                println!("pscat: wrote {p}");
                true
            }
        };
        if pages.len() == 1 {
            if !write(path, &pages[0]) {
                return ExitCode::FAILURE;
            }
        } else {
            for (i, body) in pages.iter().enumerate() {
                if !write(&numbered_path(path, i + 1), body) {
                    return ExitCode::FAILURE;
                }
            }
        }
    }
    if let Some(path) = &options.pdf
        && let Some(doc) = interp.gfx().pdf_document()
    {
        if let Err(e) = std::fs::write(path, doc) {
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

/// out.png → out-001.png (suffix before the extension).
fn numbered_path(path: &str, n: usize) -> String {
    match path.rsplit_once('.') {
        Some((stem, ext)) => format!("{stem}-{n:03}.{ext}"),
        None => format!("{path}-{n:03}"),
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
        dpi: 72.0,
        svg: None,
        pdf: None,
        pstack_on_error: false,
        spool: None,
        halftone: false,
        interactive: false,
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
            "--pstack-on-error" => options.pstack_on_error = true,
            "--dpi" => {
                let n = args.next().ok_or("missing value after --dpi")?;
                options.dpi = n
                    .parse()
                    .ok()
                    .filter(|&d: &f32| (9.0..=1200.0).contains(&d))
                    .ok_or_else(|| format!("invalid --dpi value: {n} (expected 9..1200)"))?;
            }
            "--png" => {
                options.png = Some(args.next().ok_or("missing path after --png")?);
            }
            "--svg" => {
                options.svg = Some(args.next().ok_or("missing path after --svg")?);
            }
            "--pdf" => {
                options.pdf = Some(args.next().ok_or("missing path after --pdf")?);
            }
            "--spool" => {
                options.spool = Some(args.next().ok_or("missing directory after --spool")?);
            }
            "--halftone" => options.halftone = true,
            "-i" | "--interactive" => options.interactive = true,
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
                let (w, h): (u32, u32) = spec
                    .split_once('x')
                    .and_then(|(w, h)| Some((w.parse().ok()?, h.parse().ok()?)))
                    .ok_or_else(|| format!("invalid --page value: {spec} (expected WxH)"))?;
                // 8000² ≈ a 256 MB canvas — plenty, and it keeps a typo'd
                // page size from attempting a multi-gigabyte allocation.
                if !(1..=8000).contains(&w) || !(1..=8000).contains(&h) {
                    return Err(format!("--page dimensions must be 1..8000, got {spec}"));
                }
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
    println!("  -i, --interactive   REPL + live window: type PostScript, watch it draw");
    println!("                      (an optional file.ps runs first as a prelude)");
    println!("      --headless      run the file without a window");
    println!("      --png PATH      write the final canvas as a PNG (implies --headless)");
    println!("      --speed N       interpreter steps per frame (default 100)");
    println!("      --page WxH      canvas size in points (default 612x792, US Letter)");
    println!("      --svg PATH      write the page(s) as SVG (implies --headless)");
    println!("      --pdf PATH      write the document as PDF (implies --headless)");
    println!("      --dpi N         device resolution (default 72 = 1 pixel per point)");
    println!("      --spool DIR     watch DIR and render each .ps/.eps that lands there");
    println!("      --halftone      screen the raster like a mono laser printer (window/PNG)");
    println!("      --pstack-on-error  print the operand stack after an error");
}

fn repl(interp: &mut Interp, options: &Options) -> ExitCode {
    println!(
        "pscat {} — PostScript interpreter (headless canvas; graphics ops draw off-screen)",
        env!("CARGO_PKG_VERSION")
    );
    println!("Type PostScript; 'quit' or Ctrl-D exits. The prompt shows operand-stack depth.");
    let stdin = io::stdin();
    let mut buffer = LineBuffer::new();
    loop {
        if buffer.is_mid_input() {
            // Mid-procedure or mid-string: keep reading.
            print!("...> ");
        } else {
            let depth = interp.operand_stack().len();
            if depth == 0 {
                print!("PS> ");
            } else {
                print!("PS<{depth}> ");
            }
        }
        let _ = io::stdout().flush();

        let mut line = String::new();
        match stdin.lock().read_line(&mut line) {
            Ok(0) => {
                println!();
                return ExitCode::SUCCESS;
            }
            Ok(_) => {
                let Some(source) = buffer.push_line(&line) else {
                    continue;
                };
                if let Err(e) = interp.run_str(&source) {
                    eprintln!("{}", interp.error_report(&e));
                    if options.pstack_on_error {
                        print_pstack(interp);
                    }
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
