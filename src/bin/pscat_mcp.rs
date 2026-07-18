//! pscat-mcp — pscat as MCP tools (Stage 14).
//!
//! An MCP server over stdio (newline-delimited JSON-RPC 2.0) exposing
//! three tools: `render_postscript`, `handwrite`, `eval_postscript`.
//! Works with any MCP client — Claude Code, Codex, OpenClaw, Hermes;
//! registration one-liners are in the README's "For agents" section.
//!
//! Design choice: the server **shells out to the pscat CLI** (and
//! `scripts/handwrite.sh`) rather than linking the interpreter.
//! PostScript programs print to stdout (`=`, `print`, `pstack`), and
//! in-process that would corrupt the JSON-RPC channel; a subprocess
//! keeps the protocol safe, keeps the tools in lock-step with the CLI,
//! and gets crash isolation for free. The pscat binary is found next
//! to this one (same target dir); the repo root (for handwrite.sh) is
//! two levels up from there, overridable with $PSCAT_ROOT.

use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::{Value, json};

fn main() {
    let stdin = std::io::stdin();
    let mut lines = stdin.lock().lines();
    while let Some(Ok(line)) = lines.next() {
        if line.trim().is_empty() {
            continue;
        }
        let msg: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("pscat-mcp: bad JSON on stdin: {e}");
                continue;
            }
        };
        let method = msg["method"].as_str().unwrap_or_default().to_string();
        let id = msg["id"].clone();
        // Notifications (no id) get no response, per JSON-RPC.
        if id.is_null() {
            continue;
        }
        let reply = match method.as_str() {
            "initialize" => ok(id, initialize_result(&msg)),
            "ping" => ok(id, json!({})),
            "tools/list" => ok(id, json!({ "tools": tool_list() })),
            "tools/call" => match tool_call(&msg["params"]) {
                Ok(content) => ok(id, json!({ "content": content, "isError": false })),
                Err(text) => ok(
                    id,
                    json!({ "content": [text_content(&text)], "isError": true }),
                ),
            },
            _ => json!({
                "jsonrpc": "2.0", "id": id,
                "error": { "code": -32601, "message": format!("method not found: {method}") }
            }),
        };
        let mut out = std::io::stdout().lock();
        let _ = writeln!(out, "{reply}");
        let _ = out.flush();
    }
}

fn ok(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn initialize_result(msg: &Value) -> Value {
    // Echo the client's protocol version (we have no version-specific
    // behavior); fall back to a widely supported revision.
    let version = msg["params"]["protocolVersion"]
        .as_str()
        .unwrap_or("2024-11-05");
    json!({
        "protocolVersion": version,
        "capabilities": { "tools": {} },
        "serverInfo": { "name": "pscat", "version": env!("CARGO_PKG_VERSION") }
    })
}

fn text_content(text: &str) -> Value {
    json!({ "type": "text", "text": text })
}

fn image_content(png: &[u8]) -> Value {
    json!({ "type": "image", "data": base64(png), "mimeType": "image/png" })
}

fn tool_list() -> Value {
    json!([
        {
            "name": "render_postscript",
            "description": "Render a PostScript program. Returns the page(s) as PNG images \
                (default), or SVG text, or writes a PDF/other artifact to out_path. \
                Partial renders are returned even when the program errors, with the \
                error text alongside.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "source": { "type": "string", "description": "The PostScript program" },
                    "width": { "type": "integer", "description": "Page width in points (default 612)" },
                    "height": { "type": "integer", "description": "Page height in points (default 792)" },
                    "dpi": { "type": "number", "description": "Device resolution (default 72; 144 = 2x pixels)" },
                    "format": { "type": "string", "enum": ["png", "svg", "pdf"], "description": "Output format (default png)" },
                    "halftone": { "type": "boolean", "description": "Screen the raster like a mono laser printer (png only)" },
                    "out_path": { "type": "string", "description": "Also write the artifact(s) here. Required for pdf." }
                },
                "required": ["source"]
            }
        },
        {
            "name": "handwrite",
            "description": "Render text as a handwritten note (PNG): a dynamic pen-written \
                font with per-letter variation, word-wrapped, auto-sized page. \
                Lowercase letters, digits and .,-'!? only; anything else is invisible.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "text": { "type": "string", "description": "The note to write (newlines force line breaks)" },
                    "size": { "type": "number", "description": "Font size in points (default 36)" },
                    "width": { "type": "integer", "description": "Page width in points (default 612)" },
                    "paper": { "type": "string", "enum": ["plain", "ruled"], "description": "Paper style (default plain)" },
                    "ink": { "type": "string", "description": "Ink color as R,G,B in 0..1 (default 0.13,0.14,0.34)" },
                    "jitter": { "type": "number", "description": "Ink wobble; 0 = calm (default 16)" },
                    "seed": { "type": "integer", "description": "Random seed; same seed, same page (default 1985)" },
                    "dpi": { "type": "number", "description": "Render resolution (default 72)" },
                    "out_path": { "type": "string", "description": "Also write the PNG here" }
                },
                "required": ["text"]
            }
        },
        {
            "name": "eval_postscript",
            "description": "Run a PostScript program headlessly and return what it printed \
                (=, print, pstack). On error: the standard PostScript error name plus a \
                gs-style operand-stack post-mortem. The debugging loop.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "source": { "type": "string", "description": "The PostScript program" }
                },
                "required": ["source"]
            }
        }
    ])
}

fn tool_call(params: &Value) -> Result<Value, String> {
    let args = &params["arguments"];
    match params["name"].as_str().unwrap_or_default() {
        "render_postscript" => render_postscript(args),
        "handwrite" => handwrite(args),
        "eval_postscript" => eval_postscript(args),
        other => Err(format!("unknown tool: {other}")),
    }
}

// --- the tools ------------------------------------------------------

fn render_postscript(args: &Value) -> Result<Value, String> {
    let source = args["source"].as_str().ok_or("source is required")?;
    let format = args["format"].as_str().unwrap_or("png");
    let out_path = args["out_path"].as_str();
    if format == "pdf" && out_path.is_none() {
        return Err("pdf output needs out_path (a PDF is a file, not a chat message)".into());
    }
    let dir = scratch_dir()?;
    let out = out_path
        .map(PathBuf::from)
        .unwrap_or_else(|| dir.join(format!("page.{format}")));

    let mut cmd = Command::new(pscat_bin()?);
    let (w, h) = (
        args["width"].as_u64().unwrap_or(612),
        args["height"].as_u64().unwrap_or(792),
    );
    cmd.arg("--page").arg(format!("{w}x{h}"));
    if let Some(dpi) = args["dpi"].as_f64() {
        cmd.arg("--dpi").arg(dpi.to_string());
    }
    if args["halftone"].as_bool() == Some(true) {
        cmd.arg("--halftone");
    }
    cmd.arg(match format {
        "png" => "--png",
        "svg" => "--svg",
        "pdf" => "--pdf",
        other => return Err(format!("unknown format: {other}")),
    });
    cmd.arg(&out).arg("-");
    let run = pipe_through(&mut cmd, source.as_bytes())?;

    // Artifacts are written even after a PostScript error (partial
    // canvas); pass the error text along rather than hiding the render.
    let mut content = Vec::new();
    if !run.success {
        content.push(text_content(&format!("PostScript error:\n{}", run.stderr)));
    }
    let pages = collect_pages(&out);
    if pages.is_empty() {
        return Err(format!(
            "no output produced\nstdout: {}\nstderr: {}",
            run.stdout, run.stderr
        ));
    }
    for page in &pages {
        match format {
            "png" => {
                let bytes = std::fs::read(page).map_err(|e| format!("read {page:?}: {e}"))?;
                content.push(image_content(&bytes));
            }
            "svg" => {
                let text =
                    std::fs::read_to_string(page).map_err(|e| format!("read {page:?}: {e}"))?;
                content.push(text_content(&text));
            }
            _ => {}
        }
    }
    if let Some(p) = out_path {
        content.push(text_content(&format!(
            "wrote {} file(s) at {p}",
            pages.len()
        )));
    }
    Ok(Value::Array(content))
}

fn handwrite(args: &Value) -> Result<Value, String> {
    let text = args["text"].as_str().ok_or("text is required")?;
    let dir = scratch_dir()?;
    let out = match args["out_path"].as_str() {
        Some(p) => PathBuf::from(p),
        None => dir.join("note.png"),
    };

    let script = repo_root()?.join("scripts/handwrite.sh");
    let mut cmd = Command::new(&script);
    cmd.arg("-o").arg(&out);
    for (flag, key) in [
        ("--size", "size"),
        ("--width", "width"),
        ("--jitter", "jitter"),
        ("--seed", "seed"),
        ("--dpi", "dpi"),
    ] {
        if let Some(n) = args[key].as_f64() {
            // Integers render without a trailing .0 for the shell.
            let s = if n.fract() == 0.0 {
                format!("{}", n as i64)
            } else {
                n.to_string()
            };
            cmd.arg(flag).arg(s);
        }
    }
    if let Some(paper) = args["paper"].as_str() {
        cmd.arg("--paper").arg(paper);
    }
    if let Some(ink) = args["ink"].as_str() {
        cmd.arg("--ink").arg(ink);
    }
    cmd.arg(text);
    let run = pipe_through(&mut cmd, b"")?;
    if !run.success {
        return Err(format!("handwrite failed:\n{}", run.stderr));
    }
    let bytes = std::fs::read(&out).map_err(|e| format!("read {out:?}: {e}"))?;
    let mut content = vec![image_content(&bytes)];
    if args["out_path"].is_string() {
        content.push(text_content(&format!("wrote {}", out.display())));
    }
    Ok(Value::Array(content))
}

fn eval_postscript(args: &Value) -> Result<Value, String> {
    let source = args["source"].as_str().ok_or("source is required")?;
    let mut cmd = Command::new(pscat_bin()?);
    cmd.arg("--headless").arg("--pstack-on-error").arg("-");
    let run = pipe_through(&mut cmd, source.as_bytes())?;
    let mut report = String::new();
    if !run.stdout.is_empty() {
        report.push_str(&run.stdout);
    }
    if !run.success {
        if !report.is_empty() && !report.ends_with('\n') {
            report.push('\n');
        }
        report.push_str(&run.stderr);
    }
    if report.is_empty() {
        report.push_str("(ran clean, printed nothing)");
    }
    if run.success {
        Ok(Value::Array(vec![text_content(&report)]))
    } else {
        Err(report)
    }
}

// --- plumbing -------------------------------------------------------

struct RunOutput {
    success: bool,
    stdout: String,
    stderr: String,
}

fn pipe_through(cmd: &mut Command, stdin: &[u8]) -> Result<RunOutput, String> {
    let mut child = cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("cannot run {:?}: {e}", cmd.get_program()))?;
    child
        .stdin
        .as_mut()
        .expect("piped stdin")
        .write_all(stdin)
        .map_err(|e| format!("write to child: {e}"))?;
    let out = child
        .wait_with_output()
        .map_err(|e| format!("wait for child: {e}"))?;
    Ok(RunOutput {
        success: out.status.success(),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    })
}

/// The pscat binary lives next to this one.
fn pscat_bin() -> Result<PathBuf, String> {
    let me = std::env::current_exe().map_err(|e| format!("current_exe: {e}"))?;
    let bin = me.with_file_name("pscat");
    if bin.exists() {
        Ok(bin)
    } else {
        Err(format!("pscat binary not found at {}", bin.display()))
    }
}

/// The repo root: $PSCAT_ROOT, or two levels up from the binary
/// (target/<profile>/pscat-mcp in a cargo layout).
fn repo_root() -> Result<PathBuf, String> {
    if let Ok(root) = std::env::var("PSCAT_ROOT") {
        return Ok(PathBuf::from(root));
    }
    let me = std::env::current_exe().map_err(|e| format!("current_exe: {e}"))?;
    me.ancestors()
        .nth(3)
        .map(Path::to_path_buf)
        .filter(|r| r.join("scripts/handwrite.sh").exists())
        .ok_or_else(|| {
            "repo root not found (set PSCAT_ROOT to the pscat checkout for handwrite)".into()
        })
}

fn scratch_dir() -> Result<PathBuf, String> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
        "pscat-mcp-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).map_err(|e| format!("scratch dir: {e}"))?;
    Ok(dir)
}

/// Single-page output keeps the exact path; multi-page renders write
/// out-001.ext, out-002.ext, ... (the CLI's numbered_path convention).
fn collect_pages(out: &Path) -> Vec<PathBuf> {
    if out.exists() {
        return vec![out.to_path_buf()];
    }
    let (stem, ext) = (
        out.file_stem().unwrap_or_default().to_string_lossy(),
        out.extension().unwrap_or_default().to_string_lossy(),
    );
    let mut pages = Vec::new();
    for n in 1.. {
        let numbered = out.with_file_name(format!("{stem}-{n:03}.{ext}"));
        if !numbered.exists() {
            break;
        }
        pages.push(numbered);
    }
    pages
}

fn base64(data: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = u32::from(b[0]) << 16 | u32::from(b[1]) << 8 | u32::from(b[2]);
        out.push(ALPHABET[(n >> 18 & 63) as usize] as char);
        out.push(ALPHABET[(n >> 12 & 63) as usize] as char);
        out.push(if chunk.len() > 1 {
            ALPHABET[(n >> 6 & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}
