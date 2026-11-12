# Decisions

## 2026-11-08

- Architecture: no separate vault/treasury. One contract per goal, `fee_to` = the goal contract address.
- Factory: Nearly. Paying fees to a third-party account was verified on mainnet (launch #3680).
- Immediate share: 50% (`immediate_bps = 5000`), a `new` parameter.
- Verifier in the MVP: a single account. A 3-of-5 committee only as a simulation in the frontend.
- Challenge window: a `new` parameter, 2 minutes in the demo.
- Challenge bond: none.
- Forfeited fees: stay in the contract forever (a neutral reserve).
- Fees from future trades after a milestone fails: forfeited in proportion to the milestone weight.
- Number of milestones: 1–5.
- Resubmission after rejection: no.
- Proof files: on our server; the contract stores the link and sha256.
- Demo network: testnet only.

## 2026-11-09

- The refund of a failed payout must not count as income: `sync` never lowers
  `last_balance`, and while a payout is in flight (`payout_in_flight`), `sync` is skipped and `claim`
  fails with `"Payout in progress"`. Callback gas is 10 TGas.
- Default challenge window is 24 h (86400000 ms), decided by the team on 9 November.

## 2026-11-10

- Following the audit (docs/AUDIT.md): 24 h dispute timeout. If the verifier has not resolved a dispute
  within 24 hours after `challenge`, anyone calls `resolve_timeout`, and the milestone is accepted for
  the creator (Accepted). After the timeout `resolve` no longer works.
- `new` limits: challenge window 60 s – 30 days, deadline at most 365 days ahead, creator and
  verifier differ from the contract address, deposit ≥ storage + 2 × 16000 bytes.

## 2026-11-12

- Hackathon plan from the team: 3–5 milestones per goal (was 1–5).
- The goal spec (milestones, acceptance criteria, evidence) is a JSON file in the repo. `new` takes
  its sha256 as `goal_spec_hash`; `goal_uri` is a raw GitHub link pinned to a commit.

## Open

- Can the creator change `fee_to` after launch in Nearly.
- How Nearly pays the token part of the fees to the contract.
- Which tracks to list in the submission.
- Who submits the application and receives the prize.
