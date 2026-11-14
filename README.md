# ProoFee

**Memecoin creator fees locked in an on-chain goal on NEAR and paid out only as public milestones are
accepted, so a creator cannot take all the fees and walk away.**

NEAR Hackathon 2026, memecoins track (track 11). **Testnet only**, nothing is deployed on mainnet.
The code was written with the help of Claude Code and Codex (hackathon rule Q-005).

## Try the live demo

Everything below runs on NEAR testnet. You need no wallet and no keys: the page reads the contracts
over JSON-RPC in your browser, and every step links to its transaction on nearblocks.

**Goal 5, bonded challenges** (a challenge costs a bond held by the Bond Treasury):

Milestones are numbered from 1 here and on the page. Contract calls and request ids use an index
from 0: milestone 1 is `index: 0`, and request `m0-2` is the second request on milestone 1.

1. Open [paonxeth.github.io/proofee/client/demo.html](https://paonxeth.github.io/proofee/client/demo.html)
   → you see "Ship ProoFee on NEAR testnet (bonded challenges)": three milestones, fee balances,
   the goal spec hash check, links to the goal and the Treasury.
2. Look at milestone 1 → **Accepted**; its request `m0-2` is Settled and the bond is **Slashed**
   (the challenger was wrong, the bond went to the buyback reserve). An earlier request on this
   milestone, `m0-1`, was not qualified and its bond was refunded (see the run log).
3. Look at milestone 2 → **Rejected**; request `m1-1` is Settled and the bond is **Refunded**
   (the challenger was right; they get the bond back, there is no extra reward).
4. Look at milestone 3 → **Pending**: no proof yet.
5. Click the goal link → on [nearblocks](https://testnet.nearblocks.io/address/goal-5.proofee-anton.testnet)
   you see every call of the run; the step-by-step list with each transaction is in
   [Demo goal 5](#demo-goal-5-bonded-challenges) below.

**Goal 4, free challenges** (the original mode, no Treasury):

1. Open [demo.html?goal=goal-4.proofee-anton.testnet](https://paonxeth.github.io/proofee/client/demo.html?goal=goal-4.proofee-anton.testnet)
   → milestone 1 **Accepted** (no challenge in the window), milestone 2 **Rejected** (challenged,
   the verifier rejected it), milestone 3 Pending.
2. The creator claimed 0.7 NEAR; the run with transactions is in [docs/DEMO_RUN.md](docs/DEMO_RUN.md).

## What we built

Statuses as in [docs/IMPLEMENTATION_STATUS.md](docs/IMPLEMENTATION_STATUS.md): **Testnet verified**
= run on testnet with a transaction; **Implemented** = in code and covered by tests, not run on
testnet; **Planned** = not built.

| Module | What it is | Status |
| --- | --- | --- |
| [contracts/goal](contracts/goal) | The goal escrow contract (Rust, near-sdk 5): fee accounting, immediate share, milestones, proofs, challenges, `claim`. No admin methods. [docs/SPEC.md](docs/SPEC.md) | Testnet verified (goal-4, goal-5) |
| Free challenges | anyone challenges, the verifier resolves | Testnet verified (goal-4) |
| Bonded challenges | the goal is connected to [bond-treasury](bond-treasury) (Bond Treasury v0.2): bond, qualify, arbitrate, refund or slash. Earlier Treasury runs against its Mock Goal: [TESTNET_RESULTS.md](bond-treasury/TESTNET_RESULTS.md) | Testnet verified (goal-5) |
| 24 h dispute timeout, retries of failed Treasury calls | `resolve_timeout`, `*Retryable` stages | Implemented (24 h does not fit in a testnet demo) |
| [launcher](launcher) | Node.js CLI and an immutable LP Locker (Rust): token, Rhea DCL pool, locked LP position, LP fees converted to NEAR and sent to a goal. [REPORT.md](launcher/REPORT.md) | Testnet verified, CLI only |
| [client](client) | Framework-free TypeScript client: reads a goal and Treasury bonds, builds unsigned transactions for a wallet; the read-only live page | Reading: testnet verified (live tests, live page). Transaction builders: unit tests only |
| Web app for trading or launching tokens | — | Planned: there is no web trading UI; launches go through the CLI |
| Reward for a successful challenger | — | Planned: a correct challenger only gets the bond back |
| Parallel challenge requests (fix for AUDIT M-4) | — | Planned |
| Mainnet deployment | — | Planned: testnet only |

## Known limitations

What this does not do, honestly. Codes like M-4 refer to findings in [docs/AUDIT.md](docs/AUDIT.md),
where each one has its fix status.

- **Testnet only.** Nothing is deployed on mainnet.
- **No independent audit.** [docs/AUDIT.md](docs/AUDIT.md) is our own review against the
  `near-contract-audit` checklist, with its findings and their fix status; no third party has
  audited the contracts.
- **No web trading.** Tokens are launched and traded through the [launcher](launcher) CLI and Rhea
  DCL directly; the live page only reads goals.
- **No reward for a successful challenger.** A correct challenge only gets its bond back; a
  slashed bond goes to the buyback reserve, not to anyone.
- **Do not send tokens (NEP-141) to a goal, including the memecoin itself and wNEAR.** The contract accounts
  for NEAR only. `ft_transfer` to a goal is an irreversible loss; `ft_transfer_call` fails and the
  tokens are returned.
- The verifier cannot challenge from their own account, but can from a second one and then reject
  the milestone. With a Bond Treasury such a challenge costs a bond; real protection still needs a
  multisig or DAO verifier (M-3).
- Bonded challenges: one request at a time per milestone, so a creator with a second account can
  file empty requests and block real challengers (M-4, open; see [Demo goal 5](#demo-goal-5-bonded-challenges)).
- If the creator deletes their top-level account, someone else can take the name and
  claim the claimable amount (L-5).
- In the demo the creator and the verifier belong to the same person: the verifier is a
  subaccount of `MASTER`. Real goals must not do this (L-5).
- Methods do not require 1 yoctoNEAR, so they can be called with a function-call key without
  wallet confirmation. A leaked function-call key of the verifier would allow rejecting milestones (L-6).
- While a payout is in flight, `unsynced` in `get_balances` may show an arrived refund as
  income (I-2). A challenge sent in the last seconds of the window may not make it in time (I-7).

## Quickstart for judges

**Without keys (2 minutes).** Open the [live demo](https://paonxeth.github.io/proofee/client/demo.html)
and follow [Try the live demo](#try-the-live-demo). Check any number against the chain:
[goal-5](https://testnet.nearblocks.io/address/goal-5.proofee-anton.testnet),
[Bond Treasury](https://testnet.nearblocks.io/address/treasury.proofee-bond-lab-20261112-1794481933490.testnet),
[goal-4](https://testnet.nearblocks.io/address/goal-4.proofee-anton.testnet) on nearblocks, and the
transaction tables in [docs/DEMO_RUN_TREASURY.md](docs/DEMO_RUN_TREASURY.md) and
[docs/DEMO_RUN.md](docs/DEMO_RUN.md).

**Locally.** Nothing here sends transactions.

```sh
git clone https://github.com/paonxeth/proofee && cd proofee

# client: Node.js 23.6 or newer (runs the TypeScript tests directly; checked on 26)
cd client && npm install && npm test   # offline unit tests
npm run test:live                      # read-only: goal-4 and goal-5 on testnet match the run logs
cd ..

# contract: Rust (pinned in contracts/goal/rust-toolchain.toml)
cd contracts/goal && cargo test        # unit + sandbox tests; the first run downloads near-sandbox
```

## How the contract works

Full description: [docs/SPEC.md](docs/SPEC.md); decisions: [docs/DECISIONS.md](docs/DECISIONS.md);
audit: [docs/AUDIT.md](docs/AUDIT.md); what is built and verified on testnet:
[docs/IMPLEMENTATION_STATUS.md](docs/IMPLEMENTATION_STATUS.md).

In short:
- `new` fixes the creator, the verifier, the goal spec hash (`goal_spec_hash`, sha256 of the file at
  `goal_uri`), the immediate share (`immediate_bps`), the challenge window and
  3–5 milestones with weights and deadlines. There are no admin methods and no parameter changes.
- Any NEAR received above the accounted balance is income. `sync` counts it.
- A milestone: `submit_proof` (creator) → challenge window → `finalize` (anyone) or
  `challenge` (anyone except the creator and the verifier) → `resolve` (verifier). If the verifier has not resolved
  the dispute within 24 h, anyone calls `resolve_timeout`, and the milestone is accepted for the creator. A
  milestone not submitted before its deadline is closed with `expire`.
- With a Bond Treasury (optional `treasury` in `new`, SPEC 6a) a challenge costs a bond
  B = clamp(F/5, 1, 5) NEAR, where F is the disputed milestone's share of counted fees. The
  challenger pays the bond into the Treasury, not the goal; the goal only instructs it. The verifier
  first `qualify`s the challenge (not admissible → bond refunded, milestone back to Submitted), then
  `arbitrate`s: proof valid → milestone Accepted, bond slashed to the buyback reserve; proof
  invalid → milestone Rejected, bond refunded. No decision within 24 h → Accepted, bond refunded.
- An accepted milestone's share becomes claimable by the creator; a rejected or expired milestone's
  share is forfeited, including from future fees. `claim` pays out the claimable amount. A failed transfer
  is rolled back in the callback.

## Demo goal 5: bonded challenges

[`goal-5.proofee-anton.testnet`](https://testnet.nearblocks.io/address/goal-5.proofee-anton.testnet)
with the Bond Treasury
[`treasury.proofee-bond-lab-20261112-1794481933490.testnet`](https://testnet.nearblocks.io/address/treasury.proofee-bond-lab-20261112-1794481933490.testnet).
Same milestones and spec as goal-4, 15-minute challenge window, release wasm from
[release/goal-treasury](release/goal-treasury) (code hash `4pdqE8BB…nt5T`, matches the network).
Run on 13 Nov 2026, every step with readbacks: [docs/DEMO_RUN_TREASURY.md](docs/DEMO_RUN_TREASURY.md).

Do X → see Y (fees: 1 NEAR, so F = 0.15 / 0.25 NEAR and every bond is the 1 NEAR minimum):

1. Challenger files a request with an empty reason, deposits 1 NEAR into the Treasury, confirms
   ([prepare](https://testnet.nearblocks.io/txns/HqypaBASGyHZFPM5CWaAp74LKeZoUAQdsYBmPga42r2X), [deposit](https://testnet.nearblocks.io/txns/cK5XTDDqNZWjwWcbKYaJu74HMvpGnqXNqR5MnUqFFVA), [confirm](https://testnet.nearblocks.io/txns/9xXnwLxdfLb831kkS8dSHJeFnyFqvKArWZUaDo6oGVqt))
   → milestone 1 Challenged, bond Bound.
2. Verifier: `qualify(false)` ([EgPzisJs](https://testnet.nearblocks.io/txns/EgPzisJsgadQr6rgV8R5DZGFXJS7cCShJRQLmMXkbaUX)) → bond **refunded** to the challenger in the
   same transaction, milestone 1 back to Submitted.
3. Second request on the same milestone ([prepare](https://testnet.nearblocks.io/txns/Gk7RKH91EMoaJFbvt5XwgBeUMQtmZerr4VtQU3N5ugqs), [deposit](https://testnet.nearblocks.io/txns/vQ2TnqVQRiWDUxiwnEzLeTPE5rs9A2oCtRxZypMUhW7), [confirm](https://testnet.nearblocks.io/txns/3ysmPXjmQ6H2Kp1UdjSjgRbWyVpN4wAVgNXibLRXcvLr)), verifier
   `qualify(true)` ([CXtr1rQv](https://testnet.nearblocks.io/txns/CXtr1rQvPrFBrnqG8Y8891knKPCNiviuDFeUNXuYYVge)) and `arbitrate(valid)` ([3D5YQZoS](https://testnet.nearblocks.io/txns/3D5YQZoS2aMMMTkjA6nKeaVnWrmJC27YWqQijqUmqXpK)) → milestone 1
   **Accepted**, bond **slashed**: Treasury `get_buyback_balance(goal-5)` = 1 NEAR.
4. Milestone 2, weak proof, challenged ([prepare](https://testnet.nearblocks.io/txns/GkX2iKTVcv8fWcLbA4f66Jm5qN5Y8sF9k5YNsxxX2Jw5), [deposit](https://testnet.nearblocks.io/txns/AkUJMpDtPusDoEqUbE445fDeSSginY55XcNPtmwf1wao), [confirm](https://testnet.nearblocks.io/txns/7dXPHLiSHnhVrAd3HbyPAQ5WoJ67tW1o7L3Ap8bfarqV)), qualified ([2RoiAdfv](https://testnet.nearblocks.io/txns/2RoiAdfvq7jqqNXdRgxF73ghgee8KrGHfrcVd1aum9GE)),
   `arbitrate(invalid)` ([2bYLpXnH](https://testnet.nearblocks.io/txns/2bYLpXnHKtQ2BvUESrxFnjUbXPBi3RsqeYqaNFL7Ru9S)) → milestone 2 **Rejected**, bond **refunded**.
5. `sync` ([39j6fazY](https://testnet.nearblocks.io/txns/39j6fazYgjXX9qCs5mo9svp1N93zA3sW7fGZr3P6LCYe)) → `total_received` is still 1 NEAR: bonds never become goal income.
6. Creator `claim` ([G1Z7L832](https://testnet.nearblocks.io/txns/G1Z7L832NU1EXnQKTddojJUkRVk5MeVyN6xvwBcRBw1j)) → 0.35 NEAR = 0.2 immediate + 0.15 of milestone 1; a
   second `claim` fails with `Nothing to claim` ([HpbMncVx](https://testnet.nearblocks.io/txns/HpbMncVxAfAnsHV4VsUgHLzozSh77sxZygNtS3zb2N8i)).

Result: milestone 1 Accepted, 2 Rejected, 3 Pending (closed later by a real post on X); bonds
Refunded / Slashed / Refunded. The challenger got both refunds back in full (gas only) and lost the
slashed bond.

Honestly, what this does not do yet:
- **No reward for the challenger.** A successful challenge (proof invalid) only returns the bond;
  the slashed bond of a failed challenge goes to the buyback reserve, not to anyone.
- **AUDIT M-4, open.** A request costs only gas and holds the milestone for up to 20 minutes before
  a bond is bound; one request at a time per milestone, so a creator with a second account can keep
  filing empty requests and block real challengers for the whole window. Each request also
  permanently takes storage in the Treasury. Fix after the hackathon: parallel requests, the first
  bound one wins ([docs/AUDIT.md](docs/AUDIT.md)).
- The Treasury admin is a different person than the goal creator; registration of a goal
  (`register_goal`, [FTA8LQNn](https://testnet.nearblocks.io/txns/FTA8LQNnQu3FoupeXzico2TypUr4gXS75iJbYUbPqCFo)) is done by them.

```sh
near contract call-function as-read-only goal-5.proofee-anton.testnet get_goal \
  json-args '{}' network-config testnet now
```

## Demo goal 4: free challenges

[`goal-4.proofee-anton.testnet`](https://testnet.nearblocks.io/address/goal-4.proofee-anton.testnet)
— "Ship ProoFee on NEAR testnet": 20% immediate, three milestones of 15/25/40% of all fees,
2-minute challenge window, the goal spec ([`specs/demo-goal.json`](specs/demo-goal.json)) pinned
on-chain by `goal_spec_hash`, the account has no keys.

- [docs/DEMO_RUN.md](docs/DEMO_RUN.md) — both demo scenarios run on this goal, every step with
  its transaction: milestone 1 accepted after the window, milestone 2 challenged and rejected,
  `claim` of 0.7 NEAR.
- [docs/IMPLEMENTATION_STATUS.md](docs/IMPLEMENTATION_STATUS.md) — what is verified on testnet,
  implemented, simulated or planned.
- Live page: [paonxeth.github.io/proofee/client/demo.html](https://paonxeth.github.io/proofee/client/demo.html)
  — goal-5 by default (with each bonded request and its bond status from the Treasury);
  goal-4: [`?goal=goal-4.proofee-anton.testnet`](https://paonxeth.github.io/proofee/client/demo.html?goal=goal-4.proofee-anton.testnet).
  The goal, milestones, balances and the spec hash check, read from testnet in the browser.
- [client/](client/) — framework-free TypeScript client for the frontend: reads a goal over
  JSON-RPC and builds unsigned transactions for a wallet.

```sh
near contract call-function as-read-only goal-4.proofee-anton.testnet get_goal \
  json-args '{}' network-config testnet now
near contract call-function as-read-only goal-4.proofee-anton.testnet get_balances \
  json-args '{}' network-config testnet now
```

The previous contract version is deployed at
[`goal-3.proofee-anton.testnet`](https://testnet.nearblocks.io/address/goal-3.proofee-anton.testnet)
(50% immediate, milestones 40/30/30, no `goal_spec_hash`, the verifier could still challenge).

## Tests

```sh
cd contracts/goal && cargo test
```

Unit tests run in the near-sdk mock. Integration tests (`contracts/goal/tests/integration.rs`)
start a local sandbox via near-sandbox; the first run downloads its binary.
`contracts/goal/tests/treasury.rs` runs the goal against the real Bond Treasury wasm
(`bond-treasury/release/bond_treasury.wasm`) in the sandbox: qualification rejected, valid,
invalid, timeout, outsiders, worst-case storage growth. It builds the goal with the test-only
feature `short-timeouts` (5-minute dispute timeout); the release wasm keeps 24 h.

## Deploying to testnet

You need Rust (the version is pinned in `contracts/goal/rust-toolchain.toml`), `cargo-near` and
`near-cli-rs`. The `MASTER` key must be in the keychain, and the verifier account
`verifier.$MASTER` must exist.

```sh
scripts/deploy-testnet.sh                        # draft goal, goal-<first free N>.proofee-anton.testnet
scripts/deploy-testnet.sh scripts/goals/demo.json  # goal parameters from a file
N=5 MASTER=me.testnet scripts/deploy-testnet.sh
DRY_RUN=1 scripts/deploy-testnet.sh scripts/goals/demo.json  # only print the new() arguments
TREASURY=<treasury>.testnet WASM_FILE=release/goal-treasury/contract.wasm \
  scripts/deploy-testnet.sh scripts/goals/demo-treasury.json  # bonded challenges, prebuilt wasm
```

`TREASURY` empty (default) = free challenges as before. With `TREASURY` the Treasury admin must
call `register_goal` for the new goal before the first challenge. `WASM_FILE` deploys a prebuilt
wasm without rebuilding and compares the network code hash with the file.

The goal spec (`spec_file` in the goal file, e.g. `specs/demo-goal.json`) must be committed and
pushed. The script passes `goal_uri` as a raw GitHub link pinned to the commit and
`goal_spec_hash` as the file's sha256.

The script builds the wasm and creates the goal in one transaction from `MASTER`: account, 3 NEAR, code and
`new`. The goal has no keys; NEAR can leave it only through `claim`.

## License

MIT, see [LICENSE](LICENSE).
