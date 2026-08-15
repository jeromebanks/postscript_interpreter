//! The machine-readable capability catalog (issue #39) stays honest
//! about the PostScript libraries it describes, checked in both
//! directions:
//!   - every cataloged palette/template/procedure name must still be
//!     defined where the catalog says it is (a name gets renamed or
//!     removed -> this fails, not `--capabilities` silently);
//!   - every top-level name those libraries actually define must be
//!     either cataloged or explicitly listed as an internal helper in
//!     `capabilities::{ARTKIT_INTERNAL, PAGEKIT_INTERNAL}` -- so a
//!     newly added public palette/proc that nobody registered fails
//!     here instead of just being missing from `--capabilities`.

use std::collections::BTreeSet;
use std::process::Command;

use pscat::Interp;
use pscat::capabilities::{self, ARTKIT_INTERNAL, CapabilityKind, PAGEKIT_INTERNAL};

fn names_by(kind: CapabilityKind, source: &str) -> BTreeSet<String> {
    capabilities::catalog()
        .into_iter()
        .filter(|c| c.kind == kind && c.source == source)
        .map(|c| c.name)
        .collect()
}

/// Every top-level name currently bound in `userdict` -- `{ pop }
/// forall` leaves each key as an operand (value popped, key kept), so
/// once the loop finishes the whole enumeration sits on the operand
/// stack in one call, no printing/parsing needed.
fn userdict_names(it: &mut Interp) -> BTreeSet<String> {
    it.run_str("userdict { pop } forall")
        .unwrap_or_else(|e| panic!("enumerate userdict failed: {}", it.error_report(&e)));
    it.operand_stack()
        .iter()
        .map(|o| o.repr().trim_start_matches('/').to_string())
        .collect()
}

fn load(it: &mut Interp, path: &str) {
    let src = std::fs::read(path).unwrap_or_else(|e| panic!("{path} present: {e}"));
    it.run_source(&src)
        .unwrap_or_else(|e| panic!("{path} failed to load: {}", it.error_report(&e)));
}

fn assert_name_sets_match(context: &str, got: &BTreeSet<String>, expected: &BTreeSet<String>) {
    let missing_from_catalog: Vec<_> = got.difference(expected).collect();
    let stale_in_catalog: Vec<_> = expected.difference(got).collect();
    assert!(
        missing_from_catalog.is_empty() && stale_in_catalog.is_empty(),
        "{context}: names defined but not cataloged/allowlisted: {missing_from_catalog:?}; \
         names cataloged/allowlisted but not actually defined: {stale_in_catalog:?}"
    );
}

#[test]
fn artkit_names_match_the_catalog_exactly() {
    let mut it = Interp::new();
    load(&mut it, "lib/artkit.ps");
    let got = userdict_names(&mut it);

    let mut expected = names_by(CapabilityKind::Procedure, "lib/artkit.ps");
    expected.extend(ARTKIT_INTERNAL.iter().map(|s| s.to_string()));

    assert_name_sets_match("lib/artkit.ps", &got, &expected);
}

#[test]
fn style_pack_names_match_the_catalog_exactly() {
    let mut base_it = Interp::new();
    load(&mut base_it, "lib/artkit.ps");
    let artkit_baseline = userdict_names(&mut base_it);

    for (file, source) in [
        ("lib/styles/steampunk.ps", "lib/styles/steampunk.ps"),
        ("lib/styles/psychedelic.ps", "lib/styles/psychedelic.ps"),
        ("lib/styles/scifi.ps", "lib/styles/scifi.ps"),
        ("lib/styles/toon.ps", "lib/styles/toon.ps"),
    ] {
        let mut it = Interp::new();
        load(&mut it, "lib/artkit.ps");
        load(&mut it, file);
        let all = userdict_names(&mut it);
        // Palettes are registered with `Palettes /name [...] put` (a
        // mutation of the existing dict), not `def`, so they never
        // land in userdict -- pack_specific is procedures/dials only,
        // checked against tests::palettes_match_the_catalog_exactly
        // separately. Set-difference also means a pack that *redefined*
        // an artkit name (shadowing it with its own `def` of the same
        // name) would be swallowed here rather than flagged -- no pack
        // does that today, and it's an accepted gap in this reverse
        // check, not a case this test is designed to catch.
        let pack_specific: BTreeSet<String> = all.difference(&artkit_baseline).cloned().collect();

        let expected = names_by(CapabilityKind::Procedure, source);
        assert_name_sets_match(file, &pack_specific, &expected);
    }
}

#[test]
fn palettes_match_the_catalog_exactly() {
    let mut it = Interp::new();
    for file in [
        "lib/artkit.ps",
        "lib/styles/steampunk.ps",
        "lib/styles/psychedelic.ps",
        "lib/styles/scifi.ps",
        "lib/styles/toon.ps",
        "lib/pagekit.ps",
    ] {
        load(&mut it, file);
    }
    it.run_str("Palettes { pop } forall")
        .unwrap_or_else(|e| panic!("enumerate Palettes failed: {}", it.error_report(&e)));
    let got: BTreeSet<String> = it
        .operand_stack()
        .iter()
        .map(|o| o.repr().trim_start_matches('/').to_string())
        .collect();

    let expected: BTreeSet<String> = capabilities::catalog()
        .into_iter()
        .filter(|c| c.kind == CapabilityKind::Palette)
        .map(|c| c.name)
        .collect();

    assert_name_sets_match("Palettes dict (all contributors)", &got, &expected);
}

#[test]
fn pagekit_names_match_the_catalog_exactly() {
    let mut base_it = Interp::new();
    load(&mut base_it, "lib/artkit.ps");
    let artkit_baseline = userdict_names(&mut base_it);

    let mut it = Interp::new();
    load(&mut it, "lib/artkit.ps");
    load(&mut it, "lib/pagekit.ps");
    let all = userdict_names(&mut it);
    let pagekit_specific: BTreeSet<String> = all.difference(&artkit_baseline).cloned().collect();

    let mut expected = names_by(CapabilityKind::Template, "lib/pagekit.ps");
    expected.extend(PAGEKIT_INTERNAL.iter().map(|s| s.to_string()));

    assert_name_sets_match("lib/pagekit.ps", &pagekit_specific, &expected);
}

/// Type 3 faces don't go through `findfont`'s Rust-side registry
/// (`--fonts`/`resolve`) -- they're defined by `definefont` straight
/// into PostScript's own `FontDirectory`. `findfont` on an unknown name
/// substitutes rather than erroring (a deliberate deviation, see
/// HANDOFF.md), so success/failure of `findfont` itself can't tell a
/// real face from a substituted one; checking `FontDirectory` directly
/// is what actually proves the file defines the name the catalog claims.
#[test]
fn type3_faces_are_actually_defined_after_loading() {
    // Type3 entries carry per-face source paths, not one shared
    // constant, so gather (name, source) pairs from the catalog
    // instead of filtering by a fixed source string.
    let entries: Vec<(String, String)> = capabilities::catalog()
        .into_iter()
        .filter(|c| c.kind == CapabilityKind::Type3Face)
        .map(|c| (c.name, c.source))
        .collect();

    // Reverse direction: every `.ps` file under lib/fonts/ must be
    // cataloged, not just every cataloged entry checked forward --
    // otherwise a new face added to the directory and never
    // registered in capabilities.rs would pass silently.
    let cataloged_sources: BTreeSet<String> =
        entries.iter().map(|(_, source)| source.clone()).collect();
    let on_disk: BTreeSet<String> = std::fs::read_dir("lib/fonts")
        .expect("lib/fonts present")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("ps"))
        .map(|p| format!("lib/fonts/{}", p.file_name().unwrap().to_string_lossy()))
        .collect();
    assert_name_sets_match("lib/fonts/*.ps", &on_disk, &cataloged_sources);

    for (name, source) in entries {
        let mut it = Interp::with_page(200, 200).expect("page");
        load(&mut it, &source);
        it.run_str(&format!("FontDirectory /{name} known"))
            .unwrap_or_else(|e| panic!("{source}: known check failed: {}", it.error_report(&e)));
        let stack = it.operand_stack();
        assert_eq!(
            stack.last().map(|o| o.repr()).as_deref(),
            Some("true"),
            "{source} does not define /{name} in FontDirectory"
        );
    }
}

#[test]
fn fonts_agree_with_available_fonts() {
    let from_capabilities: BTreeSet<String> = capabilities::catalog()
        .into_iter()
        .filter(|c| c.kind == CapabilityKind::Font)
        .map(|c| c.name)
        .collect();
    let from_available: BTreeSet<String> = pscat::font::catalog_entries()
        .into_iter()
        .map(|e| e.name)
        .collect();
    assert_eq!(
        from_capabilities, from_available,
        "capabilities::catalog()'s font entries must match font::catalog_entries() exactly \
         (both should derive from the same source, not drift independently)"
    );
}

#[test]
fn cli_capabilities_flag_prints_valid_matching_json() {
    let exe = env!("CARGO_BIN_EXE_pscat");
    let out = Command::new(exe)
        .arg("--capabilities")
        .output()
        .expect("run pscat --capabilities");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("--capabilities prints valid JSON");
    assert!(json["pscat_version"].is_string());
    let caps = json["capabilities"].as_array().expect("capabilities array");
    assert!(
        caps.len() > 100,
        "expected a substantial catalog, got {}",
        caps.len()
    );
    for cap in caps {
        for key in [
            "name",
            "kind",
            "description",
            "parameters",
            "source",
            "example",
            "availability",
        ] {
            assert!(cap.get(key).is_some(), "entry missing {key:?}: {cap}");
        }
    }
}
