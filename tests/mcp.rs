//! pscat-mcp integration (Stage 14): drive the real binary over stdio
//! with newline-delimited JSON-RPC, the way an MCP client would.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdout, Command, Stdio};

use serde_json::{Value, json};

struct Server {
    child: Child,
    reader: BufReader<ChildStdout>,
}

impl Server {
    fn start() -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_pscat-mcp"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn pscat-mcp");
        let reader = BufReader::new(child.stdout.take().expect("stdout"));
        Server { child, reader }
    }

    fn call(&mut self, msg: Value) -> Value {
        let stdin = self.child.stdin.as_mut().expect("stdin");
        writeln!(stdin, "{msg}").expect("write request");
        stdin.flush().expect("flush");
        let mut line = String::new();
        self.reader.read_line(&mut line).expect("read response");
        serde_json::from_str(&line).expect("valid JSON response")
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn initialized(server: &mut Server) {
    let resp = server.call(json!({
        "jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": { "protocolVersion": "2024-11-05", "capabilities": {},
                    "clientInfo": { "name": "test", "version": "0" } }
    }));
    assert_eq!(resp["result"]["serverInfo"]["name"], "pscat");
    assert_eq!(resp["result"]["protocolVersion"], "2024-11-05");
}

#[test]
fn handshake_and_tool_list() {
    let mut s = Server::start();
    initialized(&mut s);
    let resp = s.call(json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }));
    let names: Vec<&str> = resp["result"]["tools"]
        .as_array()
        .expect("tools array")
        .iter()
        .map(|t| t["name"].as_str().expect("name"))
        .collect();
    assert_eq!(
        names,
        ["render_postscript", "handwrite", "eval_postscript"],
        "the advertised tool set"
    );
    for tool in resp["result"]["tools"].as_array().unwrap() {
        assert!(
            tool["inputSchema"]["required"].is_array(),
            "every tool declares required params"
        );
    }
}

#[test]
fn render_returns_a_png_image() {
    let mut s = Server::start();
    initialized(&mut s);
    let resp = s.call(json!({
        "jsonrpc": "2.0", "id": 3, "method": "tools/call",
        "params": { "name": "render_postscript",
                    "arguments": { "source": "0 0 40 40 rectfill showpage",
                                   "width": 40, "height": 40 } }
    }));
    let content = &resp["result"]["content"][0];
    assert_eq!(content["type"], "image");
    assert_eq!(content["mimeType"], "image/png");
    // Base64 of the PNG signature: any PNG starts iVBORw0KGgo.
    let data = content["data"].as_str().expect("base64 data");
    assert!(data.starts_with("iVBORw0KGgo"), "PNG magic in base64");
    assert_eq!(resp["result"]["isError"], false);
}

#[test]
fn eval_reports_prints_and_errors() {
    let mut s = Server::start();
    initialized(&mut s);
    let resp = s.call(json!({
        "jsonrpc": "2.0", "id": 4, "method": "tools/call",
        "params": { "name": "eval_postscript",
                    "arguments": { "source": "3 4 add ==" } }
    }));
    assert_eq!(resp["result"]["isError"], false);
    assert!(
        resp["result"]["content"][0]["text"]
            .as_str()
            .expect("text")
            .contains('7')
    );
    // And a PostScript error comes back as a tool error with the
    // standard name plus the operand-stack post-mortem.
    let resp = s.call(json!({
        "jsonrpc": "2.0", "id": 5, "method": "tools/call",
        "params": { "name": "eval_postscript",
                    "arguments": { "source": "1 2 bogus" } }
    }));
    assert_eq!(resp["result"]["isError"], true);
    let text = resp["result"]["content"][0]["text"].as_str().expect("text");
    assert!(text.contains("undefined"), "error named: {text}");
    assert!(text.contains("Operand stack"), "post-mortem: {text}");
}

#[test]
fn handwrite_returns_a_note() {
    let mut s = Server::start();
    initialized(&mut s);
    let resp = s.call(json!({
        "jsonrpc": "2.0", "id": 6, "method": "tools/call",
        "params": { "name": "handwrite",
                    "arguments": { "text": "hello from mcp", "size": 24, "seed": 3 } }
    }));
    assert_eq!(
        resp["result"]["isError"], false,
        "handwrite failed: {}",
        resp["result"]["content"][0]["text"]
    );
    assert_eq!(resp["result"]["content"][0]["type"], "image");
}

#[test]
fn render_stays_silent_about_lint_when_the_page_is_clean() {
    let mut s = Server::start();
    initialized(&mut s);
    let resp = s.call(json!({
        "jsonrpc": "2.0", "id": 8, "method": "tools/call",
        "params": { "name": "render_postscript",
                    "arguments": { "source": "0 0 40 40 rectfill showpage",
                                   "width": 40, "height": 40 } }
    }));
    assert_eq!(resp["result"]["isError"], false);
    let content = resp["result"]["content"].as_array().expect("content array");
    assert!(
        content
            .iter()
            .all(|c| !c["text"].as_str().unwrap_or_default().starts_with("Lint:")),
        "no Lint block on a clean render: {content:?}"
    );
}

#[test]
fn render_surfaces_lint_findings_for_a_blank_page() {
    let mut s = Server::start();
    initialized(&mut s);
    let resp = s.call(json!({
        "jsonrpc": "2.0", "id": 9, "method": "tools/call",
        "params": { "name": "render_postscript",
                    "arguments": { "source": "showpage", "width": 40, "height": 40 } }
    }));
    assert_eq!(resp["result"]["isError"], false);
    let content = resp["result"]["content"].as_array().expect("content array");
    let lint = content
        .iter()
        .find_map(|c| c["text"].as_str().filter(|t| t.starts_with("Lint:")))
        .expect("a Lint block for a blank page");
    assert!(lint.contains("blank-page"), "lint block: {lint}");
}

#[test]
fn unknown_method_is_a_jsonrpc_error() {
    let mut s = Server::start();
    initialized(&mut s);
    let resp = s.call(json!({ "jsonrpc": "2.0", "id": 7, "method": "resources/list" }));
    assert_eq!(resp["error"]["code"], -32601);
}
