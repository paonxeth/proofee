#!/usr/bin/env bash
# near-cli-rs, atomic no-key Treasury creation; no existing account overwritten.
set -euo pipefail
: "${MASTER:?Set existing funded TESTNET parent account with key in OS keychain}"
: "${TREASURY:?Set treasury.<MASTER>}"
[[ "$MASTER" == *.testnet && "$TREASURY" == *."$MASTER" ]] || { echo 'Testnet subaccounts only'; exit 1; }
WASM="$(cd "$(dirname "$0")/.." && pwd)/target/wasm32-unknown-unknown/release/bond_treasury.wasm"
[[ -s "$WASM" ]] || { echo 'Build WASM first'; exit 1; }
if near --quiet account view-account-summary "$TREASURY" network-config testnet now >/dev/null 2>&1; then
 echo 'Account already exists; refusing deployment'; exit 1
fi
ARGS=$(python3 -c 'import json,sys; print(json.dumps({"admin":sys.argv[1]}))' "$MASTER")
# Principal is NOT the initial storage endowment. 10 test NEAR funds code/state/gas.
near transaction construct-transaction "$MASTER" receiver-id "$TREASURY" \
 add-action create-account \
 add-action transfer '10 NEAR' \
 add-action deploy-contract use-file "$WASM" without-init-call \
 add-action function-call new json-args "$ARGS" prepaid-gas '100 Tgas' attached-deposit '0 NEAR' \
 skip network-config testnet sign-with-keychain send
near contract call-function as-read-only "$TREASURY" get_liabilities json-args '{}' network-config testnet now
near account list-keys "$TREASURY" network-config testnet now
