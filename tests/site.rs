//! The Stage 17 site build: `scripts/build_site.sh` must assemble a
//! complete `_site/` — pages, wasm, playground sources, renders.
//! Skips gracefully when the wasm target isn't installed (gs-test
//! style); the Pages workflow runs the same script in CI.

use std::process::Command;

#[test]
fn build_site_assembles_everything() {
    let targets = Command::new("rustup")
        .args(["target", "list", "--installed"])
        .output();
    if !targets.is_ok_and(|o| {
        o.status.success() && String::from_utf8_lossy(&o.stdout).contains("wasm32-unknown-unknown")
    }) {
        eprintln!("skipping site test: wasm32-unknown-unknown target not installed");
        return;
    }

    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let build = Command::new(root.join("scripts/build_site.sh"))
        .output()
        .expect("run build_site.sh");
    assert!(
        build.status.success(),
        "site build failed:\n{}",
        String::from_utf8_lossy(&build.stderr)
    );

    let out = root.join("_site");
    for page in [
        "index.html",
        "playground.html",
        "gallery.html",
        "architecture.html",
        "extending.html",
        "javascript.html",
        "agents.html",
        "style.css",
        "pscat.js",
        "pscat.wasm",
    ] {
        assert!(out.join(page).exists(), "missing {page}");
    }
    // Every example the playground picker offers must be staged.
    let playground = std::fs::read_to_string(out.join("playground.html")).expect("read");
    for line in playground.lines() {
        if let Some(rest) = line.trim().strip_prefix("<option value=\"") {
            let name = rest.split('"').next().expect("option value");
            if !name.is_empty() {
                assert!(
                    out.join("examples").join(name).exists(),
                    "picker offers {name} but it isn't staged"
                );
            }
        }
    }
    // Every render the gallery shows must exist.
    let gallery = std::fs::read_to_string(out.join("gallery.html")).expect("read");
    for piece in gallery.split("assets/renders/").skip(1) {
        let name = piece.split('"').next().expect("img src");
        assert!(
            out.join("assets/renders").join(name).exists(),
            "gallery shows {name} but it wasn't rendered"
        );
    }
}
