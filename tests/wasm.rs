//! The Stage 16 browser build: compile the interpreter to
//! wasm32-unknown-unknown and drive it under node through the real
//! JS library (web/pscat.js) — instantiate, run a program, count
//! pixels, exercise the error path. Skips gracefully (gs-test style)
//! when the wasm target or node isn't installed.

use std::process::Command;

fn have(cmd: &str, args: &[&str], needle: &str) -> bool {
    Command::new(cmd)
        .args(args)
        .output()
        .is_ok_and(|o| o.status.success() && String::from_utf8_lossy(&o.stdout).contains(needle))
}

#[test]
fn wasm_build_runs_under_node() {
    if !have(
        "rustup",
        &["target", "list", "--installed"],
        "wasm32-unknown-unknown",
    ) {
        eprintln!("skipping wasm test: wasm32-unknown-unknown target not installed");
        return;
    }
    if !Command::new("node")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
    {
        eprintln!("skipping wasm test: node not installed");
        return;
    }

    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let build = Command::new(root.join("scripts/build_wasm.sh"))
        .output()
        .expect("run build_wasm.sh");
    assert!(
        build.status.success(),
        "wasm build failed:\n{}",
        String::from_utf8_lossy(&build.stderr)
    );

    let driver = format!(
        r#"
import {{ readFileSync }} from 'node:fs';
import {{ Pscat }} from 'file://{lib}';
const ps = await Pscat.instantiate(readFileSync('{wasm}'));
const rc = ps.run('0.2 0.4 0.9 setrgbcolor 10 10 80 80 rectfill showpage', 100, 100);
if (rc !== 0) {{ console.error('run failed: ' + ps.error); process.exit(1); }}
if (ps.width !== 100 || ps.height !== 100) {{ console.error('bad page size'); process.exit(1); }}
const px = ps.pixels();
let blue = 0;
for (let i = 0; i < px.length; i += 4) if (px[i + 2] > 180 && px[i] < 120) blue++;
if (blue < 3000) {{ console.error('too few blue pixels: ' + blue); process.exit(1); }}
// The error path: report comes back, the session object stays usable.
const rc2 = ps.run('1 2 bogus', 100, 100);
if (rc2 !== -1) {{ console.error('expected error status, got ' + rc2); process.exit(1); }}
if (!ps.error.includes('undefined')) {{ console.error('error not named: ' + ps.error); process.exit(1); }}
if (ps.run('0 0 50 50 rectfill', 100, 100) !== 0) {{ console.error('not reusable after error'); process.exit(1); }}
console.log('WASM_OK ' + blue);
"#,
        lib = root.join("web/pscat.js").display(),
        wasm = root.join("web/pscat.wasm").display(),
    );
    let dir = std::env::temp_dir().join(format!("pscat-wasm-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let script = dir.join("smoke.mjs");
    std::fs::write(&script, driver).expect("write driver");

    let out = Command::new("node")
        .arg(&script)
        .output()
        .expect("run node");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success() && stdout.contains("WASM_OK"),
        "node smoke test failed\nstdout: {stdout}\nstderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}
