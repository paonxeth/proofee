# Audit of the `contracts/goal` contract

Date: 10 November 2026 · commit `55571d0` · checked against `docs/SPEC.md` v1.0.
Files: `src/lib.rs`, `src/math.rs`, `src/events.rs`. The code was not changed.

## How it was checked

- Manual review against the `near-contract-audit` checklist: callbacks and `#[private]`, the
  "state → transfer" order, access control, callback gas, storage, arithmetic, JSON types.
- `cargo test`: 66 unit tests and 2 integration tests, all green.
- Separate measurements in the sandbox (near-sandbox). The test lived in a copy of the crate in a
  temporary folder and did not get into the repository:
  - **A.** Maximum-size state: 5 milestones, all strings at the limit and made of
    4-byte characters (`𝕏`), 5 proofs, 5 challenges from a stranger. Then `claim`.
  - **B.** A goal deployed with a balance of "storage + 5 bytes", `immediate_bps = 10000`,
    1 NEAR received, then `claim`.

| Measurement | Result |
| --- | --- |
| wasm | 159 835 bytes, about 1.6 NEAR for storage |
| storage after `new` (maximum strings) | 163 615 bytes |
| + 5 proofs (creator only) | +5 520 bytes |
| + 5 challenges (any account) | +5 720 bytes (about 0.057 NEAR at the contract's expense) |
| `on_claim_transfer` gas on the maximum state | **3.0 TGas** of the allotted 10 |
| `claim` with 15 TGas prepaid gas | succeeds |
| contract balance after 10+ calls with no income | unchanged, `unsynced = 0` (the 30% gas reward does not reach the contract) |
| measurement B | `claim` fails with `LackBalanceForState` (1.1·10²⁰ yocto = 11 bytes short), see L-1 |

## Summary

The accounting is correct. The refund of a failed payout is not counted as income in any
receipt order. There is no reentrancy. The callback is protected by `#[private]` and has about
3× the gas it needs. Overflows are ruled out. **There are no High findings.**

The main risks are in the rules of the game, not in the arithmetic. A stranger can freeze any
milestone for free if the verifier is inactive or deleted. The `new` parameters are almost
unbounded from above and below. There is one real storage edge case in which
`claim` stops working forever.

---

## Findings

### M-1. A free challenge with no timeout freezes milestones · Medium · known (2)

**Where:** `src/lib.rs:258` (`challenge`), `src/lib.rs:292` (`resolve`).

**Scenario.** Any account except the creator can call `challenge`. There is no bond; gas
costs about 0.0003 NEAR. The only way out of Challenged is `resolve`, and only the
verifier can call it.
1. An attacker challenges every submitted proof. That is 5 transactions per goal.
2. If the verifier does not respond (lost the key, abandoned the project, account deleted, see L-5),
   the milestones stay in Challenged forever. Their share stays in `pending` forever: it never
   becomes `unlocked` or `forfeited`. The creator gets nothing but `immediate`.
3. Even with an active verifier, anyone can force them to review every milestone by hand.

**Fix (cheap, about 30 lines and 3 tests).** Add a `challenged_at_ms: Option<u64>` field to
`Milestone` and a method
`resolve_timeout(index)`: anyone can call it if `now ≥ challenged_at_ms +
RESOLVE_TIMEOUT_MS` (for example 7 days, a constant). The default outcome is **Accepted**:
a challenge is free, and with Rejected as the default anyone could forfeit the creator's
share. SPEC sections 4, 6, 7 need editing and the decision goes into DECISIONS. No migration
is needed: every goal is deployed fresh.

### M-2. No lower bound on the challenge window · Medium · known (4)

**Where:** `src/lib.rs:135`.

**Scenario.** `new(..., challenge_window_ms: 1, ...)` succeeds. `challenge` requires
`now < submitted_at_ms + 1`, i.e. execution in the same block as `submit_proof`. In
practice this is impossible: someone else's transaction executes in the next block, and nobody sees the proof before
it is published. Already in the next block (about 1 s) anyone can call `finalize`.
As a result the creator accepts all their own milestones, and the verifier is not needed. With a 1–5
second window the picture is almost the same.

**Answer to the question "can a 1 ms window be passed": yes, it goes through.**

**Fix (1 line and a test).** `const MIN_CHALLENGE_WINDOW_MS: u64 = 60_000;` and
`require!(challenge_window_ms >= MIN_CHALLENGE_WINDOW_MS, "Window too short")`.
The demo scenario (120 000) and the integration test (600 000) pass. The upper bound
is best added in the same change (see L-3).

### M-3. The verifier can challenge and reject a milestone themselves · Medium · known (1) · partially fixed

**Where:** `src/lib.rs:261` (only `!= creator` is checked), `src/lib.rs:292`.

**Scenario.** The verifier calls `challenge(i, "...")`, then `resolve(i, false)`. The milestone's
share is forfeited from both past and future fees. No signal from a third party is
needed.

**The cheap fix is partial.** Add
`require!(challenger != self.verifier, "Verifier cannot challenge")`. It is one line,
but it protects weakly: the verifier will challenge from another account. The benefit is that
the `proof_challenged` events will not show obvious self-dealing. Real protection is a multisig
or DAO verifier (SPEC 2) and a challenge bond. Both are outside the MVP.

### L-1. `claim` does not account for the 16 bytes of `payout_in_flight`: payouts can break forever · Low · confirmed

**Where:** `src/lib.rs:332-339` (the reserve is computed from `env::storage_usage()` before the state
is written), `src/lib.rs:351`.

**Scenario.** `payout_in_flight: Option<NearToken>` takes 1 byte in Borsh as `None`
and 17 bytes as `Some`. In `claim` the reserve is computed from the current `storage_usage`. near-sdk
writes the state only after the method body, so the extra 16 bytes are not included in the
reserve. If `claimable ≥ free`, the contract pays out everything free. At the end of the receipt
the runtime sees a shortfall of 16 × 10¹⁹ yocto and rolls back the whole `claim` with
`LackBalanceForState`.

Measurement B confirms this. Worse, with `immediate_bps = 10000` (or once all milestones
are accepted) the difference `free − claimable` equals the initial margin and does not grow with new fees:
every new income increases both the balance and `claimable` by the same amount. So
`claim` will never work again. The money is not lost, but the creator will not receive it.

`scripts/deploy-testnet.sh` deposits 3 NEAR with storage around 1.6 NEAR, so this does not
reproduce in the demo. The problem appears if the deploy deposit is reduced or
if the frontend deploys goals itself.

**Fix (2 lines and a test).** Subtract a constant reserve in `claim`:
`const STORAGE_RESERVE_BYTES: u64 = 1_000;` and
`storage_cost = (storage_usage + STORAGE_RESERVE_BYTES) × byte_cost`. Plus L-2.

### L-2. Storage grows at the contract's expense, no minimum reserve in `new` · Low

**Where:** `src/lib.rs:267` (`reason`), `src/lib.rs:244` (`proof_uri`),
`src/lib.rs:97` and `:130` (limits in characters), `src/lib.rs:119` (`new`).

**Scenario.**
- Any account except the creator can write `challenger` (up to
  64 bytes) and `reason` (280 characters, i.e. up to 1 120 bytes in UTF-8) once per milestone. Measured: +5 720 bytes
  for 5 milestones, about 0.057 NEAR. Growth is bounded: each milestone can be challenged only
  once. There is no unbounded bloat.
- The contract pays. If the margin above storage is smaller than this amount, it eats into `pending` and
  `claimable`: `claim` is capped by `free` (`src/lib.rs:339`), and in the extreme case it breaks
  (L-1). Worse, if there is no margin at all, `challenge` itself fails with
  `LackBalanceForState`, and challenging a proof becomes impossible.
- Limits are counted in characters (`chars().count()`). This matches SPEC, but in bytes
  it is up to 4 times more.

**Fix (2 lines and a test).** At the end of `new` check
`require!(env::account_balance() >= storage_cost(storage_usage + MAX_STATE_GROWTH_BYTES),
"Deposit too small for storage")`, where `MAX_STATE_GROWTH_BYTES` ≈ 16 000. Per measurement A
the full growth is about 11 300 bytes, plus 16 bytes from L-1. Alternative: check a minimum
margin, for example 0.2 NEAR above the current storage. Limits in bytes (`len()` instead of
`chars().count()`) are optional; that is a SPEC change.

### L-3. No upper bounds on the window and deadlines: a milestone can be locked forever · Low

**Where:** `src/lib.rs:109-114` (`window_end`), `src/lib.rs:135`, `src/lib.rs:146`.

**Scenario.**
- `challenge_window_ms = u64::MAX`. After `submit_proof`, `window_end` fails with
  `"Math overflow"`, so both `challenge` and `finalize` always fail. The milestone stays
  in Submitted forever (`expire` works only from Pending), its share is in
  `pending` forever. A 100-year window is effectively the same.
- `deadline_ms = u64::MAX`. The milestone never becomes Failed. The creator can skip the
  proof and the share is never forfeited: the promise "failure forfeits fees" loses its force.
- `deadline_ms = now + 1`. The milestone fails immediately. Not dangerous, but pointless.

**Fix (2 lines and tests).** `MAX_CHALLENGE_WINDOW_MS = 30 days`,
`require!(m.deadline_ms <= now_ms + MAX_DEADLINE_MS)` with `MAX_DEADLINE_MS = 365 days`.

### L-4. `creator` or `verifier` can equal the contract's own address · Low

**Where:** `src/lib.rs:128`.

**Scenario.** The goal has no keys, so nobody can sign a call on the contract's behalf.
- `verifier == current_account_id`: nobody can call `resolve`. Any stranger
  challenges a proof and the milestone is frozen forever (as in M-1, only immediately).
- `creator == current_account_id`: nobody can call `claim`, all the money is locked.

**There is a `verifier == creator` check** (test `new_rejects_verifier_equal_to_creator`).
Different accounts of the same person, or a verifier that is a subaccount of the creator
(`v.creator.testnet`), cannot be told apart by the contract.

**Fix (2 lines and a test).** Add
`require!(creator != env::current_account_id() && verifier != env::current_account_id(), "...")`.

### L-5. Deleted verifier or creator account · Low (answer to a question)

**Where:** `src/lib.rs:295`, `src/lib.rs:324`.

**What happens.**
- **The verifier is deleted.** Nobody can call `resolve`, all Challenged milestones are frozen
  forever (M-1). While the account is deleted, new proofs can be challenged, and the same awaits them.
- **The name is freed.** A top-level account (`proofee-verifier.testnet`, on mainnet
  `*.near`) can be **registered by anyone** through the registrar after deletion. The new
  owner becomes the verifier with all its rights. A subaccount (`v.team.testnet`)
  can be recreated only by its parent. An implicit account is recreated by a transfer, but
  is still controlled only by the key owner.
- **The creator is deleted.** The transfer in `claim` fails. The callback rolls the counters back, the refund
  is not counted as income (covered by tests). But nobody can call `claim` any more. If
  someone else takes the top-level name, they take everything claimable.

**Fix.** The timeout from M-1 covers the verifier case. Organizationally:
the verifier is a subaccount of the team account (`verifier.proofee.testnet`), later a
multisig or DAO. `scripts/deploy-testnet.sh` currently uses
`proofee-verifier.testnet`, which is a top-level account.

### L-6. Sensitive methods can be called with a limited access key · Low

**Where:** `src/lib.rs:234` (`submit_proof`), `src/lib.rs:292` (`resolve`),
`src/lib.rs:322` (`claim`).

**Scenario.** The methods do not require 1 yocto, so they can be called with a function-call
key without wallet confirmation. If the frontend obtains such a key for the goal contract at sign-in
and stores it in the browser, an XSS or a leak of the verifier's key would allow
rejecting milestones (`resolve(i, false)`). For `claim` there is no harm: the money goes to the
creator anyway.

**Fix.** `#[payable]` plus `assert_one_yocto()` on `resolve` and, optionally, on
`submit_proof`. This contradicts SPEC 7 ("methods without `#[payable]`") and changes the UX: every
call is confirmed in the wallet. **Not recommended before 14 November.** It is enough not to request
a function-call key for the goal contract from the wallet.

### I-1. Callback gas: 10 TGas is enough · Info

**Where:** `src/lib.rs:15`, `src/lib.rs:353-357`.

Measurement A: on the maximum state (about 11 KB of Borsh) `on_claim_transfer` burns
3.0 TGas, a margin of about 3.3×. Unused `claim` gas is distributed to the callback
on top of the static 10 TGas: `claim` with 15 TGas prepaid gas succeeded. If `claim`
had less gas than needed to create the callback, the whole transaction would fail and
the state would roll back. No stuck flag would result. The callback itself cannot fail:
`checked_sub`/`checked_add` are protected by an invariant (`claimed ≥ amount`, because
`amount` was just added), and `promise_result_checked(0, 0)` gets an empty result
for a transfer.

### I-2. Income accounting during an asynchronous payout is correct · Info

**Where:** `src/lib.rs:212-231`, `:343-351`, `:363-384`.

All orderings of "payout → refund → callback → new income" were analyzed:
- refund before the callback: `sync` is skipped, the callback puts `last_balance` back;
- refund after the callback: the balance is temporarily below `last_balance`, `sync` does not lower it;
- income X arrives between a rollback callback and the refund: `sync` temporarily counts
  `max(0, X − amount)`, the rest is counted when the refund arrives. A temporary undercount,
  not a double count. The total always converges; covered by
  `income_during_failed_payout_counted_once` and neighboring tests.

Minor: while a payout is in flight, `get_balances().unsynced` shows an arrived refund as
"uncounted income". The frontend should not show `unsynced` during an active payout.
`deposit()` also silently counts nothing at that time (`sync` is skipped); the money
is counted later.

### I-3. The callback trusts the `amount` argument · Info

**Where:** `src/lib.rs:364`.

The rollback amount comes from the argument, not from `payout_in_flight`. This is safe:
`#[private]` (confirmed by the integration test `on_claim_transfer_is_private`), the goal
has no keys, and a fake call without a promise result fails by itself. For consistency one could use
`self.payout_in_flight.take().unwrap_or(amount)`: that version does not fail and does not violate
SPEC 5.3 step 7. Optional.

### I-4. `new` without `#[private]`; template workflows with `without-init-call` · Info

**Where:** `src/lib.rs:118`, `contracts/goal/.github/workflows/deploy-*.yml`.

`#[init]` prevents calling `new` a second time. The first call is open to everyone, but
`scripts/deploy-testnet.sh` does create, deploy and `new` in one transaction, so
there is no window to front-run it. `#[private]` would break the deploy here: the predecessor is MASTER.
However, cargo-near templates remain in `contracts/goal/.github/workflows/`:
`deploy-production.yml` runs on push to `main` and does
`cargo near deploy ... without-init-call`. GitHub does not run them from a subfolder, but if they
are moved to the root, the contract ends up without `new`, and the first comer can initialize it
with their own `creator`. Besides, they mention "production", which conflicts with the
"testnet only" rule. Recommend deleting them.

### I-5. Fees in tokens (NEP-141) · Info · known (3)

**Where:** the whole contract; SPEC 2, 12.

`ft_transfer` of the memecoin or wNEAR to a goal is an **irreversible loss**: there are no keys and
no withdrawal method. `ft_transfer_call` fails (there is no `ft_on_transfer`), and the tokens are returned.
There is no cheap fix within the MVP. The minimal safe step is
an `ft_on_transfer` that returns the whole amount (`PromiseOrValue::Value(amount)`), but
it does not help against a plain `ft_transfer`. For testnet it is enough to state this in the
README and in the frontend. Before mainnet, `sync_ft` and token payouts are needed (SPEC 12).

### I-6. Arithmetic and rounding · Info (no issues)

**Where:** `src/math.rs:22-61`, `src/lib.rs:447-463`.

- `R × 10000` overflows u128 only for R > 3.4·10³⁴ yocto, which is 28 times more than
  the entire NEAR supply. Plus `checked_mul` and `overflow-checks = true` in release.
- `W_acc + W_lost ≤ 10000`, so `U + F ≤ L` and `pending` never goes negative.
- `I + U` only grows (R and W_acc never decrease), and `claimed` grows by at most
  `claimable`, so `claimed ≤ I + U` always.
- The rounding remainder (up to 3 yocto per call) stays in the contract. The contract never pays
  out more than it received, as SPEC 9 requires.
- No `float`. `u64` values in JSON (`deadline_ms`, window) are milliseconds; below 2⁵³ JS does not
  distort them.

### I-7. Time boundaries are consistent · Info (no issues)

`submit_proof`: `now < deadline`, `expire`: `now ≥ deadline`. `challenge`: `now < end`,
`finalize`: `now ≥ end`. No overlaps and no gaps; there are tests on the exact boundaries.
Practical note: a transaction executes 1–2 blocks after it is sent, so
a challenge sent a second before the window ends may get `"Window closed"`.
The frontend should warn when less than 10 s of the window is left.

### I-8. Minor items · Info

- Empty `proof_uri` and `reason` are allowed. This does not contradict SPEC, but an empty proof
  looks odd (`src/lib.rs:244`, `:267`).
- `set_token` does not validate the address and is set once. A creator mistake cannot be fixed
  (`src/lib.rs:194`). The field is informational, no harm.
- `Cargo.toml`: `repository = "https://github.com/<xxx>/<xxx>"` and the template
  `description` end up in the contract's NEP-330 metadata.

---

## Access control by method

| Method | Who can call | Check | Notes |
| --- | --- | --- | --- |
| `new` | anyone, once (`#[init]`) | no state yet | atomic deploy (I-4); parameter bounds: M-2, L-3, L-4 |
| `set_token` | creator, once | `:196`, `:199` | — |
| `deposit` | anyone, `#[payable]` | — | — |
| `sync` | anyone | — | does nothing while a payout is in flight |
| `submit_proof` | creator | `:237` | L-6 |
| `challenge` | anyone except creator | `:261` | M-1, M-3 |
| `finalize` | anyone | time | — |
| `resolve` | verifier | `:295` | L-5, L-6 |
| `expire` | anyone | time | — |
| `claim` | creator | `:324` | money goes to the creator only |
| `on_claim_transfer` | the contract itself only (`#[private]`) | confirmed by an integration test | I-3 |
| `get_goal`, `get_balances` | view | — | — |

There are no admin methods and no parameter-changing methods, as SPEC 7 requires.

## Compliance with SPEC

There are no behavioral deviations from SPEC: the 5.2 formulas, the 5.3 step order, the section 6
transitions, the section 8 events and the section 3 `new` checks are implemented as written. The texts of
the additional errors (`"Creator and verifier must differ"`, `"Need 1-5 milestones"`,
`"Token already set"`, `"Math overflow"` and others) are not listed in SPEC 7. This is not a
violation, but they should be added to the list.

All findings above are gaps in the SPEC itself (checks it does not require), not deviations
of the code from it. So every fix starts with a SPEC edit.

## Summary table

| ID | Severity | Location | Issue | Known | Fix | Cost |
| --- | --- | --- | --- | --- | --- | --- |
| M-1 | Medium | lib.rs:258, :292 | a free challenge with no timeout freezes milestones | 2 | `resolve_timeout` → Accepted | ~30 lines |
| M-2 | Medium | lib.rs:135 | a 1 ms window disables challenges | 4 | `MIN_CHALLENGE_WINDOW_MS` | 1 line |
| M-3 | Medium | lib.rs:261 | the verifier challenges and rejects by themselves | 1 | `challenger != verifier` (partial) | 1 line |
| L-1 | Low | lib.rs:332-351 | +16 bytes of `payout_in_flight` → `claim` breaks forever | — | reserve bytes in the cap | 2 lines |
| L-2 | Low | lib.rs:119, :267 | storage grows at the contract's expense, no minimum deposit | — | balance check in `new` | 2 lines |
| L-3 | Low | lib.rs:109, :135, :146 | `u64::MAX` window or deadline locks a milestone | — | upper bounds | 2 lines |
| L-4 | Low | lib.rs:128 | creator/verifier = the contract address | — | 2 `require!` | 2 lines |
| L-5 | Low | lib.rs:295, :324 | deleted account; someone else can take a top-level name | — | M-1 + verifier subaccount | organizational |
| L-6 | Low | lib.rs:234, :292 | no `assert_one_yocto` | — | after the hackathon | — |
| I-1 | Info | lib.rs:15 | callback: 3.0 of 10 TGas | — | not needed | — |
| I-2 | Info | lib.rs:212-384 | accounting during payout and refund is correct; `unsynced` in flight | — | frontend | — |
| I-3 | Info | lib.rs:364 | the callback takes `amount` from the argument | — | optional | 1 line |
| I-4 | Info | lib.rs:118, workflows | template `deploy-*.yml` with `without-init-call` | — | delete the files | — |
| I-5 | Info | — | tokens sent via `ft_transfer` are lost | 3 | README and frontend; `sync_ft` before mainnet | — |
| I-6 | Info | math.rs | arithmetic is fine | — | — | — |
| I-7 | Info | lib.rs | time boundaries are fine | — | warning in the frontend | — |
| I-8 | Info | lib.rs, Cargo.toml | minor items | — | optional | — |

## What to fix before 14 November

Each item is a separate step: SPEC edit, then code, then test, then `cargo test`.

1. **Parameter bounds in `new`** (M-2, L-3, L-4). One edit to SPEC section 3 and
   about 6 `require!`s: window from 60 s to 30 days, deadline at most 365 days ahead, creator and
   verifier differ from the contract address. Breaks nothing in the demo, and it is the cheapest
   way to close known limitation 4.
2. **Storage reserve** (L-1, L-2): a constant reserve in `claim` and a minimum
   balance check at the end of `new`. Two lines, but they remove the only scenario found
   in which the creator's money becomes permanently unavailable.
3. **Dispute timeout** (M-1), if there is half a day for it. This is the most useful change in
   substance: it closes known limitation 2 and the deleted-verifier case
   (L-5). It first needs a team decision on the default outcome (Accepted recommended) and
   an entry in DECISIONS.
4. **No code:** delete `contracts/goal/.github/workflows/deploy-*.yml` (I-4). In
   `deploy-testnet.sh` use a verifier subaccount (L-5). State in the README that
   tokens must not be sent to a goal (I-5).

**Do not touch before the deadline:** `assert_one_yocto` (L-6, changes UX and SPEC), accepting tokens
(I-5), `challenger != verifier` (M-3: weak protection; can be added if items 1–2
are already done, but the right solution is a multisig or DAO).

---

## Fix status

Updated 12 November 2026. Every fix followed the order SPEC → code → test →
`cargo test`. After the last commit: 83 unit tests and 3 integration tests, all green.
Nothing was deployed to testnet.

### Fixed

| ID | Commit | What was done | Tests |
| --- | --- | --- | --- |
| M-2 | `b38aaf2` | challenge window from 60 000 to 2 592 000 000 ms (1 min – 30 days) | `new_rejects_window_below_minute`, `new_rejects_zero_window` |
| L-3 | `b38aaf2` | window ≤ 30 days, deadline ≤ now + 365 days | `new_rejects_window_above_30_days`, `new_rejects_deadline_beyond_year`, `new_accepts_upper_limits` |
| L-4 | `b38aaf2` | creator and verifier ≠ the contract address | `new_rejects_creator_equal_to_contract`, `new_rejects_verifier_equal_to_contract` |
| — | `b38aaf2` | all `new` and internal error texts added to SPEC 7 | — |
| L-1 | `6b2f445` | `claim` never pays out the cost of `storage_usage + 16 000` bytes | `claim_keeps_storage_reserve` (without the reserve the payout would be 4.16 NEAR instead of 4) |
| L-2 | `6b2f445` | at the end of `new` the balance ≥ the cost of `storage_usage + 2 × 16 000` bytes (≈ 0.32 NEAR on top of the code) | `new_rejects_deposit_below_storage_reserve`, `new_accepts_exact_storage_reserve`; integration `new_requires_storage_deposit`: 3 NEAR, as in `deploy-testnet.sh`, pass even with maximum-length strings, 1.9 NEAR is rejected |
| M-1 | `3979e55` | `challenged_at_ms`; `resolve_timeout` (anyone, 24 h after the challenge → Accepted); after 24 h `resolve` is closed | `resolve_works_until_timeout`, `resolve_rejects_after_timeout`, `resolve_timeout_rejects_before_timeout`, `resolve_timeout_accepts_for_creator`, `resolve_timeout_requires_challenge` |
| L-5 (verifier) | `3979e55`, `cc32d3a` | a deleted verifier no longer freezes milestones (timeout); in `deploy-testnet.sh` the default verifier is `verifier.$MASTER` | — |
| I-4 | `cc32d3a` | deleted `contracts/goal/.github` (template CI with `without-init-call`) and the template `README.md` | — |
| I-8 (Cargo.toml) | `cc32d3a` | filled in `description` and `repository` for NEP-330 | — |

### Partially fixed

| ID | Commit | What was done | What remains | Tests |
| --- | --- | --- | --- | --- |
| M-3 | `0481518` | `challenge` from the verifier fails with `"Verifier cannot challenge"`; self-dealing is no longer visible in `proof_challenged` events | the verifier can still challenge from a second account and reject; real protection is a multisig or DAO verifier and a challenge bond, outside the MVP | `challenge_rejects_verifier` |

### Left as is

| ID | Why |
| --- | --- |
| L-5 (creator) | If the creator deletes their top-level account, someone else can take the name and claim the claimable amount. This is solved organizationally: do not delete the creator account. |
| L-5 (demo) | In `deploy-testnet.sh` creator = `MASTER`, and the verifier is its subaccount. In the demo the team and the creator are the same person, which is fine. For real goals the verifier must not belong to the creator. |
| L-6 | `assert_one_yocto` changes SPEC 7 and the UX (every `resolve` is confirmed in the wallet). Postponed until after the hackathon; until then the frontend must not request a function-call key for the goal contract. |
| I-2 | Needs a frontend change: do not show `unsynced` while a payout is in flight. |
| I-3 | Safe as is; the change is optional. |
| I-5 | Tokens are outside the MVP. The warning "do not send tokens to a goal" is in the root README (`31db38f`) and must also be shown in the frontend. Before mainnet, `sync_ft` and token payouts are needed. |
| I-7 | Warning about the end of the challenge window is a frontend task. |
| I-8 (rest) | Empty `proof_uri`/`reason` and the unvalidated `set_token` do no harm. |

### What changed for the frontend and the demo

- New method `resolve_timeout(index)` and field `challenged_at_ms` in `get_goal().milestones`.
- The SPEC 11 demo scenario does not change: the 120 000 ms window is within bounds, and
  `resolve` in step 5 is called right away, long before 24 h.
- Before running `deploy-testnet.sh`, create `verifier.proofee-anton.testnet`.
