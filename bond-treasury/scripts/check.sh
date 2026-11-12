#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
cargo fmt --all -- --check
cargo test --workspace --locked --lib
cargo build --workspace --locked --release --target wasm32-unknown-unknown
bash tests/sandbox/run.sh
