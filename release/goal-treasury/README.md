# Release package: goal / goal-treasury

For the frontend: copy this folder to `frontend/contracts/incoming/goal/goal-treasury/`.

| File | Content |
| --- | --- |
| `source.json` | repo, commit, toolchain, build command, features (`short-timeouts` is test-only, not in this wasm) |
| `contract.wasm` | the release wasm; goal-5 is deployed from exactly this file |
| `contract.abi.json` | `cargo near abi` |
| `SHA256SUMS.txt` | `shasum -a 256 -c SHA256SUMS.txt` |
| `code_hash.txt` | base58 code hash, compare with RPC `view_account.code_hash` |
| `permissions.md` | who can call each method (SPEC 6a) |
| `gas-deposit-storage.json` | gas and deposit per method, bond formulas, deadlines, storage |
| `errors-events.md` | error texts, events, case stages (final / retryable) |
| `deployment.testnet.json` | TEMPLATE, filled after the testnet run |
| `examples/` | TEMPLATE, filled with real transactions after the run |
