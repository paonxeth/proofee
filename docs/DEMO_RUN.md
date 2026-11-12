# Demo run — goal-4.proofee-anton.testnet

Date: 12 November 2026, NEAR testnet. Both demo scenarios on the final demo goal:
milestone 1 accepted after the challenge window, milestone 2 challenged and rejected.

- Goal: [`goal-4.proofee-anton.testnet`](https://testnet.nearblocks.io/address/goal-4.proofee-anton.testnet),
  no access keys.
- Spec: [`specs/demo-goal.json`](https://raw.githubusercontent.com/paonxeth/proofee/61aacd2355c22ed5f83f3156b5cb63ed7b529c16/specs/demo-goal.json),
  `goal_spec_hash` `b5b13f8f26ffb94579183ef029c0a9ff1c25d0717a1f213cdc93b0cab1dd9b8a`.
- Parameters: immediate 2000 bps (20%), challenge window 120000 ms, milestones 1875/3125/5000 bps
  (15/25/40% of all fees).
- Accounts: creator `proofee-anton.testnet`, verifier `verifier.proofee-anton.testnet`,
  third party `proofee-stranger.testnet`.

## Proofs

| Milestone | proof_uri | proof_hash (sha256) |
| --- | --- | --- |
| 1 | [proofs/m1.md](https://raw.githubusercontent.com/paonxeth/proofee/5f7951238e4fce621b2c236f03f7caff51f0e1a7/proofs/m1.md) | `a11429a8997cdc318165b8d332bd66fa50f174ba6dcd32ac8c7275ca64fa6c5d` |
| 2 | [proofs/m2.md](https://raw.githubusercontent.com/paonxeth/proofee/5f7951238e4fce621b2c236f03f7caff51f0e1a7/proofs/m2.md) | `e177d2a3ab11b8ee1fc0367f5cdaab05f1b28bbc40a47f36f7494bb95b83966f` |

The hash is computed as for the spec: `git show <commit>:<file> | shasum -a 256`; it matches the
file served at the raw link. Proof 2 is deliberately weak: only a mockup image, no live URL.

## Steps

Amounts in NEAR; the contract returns yoctoNEAR strings.

| # | Step | Signed by | Result | Transaction |
| --- | --- | --- | --- | --- |
| 1 | `get_balances` before the start | — (view) | all fields 0, `unsynced` 0 | — |
| 2 | **SIMULATED** Nearly fees: transfer 2 NEAR to the goal | proofee-stranger.testnet | `unsynced` 2 | [5wfiSn7S…](https://testnet.nearblocks.io/txns/5wfiSn7SbCKeyKmTSqDXG6Q9dc7fmFPF5jEg53goD3eq) |
| 3a | `sync` | proofee-stranger.testnet | `fees_synced` income 2; total 2, immediate 0.4, locked 1.6, claimable 0.4 | [7c99XdGu…](https://testnet.nearblocks.io/txns/7c99XdGugTPBt2tJNzDB6UR8XLsV8Skyy7yf8pZSubcD) |
| 3b | `sync` again | proofee-stranger.testnet | no event, balances unchanged | [GHtXg9Hh…](https://testnet.nearblocks.io/txns/GHtXg9HhKDaJ7tbk9VbapLLuNpFk8SNq8yinbBpPsWoZ) |
| 4a | `submit_proof(0, m1)` | proofee-anton.testnet | `proof_submitted`, milestone 1 Submitted | [F4HUTQdq…](https://testnet.nearblocks.io/txns/F4HUTQdqk7nDqS1LRubSXvdLMhLsnDkMWQLeBq69Hxz9) |
| 4b | `finalize(0)`, 130 s after the proof | proofee-stranger.testnet | `milestone_resolved` Accepted; unlocked 0.3, claimable 0.7 | [7DHjxk7H…](https://testnet.nearblocks.io/txns/7DHjxk7HQjWDWwywHqeqUa3kM42cqKxPd5ubh9pyNfpb) |
| 5a | `submit_proof(1, m2)` | proofee-anton.testnet | `proof_submitted`, milestone 2 Submitted | [7SStPLG3…](https://testnet.nearblocks.io/txns/7SStPLG3pmK6VRLXnj8BaFdrrv83vYSURgMVuRyrsnjk) |
| 5b | `challenge(1)` by the verifier | verifier.proofee-anton.testnet | **fails** as expected: `Verifier cannot challenge` | [CsTZrERA…](https://testnet.nearblocks.io/txns/CsTZrERABmKZkymft7MMFUy2mo3Mab9n1hdumuRuMmRU) |
| 5c | `challenge(1, "Criterion requires a live URL reading chain state; the proof is a static mockup screenshot")` | proofee-stranger.testnet | `proof_challenged`, milestone 2 Challenged | [65Jnu3ro…](https://testnet.nearblocks.io/txns/65Jnu3ro4RHT3Nd8ixrSJqaAzukxYdMLhdjAqEZSYDQY) |
| 6a | `resolve(1, false)` by a third party | proofee-stranger.testnet | **fails** as expected: `Only verifier` | [4fj3fLSp…](https://testnet.nearblocks.io/txns/4fj3fLSpvJc5xmQfwjxhFEztKqfWD9uzWZbdujdxNKbo) |
| 6b | `resolve(1, false)` | verifier.proofee-anton.testnet | `milestone_resolved` Rejected; forfeited 0.5, pending 0.8 | [EpskuvXE…](https://testnet.nearblocks.io/txns/EpskuvXE57uWbbDwzKDevMZ8rRu9TMT7LSMhSdj3HCEY) |
| 7a | `claim` | proofee-anton.testnet | `claimed` 0.7; goal balance 5 → 4.3, creator +0.69959 (0.7 minus gas) | [23MxUoik…](https://testnet.nearblocks.io/txns/23MxUoikemK6hvpmUsfvGvWcDV1cZwZdgEBtbA5E5TrQ) |
| 7b | `claim` again | proofee-anton.testnet | **fails** as expected: `Nothing to claim` | [2zEsBaDE…](https://testnet.nearblocks.io/txns/2zEsBaDEYGssf96Fv2QiXZyx3B7QFYkYKsSPpyfVtLkU) |
| 8 | `get_balances` after | — (view) | see below | — |

## Reconciliation

Final `get_balances` (yoctoNEAR):

```json
{"total_received":"2000000000000000000000000","immediate":"400000000000000000000000",
 "locked":"1600000000000000000000000","unlocked":"300000000000000000000000",
 "forfeited":"500000000000000000000000","pending":"800000000000000000000000",
 "claimed":"700000000000000000000000","claimable":"0","unsynced":"0"}
```

| | Expected | Actual |
| --- | --- | --- |
| Paid to the creator: 0.4 immediate + 0.3 milestone 1 (1875 bps × 1.6) | 0.7 | 0.7 |
| Forfeited: milestone 2 (3125 bps × 1.6) | 0.5 | 0.5 |
| Pending: milestone 3 (5000 bps × 1.6) | 0.8 | 0.8 |
| **Total** | **2** | **0.7 + 0.5 + 0.8 = 2** |

All amounts match exactly, to the yoctoNEAR. Milestone statuses after the run: 1 Accepted,
2 Rejected (challenger proofee-stranger.testnet), 3 Pending.
