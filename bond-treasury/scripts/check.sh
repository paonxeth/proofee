#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
cargo fmt --all -- --check
cargo test --workspace --locked --lib
cargo build --workspace --locked --release --target wasm32-unknown-unknown
python3 scripts/check-wasm.py target/wasm32-unknown-unknown/release
bash tests/sandbox/run.sh
