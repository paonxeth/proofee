# ProoFee Treasury + Mock Goal v0.2.0 (TESTNET ONLY)

Standalone Cargo workspace, intended under bond-treasury/ in the ProoFee repository. No modification to existing contracts/goal. This is a fresh Treasury deployment, not an upgrade/migration of v0.1.

- contracts/bond_treasury: versioned authentic-Goal intent API, NEAR principal, neutral timeout refunds, slash reserve, separately funded rewards, private callbacks/attempt-safe retries.
- contracts/mock_goal: separate centralized TEST contract exercising same Treasury ABI as future real Goal.
- contracts/bond_treasury/ABI.md and contracts/mock_goal/ABI.md: exact JSON interface, permissions, returned values, states.
- contracts/bond_treasury/INTEGRATION_HANDOFF.md: changes required from real Goal developer, including sync/reward reserve proof.
- contracts/bond_treasury/GOAL_REVIEW.md: reviewed current Goal and mismatch with new protocol.
- TEST_REPORT.md, evidence/: actual tests/deploy results; these distinguish local sandbox and public Testnet.

Build/test:

```sh
rustup target add wasm32-unknown-unknown
cargo test --workspace --locked --lib
cargo build --workspace --locked --release --target wasm32-unknown-unknown
bash tests/sandbox/run.sh
```

Rust1.99.1 and near-sdk5.30.1 pinned; Cargo.lock committed. Linux x86_64, cc, python3, Node24, npm,curl required for supplied sandbox runner. It pins official nearcore2.14.1; near-workspaces5.0.0 default2.6.2 cannot deserialize modern Rust WASM. On hosts with NOFILE<65535, the TEST harness interposes only setrlimit startup requests, retaining actual kernel hard limit; RocksDB max_open_files=1000. It never raises host limits or changes chain execution; no consensus/runtime mocking. Nearcore startup diagnostics about kernel socket tuning are local-test limitations. Do not use this shim for a production node. Local genesis root renamed testnet to exercise .testnet-only initialization, with no public RPC connection.

Default build EXCLUDES admin demo registration; use --features bond-treasury/demo-registry only for isolated legacy tests, never deployment. No admin withdraw/rescue and no production payout bypass. Goal trust registry and final-failure attestations remain centralized; real Goal correctness is essential. Locked principal/rewards/Buyback/storage are separately accounted; no swap or reserve withdrawal. No Multi-Challenger, tokens, migration or arbitrary payout API.

Deploy/public demo: scripts/testnet-demo.cjs. Requires a funded dedicated TESTNET parent signer JSON kept locally outside tracked files. Never put keys in repository/chat. Install JS dependencies in tests/sandbox (npm ci --ignore-scripts), then:

```sh
NODE_PATH="$PWD/tests/sandbox/node_modules" node scripts/testnet-demo.cjs /secure/local/testnet-admin.json
```

Script hardcodes https://rpc.testnet.near.org and enforces account suffixes; refuses overwriting existing child accounts. Creates no-key Treasury and Mock atomically with code+new, using6 TEST NEAR each for code/storage/buffer (WASM storage itself costs about4NEAR and3.6NEAR). Creates payer3NEAR and verifier1NEAR with locally generated test keys. Parent needs >=18NEAR including gas headroom; choose20 test NEAR. Optional second signer file argument transfers9.8 TEST NEAR from another explicitly test-funded account to parent; used only for faucet-provisioned demo funding. It performs rejection/refund, winning challenge/refund+funded reward, losing challenge/slash, recording every transaction response and final views under ignored work/testnet-evidence. Public deployments have no full-access keys; they cannot be updated. Keep parent/payer/verifier keys in protected local files/OS keystore and import securely if taking over the demo. Never include these key files in artifacts. Contract account suffix is operational guard, not cryptographic chain detection.

Separate manual CLI deployment starter scripts/deploy-testnet.sh exists for Treasury only; for integrated demo use the JS script. Treasury CLI uses10NEAR initial endowment and atomic no-key create/deploy/new. Read ABI for exact calls and on-chain deadlines. The supplied Mock does not consume existing Goal revenue and does not change any P0 account. Final forfeiture in Mock is simulated by an authorized verifier, explicitly test-only; a real Goal needs approved reserve accounting and timeout policies.

Do not send real/mainnet funds. No Mainnet readiness or independent audit is claimed.

For demo case IDs use a fixed64-character hash (or <=121 UTF-8 bytes): Mock prefixes IDs with bond:/reward:/fund:, and Treasury enforces128-byte caller ID limit. See review notes in TEST_REPORT.md. Default scope deliberately keeps reserve withdrawals and pending-transfer emergency overrides absent.
