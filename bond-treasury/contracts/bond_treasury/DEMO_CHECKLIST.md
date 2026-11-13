# Integrated demo checklist

Use current workspace README/scripts/testnet-demo.cjs (not old Phase A CLI sequence). Actual public account IDs/hashes/views: TESTNET_RESULTS.md at workspace root; full raw public responses evidence/testnet-transactions.json. Local sandbox hashes are separate and not public explorer evidence.

1. Atomic create+6TEST NEAR+default WASM+new for Treasury and Mock; assert no account keys. Parent20TEST NEAR funds code/storage/gas, payer3, verifier1; reserve operating budget separately from bond.
2. Admin register Mock Goal; query interface_version proofee-treasury/0.2.0. Never register unreviewed public Goal.
3. Mock admin prepares immutable Intent (F10,B2,R1NEAR,explicit deadlines), Treasury authenticates Mock predecessor. Check Mock Ready and exact metadata/payer.
4. Payer deposit2NEAR -> Treasury Deposited. Payer Mock confirm_escrow -> async exact read/bind -> Mock Bound.
5. Rejection: Mock admin qualify(false) -> decision then refund. Verify Treasury Refunded and payer balance+2NEAR (Goal/admin trigger settlement, payer not paying this tx's gas). Reward absent.
6. Winning new case: qualify(true), verifier arbitrate(false) -> final-invalid attestation+refund; verify principal Refunded. Refresh Mock, fund_case1NEAR actual transfer, get_reward Funded; claim_reward -> Paid. Distinct bond/reward settlement IDs; total principal2+reward1NEAR. Funding uses only Mock test reserve, never Treasury bonds.
7. Losing new case: qualify(true), verifier arbitrate(true) -> Slashed; goal Buyback+2NEAR, no outward transfer. Final liabilities/balance reconcile.
8. Replay reward settlement -> no second transfer; wrong caller/duplicate deposits/private callback must fail. Fault injection and neutral timeout with late-verdict rejection run in sandbox; do not fabricate public equivalents if not executed.
9. Check ALL child receipt outcomes and final views, not merely successful orchestrator transaction. Pending isn't Refunded/Paid. Retryable allows dedicated retry to same payer; Pending cannot blindly retry.
10. Preserve signed tx hash/explorer, current block, bond/reward views, account balances, storage and code hashes. Any failed funding call returns attachment asynchronously; real Goal sync must not count return as revenue.

Fallback: if real Goal adapter unavailable, use explicitly labeled Mock demo and keep existing Goal P0 unmodified. No Mainnet or real funds. Team still approves product timeout periods and forfeiture semantics before real integration.
