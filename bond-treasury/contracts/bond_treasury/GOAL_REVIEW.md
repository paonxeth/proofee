# Existing Goal source review

Source: https://github.com/paonxeth/proofee/tree/efa87f13478ffbf85fc607430197b146e71b3071
No edits. This is source-derived ABI, not a downloaded deployed ABI. No handoff architecture files named in TZ were found; repository SPEC and DECISIONS describe the actual older MVP.

| Existing method | Actual behavior / integration implication |
|---|---|
| new(creator,verifier,goal_title,goal_uri,immediate_bps,challenge_window_ms,milestones) | fixed parameters, no Treasury reference; creator != verifier; 1–5 milestones |
| set_token(token:AccountId) | creator once, informational |
| deposit() payable | sync income, NOT bond escrow |
| sync() | skips while payout_in_flight; records balance increase above last_balance |
| submit_proof(index:u8,proof_uri:String,proof_hash:String) | creator; Pending -> Submitted; proof hash 64 lowercase hex |
| challenge(index:u8,reason:String) | nonpayable; anybody except creator (verifier is NOT excluded); Submitted -> Challenged immediately; no bond/case/Qualification |
| finalize(index:u8) | Submitted -> Accepted after challenge window |
| resolve(index:u8,accept:bool) | verifier predecessor; Challenged -> Accepted if true, Rejected if false; before 24h |
| resolve_timeout(index:u8) | anyone >=24h after challenged_at_ms; Challenged -> Accepted; not proof verdict |
| expire(index:u8) | Pending -> Failed after milestone deadline; cannot expire Challenged |
| claim()->Promise | creator; sync; claimable capped by storage reserve; tracks payout_in_flight and last_balance |
| on_claim_transfer(amount:NearToken) private | empty result with promise_result_checked(0,0); restore claimed/last_balance on failure |
| get_goal(), get_balances() | GoalView, BalancesView; amounts NearToken decimal strings |

Time uses milliseconds, Treasury demo uses nanoseconds. Convert with checked multiplication and lossless JSON. No prepare_challenge, bind_bond, settle, reward, case registry, upgrade or migration method exists in current Goal.

Accounting: total_received -> immediate; remainder locked. Accepted weights unlock creator share; Rejected AND Failed weights forfeit from all current and future fees. `forfeited` is computed aggregate, not a separately spendable per-case reserve; there is no spent/reward-reserved counter. Must add reserve allocation/spent counters and protect creator claim/storage against reward funding. Define whether F means locked milestone share vs total fee share; current spec has no dispute F snapshot. Existing forfeited symbol F differs from Bond TZ's F definition.

Deployment script creates Goal, transfers 3 test NEAR, deploys+initializes atomically without a key. No upgrade mechanism: existing demo goal cannot acquire new methods. Repository lists goal-3.proofee-anton.testnet; address/code hash was not verified via RPC. Phase B needs new Goal deployment, approval of SPEC/DECISIONS changes and UI routing; existing P0 goals remain unchanged.

Timeout during active arbitration: Treasury Bound deadline wins once now>=refund_after; even a late legitimate verifier slash is rejected, and anyone can refund original payer. Before deadline a settled SLASH is terminal and timeout cannot reverse it. Pending refund cannot trigger another transfer. This is demo policy; future adapter must coordinate resolution deadline and cancel late verdicts. Existing Goal's Accepted-on-timeout must be mapped to protocol-failure REFUND, never SLASH. Accepted alone does not prove a substantive Challenger loss. Goal Rejected requires approved mapping to terminal Failed eligibility for reward; do not silently equate enum names.

Required new APIs/state and asynchronous handshake: INTEGRATION_HANDOFF.md. Compatibility is pinned at source dependency version only; integration is BLOCKED_BY_GOAL.
