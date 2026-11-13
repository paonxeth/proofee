# Examples — the testnet run of goal-5

Real calls from the run on 13 Nov 2026 (log: `docs/DEMO_RUN_TREASURY.md`). Goal
`goal-5.proofee-anton.testnet`, Treasury `treasury.proofee-bond-lab-20261112-1794481933490.testnet`.

Each `NN-*.json` has the signer, the transaction in wallet-selector / near-connect format
(`wallet.signAndSendTransaction(transaction)`; gas and deposit in yocto/gas units as strings), the
transaction hash and what the readback showed.

| File | Who | Call | Outcome |
| --- | --- | --- | --- |
| `01-prepare-challenge.json` | challenger | goal `prepare_challenge` | case Ready, escrow_id known |
| `02-deposit-bond.json` | challenger | Treasury `deposit_bond`, exactly `case.amount` | bond Deposited |
| `03-confirm-escrow.json` | challenger | goal `confirm_escrow` | milestone Challenged, case Bound |
| `04-qualify-false.json` | verifier | goal `qualify(false)` | milestone Submitted, bond Refunded |
| `05-qualify-true.json` | verifier | goal `qualify(true)` | case Qualified |
| `06-arbitrate-valid.json` | verifier | goal `arbitrate(true)` | milestone Accepted, bond Slashed |
| `07-arbitrate-invalid.json` | verifier | goal `arbitrate(false)` | milestone Rejected, bond Refunded |
| `08-claim.json` | creator | goal `claim` | 0.35 NEAR paid, no bond in it |

Readbacks after the run: `get_goal.after.json`, `get_balances.after.json` (goal) and
`get_bond.qualification-rejected.json`, `get_bond.slashed.json`, `get_bond.refunded-invalid.json`
(Treasury). Timestamps `*_ns` are nanoseconds (larger than JS `Number.MAX_SAFE_INTEGER`).
