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
    sweep_param: Option<(String, Vec<SweepValue>)>,
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
        if options.grid.is_some() && options.contact_sheet.is_none() {
            // --grid shapes the contact sheet; nothing reads it
            // without one (cross-model review, PR #66: this used to
            // validate cleanly and then silently do nothing).
            eprintln!("pscat: --grid needs --contact-sheet");
            return ExitCode::FAILURE;
        }
        // Read (and dispatch) before constructing the normal-path
        // Interp below -- see read_source's doc comment for why.
        let path = options.file.as_deref().expect("checked above");
        return match read_source(path) {
            Ok(source) => run_sweep(&options, &source),
            Err(e) => {
                eprintln!("pscat: {e}");
                ExitCode::FAILURE
            }
        };
    } else if options.contact_sheet.is_some() || options.grid.is_some() {
        // Neither flag does anything without a sweep axis (cross-model
        // review, PR #66: `--contact-sheet` with no `--sweep-seed`/
        // `--sweep` silently exited clean without writing anything).
        eprintln!("pscat: --contact-sheet/--grid need --sweep-seed or --sweep");
        return ExitCode::FAILURE;
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
    let source = match read_source(path) {
        Ok(source) => source,
        Err(e) => {
            eprintln!("pscat: {e}");
            return ExitCode::FAILURE;
        }
    };

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

/// `path`'s program bytes: stdin for `-`, the file otherwise —
/// pipe-friendly for scripts and agents (`generate | pscat --png
/// out.png -`). Factored out so the sweep dispatch (`main`) can read
/// the source *before* constructing the normal-path `Interp`, rather
/// than after — cross-model review (PR #66) caught that constructing
/// it first and only branching to `run_sweep` afterward kept an
/// unused canvas (up to 256MB at max `--page`) alive for the sweep's
/// entire run, on top of the frames it streams itself.
fn read_source(path: &str) -> Result<Vec<u8>, String> {
    if path == "-" {
        let mut buf = Vec::new();
        io::Read::read_to_end(&mut io::stdin().lock(), &mut buf)
            .map_err(|e| format!("cannot read stdin: {e}"))?;
        Ok(buf)
    } else {
        std::fs::read(path).map_err(|e| format!("cannot read {path}: {e}"))
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

/// A single swept `--sweep NAME=` value. `Decimal(n, scale)` means
/// `n / 10^scale` and is how *every* decimal literal — plain ("5",
/// "-3.25", "9007199254740993", "0.0000000001") or scientific
/// ("1e-20") — is represented: exact integer arithmetic end to end,
/// both parsing a list and generating a range, so it can neither
/// drift (a `0.1` step never lands on `0.7000000000000001`) nor lose
/// precision (a huge integer literal never rounds to its nearest
/// `f64`, `1e-20` never rounds to `0`). Only a literal past
/// [`parse_decimal_exact`]'s own precision cap, or "inf"/"nan", falls
/// back to `Float`, printed via Rust's own shortest-round-trip
/// `Display`. This replaced an all-`f64` first draft after
/// a cross-model review, empirically running the binary, found three
/// distinct correctness bugs traceable to that single design choice
/// (see NOTES.md's issue #21 entry) — worth fixing at the root rather
/// than patching each symptom.
#[derive(Clone, Copy, Debug, PartialEq)]
enum SweepValue {
    Decimal(i128, u32),
    Float(f64),
}

/// A `--sweep-seed`/`--sweep` axis: either a list of forced RNG seeds
/// or a named PostScript value defined once per frame.
enum SweepAxis {
    Seed(Vec<i64>),
    Param(String, Vec<SweepValue>),
}

/// Render `source` once per sweep value (issue #21). Each frame gets
/// a fresh `Interp`, exactly like a normal single render — sweeping
/// isn't more than looping the single-render path. Continues past a
/// per-frame PostScript error rather than aborting the whole sweep
/// (matching this CLI's existing partial-render-on-error philosophy:
/// `finish_headless` writes a PNG even after an error), but the
/// overall exit code is nonzero if any frame failed.
///
/// Frames are streamed, not accumulated: `--png` writes each one as
/// it renders, and `--contact-sheet` blits each one straight into a
/// sheet allocated *before* the loop starts. Holding all N full-
/// resolution frames in memory at once (a first draft did exactly
/// that) was flagged in cross-model review — at `--dpi 300` even the
/// default page is ~34MB/frame, so 64 frames is >2GB before the first
/// byte hits disk, and an oversized `--contact-sheet` was only
/// rejected *after* paying for every frame instead of before any of
/// them rendered.
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

    // Every frame renders at this same pixel size (a probe Interp,
    // not a duplicated copy of with_page_scaled's point->pixel
    // formula) -- needed up front to size the contact sheet before
    // any frame actually runs.
    let Some(probe) = Interp::with_page_scaled(options.page.0, options.page.1, options.dpi / 72.0)
    else {
        eprintln!(
            "pscat: unusable page size {}x{}",
            options.page.0, options.page.1
        );
        return ExitCode::FAILURE;
    };
    let (cell_w, cell_h) = (probe.gfx().pixmap.width(), probe.gfx().pixmap.height());
    drop(probe);

    let mut sheet = match &options.contact_sheet {
        None => None,
        Some(_) => {
            let (cols, rows) = match options.grid {
                Some((c, r)) => {
                    if (c as usize) * (r as usize) < n {
                        eprintln!(
                            "pscat: --grid {c}x{r} has only {} cells for {n} frames",
                            c as usize * r as usize
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
            match pscat::contact_sheet::new_sheet(
                cols,
                rows,
                cell_w,
                cell_h,
                pscat::contact_sheet::GAP,
            ) {
                Ok(s) => Some((s, cols)),
                Err(msg) => {
                    eprintln!("pscat: {msg}");
                    return ExitCode::FAILURE;
                }
            }
        }
    };

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
                if options.pstack_on_error {
                    print_pstack(&interp);
                }
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
            Some(path) => {
                let out = numbered_path(path, i + 1);
                if let Err(e) = page.save_png(&out) {
                    eprintln!("pscat: cannot write {out}: {e}");
                    return ExitCode::FAILURE;
                }
                println!("pscat: sweep: {}/{n} {label}{status} -> {out}", i + 1);
            }
            None => println!("pscat: sweep: {}/{n} {label}{status}", i + 1),
        }
        if let Some((sheet, cols)) = sheet.as_mut() {
            pscat::contact_sheet::blit_cell(sheet, *cols, pscat::contact_sheet::GAP, i, &page);
        }
    }

    if matches!(axis, SweepAxis::Seed(_)) && !seed_ever_fired {
        eprintln!(
            "pscat: sweep: warning: source never called srand -- --sweep-seed had no effect, \
             every frame is identical"
        );
    }

    if let (Some((sheet, _)), Some(path)) = (&sheet, &options.contact_sheet) {
        if let Err(e) = sheet.save_png(path) {
            eprintln!("pscat: cannot write {path}: {e}");
            return ExitCode::FAILURE;
        }
        println!("pscat: wrote {path}");
    }

    if any_failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

const MAX_SWEEP: usize = 64;

/// Read a decimal literal — plain ("5", "-3.25", "9007199254740993",
/// "0.0000000001") or scientific ("1e-20", "9.999999999e-1") — as an
/// exact `(numerator, scale)` pair where the value is `numerator /
/// 10^scale`. Not "inf"/"nan": those have no such representation.
/// This is what lets [`parse_sweep_spec`] generate a range with pure
/// integer arithmetic and format a value exactly, instead of
/// accumulating binary floating-point error the way repeatedly
/// computing `a + i as f64 * step` does, or losing precision the way
/// routing a literal through `f64` does (a round-2 cross-model review
/// caught `--sweep X=1e-20` silently becoming `/X 0 def` when
/// scientific notation still fell back to `f64` here — round 3, PR
/// #66). A resulting scale/shift past 30 digits falls back to `None`
/// (the caller's `f64` path) rather than overflowing `i128` — no
/// realistic CLI literal needs more precision than that.
fn parse_decimal_exact(s: &str) -> Option<(i128, u32)> {
    let s = s.trim();
    let (neg, s) = match s.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, s.strip_prefix('+').unwrap_or(s)),
    };
    let (mantissa, exp) = match s.find(['e', 'E']) {
        Some(i) => (&s[..i], s[i + 1..].parse::<i32>().ok()?),
        None => (s, 0i32),
    };
    let (int_part, frac_part) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    if int_part.is_empty() && frac_part.is_empty() {
        return None;
    }
    if frac_part.len() > 30
        || !int_part.bytes().all(|b| b.is_ascii_digit())
        || !frac_part.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    let digits = format!("{int_part}{frac_part}");
    let magnitude: i128 = if digits.is_empty() {
        0
    } else {
        digits.parse().ok()?
    };
    // value = digits * 10^(exp - frac_part.len()); a non-negative
    // shift multiplies the magnitude up (scale 0), a negative one
    // becomes the fractional scale directly.
    let shift = exp - frac_part.len() as i32;
    let (magnitude, scale) = if shift >= 0 {
        if shift > 30 {
            return None;
        }
        (magnitude.checked_mul(10i128.checked_pow(shift as u32)?)?, 0)
    } else {
        let scale = shift.unsigned_abs();
        if scale > 30 {
            return None;
        }
        (magnitude, scale)
    };
    Some((if neg { -magnitude } else { magnitude }, scale))
}

/// The exact inverse of [`parse_decimal_exact`]: `n / 10^scale` as a
/// decimal string, trailing zeros trimmed, no decimal point at all
/// for an integer value.
fn format_decimal(n: i128, scale: u32) -> String {
    if scale == 0 {
        return n.to_string();
    }
    let neg = n < 0;
    let mag = n.unsigned_abs();
    let divisor = 10u128.pow(scale);
    let (int_part, frac_part) = (mag / divisor, mag % divisor);
    let frac_str = format!("{frac_part:0width$}", width = scale as usize);
    let frac_str = frac_str.trim_end_matches('0');
    let sign = if neg && (int_part != 0 || frac_part != 0) {
        "-"
    } else {
        ""
    };
    if frac_str.is_empty() {
        format!("{sign}{int_part}")
    } else {
        format!("{sign}{int_part}.{frac_str}")
    }
}

/// Parse `"A:B"` / `"A:B:STEP"` (inclusive range, STEP default 1,
/// must be > 0, A <= B) or `"A,B,C,..."` (explicit list, sweep order
/// as given) for `--sweep NAME=`. Every value that parses as a
/// decimal literal, plain or scientific ([`parse_decimal_exact`]),
/// generates/prints exactly via integer arithmetic; only a literal
/// past that function's own precision cap falls back to `f64`,
/// rejecting non-finite values ("inf"/"nan") so a spec like `0:inf:1`
/// errors instead of looping forever.
///
/// The frame count is bounded *inside* the generating loop (checked
/// before every push, both here and in the exact-integer path), never
/// precomputed via division-then-cast — a `0:1000000000` spec, or a
/// seed range spanning the full `i64` domain in [`parse_seed_spec`],
/// used to compute that count as a `usize` first and could overflow
/// it (`usize::MAX + 1` panics) or allocate a huge `Vec` before the
/// `MAX_SWEEP` check ever ran (cross-model review, PR #66, on both
/// counts).
///
/// `--sweep-seed` does not use this — see `parse_seed_spec`, which
/// stays in native `i64`/`i128` throughout for the same reason this
/// function prefers exact decimals: `f64` loses integer distinctness
/// above 2^53 and silently saturates out-of-range values on an `as
/// i64` cast.
fn parse_sweep_spec(spec: &str) -> Result<Vec<SweepValue>, String> {
    let bounded_push = |values: &mut Vec<SweepValue>, v: SweepValue| -> Result<(), String> {
        if values.len() >= MAX_SWEEP {
            return Err(format!(
                "sweep produces more than {MAX_SWEEP} values, over the {MAX_SWEEP}-frame limit"
            ));
        }
        values.push(v);
        Ok(())
    };
    let values = if spec.contains(':') {
        let parts: Vec<&str> = spec.split(':').collect();
        let (a_str, b_str, step_str) = match parts.as_slice() {
            [a, b] => (*a, *b, "1"),
            [a, b, s] => (*a, *b, *s),
            _ => {
                return Err(format!(
                    "invalid range: {spec:?} (expected A:B or A:B:STEP)"
                ));
            }
        };
        match (
            parse_decimal_exact(a_str),
            parse_decimal_exact(b_str),
            parse_decimal_exact(step_str),
        ) {
            (Some((a_n, a_s)), Some((b_n, b_s)), Some((step_n, step_s))) => {
                let scale = a_s.max(b_s).max(step_s);
                // checked_mul, not `*`: aligning very large magnitudes
                // to a much finer scale (e.g. an 18-digit A next to a
                // 30-decimal-place STEP) could in principle overflow
                // i128 -- reject cleanly rather than panic on it.
                let rescale = |n: i128, s: u32| n.checked_mul(10i128.pow(scale - s));
                let (Some(a), Some(b), Some(step)) = (
                    rescale(a_n, a_s),
                    rescale(b_n, b_s),
                    rescale(step_n, step_s),
                ) else {
                    return Err(format!(
                        "invalid range: {spec:?} (values too large/precise to combine exactly)"
                    ));
                };
                if step <= 0 {
                    return Err(format!("invalid range: {spec:?} (STEP must be > 0)"));
                }
                if a > b {
                    return Err(format!("invalid range: {spec:?} (A must be <= B)"));
                }
                let mut values = Vec::new();
                let mut v = a;
                while v <= b {
                    bounded_push(&mut values, SweepValue::Decimal(v, scale))?;
                    // checked_add: v is already within [a, b] (both
                    // valid i128), and step > 0 was just validated, so
                    // this can only fail by running off i128's own
                    // top end -- stop instead of panicking; the values
                    // collected so far still stand.
                    match v.checked_add(step) {
                        Some(next) => v = next,
                        None => break,
                    }
                }
                values
            }
            _ => {
                let parse_finite = |s: &str| -> Result<f64, String> {
                    s.trim()
                        .parse::<f64>()
                        .ok()
                        .filter(|v: &f64| v.is_finite())
                        .ok_or_else(|| format!("invalid number in sweep spec {spec:?}: {s:?}"))
                };
                let (a, b, step) = (
                    parse_finite(a_str)?,
                    parse_finite(b_str)?,
                    parse_finite(step_str)?,
                );
                if step <= 0.0 {
                    return Err(format!("invalid range: {spec:?} (STEP must be > 0)"));
                }
                if a > b {
                    return Err(format!("invalid range: {spec:?} (A must be <= B)"));
                }
                // No tolerance here: the plain-decimal drift this
                // once existed to hide (e.g. a `0.1` step landing on
                // `0.7000000000000001`) is handled exactly above now
                // -- this loop only runs for a genuinely non-decimal
                // literal (scientific notation past the 30-digit-
                // shift cap). A tolerance instead let a scientific
                // range generate a value past its declared bound, and
                // near f64::MAX could overflow `b + tol` to infinity
                // (round-3 cross-model review, PR #66).
                let mut values = Vec::new();
                let mut i: usize = 0;
                loop {
                    let v = a + i as f64 * step;
                    if v > b {
                        break;
                    }
                    bounded_push(&mut values, SweepValue::Float(v))?;
                    i += 1;
                }
                values
            }
        }
    } else {
        let mut values = Vec::new();
        for s in spec.split(',') {
            let v = if let Some((n, scale)) = parse_decimal_exact(s) {
                SweepValue::Decimal(n, scale)
            } else {
                SweepValue::Float(
                    s.trim()
                        .parse::<f64>()
                        .map_err(|_| format!("invalid number in sweep spec {spec:?}: {s:?}"))?,
                )
            };
            values.push(v);
        }
        values
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

/// Same grammar as [`parse_sweep_spec`] but native `i64`/`i128`
/// arithmetic throughout, for `--sweep-seed` (see that function's doc
/// comment for why `f64` is avoided, and why the frame count is
/// bounded inside the generating loop rather than precomputed).
fn parse_seed_spec(spec: &str) -> Result<Vec<i64>, String> {
    let parse_one = |s: &str| -> Result<i64, String> {
        s.trim()
            .parse()
            .map_err(|_| format!("invalid seed in {spec:?}: {s:?} (must be an integer)"))
    };
    let values = if spec.contains(':') {
        let parts: Vec<&str> = spec.split(':').collect();
        let (a, b, step) = match parts.as_slice() {
            [a, b] => (parse_one(a)?, parse_one(b)?, 1i64),
            [a, b, s] => (parse_one(a)?, parse_one(b)?, parse_one(s)?),
            _ => {
                return Err(format!(
                    "invalid range: {spec:?} (expected A:B or A:B:STEP)"
                ));
            }
        };
        if step <= 0 {
            return Err(format!("invalid range: {spec:?} (STEP must be > 0)"));
        }
        if a > b {
            return Err(format!("invalid range: {spec:?} (A must be <= B)"));
        }
        let mut values = Vec::new();
        let mut v: i128 = a.into();
        let (b128, step128): (i128, i128) = (b.into(), step.into());
        while v <= b128 {
            if values.len() >= MAX_SWEEP {
                return Err(format!(
                    "sweep produces more than {MAX_SWEEP} values, over the {MAX_SWEEP}-frame limit"
                ));
            }
            // Safe: the loop invariant keeps v within [a, b], both
            // valid i64 (checked above), so this never truncates.
            values.push(v as i64);
            v += step128;
        }
        values
    } else {
        spec.split(',')
            .map(parse_one)
            .collect::<Result<Vec<i64>, String>>()?
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

/// Format a swept `--sweep NAME=` value for both the `/NAME <v> def`
/// preamble and the per-frame stdout line. `Decimal` prints exactly
/// via [`format_decimal`] (an integer-valued decimal, e.g. `5.00`,
/// prints bare as `5`). `Float` (the scientific-notation fallback
/// path only) rounds to 12 decimals to hide binary noise, same as
/// `Decimal` never needs to.
fn format_sweep_value(v: SweepValue) -> String {
    match v {
        SweepValue::Decimal(n, scale) => format_decimal(n, scale),
        // Reached only when a literal overflows parse_decimal_exact's
        // own 30-digit-shift cap (a magnitude/precision far beyond
        // any realistic sweep parameter) -- Rust's shortest-round-
        // trip Display is the honest value for that, not a fixed
        // rounding that would just reintroduce round 3's bug one
        // level further out.
        SweepValue::Float(v) => format!("{v}"),
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
                options.sweep_seed = Some(parse_seed_spec(&spec)?);
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

    fn dec(v: SweepValue) -> String {
        format_sweep_value(v)
    }

    #[test]
    fn decimal_exact_reads_plain_and_scientific_literals() {
        assert_eq!(parse_decimal_exact("5"), Some((5, 0)));
        assert_eq!(parse_decimal_exact("-3.25"), Some((-325, 2)));
        assert_eq!(parse_decimal_exact(".5"), Some((5, 1)));
        assert_eq!(
            parse_decimal_exact("9007199254740993"),
            Some((9007199254740993, 0))
        );
        assert_eq!(parse_decimal_exact("1e10"), Some((10000000000, 0)));
        assert_eq!(parse_decimal_exact("1e-20"), Some((1, 20)));
        assert_eq!(
            parse_decimal_exact("-9.999999999e-1"),
            Some((-9999999999, 10))
        );
        // "inf"/"nan" and non-numeric text have no such representation
        // -- the caller falls back to f64 for these.
        assert_eq!(parse_decimal_exact("inf"), None);
        assert_eq!(parse_decimal_exact("abc"), None);
    }

    #[test]
    fn decimal_formatting_round_trips_and_trims() {
        assert_eq!(format_decimal(100, 2), "1"); // 1.00 -> bare integer
        assert_eq!(format_decimal(25, 2), "0.25");
        assert_eq!(format_decimal(-25, 2), "-0.25");
        assert_eq!(format_decimal(1, 10), "0.0000000001");
        assert_eq!(format_decimal(5, 0), "5");
    }

    #[test]
    fn comma_list_parses_in_order() {
        let values = parse_sweep_spec("4,1,9").unwrap();
        assert_eq!(
            values,
            vec![
                SweepValue::Decimal(4, 0),
                SweepValue::Decimal(1, 0),
                SweepValue::Decimal(9, 0)
            ]
        );
    }

    #[test]
    fn range_defaults_to_step_one_and_is_inclusive() {
        let values = parse_sweep_spec("1:4").unwrap();
        let texts: Vec<String> = values.into_iter().map(dec).collect();
        assert_eq!(texts, vec!["1", "2", "3", "4"]);
    }

    #[test]
    fn range_with_explicit_step_is_exact_no_float_drift() {
        // 0.1-style steps are exactly where the old f64 implementation
        // drifted (e.g. landing on 0.7000000000000001); exact decimal
        // arithmetic must not.
        let values = parse_sweep_spec("0:1:0.25").unwrap();
        let texts: Vec<String> = values.into_iter().map(dec).collect();
        assert_eq!(texts, vec!["0", "0.25", "0.5", "0.75", "1"]);
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

    /// Regression (cross-model review round 1, PR #66): a huge range
    /// used to eagerly `collect()` before the cap check ran, so a
    /// short spec like this could try to allocate a billion-element
    /// `Vec` instead of erroring immediately. This must return quickly.
    #[test]
    fn a_huge_range_is_rejected_without_allocating_it() {
        assert!(parse_sweep_spec("0:1000000000").is_err());
        assert!(parse_seed_spec("0:1000000000").is_err());
    }

    /// Regression (cross-model review round 2, PR #66): the frame
    /// count used to be precomputed via `((b-a)/step) as usize + 1`,
    /// which panics on `usize::MAX + 1` for a non-finite or maximally
    /// wide range instead of erroring. Must not panic.
    #[test]
    fn non_finite_and_extreme_ranges_error_instead_of_panicking() {
        assert!(parse_sweep_spec("0:inf:1").is_err());
        assert!(parse_sweep_spec("0:nan:1").is_err());
        assert!(parse_seed_spec("-9223372036854775808:9223372036854775807").is_err());
    }

    /// Regression (cross-model review round 2, PR #66): a fixed 1e-9
    /// tolerance on the old float-count formula could include a value
    /// past the declared upper bound (e.g. this spec used to also
    /// generate `X=1`, past the documented inclusive B=0.9999999999).
    /// Exact decimal arithmetic has no tolerance to get wrong.
    #[test]
    fn range_never_generates_past_its_upper_bound() {
        let values = parse_sweep_spec("0:0.9999999999:1").unwrap();
        assert_eq!(values, vec![SweepValue::Decimal(0, 10)]);
    }

    /// Regression (cross-model review round 2, PR #66): combining
    /// very large magnitudes at very different decimal scales could
    /// overflow `i128` during rescale -- must error, not panic.
    #[test]
    fn extreme_scale_mismatch_errors_instead_of_overflowing() {
        assert!(
            parse_sweep_spec("100000000000000000000000000000:1:0.000000000000000000000000000001")
                .is_err()
        );
    }

    /// Regression (cross-model review round 3, PR #66): scientific
    /// notation used to fall back to `f64` and then get truncated by
    /// a fixed 12-decimal rounding on the way out --
    /// `--sweep X=1e-20` silently became `/X 0 def`. Scientific
    /// notation is now read exactly by `parse_decimal_exact`, same as
    /// a plain decimal.
    #[test]
    fn scientific_notation_literal_preserves_exact_value() {
        let values = parse_sweep_spec("1e-20").unwrap();
        assert_eq!(values, vec![SweepValue::Decimal(1, 20)]);
        assert_eq!(dec(values[0]), "0.00000000000000000001");
    }

    /// Regression (cross-model review round 3, PR #66): a scientific-
    /// notation range used to fall back to the `f64` range path,
    /// whose tolerance could admit a value past the declared upper
    /// bound the same way a plain-decimal range once could. Now
    /// routed through the same exact integer path as a plain decimal.
    #[test]
    fn scientific_notation_range_never_generates_past_its_upper_bound() {
        let values = parse_sweep_spec("0e0:9.999999999e-1:1e0").unwrap();
        assert_eq!(values, vec![SweepValue::Decimal(0, 10)]);
    }

    #[test]
    fn empty_and_garbage_specs_error() {
        assert!(parse_sweep_spec("").is_err());
        assert!(parse_sweep_spec("abc").is_err());
    }

    #[test]
    fn integer_valued_decimals_format_bare() {
        assert_eq!(dec(SweepValue::Decimal(5, 0)), "5");
        assert_eq!(dec(SweepValue::Decimal(-3, 0)), "-3");
    }

    /// Regression (cross-model review round 2, PR #66): every
    /// parameter value -- list *or* range -- used to route through
    /// `f64`, so an integer above 2^53 silently changed value
    /// (`9007199254740993` became `9007199254740992`). Decimal values
    /// never touch `f64` at all now.
    #[test]
    fn huge_integer_literal_preserves_exact_value() {
        let values = parse_sweep_spec("9007199254740993").unwrap();
        assert_eq!(values, vec![SweepValue::Decimal(9007199254740993, 0)]);
        assert_eq!(dec(values[0]), "9007199254740993");
    }

    /// Regression (cross-model review round 1, PR #66): a literal
    /// value used to go through the same rounding as a computed
    /// range, so `--sweep X=0.0000000001` silently became `/X 0 def`.
    #[test]
    fn fractional_literal_values_print_exactly() {
        let values = parse_sweep_spec("0.0000000001").unwrap();
        assert_eq!(dec(values[0]), "0.0000000001");
    }

    #[test]
    fn seed_spec_comma_list_parses_exactly() {
        assert_eq!(parse_seed_spec("4,1,9").unwrap(), vec![4, 1, 9]);
    }

    #[test]
    fn seed_spec_range_is_inclusive() {
        assert_eq!(parse_seed_spec("1:4").unwrap(), vec![1, 2, 3, 4]);
    }

    /// Regression (cross-model review round 1, PR #66): seeds used to
    /// route through `f64`, which loses integer distinctness above
    /// 2^53 -- these two adjacent seeds used to collapse to one value.
    #[test]
    fn seed_spec_preserves_distinctness_above_2_pow_53() {
        let seeds = parse_seed_spec("9007199254740992,9007199254740993").unwrap();
        assert_eq!(seeds, vec![9007199254740992, 9007199254740993]);
        assert_ne!(seeds[0], seeds[1]);
    }

    #[test]
    fn seed_spec_rejects_non_integers() {
        assert!(parse_seed_spec("1.5,2").is_err());
    }
}
