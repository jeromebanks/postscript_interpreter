#!/usr/bin/env bash
# selftest.sh — the PS-native verification pass (issue #95, Phase A of
# docs/PS_LIBRARY_COUPLING.md's "Touchpoint 2"): run every library's
# `%%SelfTest` blocks, then strict-lint every rendering driver.
#
#   ./scripts/selftest.sh
#
# Nothing here needs Ghostscript, a golden image, or a line of Rust —
# that's the point. A PS-only change to lib/*.ps can be checked with
# this alone; `cargo test` still covers everything else (see
# docs/SELFTEST.md for exactly what this does and doesn't replace).
#
# Exits non-zero on the first failing check, printing what failed.
set -euo pipefail

# Derive the repo root from this script's own location, never from the
# caller's cwd: the drivers below load their libraries with a relative
# `(lib/artkit.ps) run`, and running from the wrong directory would
# quietly load a *different* checkout's copy rather than erroring.
ROOT=$(cd "$(dirname "$0")/.." && pwd)
cd "$ROOT"

cargo build --release --manifest-path "$ROOT/Cargo.toml" --quiet

# Ask Cargo where it put the binary rather than assuming `$ROOT/target`:
# with CARGO_TARGET_DIR or a `build.target-dir` in config.toml, the
# build lands elsewhere and this would either fail outright or, worse,
# run a *stale* binary and validate the libraries with an outdated
# harness (Codex review, PR #136).
TARGET_DIR=$(cargo metadata --format-version 1 --no-deps \
    --manifest-path "$ROOT/Cargo.toml" 2>/dev/null \
    | sed -n 's/.*"target_directory":"\([^"]*\)".*/\1/p')
BIN="${TARGET_DIR:-${CARGO_TARGET_DIR:-$ROOT/target}}/release/pscat"
if [ ! -x "$BIN" ]; then
    echo "selftest.sh: built pscat not found at $BIN" >&2
    exit 1
fi

status=0

echo "== %%SelfTest blocks =="
MANIFEST="$ROOT/selftest/libraries.txt"
if [ ! -f "$MANIFEST" ]; then
    echo "selftest.sh: missing $MANIFEST" >&2
    exit 1
fi

# Two-way cross-check against the manifest, not bare discovery.
# Discovery alone can't see a *deletion*: a library that loses its last
# block simply stops being discovered, and this pass stays green while
# its coverage drops to zero (Codex review, PR #136).
listed=$(grep -vE '^[[:space:]]*(#|$)' "$MANIFEST" || true)
if [ -z "$listed" ]; then
    echo "selftest.sh: $MANIFEST lists no libraries" >&2
    exit 1
fi

# 1. Every listed library must actually run its blocks. `--selftest`
#    already fails a file that yields none, so deletion fails here.
while IFS= read -r rel; do
    [ -n "$rel" ] || continue
    if [ ! -f "$ROOT/$rel" ]; then
        echo "selftest.sh: $MANIFEST lists $rel, which does not exist" >&2
        status=1
        continue
    fi
    if ! "$BIN" --selftest "$ROOT/$rel"; then
        status=1
    fi
done <<EOF_LISTED
$listed
EOF_LISTED

# 2. Every library that *has* blocks must be listed, so a newly
#    migrated file can't be silently left out of the pass. Recursive:
#    lib/ has real sources under fonts/ and styles/ too.
while IFS= read -r lib; do
    rel=${lib#"$ROOT"/}
    # `$(...)` inside a test discards the command's exit status, so a
    # library whose blocks are malformed would read as "no blocks" and
    # be skipped as a success — even under `set -e`.
    if ! blocks=$("$BIN" --selftest-list "$lib"); then
        echo "selftest.sh: cannot list blocks in $rel" >&2
        status=1
        continue
    fi
    [ -n "$blocks" ] || continue
    if ! printf '%s\n' "$listed" | grep -qxF "$rel"; then
        echo "selftest.sh: $rel has %%SelfTest blocks but is not listed in $MANIFEST" >&2
        status=1
    fi
done <<EOF_FOUND
$(find "$ROOT/lib" -name '*.ps' | sort)
EOF_FOUND

echo "== strict lint over the rendering drivers =="
# A bare lib/*.ps file draws nothing on load, so --lint has nothing to
# judge without a driver that actually calls into it. Each driver
# declares its own page size in a `%%SelfTestPage:` comment, since the
# scenarios are laid out for a specific canvas.
shopt -s nullglob
drivers=("$ROOT"/selftest/drivers/*.ps)
shopt -u nullglob
if [ ${#drivers[@]} -eq 0 ]; then
    echo "selftest.sh: no drivers in selftest/drivers/" >&2
    status=1
fi
for driver in "${drivers[@]}"; do
    page=$(sed -n 's/^%%SelfTestPage: *//p' "$driver" | head -1)
    if [ -z "$page" ]; then
        echo "selftest.sh: $driver has no %%SelfTestPage: WxH header" >&2
        status=1
        continue
    fi
    echo "-- ${driver#"$ROOT"/} (--page $page)"
    if ! "$BIN" --page "$page" --lint-strict "$driver"; then
        status=1
    fi
done

echo
if [ "$status" -eq 0 ]; then
    echo "selftest.sh: all checks passed"
else
    echo "selftest.sh: FAILED" >&2
fi
exit "$status"
