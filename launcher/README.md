# ProoFee Testnet Launcher + LP Locker

Testnet only. Sequential Node.js Launcher plus one immutable Rust Locker per token.
This module is isolated from `contracts/goal`, the client, and Treasury.

## Provenance

Adapted from `proofee-testnet-poc.zip` supplied by the team (Drive file
`1fhkJZO1LFuC7zLM1JbEJHJbEQdDA-vzJ`): `near.cjs`, `transport.py`, token deployment,
DCL registrations, create_pool, HotZap zero-input swap, Buy/Sell, zero-amount
remove_liquidity and explicit wNEAR withdrawal. No Nearly code is copied.
FT implementation: near-examples/FT commit `15360ac443fe91e9d55a3b3db616fe6eb6514942`,
MIT/Apache notices retained under `vendor/FT-source`. The supplied FT WASM is reused.
Goal is built unchanged from parent repository commit `bcb49340c9a9c6844a928fab093ab672920fbb21`.
`proofee-rhea-goal-poc.zip` was unavailable; it is not a claimed source.

## Install, build, test

Requires Node.js 22+, Python 3, Rust 1.100.0 with wasm32-unknown-unknown and cargo-near 0.23.0.
Use workspace-local Cargo/npm caches where the default home is read-only.

```sh
npm ci --ignore-scripts
RUSTUP_TOOLCHAIN=1.100.0 cargo test --manifest-path contracts/locker/Cargo.toml --lib
./scripts/build.sh
```

The Locker was scaffolded using `cargo near new`, then replaced with this implementation.
No generated auction logic or CI deployment secrets are required.

## Signer and first launch

Set `NEAR_SIGNER_FILE` to an absolute path **outside this repository**, mode 0600,
containing a NEAR key file with account_id, public_key and private_key. Never paste
or commit the file. All contract targets and the signer are restricted to `.testnet`.

```sh
export NEAR_SIGNER_FILE=/secure/path/isolated-testnet.json
# Optional: generate a NEW key file and request 10 Testnet NEAR. Refuses overwrite.
node scripts/bootstrap.cjs
# Edit launch.example.json: keep launch_id immutable for retries.
node scripts/launch.cjs launch.example.json
node scripts/e2e.cjs
export NEAR_CALLER_FILE=/secure/path/separate-testnet-caller.json
node scripts/security.cjs
python scripts/finalize.py
```

Existing funding/key material is never overwritten. `bootstrap.cjs` is Testnet faucet
creation, not a mainnet wallet. Creation failure leaves the generated key file for
inspection/recovery. Faucet availability and funding are not guaranteed.

The example creates a NEW Goal; it never uses goal-4. For product launches provide
`goal_id` and `creator_id`; Launcher verifies get_goal.creator and immediate_bps=2000.
No Goal distribution logic is modified. The example verifier is a distinct account ID
reserved under the isolated root; milestone resolution is outside this E2E.

## Design and guarantees

* Fixed supply = 1,000,000,000 × 10^decimals, decimals 0..29 to fit u128.
* Token and Locker are created/deployed/initialized in separate atomic account batches,
  with **no AddKey**. Initial supply belongs to Locker from token initialization.
* LP is minted directly to Locker. No creator-held intermediate position or NFT
  approval is necessary. `get_launch` binds creator, token, pool, position and Goal.
* Only fee tier 10000, token_x/wrap.testnet, one-sided range above init point. No
  initial wNEAR is needed. Pool starts with zero active liquidity; a Buy traverses
  into the range. Price in NEAR per whole token at point p is
  1.0001^p × 10^(decimals−24). Range points are multiples of 200.
* The entire minted supply is sent through HotZap. DCL integer rounding leaves tiny
  **principal dust** in Locker's DCL internal assets. It is permanently retained,
  excluded from fees, and reported separately. Do not call the dust LP liquidity.
* No arbitrary-call, positive remove_liquidity, NFT transfer/approval, storage withdrawal,
  upgrade, delete-account, recipient setter, or balance-sweep API exists in Locker.
  Keylessness must be verified on chain before the UI says locked.
* collect invokes remove_liquidity with amount=0. Both fee assets remain distinct.
  Token fees are withdrawn, quoted, and swapped on this pool with a positive 1% floor
  and skip_unwrap_near=true. Received wNEAR is measured by FT balance difference;
  native gas rewards/endowment are not counted as LP income. The wNEAR fee leg is
  withdrawn separately. Both are unwrapped in an exact known amount, then paid only
  to the immutable Goal. Native operational reserves and principal dust are not swept.
* No tax, bonding curve, migration, governance or Treasury changes.

## Status and recovery

`advance()` and `collect()` are permissionless, nonpayable, deposit 0, attach 300 TGas.
One advance performs one external operation and private callback. Check get_launch
and transaction receipts after every invocation. Active means the launch is verified
or a fee cycle has completed; Busy means an operation is in flight or an ambiguity
was intentionally frozen. `last_error` distinguishes known failures.

The Launcher records config digest before writes and refuses a changed config for
an existing launch_id. It reads chain state before creating anything and verifies
supply/bindings. It does not reinitialize or mint again on resume. A submitted tx
hash is recorded before broadcast; on a network timeout query that exact hash and
wait for FINAL before rerunning. Do not run two Launcher processes for one signer.

Read-only failures can be retried. Failed Goal transfers preserve the exact debt and
can be retried; successful payment clears it once. Fully refunded seed/swap can be
retried. Partial/ambiguous state-changing receipts or a failed private callback stop
progress; **there is deliberately no admin force-reset** that could duplicate payouts
or expose principal. Inspect receipts; such states can require a new launch or a
reviewed future contract revision. This MVP does not guarantee recovery from every
possible callback failure. A frozen keyless deployment cannot be upgraded.

DCL is an external contract with its own administration and upgrade assumptions.
The lock constrains ProoFee's creator/admin; it cannot protect against changes to
Rhea itself. Permissionless fee conversion uses this pool's current quote and 1%
slippage; it is not an oracle-price guarantee or MEV protection.

See `INTEGRATION_HANDOFF.md`, `REPORT.md`, `evidence/` and `abi/` in the release bundle.
RPC defaults to https://rpc.testnet.fastnear.com; an allowlisted alternative may be
selected through NEAR_TESTNET_RPC. Before signing, chain_id must equal testnet.

The local runtime writes to ignored `run/evidence/` unless PROOFEE_EVIDENCE_DIR is set.
