//! Shared resource-root resolution, used by the font catalog
//! (`font.rs`), the `run`/`file` operators (`ops/file.rs`), and
//! `pscat-mcp`'s handwrite path. One place to reason about "where do
//! pscat's runtime assets live" instead of drifting copies of
//! similar logic in each caller.
//!
//! Candidate roots, in the order each caller composes them:
//! `$PSCAT_ROOT` (an explicit override, but still just a candidate —
//! subject to the same existence check as everything else, so a
//! misconfigured override falls through rather than hard-failing),
//! the running executable's own directory (a bundle: the binary
//! sits next to `lib/` and `fonts/catalog/`), three levels up from
//! the executable (a cargo dev build: `target/release/pscat` →
//! the checkout root), the crate root at compile time (dev builds
//! only), and — for `run`/`file` specifically — the current
//! directory, checked *before* the heuristics below (see
//! `program_file`'s doc comment for why that one differs).

use std::path::{Path, PathBuf};

/// First candidate root under which `root.join(rel)` satisfies
/// `exists`. Roots and the predicate are parameters, not read from
/// the environment/filesystem directly here — that's what makes
/// candidate ordering unit-testable without real I/O or env
/// mutation (`std::env::set_var` is `unsafe` as of edition 2024, and
/// mutating real env vars is fragile under a parallel test runner
/// regardless).
pub fn first_existing(
    roots: &[Option<PathBuf>],
    rel: &str,
    exists: &dyn Fn(&Path) -> bool,
) -> Option<PathBuf> {
    roots
        .iter()
        .flatten()
        .map(|root| root.join(rel))
        .find(|p| exists(p))
}

/// The heuristic roots shared by every caller here, independent of
/// `$PSCAT_ROOT` and CWD (each caller decides where those two fit
/// in its own order). Reads the real environment/executable path —
/// not injectable, since nothing about *this* logic needs mocking;
/// only candidate *ordering* (tested via `first_existing` above)
/// does.
#[cfg(not(target_arch = "wasm32"))]
pub fn heuristic_roots() -> Vec<Option<PathBuf>> {
    // current_exe()'s own docs warn it may return a non-canonical
    // path — critically, invoking a *symlink* to the binary (e.g.
    // one dropped in ~/.local/bin or /opt/homebrew/bin pointing at
    // the real install) can return the symlink's own location, not
    // the real binary's, which would put lib/fonts/catalog nowhere
    // near where this then looks. canonicalize() resolves that;
    // fall back to the raw path only if canonicalization itself
    // fails (e.g. the binary was removed after the process started).
    let exe = std::env::current_exe()
        .ok()
        .map(|p| std::fs::canonicalize(&p).unwrap_or(p));
    vec![
        exe.as_deref().and_then(Path::parent).map(Path::to_path_buf),
        exe.as_deref()
            .and_then(|p| p.ancestors().nth(3))
            .map(Path::to_path_buf),
        Some(PathBuf::from(env!("CARGO_MANIFEST_DIR"))),
    ]
}

#[cfg(not(target_arch = "wasm32"))]
fn env_root() -> Option<PathBuf> {
    std::env::var_os("PSCAT_ROOT").map(PathBuf::from)
}

/// Find the font catalog directory: `$PSCAT_ROOT`, then the
/// heuristics, then CWD — today's order, unchanged by this module's
/// introduction (it previously lived, duplicated, in `font.rs`).
#[cfg(not(target_arch = "wasm32"))]
pub fn catalog_dir() -> Option<PathBuf> {
    let mut roots = vec![env_root()];
    roots.extend(heuristic_roots());
    roots.push(Some(PathBuf::from(".")));
    first_existing(&roots, "fonts/catalog", &|p| p.is_dir())
}

/// Resolve a PostScript program's relative `run`/`file` path against
/// pscat's install layout. `$PSCAT_ROOT` and CWD are checked before
/// the heuristics — unlike `catalog_dir`, this path can shadow a
/// file the caller actually meant relative to CWD (e.g. `(helper.ps)
/// run` from a subdirectory), so the heuristics only kick in once a
/// direct CWD-relative read has already failed. That keeps this
/// change strictly additive for anyone not setting `$PSCAT_ROOT`:
/// identical behavior to before this module existed, with bundle
/// resolution only engaging on what would otherwise be an
/// `undefinedfilename`.
#[cfg(not(target_arch = "wasm32"))]
pub fn program_file(rel: &str) -> Option<PathBuf> {
    let mut roots = vec![env_root(), Some(PathBuf::from("."))];
    roots.extend(heuristic_roots());
    first_existing(&roots, rel, &|p| p.exists())
}

#[cfg(target_arch = "wasm32")]
pub fn catalog_dir() -> Option<PathBuf> {
    None
}

#[cfg(target_arch = "wasm32")]
pub fn program_file(_rel: &str) -> Option<PathBuf> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_existing_returns_first_matching_root() {
        let exists = |p: &Path| p == Path::new("/b/x.ps");
        let roots = vec![
            Some(PathBuf::from("/a")),
            Some(PathBuf::from("/b")),
            Some(PathBuf::from("/c")),
        ];
        assert_eq!(
            first_existing(&roots, "x.ps", &exists),
            Some(PathBuf::from("/b/x.ps"))
        );
    }

    #[test]
    fn first_existing_skips_none_candidates() {
        let exists = |p: &Path| p == Path::new("/c/x.ps");
        let roots = vec![None, Some(PathBuf::from("/c")), None];
        assert_eq!(
            first_existing(&roots, "x.ps", &exists),
            Some(PathBuf::from("/c/x.ps"))
        );
    }

    #[test]
    fn first_existing_none_when_nothing_matches() {
        let exists = |_: &Path| false;
        let roots = vec![Some(PathBuf::from("/a"))];
        assert_eq!(first_existing(&roots, "x.ps", &exists), None);
    }

    #[test]
    fn first_existing_absolute_rel_ignores_every_root() {
        // PathBuf::join replaces the base entirely when the RHS is
        // absolute — confirms run/file's absolute-path callers are
        // unaffected by candidate order or count.
        let exists = |p: &Path| p == Path::new("/abs/x.ps");
        let roots = vec![Some(PathBuf::from("/a")), Some(PathBuf::from("/b"))];
        assert_eq!(
            first_existing(&roots, "/abs/x.ps", &exists),
            Some(PathBuf::from("/abs/x.ps"))
        );
    }

    #[test]
    fn program_file_order_checks_cwd_before_heuristics() {
        // Mirrors program_file's actual composition (env, cwd, then
        // heuristics) so a regression in that order is caught here
        // without needing a real filesystem or env mutation.
        let cwd_has_it = |p: &Path| p == Path::new("./x.ps");
        let roots = vec![
            None, // no $PSCAT_ROOT
            Some(PathBuf::from(".")),
            Some(PathBuf::from("/heuristic/a")),
            Some(PathBuf::from("/heuristic/b")),
        ];
        assert_eq!(
            first_existing(&roots, "x.ps", &cwd_has_it),
            Some(PathBuf::from("./x.ps")),
            "cwd must win over heuristic roots when both would resolve"
        );
    }

    #[test]
    fn catalog_dir_order_checks_heuristics_before_cwd() {
        // catalog_dir has the opposite order from program_file — no
        // caller-supplied path to shadow, so heuristics come first.
        let only_cwd_has_it = |p: &Path| p == Path::new("./fonts/catalog");
        let heuristic_first_roots = vec![
            None, // no $PSCAT_ROOT
            Some(PathBuf::from("/heuristic/a")),
            Some(PathBuf::from("/heuristic/b")),
            Some(PathBuf::from(".")),
        ];
        assert_eq!(
            first_existing(&heuristic_first_roots, "fonts/catalog", &only_cwd_has_it),
            Some(PathBuf::from("./fonts/catalog"))
        );
    }
}
