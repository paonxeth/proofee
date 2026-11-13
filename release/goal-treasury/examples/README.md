# Examples (template)

Filled after the testnet run of goal-5 (docs/DEMO_RUN_TREASURY.md): for each step the exact
call (method, JSON args, gas, deposit, signer), the transaction hash and the `get_goal` /
Treasury `get_bond` readback.

Planned files:
- `01-prepare-challenge.json`, `02-deposit-bond.json`, `03-confirm-escrow.json`
- `04-qualify-false.json` (qualification rejected, refund)
- `05-qualify-true.json`, `06-arbitrate-valid.json` (slash)
- `07-arbitrate-invalid.json` (refund)
- `get_goal.after.json`, `get_bond.*.json`
