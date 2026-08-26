//! Generates `src/capabilities.rs`'s tag-driven catalog rows from
//! `% @...` doc-comment tags in `lib/*.ps` (issue #94, follow-up to
//! #92's `docs/PS_LIBRARY_COUPLING.md`, "Touchpoint 1").
//!
//! Runs entirely at build time, on the host filesystem -- this is a
//! normal cargo build script, not a wasm-runtime disk read. It writes
//! fully-resolved Rust source (string literals, no further parsing
//! needed) to `$OUT_DIR/capabilities_generated.rs`, which
//! `src/capabilities.rs` splices in with `include!`. By the time the
//! wasm target compiles, the generated file is just literal Rust data
//! like any other `const`/`static` -- zero runtime file I/O, wasm
//! included.
//!
//! ## Tag grammar
//!
//! A tag line is a comment (`%`) whose content, after stripping the
//! leading `%` and whitespace, starts with `@`. Deliberately *not*
//! `%%` (this repo's DSC comments -- `%%Title:`/`%%For:` already feed
//! PDF `/Info`) and not `%!` (the `%!PS-Adobe-3.0` interpreter
//! shebang) -- `% @tag:` can't collide with either.
//!
//! Recognized tags:
//! - `% @kind: Procedure|Dial|Template` -- per entry, the three kinds
//!   [`find_top_level_defs`]'s `/name ... def` discovery can actually
//!   see. `Font`, `Type3Face`, and `Palette` are explicitly rejected,
//!   each for its own reason (Codex review, PR #97, two rounds):
//!   fonts are enumerated live from `font::catalog_entries`, never
//!   tag-driven; a Type 3 face binds with `/Name Dict definefont pop`,
//!   not `/name ... def` (see `lib/handscript.ps`/`hangul.ps`); a
//!   palette is registered with `Palettes /name [...] put`, a dict
//!   mutation, not a `def` binding either (see `lib/artkit.ps`/the
//!   style packs). Migrating a file that needs one of these kinds
//!   requires adding the matching discovery to this file first --
//!   accepting the tag without it would silently drop the entry.
//! - `% @summary: <one-line description>` -- per entry
//! - `% @example: <ps code>` -- per entry
//! - `% @param: /Name description text (default D)` -- 0+ per entry
//! - `% @internal` -- bare marker; mutually exclusive with the above
//! - `% @requires: (lib/artkit.ps) run` -- file-level, at most once,
//!   the prerequisite `run` chain before this file's own `run` works
//!   (empty/absent = none, e.g. `lib/artkit.ps` itself)
//!
//! Any other `@word` inside a migrated file is a build error --
//! silently ignoring an unrecognized tag (a typo'd `@parm:`, say)
//! would drop data with no signal, exactly the drift this mechanism
//! exists to prevent.
//!
//! ## Placement
//!
//! A tag block is the contiguous run of `% @...` lines immediately
//! touching (directly above, no gap) a top-level `/name ... def` (or
//! `bind def`). Tags are inserted fresh, not by relocating the
//! existing long prose headers -- the scan stops at the first
//! non-`@` line, so old prose sitting directly above a tag block is
//! never accidentally read as part of it.
//!
//! ## Migration is opt-in, per file, detected from content
//!
//! A file is "migrated" iff it contains at least one `% @...` line
//! anywhere. Every `lib/*.ps` file this build script discovers
//! (`lib/*.ps` plus one level into `lib/styles/`; `lib/fonts/` is a
//! separate, live-enumerated mechanism -- see `src/font.rs`) must be
//! *either* migrated (in which case every top-level definition needs
//! `@internal` or a full `@kind`/`@summary`/`@example` set, enforced
//! strictly) *or* listed in [`LEGACY_FILES`] below. A file in neither
//! bucket -- including a brand-new file nobody has touched yet --
//! fails the build immediately, closing the "new sibling file
//! silently uncataloged" gap `docs/PS_LIBRARY_COUPLING.md` calls
//! mandatory. A file that starts using tags must be removed from
//! [`LEGACY_FILES`] (also enforced) so the two mechanisms never
//! silently disagree about who owns a file.

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

/// `lib/*.ps` files (and `lib/styles/*.ps`) that stay on the
/// hand-written `capabilities.rs` `ENTRIES` table for now -- not yet
/// retrofitted with `% @...` tags. Migrating one of these is: add the
/// tags, remove its file name from this list, delete its now-redundant
/// `Entry` rows from `ENTRIES`. `graph.ps`/`dataviz.ps`/`etching.ps`
/// aren't in `capabilities.rs` at all today (a deliberate scope cut --
/// see `CAPABILITIES.md`'s "Scope cuts") but still need to be listed
/// here, or this script has no way to distinguish "deliberately
/// uncataloged" from "forgotten."
const LEGACY_FILES: &[&str] = &[
    "lib/artkit.ps",
    "lib/pagekit.ps",
    "lib/styles/steampunk.ps",
    "lib/styles/psychedelic.ps",
    "lib/styles/scifi.ps",
    "lib/styles/toon.ps",
    "lib/handscript.ps",
    "lib/hangul.ps",
    "lib/graph.ps",
    "lib/dataviz.ps",
    "lib/etching.ps",
];

const KNOWN_TAGS: &[&str] = &[
    "kind", "summary", "example", "param", "internal", "requires",
];

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=lib");

    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR set");
    let lib_dir = Path::new(&manifest_dir).join("lib");

    let mut files = Vec::new();
    collect_ps_files(&lib_dir, &mut files);
    files.sort();

    let mut legacy_remaining: BTreeSet<&str> = LEGACY_FILES.iter().copied().collect();

    let mut generated_entries = String::new();
    let mut internal_consts = String::new();
    let mut migrated_files = String::new();

    for path in &files {
        println!("cargo:rerun-if-changed={}", path.display());
        let rel = relative_source_path(&manifest_dir, path);
        let text = std::fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("build.rs: failed to read {rel}: {e}"));

        let is_migrated = text.lines().any(|l| tag_line(l).is_some());

        if !is_migrated {
            if !legacy_remaining.remove(rel.as_str()) {
                panic!(
                    "build.rs: {rel} has no `% @...` doc-comment tags and is not listed in \
                     build.rs's LEGACY_FILES -- either tag it (see lib/paintkit.ps for the \
                     convention) or add it to LEGACY_FILES explicitly if it's deliberately \
                     staying on the hand-written capabilities.rs ENTRIES path for now."
                );
            }
            continue;
        }

        if LEGACY_FILES.contains(&rel.as_str()) {
            panic!(
                "build.rs: {rel} is listed in LEGACY_FILES but contains `% @...` tags -- remove \
                 it from LEGACY_FILES now that it has opted into the tag-driven catalog."
            );
        }

        let parsed = parse_file(&rel, &text);

        for entry in &parsed.entries {
            write_entry(&mut generated_entries, &rel, &parsed.requires, entry);
        }

        let const_name = internal_const_name(&rel);
        let _ = writeln!(internal_consts, "pub const {const_name}: &[&str] = &[");
        for name in &parsed.internal_names {
            let _ = writeln!(internal_consts, "    {name:?},");
        }
        let _ = writeln!(internal_consts, "];\n");

        let _ = writeln!(migrated_files, "    MigratedFile {{");
        let _ = writeln!(migrated_files, "        source: {rel:?},");
        let _ = writeln!(migrated_files, "        requires: {:?},", parsed.requires);
        let _ = writeln!(migrated_files, "        internal_names: {const_name},");
        let _ = writeln!(migrated_files, "    }},");
    }

    if !legacy_remaining.is_empty() {
        panic!(
            "build.rs: LEGACY_FILES names file(s) that don't exist under lib/ (or aren't a .ps \
             file this script scans): {legacy_remaining:?} -- update LEGACY_FILES."
        );
    }

    let mut out = String::new();
    out.push_str(
        "// AUTO-GENERATED by build.rs from `% @...` doc-comment tags in lib/*.ps.\n\
         // Do not edit by hand -- edit the tags in the .ps source instead.\n\n",
    );
    out.push_str("static GENERATED_ENTRIES: &[GeneratedEntry] = &[\n");
    out.push_str(&generated_entries);
    out.push_str("];\n\n");
    out.push_str(&internal_consts);
    out.push_str("static MIGRATED_FILES: &[MigratedFile] = &[\n");
    out.push_str(&migrated_files);
    out.push_str("];\n");

    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR set");
    let dest = Path::new(&out_dir).join("capabilities_generated.rs");
    std::fs::write(&dest, out).unwrap_or_else(|e| panic!("build.rs: write {dest:?}: {e}"));
}

/// `lib/*.ps` at the top level, plus one level into `lib/styles/`.
/// `lib/fonts/` is skipped -- a separate, live-enumerated mechanism
/// (`src/font.rs`), not part of this tag-driven catalog. Any other
/// subdirectory is a hard error rather than a silent skip: a new
/// `lib/<something>/` needs a conscious decision here about whether
/// it should be scanned, not a default that happens to ignore it.
fn collect_ps_files(lib_dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(lib_dir).unwrap_or_else(|e| panic!("read {lib_dir:?}: {e}")) {
        let path = entry
            .unwrap_or_else(|e| panic!("read_dir entry in {lib_dir:?}: {e}"))
            .path();
        if path.is_dir() {
            match path.file_name().and_then(|n| n.to_str()) {
                Some("fonts") => continue,
                Some("styles") => {
                    for e2 in
                        std::fs::read_dir(&path).unwrap_or_else(|e| panic!("read {path:?}: {e}"))
                    {
                        let p2 = e2
                            .unwrap_or_else(|e| panic!("read_dir entry in {path:?}: {e}"))
                            .path();
                        if p2.extension().and_then(|e| e.to_str()) == Some("ps") {
                            out.push(p2);
                        }
                    }
                }
                other => panic!(
                    "build.rs: unrecognized subdirectory lib/{other:?} -- update \
                     collect_ps_files in build.rs to say whether it should be scanned for \
                     `% @...` tags (like lib/styles/) or skipped (like lib/fonts/)."
                ),
            }
            continue;
        }
        if path.extension().and_then(|e| e.to_str()) == Some("ps") {
            out.push(path);
        }
    }
}

fn relative_source_path(manifest_dir: &str, path: &Path) -> String {
    let rel = path
        .strip_prefix(manifest_dir)
        .unwrap_or_else(|_| panic!("{path:?} not under manifest dir {manifest_dir:?}"));
    rel.components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

/// `lib/paintkit.ps` -> `PAINTKIT_INTERNAL`, matching the naming
/// convention the hand-written `capabilities.rs` already uses for
/// `ARTKIT_INTERNAL`/`PAGEKIT_INTERNAL`/etc.
fn internal_const_name(rel: &str) -> String {
    let stem = Path::new(rel)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or_else(|| panic!("no file stem for {rel:?}"));
    format!("{}_INTERNAL", stem.to_uppercase())
}

/// If `line`, trimmed, is `% @word[: value]`, returns `(word, value)`
/// (value is `""` for a bare tag like `@internal`). Owned strings --
/// this is a build script parsing a file at most a few thousand lines
/// once per build, not a hot path worth fighting lifetimes for.
fn tag_line(line: &str) -> Option<(String, String)> {
    let t = line.trim_start().strip_prefix('%')?.trim_start();
    let t = t.strip_prefix('@')?;
    let word_end = t
        .find(|c: char| c == ':' || c.is_whitespace())
        .unwrap_or(t.len());
    let word = t[..word_end].to_string();
    let rest = &t[word_end..];
    let rest = rest.strip_prefix(':').unwrap_or(rest).trim();
    Some((word, rest.to_string()))
}

struct ParsedEntry {
    name: String,
    kind: String,
    summary: String,
    example: String,
    params: Vec<(String, String, Option<String>)>,
}

struct ParsedFile {
    requires: String,
    entries: Vec<ParsedEntry>,
    internal_names: Vec<String>,
}

fn parse_file(rel: &str, text: &str) -> ParsedFile {
    let lines: Vec<&str> = text.lines().collect();

    // Every tag-shaped line must use a known tag word -- an
    // unrecognized `@word` is silently-dropped data otherwise, the
    // exact drift this mechanism exists to prevent.
    for (i, line) in lines.iter().enumerate() {
        if let Some((word, _)) = tag_line(line)
            && !KNOWN_TAGS.contains(&word.as_str())
        {
            panic!(
                "build.rs: {rel}:{}: unknown tag `@{word}` -- known tags: {KNOWN_TAGS:?}",
                i + 1
            );
        }
    }

    let mut requires: Option<String> = None;
    for (i, line) in lines.iter().enumerate() {
        if let Some((word, val)) = tag_line(line)
            && word == "requires"
        {
            if requires.is_some() {
                panic!("build.rs: {rel}:{}: duplicate @requires", i + 1);
            }
            requires = Some(val);
        }
    }
    let requires = requires.unwrap_or_default();

    let defs = find_top_level_defs(text);

    let mut entries = Vec::new();
    let mut internal_names = Vec::new();

    for (name, start_line) in defs {
        let block = collect_tag_block(&lines, start_line);

        if block.is_empty() {
            panic!(
                "build.rs: {rel}: top-level definition `/{name}` (line {start_line}) has no \
                 `% @...` tag block directly above it -- add `% @internal` if it's a private \
                 helper, or `% @kind:`/`% @summary:`/`% @example:` if it's public API."
            );
        }

        let is_internal = block.iter().any(|(w, _)| w == "internal");
        let has_public_tag = block
            .iter()
            .any(|(w, _)| matches!(w.as_str(), "kind" | "summary" | "example" | "param"));

        if is_internal {
            if has_public_tag {
                panic!(
                    "build.rs: {rel}: `/{name}` (line {start_line}) has both `@internal` and a \
                     public tag (@kind/@summary/@example/@param) -- pick one."
                );
            }
            internal_names.push(name);
            continue;
        }

        let mut kind = None;
        let mut summary = None;
        let mut example = None;
        let mut params = Vec::new();
        for (word, val) in &block {
            match word.as_str() {
                "kind" => {
                    if kind.is_some() {
                        panic!(
                            "build.rs: {rel}: `/{name}` (line {start_line}) has duplicate @kind"
                        );
                    }
                    kind = Some(parse_kind(rel, &name, start_line, val));
                }
                "summary" => {
                    if summary.is_some() {
                        panic!(
                            "build.rs: {rel}: `/{name}` (line {start_line}) has duplicate @summary"
                        );
                    }
                    if val.trim().is_empty() {
                        panic!(
                            "build.rs: {rel}: `/{name}` (line {start_line}): @summary has no value"
                        );
                    }
                    summary = Some(val.clone());
                }
                "example" => {
                    if example.is_some() {
                        panic!(
                            "build.rs: {rel}: `/{name}` (line {start_line}) has duplicate @example"
                        );
                    }
                    if val.trim().is_empty() {
                        panic!(
                            "build.rs: {rel}: `/{name}` (line {start_line}): @example has no value"
                        );
                    }
                    example = Some(val.clone());
                }
                "param" => params.push(parse_param(rel, &name, start_line, val)),
                "requires" => {} // file-level; already consumed above
                other => unreachable!("filtered by the known-tag check above: {other}"),
            }
        }

        let kind = kind.unwrap_or_else(|| {
            panic!("build.rs: {rel}: `/{name}` (line {start_line}) is missing @kind")
        });
        let summary = summary.unwrap_or_else(|| {
            panic!("build.rs: {rel}: `/{name}` (line {start_line}) is missing @summary")
        });
        let example = example.unwrap_or_else(|| {
            panic!("build.rs: {rel}: `/{name}` (line {start_line}) is missing @example")
        });

        entries.push(ParsedEntry {
            name,
            kind,
            summary,
            example,
            params,
        });
    }

    ParsedFile {
        requires,
        entries,
        internal_names,
    }
}

fn parse_kind(rel: &str, name: &str, line: usize, val: &str) -> String {
    match val {
        "Procedure" | "Dial" | "Template" => val.to_string(),
        "Font" => panic!(
            "build.rs: {rel}: `/{name}` (line {line}): @kind: Font is not supported here -- \
             Font capabilities are enumerated live from font::catalog_entries(), not tag-driven \
             (see tests/capabilities.rs::fonts_agree_with_available_fonts)."
        ),
        "Type3Face" => panic!(
            "build.rs: {rel}: `/{name}` (line {line}): @kind: Type3Face is not supported yet -- \
             find_top_level_defs only discovers `/name ... def` bindings, and a Type 3 face is \
             bound with `/Name Dict definefont pop` instead (see lib/handscript.ps/hangul.ps), \
             so no tag placed above it would ever be picked up. Migrating handscript.ps/\
             hangul.ps needs definefont discovery added to build.rs first (Codex review, PR #97)."
        ),
        "Palette" => panic!(
            "build.rs: {rel}: `/{name}` (line {line}): @kind: Palette is not supported yet -- \
             a palette is registered with `Palettes /name [...] put`, a dict mutation, not a \
             `/name ... def` binding, so find_top_level_defs never sees it and a tag placed \
             above one is silently never consumed (Codex review, PR #97). Migrating a file with \
             palette entries (artkit.ps/pagekit.ps/the style packs) needs `put` discovery added \
             to build.rs first."
        ),
        other => panic!(
            "build.rs: {rel}: `/{name}` (line {line}): unrecognized @kind value {other:?} -- \
             expected one of Procedure, Dial, Template."
        ),
    }
}

/// `/Name description text (default D)` -> `(name, description, Some(D))`,
/// or `(name, description, None)` if there's no trailing `(default ...)`.
fn parse_param(rel: &str, owner: &str, line: usize, val: &str) -> (String, String, Option<String>) {
    let val = val.trim();
    let rest = val.strip_prefix('/').unwrap_or_else(|| {
        panic!(
            "build.rs: {rel}: `/{owner}` (line {line}): @param must start with `/Name`, got {val:?}"
        )
    });
    let (pname, rest) = match rest.find(char::is_whitespace) {
        Some(idx) => (&rest[..idx], rest[idx..].trim_start()),
        None => panic!(
            "build.rs: {rel}: `/{owner}` (line {line}): @param `/{rest}` has no description \
             after the name"
        ),
    };
    if pname.is_empty() {
        panic!("build.rs: {rel}: `/{owner}` (line {line}): @param has an empty `/Name`");
    }
    if rest.is_empty() {
        panic!("build.rs: {rel}: `/{owner}` (line {line}): @param `/{pname}` has no description");
    }
    if let Some(start) = rest.rfind("(default ")
        && rest.ends_with(')')
    {
        let desc = rest[..start].trim().to_string();
        let default = rest[start + "(default ".len()..rest.len() - 1]
            .trim()
            .to_string();
        // `rest` being non-empty (checked above) doesn't mean *either*
        // half survives the split non-empty -- `/Width (default 6)`
        // has no text before the parenthetical (empty description),
        // and `/Width text (default )` has nothing inside it (empty
        // default). Both produced a silently-accepted malformed row
        // before this check (Codex review, PR #97, round 2).
        if desc.is_empty() {
            panic!(
                "build.rs: {rel}: `/{owner}` (line {line}): @param `/{pname}` has no description \
                 before its `(default ...)`"
            );
        }
        if default.is_empty() {
            panic!(
                "build.rs: {rel}: `/{owner}` (line {line}): @param `/{pname}` has an empty \
                 `(default )`"
            );
        }
        return (pname.to_string(), desc, Some(default));
    }
    (pname.to_string(), rest.to_string(), None)
}

/// Walks upward from just above `start_line` (1-indexed), collecting
/// the contiguous run of `% @...` lines in top-to-bottom order. Stops
/// at the first line that isn't a tag line -- old prose sitting
/// directly above (no blank-line separator required) is never
/// accidentally included, since it doesn't start with `@`.
fn collect_tag_block(lines: &[&str], start_line: usize) -> Vec<(String, String)> {
    let mut block = Vec::new();
    if start_line < 2 {
        return block;
    }
    let mut i = start_line as isize - 2; // 0-indexed line just above start_line
    while i >= 0 {
        match tag_line(lines[i as usize]) {
            Some(tag) => {
                block.push(tag);
                i -= 1;
            }
            None => break,
        }
    }
    block.reverse();
    block
}

/// One depth-0 PostScript object as `find_top_level_defs` sees it --
/// either a `/name` literal (with the line it appeared on) or anything
/// else (a bare executable name like `Palettes`/`put`, a number, a
/// bool, or a balanced `{}`/`[]`/`<<>>` group collapsed to a single
/// object once its closer brings depth back to 0).
#[derive(Clone)]
enum TopLevelObject {
    Name(String, usize),
    Opaque,
}

/// Pushes `obj` onto `window`, a 2-slot sliding window over the most
/// recent depth-0 objects (`window[0]` = second-to-last, `window[1]` =
/// most recent) -- see [`find_top_level_defs`] for why this replaces a
/// simpler "last/first name wins" heuristic.
fn push_object(window: &mut [Option<TopLevelObject>; 2], obj: TopLevelObject) {
    window[0] = window[1].take();
    window[1] = Some(obj);
}

/// Scans the whole file for top-level `/name ... def` (or `bind def`)
/// sequences -- a definition occurring at brace/bracket/dict-literal
/// depth 0. Comments are stripped and `(...)` string contents blanked
/// out first (per line, with string/comment state carried across line
/// boundaries) so braces or `%` inside a string or comment can't
/// perturb depth tracking or get mistaken for code.
///
/// `def` pops exactly two objects off the (virtual, depth-0) stack:
/// `key value def`. A 2-slot sliding window over *every* depth-0
/// object -- not just `/name` tokens -- models that directly: when
/// `def` fires, the key is whatever occupied the window's older slot.
/// This is deliberately not "the last (or first) `/name` token seen
/// since the previous `def`" -- both of those heuristics were tried
/// and broke on real code, caught across two rounds of Codex review on
/// PR #97: "last name wins" mis-cataloged `/spmetal /brass def` (a
/// Dial bound to another name literal) as `brass`; the fix, "first
/// name wins, ignore later ones," then mis-cataloged the *next*
/// definition when an unrelated bare-token statement intervened --
/// `lib/styles/steampunk.ps` executes `Palettes /brass [...] put`
/// (pushing a stray `/brass` that "first name wins" never released)
/// immediately before `/spmetal /brass def`, so it kept naming that
/// definition `brass` too. Treating every depth-0 token as filling a
/// window slot -- opaque tokens included, so `Palettes`/`put` correctly
/// flush the stale `/brass` out of the window -- gets both cases right
/// regardless of what came before, verified against both real lines
/// with a standalone tokenizer probe.
fn find_top_level_defs(text: &str) -> Vec<(String, usize)> {
    let mut defs = Vec::new();
    let mut depth: i32 = 0;
    let mut in_string: i32 = 0;
    let mut window: [Option<TopLevelObject>; 2] = [None, None];

    for (line_idx, raw_line) in text.lines().enumerate() {
        let line_no = line_idx + 1;

        let mut clean = String::with_capacity(raw_line.len());
        let mut chars = raw_line.chars();
        while let Some(c) = chars.next() {
            if in_string > 0 {
                clean.push(' ');
                if c == '\\' {
                    chars.next();
                } else if c == '(' {
                    in_string += 1;
                } else if c == ')' {
                    in_string -= 1;
                }
                continue;
            }
            if c == '%' {
                break; // rest of the line is a comment
            }
            if c == '(' {
                in_string += 1;
                clean.push(' ');
                continue;
            }
            clean.push(c);
        }

        let padded = clean
            .replace("<<", " << ")
            .replace(">>", " >> ")
            .replace('{', " { ")
            .replace('}', " } ")
            .replace('[', " [ ")
            .replace(']', " ] ");

        for tok in padded.split_whitespace() {
            match tok {
                "{" | "[" | "<<" => {
                    depth += 1;
                    continue;
                }
                "}" | "]" | ">>" => {
                    depth -= 1;
                    // A balanced group closing back to depth 0 is one
                    // opaque object on the virtual stack -- push it so
                    // it correctly displaces whatever named object
                    // preceded it, same as any other depth-0 token.
                    if depth == 0 {
                        push_object(&mut window, TopLevelObject::Opaque);
                    }
                    continue;
                }
                _ => {}
            }
            if depth != 0 {
                continue;
            }
            if tok == "def" {
                match window[0].take() {
                    Some(TopLevelObject::Name(name, def_line)) => {
                        defs.push((name, def_line));
                    }
                    _ => panic!(
                        "build.rs: line {line_no}: `def` with no `/name` two positions back on \
                         the depth-0 stack -- this is a shape find_top_level_defs doesn't \
                         understand (not plain `/key value def`); it needs a look, not a guess."
                    ),
                }
                window = [None, None];
                continue;
            }
            if tok == "bind" {
                // `{proc} bind proc` pops one object and pushes the
                // same one back (mutated in place) -- transparent to
                // the window, not a new object. Not exercised by any
                // `lib/*.ps` file today (`bind def` doesn't appear),
                // but this file's own doc comments above claim support
                // for it, so it should actually work.
                continue;
            }
            if let Some(name) = tok.strip_prefix('/')
                && !name.is_empty()
            {
                push_object(&mut window, TopLevelObject::Name(name.to_string(), line_no));
                continue;
            }
            push_object(&mut window, TopLevelObject::Opaque);
        }
    }

    defs
}

fn write_entry(out: &mut String, source: &str, requires: &str, e: &ParsedEntry) {
    let load = if requires.is_empty() {
        format!("({source}) run")
    } else {
        format!("{requires} ({source}) run")
    };
    let _ = writeln!(out, "    GeneratedEntry {{");
    let _ = writeln!(out, "        name: {:?},", e.name);
    let _ = writeln!(out, "        kind: CapabilityKind::{},", e.kind);
    let _ = writeln!(out, "        description: {:?},", e.summary);
    let _ = writeln!(out, "        parameters: &[");
    for (pname, desc, default) in &e.params {
        let default_expr = match default {
            Some(d) => format!("Some({d:?})"),
            None => "None".to_string(),
        };
        let _ = writeln!(
            out,
            "            Param {{ name: {pname:?}, description: {desc:?}, default: {default_expr} }},"
        );
    }
    let _ = writeln!(out, "        ],");
    let _ = writeln!(out, "        source: {source:?},");
    let _ = writeln!(out, "        load: {load:?},");
    let _ = writeln!(out, "        example: {:?},", e.example);
    let _ = writeln!(out, "        availability: \"library\",");
    let _ = writeln!(out, "    }},");
}
