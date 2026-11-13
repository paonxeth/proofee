# Goal contract — errors, events, case stages (release goal-treasury)

## Errors

A failed call returns `Smart contract panicked: <text>`; match on the text.

| Text | Methods | Meaning |
| --- | --- | --- |
| `Only creator` | set_token, submit_proof, claim | wrong signer |
| `Only verifier` | resolve, qualify, arbitrate | wrong signer |
| `Only payer` | retry_registration, confirm_escrow | signer is not `case.payer` |
| `Creator cannot challenge` / `Verifier cannot challenge` | challenge, prepare_challenge | wrong signer |
| `No such milestone` | all index methods | index ≥ number of milestones |
| `Wrong status` | submit_proof, challenge, finalize, resolve, resolve_timeout, expire, prepare_challenge, confirm_escrow, qualify, arbitrate | milestone status does not allow it |
| `Wrong stage` | retry_registration, confirm_escrow, qualify, arbitrate, retry_decision, retry_settlement, resolve_timeout (treasury) | case stage does not allow it, or no case |
| `Challenge in progress` | prepare_challenge, finalize | another request is active / an unbound request is before its `bind_until` |
| `No treasury` | bonded methods | goal deployed without a treasury |
| `Bond required` | challenge | goal has a treasury: use prepare_challenge |
| `Use arbitrate` | resolve | goal has a treasury: use qualify/arbitrate |
| `Window closed` | challenge, prepare_challenge | challenge window is over |
| `Window still open` | finalize | too early |
| `Deadline passed` / `Deadline not passed` | submit_proof / expire | milestone deadline |
| `Resolve time passed` | resolve, qualify, arbitrate | free: 24 h after the challenge; treasury: `case.refund_after_ns` |
| `Resolve time not passed` | resolve_timeout | too early |
| `Reason too long` | challenge, prepare_challenge | reason > 280 characters |
| `URI too long` | new, submit_proof | > 256 characters |
| `Invalid proof hash` | submit_proof | not 64 lowercase hex |
| `Nothing to claim` | claim | — |
| `Payout in progress` | claim | previous claim callback pending |
| `Token already set` | set_token | — |
| `Math overflow` | any | must never happen |
| `new` only: `Creator and verifier must differ`, `Creator cannot be the contract`, `Verifier cannot be the contract`, `Treasury cannot be the contract, creator or verifier`, `Goal title must be 1-100 chars`, `Milestone title must be 1-100 chars`, `Invalid goal spec hash`, `immediate_bps must be <= 10000`, `Window too short`, `Window too long`, `Need 3-5 milestones`, `Weight must be > 0`, `Weights must sum to 10000`, `Deadline must be in future`, `Deadline too far`, `Deposit too small for storage` | new | invalid parameters |

**Treasury errors have no text.** The released Treasury wasm aborts without a message: a failed
`deposit_bond` (wrong amount, wrong signer, after `deposit_until_ns`, second deposit) or
`refund_expired` (not expired yet, already refunded/slashed) shows only
`WebAssembly trap: An unreachable opcode was executed`. Check the conditions in the UI before
sending, and read `get_bond` after.

**A successful transaction does not mean the Treasury step succeeded.** Bonded methods return
the result of the goal callback (`true`/`false`); a Treasury failure is recorded as a
`*Retryable` stage. Always read `get_goal` after the transaction.

## Events

NEP-297 logs: `EVENT_JSON:{"standard":"proofee","version":"1.0.0","event":"<name>","data":[{...}]}`.

| Event | Data | Emitted by |
| --- | --- | --- |
| `goal_created` | creator, verifier, milestones (count), goal_spec_hash | new |
| `fees_synced` | income, total_received | sync, deposit, claim, prepare_challenge |
| `proof_submitted` | index, proof_uri, proof_hash | submit_proof |
| `proof_challenged` | index, challenger, reason (+ case_id in treasury mode) | challenge; treasury: on_bound (the bond is bound) |
| `milestone_resolved` | index, status, by | finalize, resolve, resolve_timeout, expire, arbitrate |
| `claimed` / `claim_failed` | amount (+ total_claimed) | on_claim_transfer |
| `challenge_prepared` | index, case_id, payer, amount, deposit_until_ns, bind_until_ns, refund_after_ns | prepare_challenge |
| `challenge_qualified` | index, qualified | qualify (qualified=false also sets the milestone back to Submitted; no `milestone_resolved`) |
| `bond_settlement` | index, decision, stage | on_decided (failure), on_settled |
| `bond_settlement` (timeout) | index, decision "NeutralRefund", stage "TimeoutRequested", ok | on_timeout_refund |

The Treasury emits its own events (standard `proofee_bond`: deposited, bound, refunded, slashed, …).

## Case stages (`get_goal().milestones[i].case.stage`)

| Stage | Meaning | Next action | Kind |
| --- | --- | --- | --- |
| RegisterPending | Treasury `register_intent` in flight | wait | transient |
| RegisterRetryable | registration failed | payer: `retry_registration` | retryable |
| Ready | intent registered, `escrow_id` known | payer: Treasury `deposit_bond`, then `confirm_escrow` | waiting for payer |
| VerifyPending, BindPending | bond check / bind in flight | wait | transient |
| BindRetryable | bond not found/mismatch, or bind failed | payer: `confirm_escrow` again (before `bind_until_ns`) | retryable |
| Bound | bond bound, milestone Challenged | verifier: `qualify`; after `refund_after_ns` anyone: `resolve_timeout` | waiting for verifier |
| Qualified | challenge admissible | verifier: `arbitrate`; after `refund_after_ns`: `resolve_timeout` | waiting for verifier |
| DecisionPending, SettlementPending | Treasury decision / settlement in flight | wait | transient |
| DecisionRetryable | `authorize_decision` failed | anyone: `retry_decision` | retryable |
| SettlementRetryable | `settle_bond` failed | anyone: `retry_settlement` | retryable |
| Settled | settlement accepted by the Treasury | read Treasury `get_bond` for Refunded / Slashed | **final** |
| TimeoutRequested | `resolve_timeout` asked the Treasury for a neutral refund | read `get_bond`; if not Refunded, anyone: Treasury `refund_expired` | **final** |
| Orphaned | bond bound after `finalize` won the race | anyone: Treasury `refund_expired` after `refund_after_ns` | **final** |

An unbound request (RegisterPending … BindRetryable) stops blocking the milestone at
`bind_until_ns`: after that `finalize` and a new `prepare_challenge` work; a deposited but unbound
bond is returned by anyone with Treasury `refund_expired`. Retrying a decision or settlement after
`refund_after_ns` fails; the bond then times out neutrally through Treasury `refund_expired`.

Treasury bond status (`get_bond(escrow_id).status`): null (intent only) → Deposited → Bound →
Slashed **final** | RefundPending → Refunded **final** | RefundRetryable (anyone:
`retry_failed_refund`). `decision`: QualificationRejected, ProofValid, ProofInvalidFinal,
NeutralRefund.
