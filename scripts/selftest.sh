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
BIN="$ROOT/target/release/pscat"

status=0

echo "== %%SelfTest blocks =="
found_blocks=0
for lib in "$ROOT"/lib/*.ps; do
    # Ask the parser which files carry blocks, never `grep`: a
    # `%%SelfTest:`-shaped line inside a multiline PostScript string is
    # string content, which the parser ignores and grep cannot see. The
    # two disagreeing meant CI rejected a file shape the parser
    # deliberately supports (Codex review, PR #136).
    [ -n "$("$BIN" --selftest-list "$lib")" ] || continue
    found_blocks=1
    if ! "$BIN" --selftest "$lib"; then
        status=1
    fi
done
if [ "$found_blocks" -eq 0 ]; then
    echo "selftest.sh: no lib/*.ps file carries a %%SelfTest block" >&2
    status=1
fi

echo
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
