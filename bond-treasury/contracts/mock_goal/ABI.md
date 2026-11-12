# Mock Goal — TEST ONLY, v0.2.0

All amounts U128 decimal strings, times u64 integer ns, accounts strings. Intent fields exactly match Treasury ABI.md. State/decision simulation is centralized. new current account must end .testnet. All methods nonpayable except fund_mock. For promises, transaction result may be callback bool/void or null for Option<Promise>; always inspect get_case AND Treasury views, not just top-level transaction success. All orchestration requires 300Tgas. Goal case_id is local unique <=128 bytes.

| Method | Fields | Access | Rust return / meaning |
|---|---|---|---|
| new | admin,verifier,treasury:AccountId | init once | Self internally, void wire |
| prepare_challenge | intent:Intent | admin | Promise -> registration callback bool |
| retry_registration | case_id:String | admin, RegisterRetryable | Promise |
| confirm_escrow | case_id:String | fixed payer or admin; Ready/BindRetryable | Promise -> exact escrow validation, bind, callback bool |
| qualify | case_id:String,qualified:bool | admin, Bound | Option<Promise>; true records Qualified, false attests rejection then refund |
| arbitrate | case_id:String,valid:bool | verifier, Qualified, verifier!=payer | Promise; true Valid->slash; false final Invalid->refund |
| retry_decision | case_id:String | admin, DecisionRetryable | Promise |
| retry_settlement | case_id:String | admin, SettlementRetryable | Promise |
| resolve_timeout | case_id:String | anyone | Promise; Treasury timestamp enforces neutral refund |
| fund_mock PAYABLE | {} | admin; positive TEST donation | void |
| fund_case | case_id:String,amount:U128 | admin, Refunded/FundingRetryable, ProofInvalidFinal | Promise; mock-endowed funds simulate final forfeiture |
| recover_funding | case_id:String | anyone, FundingPending/FundingRetryable | Promise; escrow->funding mapping reconciliation |
| claim_reward | case_id:String | fixed payer or admin, Funded/PayRetryable | Promise; Treasury checks/refuses duplicate spend |
| refresh_bond | case_id:String | anyone | Promise; reconcile Refunded/Slashed and neutral timeout |
| refresh_reward | case_id:String | anyone | Promise; reconcile Paid |
| get_case | case_id:String | view | Case|null |

PRIVATE callbacks, all arguments {case_id:String,attempt:u64}; predecessor must be this Mock, attempt/stage guards reject or ignore stale result: on_registered->bool, on_verified->Promise, on_bound->bool, on_decided->Option<Promise>, on_settled->bool, on_funded->bool, on_recovered_funding->bool, on_pay_accepted->bool, on_refresh_bond->bool, on_refresh_reward->bool. Exactly one bounded promise result (max16384 bytes). Never accept supplied tx hash.

Case={intent:Intent,escrow_id:String|null,stage:Stage,decision:String|null,attempt:u64,funding_id:String|null,reward_amount:U128}.

Stage: RegisterPending,RegisterRetryable,Ready,VerifyPending,BindPending,BindRetryable,Bound,Qualified,DecisionPending,DecisionRetryable,SettlementPending,SettlementRetryable,SettlementAccepted,Refunded,Slashed,FundingPending,FundingRetryable,Funded,PayPending,PayRetryable,Paid,RefreshPending (reserved/unused). SettlementAccepted means instruction accepted only. claim_reward callback Funded means instruction accepted, not yet Paid; refresh_reward verifies Treasury. PayRetryable at Mock means request failed, NOT proof Treasury payout failed: Treasury PayRetryable requires direct permissionless retry_failed_reward, then refresh_reward. Missing callbacks can leave Mock Pending; funding recovery provided, other recovery remains a reviewed-production requirement.

No mocked withdrawal API. Admin can only orchestrate approved state transitions into Treasury's fixed-payer, limited-amount ABI. Real Goal must replace admin-simulated F/verdicts with actual milestone/forfeiture accounting; this Mock is never that implementation.
