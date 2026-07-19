#!/usr/bin/env bash
# build_wasm.sh — build the browser build (Stage 16) and put the
# module where web/pscat.js expects it.
#
#   ./scripts/build_wasm.sh
#   python3 -m http.server -d web      # then open http://localhost:8000
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
cargo build --lib --release --target wasm32-unknown-unknown \
  --manifest-path "$ROOT/Cargo.toml"
cp "$ROOT/target/wasm32-unknown-unknown/release/pscat.wasm" "$ROOT/web/pscat.wasm"
ls -lh "$ROOT/web/pscat.wasm"
