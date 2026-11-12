#!/usr/bin/env bash
set -euo pipefail
export RUSTUP_TOOLCHAIN="${RUSTUP_TOOLCHAIN:-1.100.0}"
cd "$(dirname "$0")/.."
cargo near build non-reproducible-wasm --locked --manifest-path contracts/locker/Cargo.toml
cargo near build non-reproducible-wasm --locked --manifest-path ../contracts/goal/Cargo.toml
cp contracts/locker/target/near/locker.wasm artifacts/locker.wasm
cp contracts/locker/target/near/locker_abi.json abi/locker.abi.json
cp ../contracts/goal/target/near/goal.wasm artifacts/goal.wasm
cp ../contracts/goal/target/near/goal_abi.json abi/goal.abi.json
sha256sum artifacts/*.wasm
