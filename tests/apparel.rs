//! lib/apparel.ps (issue #146): configuration -> per-piece sizes, optional
//! pieces, and clip-free, transparent artwork, checked through the real CLI.

use std::io::Write;
use std::process::{Command, Stdio};
use tiny_skia::Pixmap;

const BIN: &str = env!("CARGO_BIN_EXE_pscat");
const ROOT: &str = env!("CARGO_MANIFEST_DIR");

/// Run `config` with a mode prelude (as scripts/apparel_export.sh does).
fn pscat(config: &str, mode: &str, piece: &str, extra: &[&str]) -> (bool, String, String) {
    let source = format!(
        "/ApparelMode /{mode} def /ApparelPiece /{piece} def\n{}",
        std::fs::read_to_string(format!("{ROOT}/{config}")).unwrap()
    );
    let mut child = Command::new(BIN)
        .current_dir(ROOT)
        .args(["--headless"])
        .args(extra)
        .arg("-")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn pscat");
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(source.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn sleeves_are_optional_and_pieces_are_independent() {
    let (ok, list, err) = pscat("examples/apparel_shirt.ps", "list", "Front", &[]);
    assert!(ok, "{err}");
    let names: Vec<&str> = list.lines().map(|l| l.split(' ').next().unwrap()).collect();
    assert_eq!(names, ["Front", "Back", "SleeveLeft", "SleeveRight"]);

    // a second design, no sleeves, a different phrase and DPI: config only
    let (ok, list, err) = pscat("examples/apparel_second.ps", "list", "Front", &[]);
    assert!(ok, "{err}");
    assert_eq!(list.lines().count(), 2, "{list}");
    let (ok, size, err) = pscat("examples/apparel_second.ps", "size", "Back", &[]);
    assert!(ok, "{err}");
    assert_eq!(size.trim(), "720 288 2000 800 transparent");
}

#[test]
fn manifest_records_size_dpi_background_and_fonts() {
    let (ok, json, err) = pscat("examples/apparel_shirt.ps", "manifest", "Front", &[]);
    assert!(ok, "{err}");
    for needle in [
        r#""background":"transparent""#,
        r#""dpi":300"#,
        r#""inches":[12,14]"#,
        r#""points":[864,1008]"#,
        r#""pixels":[3600,4200]"#,
        r#""face":"AlfaSlabOne""#,
        r#""text":"No.\n1""#,
    ] {
        assert!(json.contains(needle), "missing {needle} in {json}");
    }
}

/// Every piece: transparent corners, ink present, and the ink (contour
/// included) stays inside the configured margin -- the clipping check.
#[test]
fn artwork_is_transparent_and_stays_inside_its_margin() {
    let dir = std::env::temp_dir().join(format!("pscat-apparel-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    // (config, piece, page WxH pt, margin in) -- rendered at 72 dpi for speed
    for (config, piece, w, h, margin) in [
        (
            "examples/apparel_shirt.ps",
            "Front",
            864u32,
            1008u32,
            0.5f32,
        ),
        ("examples/apparel_shirt.ps", "SleeveLeft", 252, 252, 0.25),
        ("examples/apparel_second.ps", "Back", 720, 288, 0.4),
    ] {
        let png = dir.join(format!("{piece}.png"));
        let (ok, _, err) = pscat(
            config,
            "draw",
            piece,
            &[
                "--page",
                &format!("{w}x{h}"),
                "--transparent",
                "--png",
                png.to_str().unwrap(),
            ],
        );
        assert!(ok, "{piece}: {err}");
        let pm = Pixmap::load_png(&png).unwrap();
        assert_eq!((pm.width(), pm.height()), (w, h));
        for (x, y) in [(0, 0), (w - 1, 0), (0, h - 1), (w - 1, h - 1)] {
            assert_eq!(pm.pixel(x, y).unwrap().alpha(), 0, "{piece} corner");
        }
        let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0, 0);
        for y in 0..h {
            for x in 0..w {
                if pm.pixel(x, y).unwrap().alpha() > 0 {
                    x0 = x0.min(x);
                    y0 = y0.min(y);
                    x1 = x1.max(x);
                    y1 = y1.max(y);
                }
            }
        }
        assert!(x1 > x0 && y1 > y0, "{piece}: no ink");
        let m = (margin * 72.0).floor() as u32;
        assert!(
            x0 >= m && y0 >= m && w - 1 - x1 >= m && h - 1 - y1 >= m,
            "{piece}: ink [{x0},{y0},{x1},{y1}] crosses the {m}px margin of {w}x{h}"
        );
    }
}

#[test]
fn a_phrase_that_cannot_fit_fails_loudly_instead_of_clipping() {
    let cfg = "(lib/artkit.ps) run (lib/headline.ps) run (lib/lettering.ps) run \
               (lib/apparel.ps) run << /Pieces << /Front << /Size [ 1 1 ] \
               /Margin 0.1 /Runs [ << /Text (Real) /Font /AlfaSlabOne /Size 400 \
               /At [ 0 0 ] >> ] >> >> >> apmain";
    let mut child = Command::new(BIN)
        .current_dir(ROOT)
        .args(["--headless", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(format!("/ApparelMode /draw def /ApparelPiece /Front def {cfg}").as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("headline-run-outside-region"));
}
