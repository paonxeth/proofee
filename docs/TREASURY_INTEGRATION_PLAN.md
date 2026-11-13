# Plan: connecting contracts/goal to Bond Treasury v0.2.0

Agreed 2026-11-13. Branch `feat/goal-treasury`.

## Context

Today a challenge is free: anyone can block a milestone for 24 h. Treasury v0.2.0 (`bond-treasury/`) is already deployed as a separate contract with a bond, slash to the buyback reserve and neutral refund on timeout. The goal must learn to work with it: we port the orchestration from `bond-treasury/contracts/mock_goal/src/lib.rs`, replacing the admin with the verifier/challenger. Rewards (fund_reward/pay_reward) are not built; sync/claim accounting is not touched.

Found while reading:
- Treasury: `register_intent` checks `B = clamp(F/5, 1N, 5N)`, `R = min(F/10, B/2, 2N)` and `now < deposit_until < bind_until < refund_after`; `settle_bond`/`authorize_decision` work only while `now < refund_after`; `refund_expired` (anyone) — Deposited after `bind_until` or Bound after `refund_after` → NeutralRefund. Repeating settle with the same id is a no-op, a conflict panics.
- Treasury `new` requires the `.testnet` suffix → the sandbox needs a genesis account `testnet` (`SandboxConfig.additional_accounts`, near-sandbox 0.4).
- `release/bond_treasury.wasm` matches SHA256SUMS (5dfb0699…).
- Tests today: 80 in lib.rs + 3 in math.rs = 83 unit, 3 integration.

## 1. State

```rust
pub struct Goal { ...; treasury: Option<AccountId> }          // set in new, never changes

pub struct Milestone { ...;
    challenge_attempts: u32,              // request counter → unique proof_id/case_id
    case: Option<ChallengeCase>,          // current (latest) bonded request
}

pub struct ChallengeCase {
    proof_id: String,      // "{index}:{proof_hash}:{attempt}"
    case_id: String,       // "m{index}-{attempt}"  (≤121 bytes)
    payer: AccountId,      // the challenger
    reason: String,        // stored ONLY here; in Treasury mode milestone.challenge_reason = None
                           // (the milestone field is used only by the free challenge)
    fee_snapshot: U128,    // F
    amount: U128,          // B
    reward_cap: U128,      // R (for the Treasury only, we pay no rewards)
    deposit_until_ns, bind_until_ns, refund_after_ns: u64,
    escrow_id: Option<String>,
    stage: CaseStage,
    decision: Option<Decision>,
    nonce: u64,            // protects callbacks from stale results (attempt in the Mock)
}

enum CaseStage { RegisterPending, RegisterRetryable, Ready, VerifyPending, BindPending,
    BindRetryable, Bound, Qualified, DecisionPending, DecisionRetryable,
    SettlementPending, SettlementRetryable, Settled, TimeoutRequested, Orphaned }
```

Deadlines (constants, at `prepare_challenge` at time `t0`, all in ns, `ms × 1_000_000` via `checked_mul`):
- `deposit_until = t0 + 10 min`, `bind_until = t0 + 20 min`,
- `refund_after = bind_until + RESOLVE_TIMEOUT_MS (24 h)`.
- For a milestone in Treasury mode the decision deadline = `refund_after` (not `challenged_at + 24 h`). This way the verifier always has ≥ 24 h after binding, and the goal's boundary coincides with the Treasury's: before it `qualify/arbitrate` work, after it only the timeout.

**F** = `L × weight_bps / 10000`, where `L` is locked from SPEC 5.2 by `total_received` after `sync()` in `prepare_challenge`. If a payout is in flight (`payout_in_flight`), sync is skipped and the already counted amount is used — this is exactly the "fees counted at the time of the dispute". B and R are exactly the Treasury formulas (otherwise `ECONOMICS`).

## 1a. Storage reserve (mandatory)

Problem: a case on every milestone (reason up to 280 characters, ids, deadlines) is state growth at the contract's expense. `claim` leaves only `STORAGE_RESERVE_BYTES`; if the growth is larger, after a full claim `prepare_challenge` fails with `LackBalanceForState`, and the creator effectively disables disputes.

What I do:
- Growth is bounded from above: one (latest) request is stored per milestone, a new one overwrites the old one; reason is stored once (in the case); the milestone's `challenger` is filled only at bind.
- Measurement in the sandbox (step 4, first test): a goal with 5 milestones, all strings at the limit (title 100, uri 256, reason 280 characters, multibyte UTF-8 — limits are in characters, bytes are more), Treasury with maximum F → `storage_usage` before and after: prepare on all 5 milestones → bind → qualification rejection → a new request on every milestone. The maximum growth is pinned in the test.
- `claim` already leaves `storage_usage + STORAGE_RESERVE_BYTES` (`storage_cost_with_reserve`), i.e. the current state and code are covered. The reserve covers only growth beyond that: `STORAGE_RESERVE_BYTES ≥ 1.5 × measured growth` (rounded, not less than the current 16000). The minimum deposit in `new` stays `storage + 2 × STORAGE_RESERVE_BYTES`. If it turns out the reason limits must be tightened — I ask first.
- After the build I check that the 3 NEAR in `scripts/deploy-testnet.sh` cover the new minimum (the wasm will grow); if not — I raise it and say by how much.
- Tests: (a) sandbox — the creator does a full `claim`, after which the whole worst-case scenario (5 prepare → bind → rejection → 5 new prepare) passes without `LackBalanceForState`, and `growth ≤ STORAGE_RESERVE_BYTES`; (b) unit — `new` rejects a deposit 1 yocto below the new minimum and accepts exactly the minimum (updated `MIN_DEPOSIT_YOCTO`).
- SPEC §3 and §5.3: the new number of bytes and ≈NEAR, justified by the measurement.

## 1b. Known limitation (code is not fixed, only SPEC + docs/AUDIT.md)

`prepare_challenge` costs only gas and holds the milestone until `bind_until` without a bond. The creator, from another account, can file an empty request every 20 minutes and keep real challengers out for the whole window. Plus each request permanently takes space in the Treasury (`register_intent` deletes nothing, the Treasury pays for storage). Fix after the hackathon: several requests in parallel, the first bound one wins. Recorded in SPEC (limitations section) and in AUDIT.md as an open finding — in the step 2 commit.

## 2. Methods

`new(..., treasury: Option<AccountId>)`: with Some — `treasury ≠ self, creator, verifier`. A missing field in JSON → None, so the old integration tests and scripts do not change.

With `treasury = None` everything is as today; the new methods panic with `"No treasury"`.
With `treasury = Some`: `challenge` → `"Bond required"`, `resolve` → `"Use arbitrate"`.

| Method | Who | Condition | Action |
|---|---|---|---|
| `prepare_challenge(index, reason)` | anyone except creator and verifier | Submitted, window open, no active request (no `case`, or it is Orphaned/Settled-after-rejection, or unbound and `now ≥ bind_until`) | sync, F/B/R, write case (RegisterPending) → `register_intent` → `on_registered` |
| `retry_registration(index)` | payer | RegisterRetryable | the same intent (the Treasury is idempotent) |
| *(the challenger directly)* `treasury.deposit_bond(escrow_id)` exactly B | payer | | the bond's NEAR go to the Treasury, the goal never sees them |
| `confirm_escrow(index)` | payer | Ready / BindRetryable | `get_bond` → `on_verified` (check goal/proof/case/payer/amount/deadlines, status Deposited\|Bound) → `bind_bond` → `on_bound`: milestone **Challenged**, `challenger/reason/challenged_at` are filled, event `proof_challenged` |
| `qualify(index, qualified)` | verifier | Challenged, case Bound, `now < refund_after` | true → Qualified (no Treasury call). false → milestone **Submitted** (challenger fields cleared), decision QualificationRejected → `authorize_decision` → `on_decided` → `settle_bond(REFUND, "bond:{case_id}")` → `on_settled` |
| `arbitrate(index, valid)` | verifier | Challenged, case Qualified, `now < refund_after` | valid → milestone **Accepted**, ProofValid → SLASH. invalid → milestone **Rejected**, ProofInvalidFinal → REFUND. The same decided→settled chain |
| `retry_decision(index)` / `retry_settlement(index)` | anyone | DecisionRetryable / SettlementRetryable | repeat with the same arguments (decision and settlement_id are immutable) |
| `resolve_timeout(index)` | anyone | Challenged, case Bound/Qualified, `now ≥ refund_after` | milestone **Accepted**, stage TimeoutRequested → `treasury.refund_expired` → `on_timeout_refund` (event only) |
| `finalize(index)` | as today | + not blocked by a request if it is unbound and `now ≥ bind_until` | otherwise unchanged |

The milestone status changes immediately in the verifier/timeout method ("state first"), and the Treasury chain is only about the bond. This is safe: if `authorize_decision` reaches the Treasury after `refund_after`, the bond is refunded neutrally, and the milestone status is the same as the timeout would give (valid→Accepted) or the decision (invalid→Rejected, the bond is refunded anyway).

## 3. Async chains and callback failures

All callbacks are `#[private]`, take `(index, nonce)`, **never panic** (like `on_claim_transfer`): on a stage/nonce mismatch — `return false`. Before every call: stage = *Pending, `nonce += 1`. Gas as in the Mock (register 35+15, get_bond 15 + on_verified 80, bind 35+15, authorize 35 + on_decided 85, settle 60+15, refund_expired 60+15); 300 TGas for the user.

| Failure | What happens |
|---|---|
| `register_intent` failed | RegisterRetryable → the payer calls `retry_registration`; or after 20 min a new request can be filed |
| bond not deposited / not confirmed | the milestone stays Submitted; after `bind_until` finalize and a new request are allowed; a deposited bond is returned by anyone via `treasury.refund_expired` |
| `get_bond` did not match / `bind_bond` failed | BindRetryable → the payer repeats `confirm_escrow` before `bind_until` |
| `on_bound` arrived but the milestone is no longer Submitted (race with finalize at the `bind_until` boundary) | stage Orphaned, the milestone is not touched; bond Bound → neutral refund after `refund_after` |
| `authorize_decision` failed | DecisionRetryable → `retry_decision` (anyone) before `refund_after` |
| `settle_bond` failed | SettlementRetryable → `retry_settlement`; a repeat with the same id in the Treasury is a no-op, no double payout |
| retries came too late | anyone calls `treasury.refund_expired` → NeutralRefund |
| `refund_expired` from the goal failed (e.g. already called directly) | event only; the bond is refunded through the Treasury anyway |

The goal transfers no NEAR and attaches no deposit to any Treasury call → `sync`, `claim`, `last_balance` do not change. Settled in the goal = "instruction accepted"; the actual Refunded/Slashed is read via `treasury.get_bond` (UI).

Events: `challenge_prepared` {index, case_id, payer, amount}, `proof_challenged` (at bind), `challenge_qualified` {index, qualified}, `bond_settlement` {index, decision, stage}.

## 4. Order of work and commits

1. `docs/TREASURY_INTEGRATION_PLAN.md` (this plan) — commit.
2. SPEC: §2 (the bond is now optional), §3 the `treasury` parameter, §4 state, §6 transitions, §7 methods/errors, §8 events, §10 tests; DECISIONS: entry 2026-11-13 (F, deadlines 10/10 min + 24 h, qualification rejection → Submitted, no reward). — commit.
3. Code in `contracts/goal/src/lib.rs` (+ `treasury.rs` for the case/callbacks, so lib.rs does not grow too large) and unit tests; existing tests change only mechanically (add `None` to 4 direct `Goal::new` calls and the helper). — commit.
4. `contracts/goal/tests/treasury.rs` — sandbox with the real `bond-treasury/release/bond_treasury.wasm`: first the storage measurement and the `fast_forward` speed, then raising `STORAGE_RESERVE_BYTES` (+ SPEC, + deposit unit test) in a separate commit, then the scenarios. — commits.

Nothing is deployed to testnet. At 19:00 — checkpoint: if the sandbox is not green, I commit what exists and write what is left.

## 5. Tests

Unit (new, via `VMContextBuilder` + `PromiseResult`, following the Mock tests): `new` without treasury = old behavior; `treasury == creator` is rejected; `challenge`/`resolve` panic in Treasury mode; F/B/R calculation (F=0 → B=1N,R=0; large F → cap 5N/2N); prepare from creator/verifier is forbidden; a repeated prepare with an active request is forbidden, allowed after `bind_until`; an unconfirmed request does not block finalize after `bind_until` and blocks it before; every callback: success, failure → Retryable, stale nonce → no-op; `on_bound` after finalize → Orphaned; qualify/arbitrate only by the verifier and only before `refund_after`; resolve_timeout before/after `refund_after`; `sync` does not change `total_received` throughout the scenario.

Sandbox (`tests/treasury.rs`): genesis `testnet` → `treasury.testnet` (new + register_goal + fund_storage), goal with `treasury`, fees 10 NEAR, proof:
1. qualification rejected → bond Refunded, challenger balance +B, milestone Submitted;
2. valid → Slashed, `get_buyback_balance(goal) = B`, milestone Accepted;
3. invalid → Refunded, milestone Rejected;
4. timeout: `fast_forward` past `refund_after` → `resolve_timeout` → Accepted, decision NeutralRefund, challenger +B;
5. repeated settlement: `retry_settlement` after Settled → error; direct `settle_bond`/`refund_expired` in the Treasury → error; the challenger's balance did not change a second time;
6. outsider: `qualify`/`arbitrate` → `"Only verifier"`; `confirm_escrow` not from the payer → `"Only payer"`.
In all scenarios: the goal's `total_received` did not grow by B.

7. storage reserve (see 1a): full claim → the worst-case scenario passes, growth ≤ reserve.

Timeout: ~26 h in the sandbox (~310 thousand `fast_forward` blocks). Checked first. If too slow — feature `short-timeouts` in the goal's `Cargo.toml`: under `#[cfg(feature = "short-timeouts")]` `RESOLVE_TIMEOUT_MS` and the deposit/bind windows are shortened (e.g. 5 min / 1 / 1 min); the Treasury sandbox tests build the wasm via `cargo_near_build` with this feature. The release wasm (`cargo near build non-reproducible-wasm`, the container command in Cargo.toml) — without the feature, 24 h. The old 3 integration tests keep using the regular build. Plus a unit test that without the feature the constant = 86_400_000.

## Verification

- `cd contracts/goal && cargo test` — 83 old unit + new, 3 old integration + new sandbox, all green.
- `cargo near build non-reproducible-wasm` builds.
- `git log` in `feat/goal-treasury` — 4 commits by steps; `main` untouched.
