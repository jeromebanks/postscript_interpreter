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
    /// Self-check/lint mode (issue #17): print diagnostics for common
    /// silent-failure mistakes after a headless run.
    lint: bool,
    /// Reseed with each value in turn, once per sweep frame (issue
    /// #21): overrides every `srand` call transparently, so it works
    /// on found art unmodified.
    sweep_seed: Option<Vec<i64>>,
    /// Predefine `/NAME <value> def` in userdict before each sweep
    /// frame (issue #21): opt-in, the source must look `/NAME` up
    /// itself (e.g. `/NAME where { pop NAME } { 0 } ifelse`).
    sweep_param: Option<(String, Vec<f64>)>,
    /// Composite every sweep frame into one grid PNG.
    contact_sheet: Option<String>,
    /// Explicit (cols, rows) for --contact-sheet; default is a
    /// square-ish grid sized to the sweep count.
    grid: Option<(u32, u32)>,
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

    if options.sweep_seed.is_some() || options.sweep_param.is_some() {
        if options.sweep_seed.is_some() && options.sweep_param.is_some() {
            eprintln!(
                "pscat: --sweep-seed and --sweep are mutually exclusive (one sweep axis per run)"
            );
            return ExitCode::FAILURE;
        }
        if options.eval.is_some()
            || options.interactive
            || options.svg.is_some()
            || options.pdf.is_some()
            || options.lint
            || options.spool.is_some()
        {
            eprintln!(
                "pscat: --sweep-seed/--sweep run a file or stdin headlessly \
                 (drop -e/--interactive/--svg/--pdf/--lint/--spool)"
            );
            return ExitCode::FAILURE;
        }
        if options.file.is_none() {
            eprintln!("pscat: --sweep-seed/--sweep need a file or - argument");
            return ExitCode::FAILURE;
        }
        if options.png.is_none() && options.contact_sheet.is_none() {
            eprintln!(
                "pscat: --sweep-seed/--sweep need --png and/or --contact-sheet to write output"
            );
            return ExitCode::FAILURE;
        }
    }

    if let Some(dir) = &options.spool {
        if options.file.is_some()
            || options.eval.is_some()
            || options.headless
            || options.interactive
            || options.png.is_some()
            || options.svg.is_some()
            || options.pdf.is_some()
            || options.lint
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
            || options.lint
        {
            eprintln!("pscat: --interactive needs a window (drop the headless/output flags)");
            return ExitCode::FAILURE;
        }
        // A file given alongside --interactive runs as a prelude —
        // load a library, then explore it by hand. Not `-`: stdin is
        // the REPL's own channel in this mode.
        if options.file.as_deref() == Some("-") {
            eprintln!("pscat: --interactive reads stdin itself; give the prelude as a file");
            return ExitCode::FAILURE;
        }
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
    // `pscat -` reads the program from stdin — pipe-friendly for
    // scripts and agents: `generate | pscat --png out.png -`.
    let source = if path == "-" {
        let mut buf = Vec::new();
        if let Err(e) = io::Read::read_to_end(&mut io::stdin().lock(), &mut buf) {
            eprintln!("pscat: cannot read stdin: {e}");
            return ExitCode::FAILURE;
        }
        buf
    } else {
        match std::fs::read(path) {
            Ok(bytes) => bytes,
            Err(e) => {
                eprintln!("pscat: cannot read {path}: {e}");
                return ExitCode::FAILURE;
            }
        }
    };

    if options.sweep_seed.is_some() || options.sweep_param.is_some() {
        return run_sweep(&options, &source);
    }

    if options.pdf.is_some() {
        // %%Title:/%%For: DSC header comments -> the PDF's /Info dict
        // (issue #8), so a real reader (Kindle, Books, ...) shows an
        // authored title instead of a bare filename. Only meaningful
        // for an actual source file/stdin, not -e's inline snippets.
        let (title, author) = pscat::pdf::scan_document_info(&source);
        interp.gfx_mut().set_pdf_info(title, author);
    }

    if options.headless
        || options.png.is_some()
        || options.svg.is_some()
        || options.pdf.is_some()
        || options.lint
    {
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
    if options.lint {
        report_lint(interp, options);
    }
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

/// `--lint`: print findings to stderr, one per line, prefixed
/// `pscat: lint:` so a caller can grep for them (or, for `pscat-mcp`,
/// parse them back out of the child's stderr). Runs regardless of `ok`
/// — a blank page after a crash is exactly the kind of thing lint
/// should still surface — and doesn't affect the exit code, since a
/// finding is advisory, not fatal.
fn report_lint(interp: &Interp, options: &Options) {
    // The blank-page/stack-leak checks assume the run was meant to
    // produce a page — true for any file or stdin run, whether or not
    // an output format flag was also given (`--lint file.ps` with no
    // `--png` still runs a real program). Only `-e`'s snippets are the
    // "leave a result on the stack, don't necessarily draw anything"
    // exception (Codex review, PR #59: gating on the output-format
    // flags instead meant `pscat --lint file.ps` silently skipped both
    // checks whenever `--png`/`--svg`/`--pdf` weren't also given) --
    // *unless* `-e` is paired with an explicit output flag, which is a
    // real render request regardless of the source being a snippet
    // (round 7: `pscat --lint --png out.png -e 'showpage'` reported
    // clean on a blank artifact because `eval.is_none()` alone gated
    // this, ignoring that an output format was explicitly asked for).
    let render_checks = options.eval.is_none()
        || options.png.is_some()
        || options.svg.is_some()
        || options.pdf.is_some();
    let findings = interp.lint(render_checks);
    if findings.is_empty() {
        eprintln!("pscat: lint: clean");
        return;
    }
    for f in &findings {
        eprintln!("pscat: lint: [{}] {}", f.check, f.message);
    }
}

/// out.png → out-001.png (suffix before the extension).
fn numbered_path(path: &str, n: usize) -> String {
    match path.rsplit_once('.') {
        Some((stem, ext)) => format!("{stem}-{n:03}.{ext}"),
        None => format!("{path}-{n:03}"),
    }
}

/// A `--sweep-seed`/`--sweep` axis: either a list of forced RNG seeds
/// or a named PostScript value defined once per frame.
enum SweepAxis {
    Seed(Vec<i64>),
    Param(String, Vec<f64>),
}

/// Render `source` once per sweep value (issue #21). Each frame gets
/// a fresh `Interp`, exactly like a normal single render — sweeping
/// isn't more than looping the single-render path. Continues past a
/// per-frame PostScript error rather than aborting the whole sweep
/// (matching this CLI's existing partial-render-on-error philosophy:
/// `finish_headless` writes a PNG even after an error), but the
/// overall exit code is nonzero if any frame failed.
fn run_sweep(options: &Options, source: &[u8]) -> ExitCode {
    let axis = if let Some(seeds) = &options.sweep_seed {
        SweepAxis::Seed(seeds.clone())
    } else if let Some((name, values)) = &options.sweep_param {
        SweepAxis::Param(name.clone(), values.clone())
    } else {
        unreachable!("run_sweep is only called once a sweep axis is set")
    };
    let n = match &axis {
        SweepAxis::Seed(v) => v.len(),
        SweepAxis::Param(_, v) => v.len(),
    };

    let mut frames: Vec<tiny_skia::Pixmap> = Vec::with_capacity(n);
    let mut any_failed = false;
    let mut seed_ever_fired = false;

    for i in 0..n {
        let Some(mut interp) =
            Interp::with_page_scaled(options.page.0, options.page.1, options.dpi / 72.0)
        else {
            eprintln!(
                "pscat: unusable page size {}x{}",
                options.page.0, options.page.1
            );
            return ExitCode::FAILURE;
        };
        let label = match &axis {
            SweepAxis::Seed(seeds) => {
                interp.set_seed_override(Some(seeds[i]));
                format!("seed={}", seeds[i])
            }
            SweepAxis::Param(name, values) => {
                let text = format_sweep_value(values[i]);
                let preamble = format!("/{name} {text} def");
                if let Err(e) = interp.run_source(preamble.as_bytes()) {
                    eprintln!(
                        "pscat: sweep: internal error defining {name}: {}",
                        interp.error_report(&e)
                    );
                    return ExitCode::FAILURE;
                }
                format!("{name}={text}")
            }
        };

        let ok = match interp.run_source(source) {
            Ok(()) => true,
            Err(e) => {
                eprintln!("pscat: sweep: frame {}/{n} ({label}) failed:", i + 1);
                eprintln!("{}", interp.error_report(&e));
                any_failed = true;
                false
            }
        };
        if matches!(axis, SweepAxis::Seed(_)) && interp.seed_override_fired() {
            seed_ever_fired = true;
        }

        let page = {
            let gfx = interp.gfx();
            let mut pages: Vec<&tiny_skia::Pixmap> = gfx.pages().iter().collect();
            if pages.is_empty() || gfx.has_trailing_art() {
                pages.push(&gfx.pixmap);
            }
            // Non-empty by construction above (mirrors finish_headless's
            // own page-selection invariant). Only the *last* page is
            // kept: a sweep frame that itself does multiple showpages
            // collapses to its final page — multi-page sweep sources
            // aren't a target use case here (documented scope cut).
            let last = pages[pages.len() - 1];
            if options.halftone {
                pscat::halftone::screen(last)
            } else {
                last.clone()
            }
        };

        let status = if ok { "" } else { " (failed)" };
        match &options.png {
            Some(path) => println!(
                "pscat: sweep: {}/{n} {label}{status} -> {}",
                i + 1,
                numbered_path(path, i + 1)
            ),
            None => println!("pscat: sweep: {}/{n} {label}{status}", i + 1),
        }
        frames.push(page);
    }

    if matches!(axis, SweepAxis::Seed(_)) && !seed_ever_fired {
        eprintln!(
            "pscat: sweep: warning: source never called srand -- --sweep-seed had no effect, \
             every frame is identical"
        );
    }

    if let Some(path) = &options.png {
        for (i, page) in frames.iter().enumerate() {
            let out = numbered_path(path, i + 1);
            if let Err(e) = page.save_png(&out) {
                eprintln!("pscat: cannot write {out}: {e}");
                return ExitCode::FAILURE;
            }
        }
    }

    if let Some(path) = &options.contact_sheet {
        let (cols, rows) = match options.grid {
            Some((c, r)) => {
                if ((c * r) as usize) < n {
                    eprintln!(
                        "pscat: --grid {c}x{r} has only {} cells for {n} frames",
                        c * r
                    );
                    return ExitCode::FAILURE;
                }
                (c, r)
            }
            None => {
                let cols = (n as f64).sqrt().ceil() as u32;
                (cols, (n as u32).div_ceil(cols))
            }
        };
        match pscat::contact_sheet::compose(&frames, cols, rows, pscat::contact_sheet::GAP) {
            Ok(sheet) => {
                if let Err(e) = sheet.save_png(path) {
                    eprintln!("pscat: cannot write {path}: {e}");
                    return ExitCode::FAILURE;
                }
                println!("pscat: wrote {path}");
            }
            Err(msg) => {
                eprintln!("pscat: {msg}");
                return ExitCode::FAILURE;
            }
        }
    }

    if any_failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

const MAX_SWEEP: usize = 64;

/// Parse `"A:B"` / `"A:B:STEP"` (inclusive range, STEP default 1,
/// must be > 0, A <= B) or `"A,B,C,..."` (explicit list, sweep order
/// as given). Capped at `MAX_SWEEP` values — the same spirit as
/// `--page`/`--dpi`'s clamps, so a typo'd range can't drive an
/// unbounded render.
fn parse_sweep_spec(spec: &str) -> Result<Vec<f64>, String> {
    let parse_one = |s: &str| -> Result<f64, String> {
        s.trim()
            .parse()
            .map_err(|_| format!("invalid number in sweep spec {spec:?}: {s:?}"))
    };
    let values = if spec.contains(':') {
        let parts: Vec<&str> = spec.split(':').collect();
        let (a, b, step) = match parts.as_slice() {
            [a, b] => (parse_one(a)?, parse_one(b)?, 1.0),
            [a, b, s] => (parse_one(a)?, parse_one(b)?, parse_one(s)?),
            _ => {
                return Err(format!(
                    "invalid range: {spec:?} (expected A:B or A:B:STEP)"
                ));
            }
        };
        if step <= 0.0 {
            return Err(format!("invalid range: {spec:?} (STEP must be > 0)"));
        }
        if a > b {
            return Err(format!("invalid range: {spec:?} (A must be <= B)"));
        }
        let count = ((b - a) / step + 1e-9).floor() as usize + 1;
        (0..count).map(|i| a + i as f64 * step).collect()
    } else {
        spec.split(',')
            .map(parse_one)
            .collect::<Result<Vec<f64>, String>>()?
    };
    if values.is_empty() {
        return Err(format!("empty sweep: {spec:?}"));
    }
    if values.len() > MAX_SWEEP {
        return Err(format!(
            "sweep produces {} values, over the {MAX_SWEEP}-frame limit",
            values.len()
        ));
    }
    Ok(values)
}

/// Format a swept value for both the `/NAME <v> def` preamble and the
/// per-frame stdout line — an integer-valued float prints bare (`5`,
/// not `5.0`); anything else rounds to 9 decimals to hide binary
/// floating-point noise from range generation (e.g. a `0.1` step
/// landing on `0.7000000000000001`), then trims trailing zeros.
fn format_sweep_value(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e15 {
        return format!("{}", v as i64);
    }
    let s = format!("{v:.9}");
    let s = s.trim_end_matches('0');
    s.trim_end_matches('.').to_string()
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
        lint: false,
        sweep_seed: None,
        sweep_param: None,
        contact_sheet: None,
        grid: None,
    };
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                print_usage();
                std::process::exit(0);
            }
            "--fonts" => {
                for name in pscat::font::available_fonts() {
                    println!("{name}");
                }
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
            "--lint" => options.lint = true,
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
            "--sweep-seed" => {
                let spec = args.next().ok_or("missing SPEC after --sweep-seed")?;
                let values = parse_sweep_spec(&spec)?;
                let seeds = values
                    .iter()
                    .map(|&v| {
                        if v.fract() == 0.0 {
                            Ok(v as i64)
                        } else {
                            Err(format!("--sweep-seed values must be integers, got {v}"))
                        }
                    })
                    .collect::<Result<Vec<i64>, String>>()?;
                options.sweep_seed = Some(seeds);
            }
            "--sweep" => {
                let spec = args.next().ok_or("missing NAME=SPEC after --sweep")?;
                let (name, rest) = spec
                    .split_once('=')
                    .ok_or_else(|| format!("invalid --sweep value: {spec} (expected NAME=SPEC)"))?;
                let valid_name = name.starts_with(|c: char| c.is_ascii_alphabetic())
                    && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
                if !valid_name {
                    return Err(format!(
                        "invalid --sweep name: {name:?} (expected an identifier)"
                    ));
                }
                let values = parse_sweep_spec(rest)?;
                options.sweep_param = Some((name.to_string(), values));
            }
            "--contact-sheet" => {
                options.contact_sheet =
                    Some(args.next().ok_or("missing path after --contact-sheet")?);
            }
            "--grid" => {
                let spec = args.next().ok_or("missing COLSxROWS after --grid")?;
                let (c, r): (u32, u32) = spec
                    .split_once('x')
                    .and_then(|(c, r)| Some((c.parse().ok()?, r.parse().ok()?)))
                    .ok_or_else(|| format!("invalid --grid value: {spec} (expected COLSxROWS)"))?;
                // Bounded by MAX_SWEEP on each axis, not just nonzero: a
                // grid axis larger than the frame cap can never be
                // useful, and this keeps `cols * rows` (checked against
                // the frame count in run_sweep) from overflowing u32 on
                // a fat-fingered `--grid 70000x70000`.
                if !(1..=MAX_SWEEP as u32).contains(&c) || !(1..=MAX_SWEEP as u32).contains(&r) {
                    return Err(format!(
                        "--grid dimensions must be 1..{MAX_SWEEP}, got {spec}"
                    ));
                }
                options.grid = Some((c, r));
            }
            // A bare `-` is the stdin pseudo-file, not an option.
            _ if arg.starts_with('-') && arg != "-" => {
                return Err(format!("unknown option: {arg}"));
            }
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
    println!("usage: pscat [options] [file.ps | -]");
    println!();
    println!("Runs file.ps in a live window (watch it draw), or a REPL if no file.");
    println!("'-' reads the program from stdin (use with --png/--svg/--pdf/--headless).");
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
    println!("      --lint          check the finished run for common mistakes (implies");
    println!("                      --headless); a blank page, an unbalanced gsave, stuff");
    println!("                      left on the stack — printed to stderr, doesn't affect");
    println!("                      the exit code");
    println!("      --fonts         list every findfont-reachable face and alias, then exit");
    println!();
    println!("Sweeps (render file.ps once per value, needing --png and/or --contact-sheet):");
    println!("      --sweep-seed SPEC   reseed with each value in turn (A:B, A:B:STEP, or");
    println!("                          A,B,C); overrides every srand call, so it works on");
    println!("                          found art unmodified");
    println!("      --sweep NAME=SPEC   define /NAME to each value in turn before running;");
    println!("                          the source must look it up itself, e.g.");
    println!("                          /NAME where {{ pop NAME }} {{ 0 }} ifelse");
    println!("      --contact-sheet PATH   composite every sweep frame into one grid PNG");
    println!("      --grid COLSxROWS    grid shape for --contact-sheet (default: square-ish)");
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

#[cfg(test)]
mod sweep_tests {
    use super::*;

    #[test]
    fn comma_list_parses_in_order() {
        assert_eq!(parse_sweep_spec("4,1,9").unwrap(), vec![4.0, 1.0, 9.0]);
    }

    #[test]
    fn range_defaults_to_step_one_and_is_inclusive() {
        assert_eq!(parse_sweep_spec("1:4").unwrap(), vec![1.0, 2.0, 3.0, 4.0]);
    }

    #[test]
    fn range_with_explicit_step() {
        assert_eq!(
            parse_sweep_spec("0:1:0.25").unwrap(),
            vec![0.0, 0.25, 0.5, 0.75, 1.0]
        );
    }

    #[test]
    fn range_rejects_zero_or_negative_step() {
        assert!(parse_sweep_spec("0:1:0").is_err());
        assert!(parse_sweep_spec("0:1:-1").is_err());
    }

    #[test]
    fn range_rejects_a_greater_than_b() {
        assert!(parse_sweep_spec("5:1").is_err());
    }

    #[test]
    fn spec_over_the_cap_errors() {
        let spec = format!("0:{}", MAX_SWEEP);
        assert!(parse_sweep_spec(&spec).is_err());
    }

    #[test]
    fn empty_and_garbage_specs_error() {
        assert!(parse_sweep_spec("").is_err());
        assert!(parse_sweep_spec("abc").is_err());
    }

    #[test]
    fn integer_valued_floats_format_bare() {
        assert_eq!(format_sweep_value(5.0), "5");
        assert_eq!(format_sweep_value(-3.0), "-3");
    }

    #[test]
    fn fractional_values_format_and_trim() {
        assert_eq!(format_sweep_value(0.5), "0.5");
        // 7 * 0.1 lands on 0.7000000000000001 in f64 -- must not leak
        // that noise into the generated PostScript or the stdout line.
        let noisy = 7.0 * 0.1;
        assert_eq!(format_sweep_value(noisy), "0.7");
    }
}
