# Demo run (plan) — bonded challenges, goal-5.proofee-anton.testnet

Status: **run on 13 Nov 2026, 19:46–19:52 UTC, everything as planned** (log at the end). Every
command is checked against near-cli-rs 0.31.1 (`near --help`). Testnet only.

All three Treasury outcomes (SPEC 6a) on two milestones:

| Milestone | Proof | Request | Outcome |
| --- | --- | --- | --- |
| 0 — Goal Contract live | `proofs/m1.md` (strong) | 1st: empty reason | **QualificationRejected**: bond **refunded**, milestone back to Submitted |
| 0 | same proof | 2nd: disputed anyway, qualified | **ProofValid**: milestone Accepted, bond **slashed** to the buyback reserve |
| 1 — Public web app | `proofs/m2.md` (static mockup, weak) | "no live URL" | **ProofInvalidFinal**: milestone Rejected, bond **refunded** |
| 2 — Demo published on X | — | — | not touched: stays Pending, closed later by a real post on X |

Both requests on milestone 0 must be filed within its challenge window (15 min after the proof).

## Accounts and parameters

- Goal: `goal-5.proofee-anton.testnet` (does not exist yet, checked 13 Nov), built from branch
  `feat/goal-treasury`, `scripts/goals/demo-treasury.json`: same milestones and spec as
  `demo.json` (`specs/demo-goal.json`), immediate 2000 bps, **challenge window 900000 ms
  (15 min)** instead of 2 min: two requests on milestone 0, sent by hand with a readback after
  every transaction, must both fit into the window.
- Treasury: `treasury.proofee-bond-lab-20261112-1794481933490.testnet`, `proofee-treasury/0.2.0`,
  code sha256 `5dfb0699…0c03` — the same wasm as `bond-treasury/release/bond_treasury.wasm` used
  in our sandbox tests. Admin: `proofee-bond-lab-20261112-1794481933490.testnet`.
- Creator (MASTER): `proofee-anton.testnet`. Verifier: `verifier.proofee-anton.testnet`.
  Challenger: `proofee-stranger.testnet`. Keys of all three are in the keychain (checked).
- Milestone deadlines (from the spec): 0 — 14 Nov 20:59 UTC, 1 — 15 Nov, 2 — 21 Nov.
- Fees: 1 NEAR → locked 0.8 NEAR. F for milestone 0 = 0.8 × 18.75% = 0.15 NEAR, milestone 1 =
  0.25. Both below 5 NEAR, so **B = 1 NEAR** on every request (the 1 NEAR
  floor); R = F/10 (passed to the Treasury, no reward is paid).
- Deadlines per request: deposit within 10 min of `prepare_challenge`, bind (`confirm_escrow`)
  within 20 min, verifier decisions within 24 h after that.

## NEAR needed

Balances on 13 Nov: MASTER 2.28, verifier 1.00, stranger 7.997, Treasury 8 (free ≈ 1.83 after
storage 4.07, buyback 2, buffer 0.1). Gas on testnet: 300 TGas prepaid = at most 0.03 NEAR,
unused gas is refunded.

| Account | Spends | Needs at least | Has | Action |
| --- | --- | --- | --- | --- |
| proofee-stranger.testnet | 1.2 top-up to MASTER, 1 fees, 1 slashed bond, ≤ 0.2 gas (9 calls); 1 more bond is locked at a time and comes back | ≈ 4.4 | 7.997 | none; ends with ≈ 4.6 |
| proofee-anton.testnet | 3 to the goal account, deploy gas, 2 × `submit_proof` | ≈ 3.1 | 2.28 | **+1.2 from stranger** (step 1) |
| verifier.proofee-anton.testnet | 3 × `qualify` + 2 × `arbitrate`, ≤ 0.1 gas | 0.15 | 1.00 | none |
| goal-5 | gets 3 (storage) + 1 (fees); never holds a bond | — | — | — |
| Treasury | storage for 3 intents ≈ 0.015 from its free balance; holds the bonds | — | free ≈ 1.83 | none |
| Treasury admin | `register_goal` gas ≈ 0.001 | — | 3.56 | one call (step 0) |

## Steps

Transaction hashes are recorded in the log at the end.

Shell variables for all commands (zsh):

```zsh
GOAL=goal-5.proofee-anton.testnet
TREASURY=treasury.proofee-bond-lab-20261112-1794481933490.testnet
CREATOR=proofee-anton.testnet
VERIFIER=verifier.proofee-anton.testnet
CHALLENGER=proofee-stranger.testnet
RAW=https://raw.githubusercontent.com/paonxeth/proofee
# view helpers: JSON goes to stdout, the rest to stderr
gview() { near contract call-function as-read-only $GOAL "$1" json-args '{}' network-config testnet now 2>/dev/null; }
tview() { near contract call-function as-read-only $TREASURY "$1" json-args "$2" network-config testnet now 2>/dev/null; }
case_of() { gview get_goal | jq ".milestones[$1] | {status, case: (.case | {case_id, escrow_id, amount, stage, decision})}"; }
```

### 0. Treasury admin registers the goal (before step 4)

Sent by the Treasury admin, not by us. `register_goal` is append-only: the goal id must be exactly
`goal-5.proofee-anton.testnet`. It may be called before the goal exists.

```zsh
near contract call-function as-transaction treasury.proofee-bond-lab-20261112-1794481933490.testnet register_goal json-args '{"goal":"goal-5.proofee-anton.testnet","policy_version":"proofee-goal/1.1"}' prepaid-gas '30 Tgas' attached-deposit '0 NEAR' sign-as proofee-bond-lab-20261112-1794481933490.testnet network-config testnet sign-with-keychain send
```

Check (anyone): `tview get_goal '{"goal":"goal-5.proofee-anton.testnet"}'` →
`{"authorized":true,"policy_version":"proofee-goal/1.1"}`.

### 1. Top up MASTER from the challenger

```zsh
near tokens $CHALLENGER send-near $CREATOR '1.2 NEAR' network-config testnet sign-with-keychain send
```

### 2. Deploy the goal

```zsh
WASM_FILE=release/goal-treasury/contract.wasm DRY_RUN=1 N=5 TREASURY=$TREASURY scripts/deploy-testnet.sh scripts/goals/demo-treasury.json   # check arguments
WASM_FILE=release/goal-treasury/contract.wasm N=5 TREASURY=$TREASURY scripts/deploy-testnet.sh scripts/goals/demo-treasury.json
```

The prebuilt release wasm is deployed without rebuilding (code hash `4pdqE8BBA9HivuaHTNeV4xjzFYCwtXoPV54GJ8UQnt5T`, the script compares it with the network). The script checks `interface_version` and warns if step 0 is not done yet. Check: `get_goal` has
`"treasury": "<TREASURY>"`, keys list is empty.

### 3. Simulated fees: 1 NEAR + sync

```zsh
near tokens $CHALLENGER send-near $GOAL '1 NEAR' network-config testnet sign-with-keychain send
near contract call-function as-transaction $GOAL sync json-args '{}' prepaid-gas '30 Tgas' attached-deposit '0 NEAR' sign-as $CHALLENGER network-config testnet sign-with-keychain send
gview get_balances | jq '{total_received, locked}'   # 1 NEAR, 0.8 NEAR
```

Remember `total_received`: it must stay 1 NEAR to the end (bonds are never goal income).

### A bonded challenge on milestone I (used in 4, 5, 6)

`prepare_challenge` must come within the 15 min window after the proof; deposit and confirm right after.

```zsh
I=0; REASON='...'
near contract call-function as-transaction $GOAL prepare_challenge json-args "{\"index\":$I,\"reason\":\"$REASON\"}" prepaid-gas '300 Tgas' attached-deposit '0 NEAR' sign-as $CHALLENGER network-config testnet sign-with-keychain send
case_of $I                     # stage "Ready", amount "1000000000000000000000000"
ESCROW=$(gview get_goal | jq -r ".milestones[$I].case.escrow_id")
near contract call-function as-transaction $TREASURY deposit_bond json-args "{\"escrow_id\":\"$ESCROW\"}" prepaid-gas '30 Tgas' attached-deposit '1 NEAR' sign-as $CHALLENGER network-config testnet sign-with-keychain send
near contract call-function as-transaction $GOAL confirm_escrow json-args "{\"index\":$I}" prepaid-gas '300 Tgas' attached-deposit '0 NEAR' sign-as $CHALLENGER network-config testnet sign-with-keychain send
case_of $I                     # status "Challenged", stage "Bound"
tview get_bond "{\"escrow_id\":\"$ESCROW\"}" | jq '{status, decision, amount}'   # "Bound"
```

`deposit_bond` must attach exactly `amount` (1 NEAR); if `case_of` shows another amount, attach
that instead. If `confirm_escrow` leaves stage `BindRetryable`, repeat it (before the 20 min bind
deadline).

### 4. Milestone 0: qualification rejected, then proof valid → slash

```zsh
near contract call-function as-transaction $GOAL submit_proof json-args "{\"index\":0,\"proof_uri\":\"$RAW/5f7951238e4fce621b2c236f03f7caff51f0e1a7/proofs/m1.md\",\"proof_hash\":\"a11429a8997cdc318165b8d332bd66fa50f174ba6dcd32ac8c7275ca64fa6c5d\"}" prepaid-gas '30 Tgas' attached-deposit '0 NEAR' sign-as $CREATOR network-config testnet sign-with-keychain send

# 4a. First request, empty reason: I=0, REASON='' (case m0-1)
near contract call-function as-transaction $GOAL qualify json-args '{"index":0,"qualified":false}' prepaid-gas '300 Tgas' attached-deposit '0 NEAR' sign-as $VERIFIER network-config testnet sign-with-keychain send
case_of 0                                                  # Submitted, "Settled", "QualificationRejected"
tview get_bond "{\"escrow_id\":\"$ESCROW\"}" | jq .status   # "Refunded"

# 4b. Second request on the same milestone: I=0, REASON='The contract is not verified on-chain' (case m0-2)
near contract call-function as-transaction $GOAL qualify json-args '{"index":0,"qualified":true}' prepaid-gas '300 Tgas' attached-deposit '0 NEAR' sign-as $VERIFIER network-config testnet sign-with-keychain send
near contract call-function as-transaction $GOAL arbitrate json-args '{"index":0,"valid":true}' prepaid-gas '300 Tgas' attached-deposit '0 NEAR' sign-as $VERIFIER network-config testnet sign-with-keychain send
case_of 0                                                  # Accepted, "Settled", "ProofValid"
tview get_bond "{\"escrow_id\":\"$ESCROW\"}" | jq .status   # "Slashed"
tview get_buyback_balance "{\"goal\":\"$GOAL\"}"            # "1000000000000000000000000"
```

The second request needs the first one Settled (otherwise `Challenge in progress`) and must be
filed before the window closes (15 min after the proof).

### 5. Milestone 1: proof invalid → refund

```zsh
near contract call-function as-transaction $GOAL submit_proof json-args "{\"index\":1,\"proof_uri\":\"$RAW/5f7951238e4fce621b2c236f03f7caff51f0e1a7/proofs/m2.md\",\"proof_hash\":\"e177d2a3ab11b8ee1fc0367f5cdaab05f1b28bbc40a47f36f7494bb95b83966f\"}" prepaid-gas '30 Tgas' attached-deposit '0 NEAR' sign-as $CREATOR network-config testnet sign-with-keychain send
# bonded challenge: I=1, REASON='Criterion requires a live URL reading chain state; the proof is a static mockup'
near contract call-function as-transaction $GOAL qualify json-args '{"index":1,"qualified":true}' prepaid-gas '300 Tgas' attached-deposit '0 NEAR' sign-as $VERIFIER network-config testnet sign-with-keychain send
near contract call-function as-transaction $GOAL arbitrate json-args '{"index":1,"valid":false}' prepaid-gas '300 Tgas' attached-deposit '0 NEAR' sign-as $VERIFIER network-config testnet sign-with-keychain send
case_of 1                                                  # Rejected, "Settled", "ProofInvalidFinal"
tview get_bond "{\"escrow_id\":\"$ESCROW\"}" | jq .status   # "Refunded"
```

Check the challenger's balance before the deposit and after: the difference is gas only.

### 6. Final checks

```zsh
near contract call-function as-transaction $GOAL sync json-args '{}' prepaid-gas '30 Tgas' attached-deposit '0 NEAR' sign-as $CHALLENGER network-config testnet sign-with-keychain send
gview get_balances | jq '{total_received, unlocked, forfeited, claimable}'   # total_received still 1 NEAR
tview get_buyback_balance "{\"goal\":\"$GOAL\"}"                              # exactly 1 NEAR (one slash)
near account view-account-summary $CHALLENGER network-config testnet now      # ≈ 7.997 − 1.2 − 1 − 1 − gas
```

Expected goal balances: milestone 0 Accepted (15% of fees unlocked), milestone 1 Rejected
(25% forfeited), milestone 2 Pending. Challenger: 3 bonds deposited, 2 refunded, 1 slashed.

## Fallback: our own Treasury on proofee-anton.testnet

If the Treasury admin does not answer. Not needed otherwise.

- Account `treasury.proofee-anton.testnet` (Treasury `new` requires the `.testnet` suffix), code
  `bond-treasury/release/bond_treasury.wasm` (402 770 bytes, the same sha256 as the live one),
  `admin = proofee-anton.testnet`.
- Cost: code storage 402 770 B × 10¹⁹ = **4.03 NEAR**, state and account ≈ 0.01, operating
  buffer 0.1 (required by the solvency check), each intent ≈ 0.005. Minimum ≈ 4.15 NEAR;
  **send 5 NEAR**. The account has no keys and no withdraw method: these 5 NEAR are locked for
  good, and slashed bonds stay in its buyback reserve.
- MASTER would then need ≈ 3.1 + 5 = 8.1 NEAR (has 2.28): +6 NEAR. The stranger cannot cover it
  (it needs ≈ 3.2 for its own part out of 7.997), so ≈ 2 NEAR more from the faucet.
- Trade-off: the creator is then also the Treasury admin (could de-authorize the goal; exits and
  refunds still work without the admin). Acceptable for a demo, worth a line in the video.

```zsh
near transaction construct-transaction proofee-anton.testnet receiver-id treasury.proofee-anton.testnet \
  add-action create-account \
  add-action transfer '5 NEAR' \
  add-action deploy-contract use-file bond-treasury/release/bond_treasury.wasm without-init-call \
  add-action function-call new json-args '{"admin":"proofee-anton.testnet"}' prepaid-gas '100 Tgas' attached-deposit '0 NEAR' \
  skip network-config testnet sign-with-keychain send
near contract call-function as-transaction treasury.proofee-anton.testnet register_goal json-args '{"goal":"goal-5.proofee-anton.testnet","policy_version":"proofee-goal/1.1"}' prepaid-gas '30 Tgas' attached-deposit '0 NEAR' sign-as proofee-anton.testnet network-config testnet sign-with-keychain send
```

Then step 2 with `TREASURY=treasury.proofee-anton.testnet`. The treasury is fixed in `new`, so
decide before deploying goal-5.

## Log

| # | Step | Signed by | Result | Transaction |
| --- | --- | --- | --- | --- |
| 0 | `register_goal` goal-5, policy `proofee-goal/1.1` | proofee-bond-lab-20261112-1794481933490.testnet (Treasury admin) | `get_goal` → `{"authorized":true,"policy_version":"proofee-goal/1.1"}` | [FTA8LQNn…](https://testnet.nearblocks.io/txns/FTA8LQNnQu3FoupeXzico2TypUr4gXS75iJbYUbPqCFo) |
| 1 | Top up MASTER: 1.2 NEAR | proofee-stranger.testnet | MASTER 2.28 → 3.48, stranger 7.997 → 6.797 | [BAmeoybo…](https://testnet.nearblocks.io/txns/BAmeoyboGvWsEMJp6JkBRGfn9vzHmk4Xr9PMJUXBjMmf) |
| 2 | Deploy goal-5: create, 3 NEAR, release wasm, `new` with treasury | proofee-anton.testnet | `goal_created`; code_hash network = file `4pdqE8BB…nt5T`; no keys; `get_goal.treasury` set, window 900000 | [56MbQQPB…](https://testnet.nearblocks.io/txns/56MbQQPB4ZVouiZxu1pxzChAdnyPUQPBQmc2CHGGBA2N) |
| 3a | Fees: 1 NEAR to goal-5 | proofee-stranger.testnet | — | [BKUjgdpV…](https://testnet.nearblocks.io/txns/BKUjgdpVNuSHc6ufJeuzkh2tdgpMKz3Xi9ZyFMksQ7g7) |
| 3b | `sync` | proofee-stranger.testnet | total_received 1, immediate 0.2, locked 0.8 | [5yvKwpSB…](https://testnet.nearblocks.io/txns/5yvKwpSBRiNQ4jwHuFdS5NyJ4iB1Lse9CHBhEdSkyE3e) |
| 4 | `submit_proof` 0 (`m1.md`), 19:49:09 UTC | proofee-anton.testnet | Submitted; window to 20:04:09 | [2SJb8hMx…](https://testnet.nearblocks.io/txns/2SJb8hMxrs7CzvyAhLZ5fLRhZr8JnzXh3fctfenfYc9w) |
| 4a.1 | `prepare_challenge` 0, empty reason | proofee-stranger.testnet | case m0-1 Ready, B = 1 NEAR, F = 0.15; escrow `026a1a1e…`; Treasury `intent_registered` | [HqypaBAS…](https://testnet.nearblocks.io/txns/HqypaBASGyHZFPM5CWaAp74LKeZoUAQdsYBmPga42r2X) |
| 4a.2 | Treasury `deposit_bond` 1 NEAR | proofee-stranger.testnet | bond Deposited | [cK5XTDDq…](https://testnet.nearblocks.io/txns/cK5XTDDqNZWjwWcbKYaJu74HMvpGnqXNqR5MnUqFFVA) |
| 4a.3 | `confirm_escrow` 0 | proofee-stranger.testnet | milestone Challenged, case Bound; bond Bound | [9xXnwLxd…](https://testnet.nearblocks.io/txns/9xXnwLxdfLb831kkS8dSHJeFnyFqvKArWZUaDo6oGVqt) |
| 4a.4 | `qualify` 0 false | verifier.proofee-anton.testnet | milestone **Submitted**, case Settled QualificationRejected; bond **Refunded**; stranger −0.0013 (gas only) | [EgPzisJs…](https://testnet.nearblocks.io/txns/EgPzisJsgadQr6rgV8R5DZGFXJS7cCShJRQLmMXkbaUX) |
| 4b.1 | `prepare_challenge` 0, "The contract is not verified on-chain" | proofee-stranger.testnet | case m0-2 Ready (attempts 2); escrow `d1bf875d…` | [Gk7RKH91…](https://testnet.nearblocks.io/txns/Gk7RKH91EMoaJFbvt5XwgBeUMQtmZerr4VtQU3N5ugqs) |
| 4b.2 | Treasury `deposit_bond` 1 NEAR | proofee-stranger.testnet | bond Deposited | [vQ2TnqVQ…](https://testnet.nearblocks.io/txns/vQ2TnqVQRiWDUxiwnEzLeTPE5rs9A2oCtRxZypMUhW7) |
| 4b.3 | `confirm_escrow` 0 | proofee-stranger.testnet | milestone Challenged, case Bound; bond Bound | [3ysmPXjm…](https://testnet.nearblocks.io/txns/3ysmPXjmQ6H2Kp1UdjSjgRbWyVpN4wAVgNXibLRXcvLr) |
| 4b.4 | `qualify` 0 true | verifier.proofee-anton.testnet | case Qualified; bond Bound | [CXtr1rQv…](https://testnet.nearblocks.io/txns/CXtr1rQvPrFBrnqG8Y8891knKPCNiviuDFeUNXuYYVge) |
| 4b.5 | `arbitrate` 0 valid | verifier.proofee-anton.testnet | milestone **Accepted**, case Settled ProofValid; bond **Slashed**; buyback 1 NEAR | [3D5YQZoS…](https://testnet.nearblocks.io/txns/3D5YQZoS2aMMMTkjA6nKeaVnWrmJC27YWqQijqUmqXpK) |
| 5.1 | `submit_proof` 1 (`m2.md`) | proofee-anton.testnet | Submitted | [GxwyDatf…](https://testnet.nearblocks.io/txns/GxwyDatf9ZzhFow1TYowTqye634qYTmEsLxt2s6otRSS) |
| 5.2 | `prepare_challenge` 1, "Criterion requires a live URL…" | proofee-stranger.testnet | case m1-1 Ready, F = 0.25, B = 1; escrow `44e4ed8b…` | [GkX2iKTV…](https://testnet.nearblocks.io/txns/GkX2iKTVcv8fWcLbA4f66Jm5qN5Y8sF9k5YNsxxX2Jw5) |
| 5.3 | Treasury `deposit_bond` 1 NEAR | proofee-stranger.testnet | bond Deposited | [AkUJMpDt…](https://testnet.nearblocks.io/txns/AkUJMpDtPusDoEqUbE445fDeSSginY55XcNPtmwf1wao) |
| 5.4 | `confirm_escrow` 1 | proofee-stranger.testnet | milestone Challenged, case Bound; bond Bound | [7dXPHLiS…](https://testnet.nearblocks.io/txns/7dXPHLiSHnhVrAd3HbyPAQ5WoJ67tW1o7L3Ap8bfarqV) |
| 5.5 | `qualify` 1 true | verifier.proofee-anton.testnet | case Qualified | [2RoiAdfv…](https://testnet.nearblocks.io/txns/2RoiAdfvq7jqqNXdRgxF73ghgee8KrGHfrcVd1aum9GE) |
| 5.6 | `arbitrate` 1 invalid | verifier.proofee-anton.testnet | milestone **Rejected**, case Settled ProofInvalidFinal; bond **Refunded**; stranger −0.0013 (gas only) | [2bYLpXnH…](https://testnet.nearblocks.io/txns/2bYLpXnHKtQ2BvUESrxFnjUbXPBi3RsqeYqaNFL7Ru9S) |
| 6 | `sync` | proofee-stranger.testnet | total_received **still 1**; unlocked 0.15, forfeited 0.25, pending 0.4, claimable 0.35; buyback 1 NEAR; goal-5 balance 4 NEAR (3 + 1 fees, no bond) | [39j6fazY…](https://testnet.nearblocks.io/txns/39j6fazYgjXX9qCs5mo9svp1N93zA3sW7fGZr3P6LCYe) |

Final state: milestone 0 Accepted (m0-2 ProofValid, bond slashed), milestone 1 Rejected (m1-1
ProofInvalidFinal, bond refunded), milestone 2 Pending. Treasury: 3 bonds — `026a1a1e…` Refunded,
`d1bf875d…` Slashed, `44e4ed8b…` Refunded; `get_buyback_balance(goal-5)` = 1 NEAR. Balances after
the run: stranger 4.7907 (6.797 − 1 fees − 1 slashed − 0.006 gas), MASTER 0.4726, verifier 0.9952.
