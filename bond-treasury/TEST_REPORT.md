# Test report — Treasury/Mock v0.2.0, 12 November 2026

## Confirmed local verification

- 57 Treasury unit tests PASS, including 24 Phase A tests retained/strengthened and new production-intent/reward/economic/auth/time-boundary tests.
- 18 Mock Goal unit tests PASS: registration and retry, verification/binding, qualification, verifier authorization, decision/settlement callbacks, funding/recovery, claim, refresh, stale callback protections.
- Both default WASM builds PASS (Rust1.99.1, near-sdk5.30.1, locked dependencies).
- Real near-workspaces / nearcore2.14.1 sandbox workflow PASS:10 scenario groups plus1 focused workflow (11 total). Actual VM, signed transactions, asynchronous receipts and NEAR transfers. No mocked PromiseResult in integration suite. Full output evidence/sandbox.log; list of unit tests evidence/test-list.log, unit results evidence/unit.log. Local hashes in sandbox log are NOT public Testnet hashes.

Integration groups:
1. Production WASM export inspection (evidence/wasm-exports.json) confirms no register_demo_case; runtime scenarios check exact amount/payer; duplicate deposit; authorization/private callback rejection; Qualification rejection refunds full2NEAR.
2. Qualified losing Challenge -> Slash; timeout cannot undo slash.
3. Winning Challenge -> principal Refunded -> real separate Mock funding1NEAR -> reward Paid1NEAR; same reward settlement replay does not pay twice.
4. Two separately deployed Mock Goals with identical textual case/proof IDs: isolated escrow/reserves.
5. Active arbitration timeout -> neutral Refund; late verifier decision and reward attempt fail.
6. Actual wallet transfer failure after deleting payer -> RefundRetryable -> recreate same payer -> permissionless retry succeeds; another retry fails.
7. Actual reward transfer failure after deleting beneficiary -> PayRetryable -> recreate same beneficiary -> permissionless retry succeeds; another retry fails.
8. Simultaneous Promise.all settlements with two Mock Goals and different verifier signers; no pooled principal.
9. Deposit succeeds, late binding fails, on-chain orphan timeout refunds and prevents official binding.
10. Quiescent ledger: locked0, refund in-flight0, reserved rewards0, reward in-flight0, Buyback6NEAR, physical balance covers reserve+storage+buffer.

Unit coverage additionally includes exact timeout boundary, timeout-refund then confirmed Refunded then late slash rejected, slash before deadline then timeout rejected, stale/duplicate transfer callbacks, conflicting settlement/outcome, settlement-ID reuse across escrow, insufficient sponsored balance, insufficient gas, pending retry rejection, reward before final forfeiture/principal callback, zero/excess reward funding, duplicate funding, unauthorized cross-Goal payment, one Challenge per proof/case, immutable intent metadata, paused admission and disabled-Goal neutral exit, F0/caps/rounding and u128 maximum reference.

## Sandbox environment adaptation

Original nearcore2.6.2 cannot deserialize Rust1.99 WASM; pin official2.14.1. Both binaries require startup65535 FDs, host hard limit16384. Docker --ulimit also denied by host. Supplied Linux-only nofile_adapter.c intercepts only nearcore's exact startup setrlimit(65535,65535) request and actually retains host hard limit; not pretending kernel limit was raised. Wrapper sets RocksDB max_open_files1000 and renames local genesis root to testnet so account suffix guard can be exercised. No financial execution, node bytecode, consensus, signature, timestamp or RPC result is mocked or patched. This is valid small-fixture integration evidence, not a production-node load test. Both runner and adapter source are included for independent reproduction.

## Public Testnet — PASS

24 confirmed signed transactions recorded, including deployment/provisioning and all three challenge workflows. Treasury and Mock on separate .testnet addresses; no-key account lists verified. Rejection returned2NEAR; winning challenge returned2NEAR plus separately funded1NEAR reward; losing challenge slashed2NEAR into Mock-specific Buyback. Final Treasury balance8NEAR, Buyback2NEAR, all bond/reward locked and in-flight counters0, storage4.07045NEAR plus0.1NEAR buffer covered. Default shipped WASM hashes match on-chain code.

See TESTNET_RESULTS.md and evidence/testnet-transactions.json. Public deployment is distinct from local sandbox and is not inferred from simulation. Generated dedicated TEST-only signer keys stay in ignored local work directory and are never committed. Treasury/Mock created without keys by atomic create+fund+deploy+new. No existing Goal deployment touched.

## Limits and remaining work

Real Goal adapter and actual creator-fee/final-forfeiture bookkeeping NOT implemented (another developer's responsibility). Mock F/verdicts are authorized simulations funded with faucet TEST NEAR. Team must approve actual timeout durations, final Rejected/Failed mapping and precise F definition. No Multi-Challenger, token treasury, DEX buyback, reserve withdrawal, storage cleanup, migration or upgrade. Pending transfer with lost callback cannot safely be reset on elapsed time; bounded10Tgas callbacks ran successfully in sandbox but outage/gas-schedule risk remains. Mock funding has explicit view recovery; other lost orchestration callbacks may require further recovery design. Payer name deletion/reuse changes who controls that AccountId. Registry revocation blocks new decisions/funding but permissionless deadlines/retries still preserve exit. Conservative pending reservation can temporarily require sponsored operating funds; no haircut allowed. No independent security audit or Mainnet readiness claim.

No unrelated tests of original Goal were modified or represented as new verification; Goal checkout remains unchanged.

## Review notes for real-Goal integrator

Caller settlement namespaces bond:/reward: must stay separate from Treasury's deterministic internal timeout: marker (see ABI). Internal timeout labels are not entries in the caller settlement index; escrow outcome/state remains authoritative, so label reuse cannot re-settle that escrow. For Mock, composed IDs include textual case_id prefixes; use case_id<=121 UTF-8 bytes (prefer a fixed64-char case hash) so reward:<case> stays within Treasury128-byte limit. Treasury itself accepts128-byte case IDs; a real Goal should hash composed settlement IDs rather than append unbounded text. Oversized composed IDs reject settlement; original payer retains neutral timeout exit. These are integration constraints, not extra admin powers.

Reward liveness limitation: initial pay_reward requires the owning authorized Goal. If it is unavailable or revoked while reward is still Funded, initial payment can wait indefinitely until Goal/admin coordination restores authorization. Permissionless retry covers already attempted failed payouts, not initial execution. Production policy must address this; there is no admin withdrawal escape hatch. Treasury does not persist raw F, only fixed validated B/R; real Goal must retain the immutable F snapshot.

Additional focused real-sandbox suite evidence/bind-before-deposit.log: unpaid escrow fails on-chain verification -> BindRetryable, then actual deposit/bind and full refund succeed. Runner executes both suites. An optional earlier test with two large fast_forward calls stalled after the second call (evidence/sandbox-second-fast-forward-stall.log), not counted as passed. Final fixture uses one shared on-chain time advance for both timeout cases and passed all assertions.
