# Goal contract — who can call what (release goal-treasury)

Source of truth: `docs/SPEC.md` 1.1 (sections 6, 6a, 7). Roles:

- **creator**, **verifier** — fixed in `new`, never change.
- **payer** — the challenger of the current bonded request: `get_goal().milestones[i].case.payer`
  (the account that called `prepare_challenge`).
- **anyone** — any account, including the creator and the verifier.
- **private** — only the goal contract itself (callbacks); a call from any account fails with
  `Method <name> is private`.

"Treasury mode" = `get_goal().treasury` is not null. Without a treasury the contract behaves as
SPEC 1.0 (free challenge) and the bonded methods fail with `No treasury`.

## Goal methods

| Method | Who | Mode | Conditions (besides the role) |
| --- | --- | --- | --- |
| `new` | deployer (init, once) | both | parameter checks, balance ≥ storage + 2 × 20000 bytes |
| `set_token` | creator | both | once |
| `deposit` (payable) | anyone | both | — |
| `sync` | anyone | both | — |
| `submit_proof` | creator | both | milestone Pending, before its deadline |
| `challenge` | anyone except creator and verifier | **free only** | Submitted, window open; with a treasury: `Bond required` |
| `finalize` | anyone | both | Submitted, window closed; not while an unbound bonded request is before its `bind_until` |
| `resolve` | verifier | **free only** | Challenged, before `challenged_at + 24 h`; with a treasury: `Use arbitrate` |
| `resolve_timeout` | anyone | both | Challenged; free: after `challenged_at + 24 h`; treasury: case Bound or Qualified and after `case.refund_after_ns` |
| `expire` | anyone | both | Pending, after the deadline |
| `claim` | creator | both | no payout in flight, something to claim |
| `prepare_challenge` | anyone except creator and verifier (becomes the payer) | treasury | Submitted, window open, no active request |
| `retry_registration` | payer | treasury | case RegisterRetryable |
| `confirm_escrow` | payer | treasury | milestone Submitted, case Ready or BindRetryable; bond deposited in the Treasury |
| `qualify` | verifier | treasury | milestone Challenged, case Bound, before `refund_after_ns` |
| `arbitrate` | verifier | treasury | milestone Challenged, case Qualified, before `refund_after_ns` |
| `retry_decision` | anyone | treasury | case DecisionRetryable |
| `retry_settlement` | anyone | treasury | case SettlementRetryable |
| `on_claim_transfer` | private | both | callback of `claim` |
| `on_registered`, `on_verified`, `on_bound`, `on_decided`, `on_settled`, `on_timeout_refund` | private | treasury | callbacks; ignore stale `nonce`/stage, never panic |
| `get_goal`, `get_balances`, `contract_source_metadata` | view | both | — |

The verifier can neither file a request nor be its payer (`Verifier cannot challenge`), so the
verifier never judges their own challenge. The creator cannot challenge their own goal from the
creator account (from another account they can — see AUDIT M-4).

## Treasury methods a user calls directly

| Method | Who | Conditions |
| --- | --- | --- |
| `deposit_bond(escrow_id)` (payable) | payer only (the Treasury checks it) | attach exactly `case.amount`; before `case.deposit_until_ns`; once |
| `refund_expired(escrow_id)` | anyone | unbound bond after `bind_until_ns`, or bound bond after `refund_after_ns`; returns the bond to the payer |
| `retry_failed_refund(escrow_id)` | anyone | bond status RefundRetryable |

Everything else in the Treasury that touches a goal's bond (`register_intent`, `bind_bond`,
`authorize_decision`, `settle_bond`) can be called only by the goal contract itself; the frontend
never calls them. `register_goal` is Treasury-admin only and must be done once per goal before
the first `prepare_challenge`.
