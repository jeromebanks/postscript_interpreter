#!/usr/bin/env bash
# ci_test_summary.sh — turn a raw `cargo test` log into a markdown
# summary (pass/fail/ignored counts, failing test names, panic output)
# suitable for a PR/issue comment or $GITHUB_STEP_SUMMARY. Used by
# .github/workflows/ci.yml so CI's actual test result is corroborated
# on the PR itself, not just a self-reported checkbox.
#
#   ./scripts/ci_test_summary.sh <test-output-log> <step-outcome> [run-url]
#
# <step-outcome> is the `cargo test` step's real outcome ("success",
# "failure", or "skipped") — passed in separately because a compile
# error can make cargo test exit nonzero while producing zero "test
# result:" lines (needs to render differently from "0 failed"), and
# "skipped" means the log file doesn't exist at all (an earlier
# fmt/clippy/build step failed first, so cargo test never ran).
set -uo pipefail  # no -e: grep/awk finding no match is expected here, not a script error

LOG="$1"
OUTCOME="$2"
RUN_URL="${3:-}"

echo "## CI test results"
echo

if [ "$OUTCOME" = "skipped" ]; then
  echo "**cargo test: ⏭️ did not run** — an earlier step (fmt/clippy/build) failed first, so tests never started this run."
  if [ -n "$RUN_URL" ]; then
    echo
    echo "[View full run]($RUN_URL)"
  fi
  exit 0
fi

read -r passed failed ignored <<<"$(grep -E '^test result: ' "$LOG" | awk '
  {
    for (i = 1; i <= NF; i++) {
      if ($i == "passed;")  passed  += $(i - 1)
      if ($i == "failed;")  failed  += $(i - 1)
      if ($i == "ignored;") ignored += $(i - 1)
    }
  }
  END { printf "%d %d %d", passed + 0, failed + 0, ignored + 0 }
')"

if [ "$OUTCOME" != "success" ] && [ "${failed:-0}" -eq 0 ]; then
  echo "**cargo test: ⚠️ did not complete** — no test results were produced (likely a compile error)."
  if [ -n "$RUN_URL" ]; then
    echo
    echo "[View full run]($RUN_URL)"
  fi
  echo
  echo "<details><summary>Log tail</summary>"
  echo
  echo '```'
  tail -n 60 "$LOG"
  echo '```'
  echo "</details>"
  exit 0
fi

if [ "${failed:-0}" -gt 0 ]; then
  icon="❌"
  status="FAILED"
else
  icon="✅"
  status="passed"
fi

echo "**cargo test: ${icon} ${status}** — ${passed:-0} passed, ${failed:-0} failed, ${ignored:-0} ignored"
if [ -n "$RUN_URL" ]; then
  echo
  echo "[View full run]($RUN_URL)"
fi

if [ "${failed:-0}" -gt 0 ]; then
  echo
  echo "<details><summary>Failing tests</summary>"
  echo
  echo '```'
  grep -E '^test .* \.\.\. FAILED$' "$LOG" || true
  echo '```'
  echo "</details>"

  failure_block=$(awk '/^failures:$/ { found = 1 } found { print }' "$LOG" | head -c 4000)
  if [ -n "$failure_block" ]; then
    echo
    echo "<details><summary>Failure output</summary>"
    echo
    echo '```'
    printf '%s\n' "$failure_block"
    echo '```'
    echo "</details>"
  fi
fi
