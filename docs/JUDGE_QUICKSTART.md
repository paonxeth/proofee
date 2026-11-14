# Judge Quickstart

Use Ubuntu 24.04 x86_64 for the full test suite, Node.js 24.15.0, Rustup, Git, Python 3,
curl and a C compiler. The sandbox tests create accounts and send transactions inside a
disposable local chain. Public Testnet checks only read data. No wallets, keys or funds are needed.
Commands start from the repository root unless shown in a subshell.

## Clone and verify locally

**Do** → Clone the public repository and record the commit.

```sh
git clone https://github.com/paonxeth/proofee.git
cd proofee
git rev-parse HEAD
(cd client && npm ci && npm test && npm run build)
```

**Expect** → 12 client unit tests pass and TypeScript builds. **Verify** → These assertions
cover amount formatting, spec hash checks, RPC fallback and unsigned transaction construction.

**Do** → Install the pinned Rust versions and run the contract unit suites.

```sh
for version in 1.95.0 1.99.1 1.100.0; do
  rustup toolchain install "$version" --profile minimal --target wasm32-unknown-unknown || exit 1
done
(cd contracts/goal && cargo +1.95.0 test --locked --lib)
(cd bond-treasury && cargo +1.99.1 test --workspace --locked --lib)
(cd launcher && RUSTUP_TOOLCHAIN=1.100.0 npm test)
```

**Expect** → Cargo exits successfully for each suite. The Locker suite has 14 tests.
**Verify** → Read each Cargo test summary. Test coverage includes milestone rules,
Treasury accounting and Locker fee collection, recipient binding and failure paths.

**Do** → Run the integration suites on Ubuntu x86_64.

```sh
# cargo-near pulls hidapi, whose build script needs libudev and pkg-config
sudo apt-get update && sudo apt-get install -y libudev-dev pkg-config
cargo +1.95.0 install cargo-near --version 0.23.0 --locked
(cd contracts/goal && RUSTUP_TOOLCHAIN=1.95.0 cargo test --locked --tests -- --test-threads=1)
# the Treasury sandbox deploys target/wasm32-unknown-unknown/release/{bond_treasury,mock_goal}.wasm
(cd bond-treasury && RUSTUP_TOOLCHAIN=1.99.1 cargo build --workspace --locked --release --target wasm32-unknown-unknown)
(cd bond-treasury && bash tests/sandbox/run.sh)
```

**Expect** → Goal runs three standalone and six Treasury integration cases. The Treasury
runner installs its locked npm dependencies and verifies its pinned Linux sandbox download.
**Verify** → The commands exit zero, including the separate Treasury bind-before-deposit
case. See [sandbox prerequisites](../bond-treasury/README.md). The first run downloads
dependencies and the local sandbox binary. The shortened timeout in some tests is local-only.

## Read Goal and Treasury on NEAR Testnet

**Do** → Run `(cd client && npm run test:live)`.

**Expect** → 2 live tests pass. For `goal-4`, the contract accounts for 2 NEAR and shows
0.7 NEAR claimed. For `goal-5`, it accounts for 1 NEAR and shows 0.35 NEAR claimed.
Both show Accepted, Rejected, Pending. The two final cases in `goal-5` show Slashed and Refunded.

**Verify** → Read the [goal-4 run](DEMO_RUN.md) and [goal-5 run](DEMO_RUN_TREASURY.md).
Inspect the [0.7 NEAR payout](https://testnet.nearblocks.io/txns/23MxUoikemK6hvpmUsfvGvWcDV1cZwZdgEBtbA5E5TrQ),
[0.35 NEAR payout](https://testnet.nearblocks.io/txns/G1Z7L832NU1EXnQKTddojJUkRVk5MeVyN6xvwBcRBw1j),
[qualification refund](https://testnet.nearblocks.io/txns/EgPzisJsgadQr6rgV8R5DZGFXJS7cCShJRQLmMXkbaUX),
[slash](https://testnet.nearblocks.io/txns/3D5YQZoS2aMMMTkjA6nKeaVnWrmJC27YWqQijqUmqXpK),
and [refund after arbitration](https://testnet.nearblocks.io/txns/2bYLpXnHKtQ2BvUESrxFnjUbXPBi3RsqeYqaNFL7Ru9S).
Verify the uploaded WASM hash against `view_account.code_hash` for Goal and Treasury.
Account IDs and expected hashes are in [release metadata](../release/goal-treasury/deployment.testnet.json).

## Verify the historical Launcher run

**Do** → Run `(cd launcher && npm run verify)`.

**Expect** → 35 checks pass across the 62 final Testnet transactions. **Verify** → The
checks reconcile the token mint, decimals and supply, Rhea pool creation, position owner
and principal, both swap directions, collected and converted fees, Locker → Goal transfers,
`sync`, the 20/80 split, creator payout and the recorded negative cases.
The committed snapshots are historical readbacks. The verifier also checks actions,
events and transfer receipts against those snapshots.

**Do** → Re-fetch archive data into a temporary directory and rerun the same assertions.

```sh
(
  set -eu
  cd launcher
  evidence_dir=$(mktemp -d)
  printf 'Temporary evidence directory %s\n' "$evidence_dir"
  cp -R evidence/. "$evidence_dir/"
  rm "$evidence_dir/historical-receipts.json"
  node scripts/fetch-receipts.cjs "$evidence_dir/historical-receipts.json"
  PROOFEE_EVIDENCE_DIR="$evidence_dir" npm run verify
)
```

**Expect** → Fetch reports `PASS 62/62 receipts` and the verifier repeats the 35 checks.
A failed fetch exits nonzero. Rerun it with the same output path to resume after a rate limit.
**Verify** → Inspect the [transaction manifest](../launcher/evidence/transactions.csv),
[Launcher report](../launcher/REPORT.md) and the [actual Locker → Goal transfer](https://testnet.nearblocks.io/txns/DtbCwLB4qVQg6sZ8y6S4Zi7uXETSufSpVPoAajLLSUmT).
Its 1,191,987,982,122,365,775,599 yoctoNEAR matches the first Goal `sync` and yields
238,397,596,424,473,155,119 yoctoNEAR on `claim`, rounded down.

## Open the browser demo

**Do** → Open [goal-5](https://paonxeth.github.io/proofee/client/demo.html) and
[goal-4](https://paonxeth.github.io/proofee/client/demo.html?goal=goal-4.proofee-anton.testnet)
in a clean browser. To check this checkout before publication, run
`python3 -m http.server 8176 --bind 127.0.0.1` from the root, open
`http://127.0.0.1:8176/client/demo.html`, and use the same `?goal=` query for goal-4.

**Expect** → The respective milestone statuses and claims match the live tests.
**Verify** → The browser Network panel shows successful JSON-RPC reads to Testnet.
The current checkout tries FastNEAR first and retries transport/provider failures against
NEAR's Testnet RPC. Public RPC rate limits can make live checks fail temporarily.

## Scope

- Testnet evidence covers the deployed Goal/Treasury and the historical CLI launch and fee flow.
- The TypeScript wallet transaction builders have unit coverage. This quickstart never invokes a wallet.
- The 24-hour dispute timeout is exercised locally with shortened test timeouts. The run logs do not demonstrate a 24-hour public Testnet dispute.
- The separate [Vercel frontend](https://proofee-demo.vercel.app/) and
  [Launcher health](https://proofee.duckdns.org/health) are deployed services whose sources are
  outside this checkout. Health with `mode: plan` and `broadcastMode: disabled` confirms
  service availability in plan mode. Browser-based automated token launch is unverified.
- AI qualification and parallel challenge requests are planned. See [implementation status](IMPLEMENTATION_STATUS.md).
