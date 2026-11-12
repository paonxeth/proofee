# Treasury ABI — proofee-treasury/0.2.0

Pinned near-sdk 5.30.1. Fresh deployment/state only; not a migration of v0.1. Default production WASM excludes register_demo_case. JSON argument object uses field names below. AccountId=validated NEAR string; U128=decimal JSON string; u64=integer token (nanoseconds must use lossless encoder, not JS Number). Void returns empty value. All calls nonpayable except explicitly marked. Init once, private and nonpayable enforced by near-sdk wrappers.

| Method | JSON fields | Access | Return |
|---|---|---|---|
| new | admin:AccountId | init once; current account suffix .testnet | void |
| interface_version | {} | view | "proofee-treasury/0.2.0" |
| register_goal | goal:AccountId, policy_version:String | admin; append-only; no self-goal | void |
| set_goal_authorized | goal:AccountId, authorized:bool | admin | void |
| set_paused | paused:bool | admin; gates registration/deposit, not refund/retry | void |
| register_intent | intent:Intent | authorized predecessor Goal; goal derived from predecessor | escrow_id:String |
| fund_storage PAYABLE | {} | anyone; positive donation, no withdrawal | void |
| deposit_bond PAYABLE | escrow_id:String | original fixed payer, exact B | void |
| bind_bond | escrow_id:String | owning authorized Goal | void |
| authorize_decision | escrow_id:String, decision:Decision | owning authorized Goal | void |
| settle_bond | escrow_id:String, settlement_id:String, outcome:Outcome | owning authorized Goal | void |
| refund_expired | escrow_id:String | anyone; on-chain timestamp/state | void |
| retry_failed_refund | escrow_id:String | anyone, RefundRetryable only | void |
| on_refund PRIVATE | escrow_id:String, attempt:u64 | Treasury only, one result | bool |
| fund_reward PAYABLE | escrow_id:String, funding_id:String | owning authorized Goal, actual positive NEAR <= cap | funding hash:String |
| pay_reward | funding_id:String, settlement_id:String | owning authorized Goal, principal Refunded | void |
| retry_failed_reward | funding_id:String | anyone, PayRetryable only, fixed beneficiary | void |
| on_reward PRIVATE | funding_id:String, attempt:u64 | Treasury only, one result | bool |
| get_bond | escrow_id:String | view | Bond|null |
| get_goal | goal:AccountId | view | Goal|null |
| get_reward | funding_id:String | view | Reward|null |
| get_case_funding | escrow_id:String | view | funding hash:String|null |
| get_buyback_balance | goal:AccountId | view | U128 |
| get_liabilities | {} | view | Liabilities |

Intent={proof_id,case_id,submission_id:String,payer:AccountId,fee_snapshot,amount,reward_cap:U128,deposit_until_ns,bind_until_ns,refund_after_ns:u64}. Treasury checks exact integer B=max(1N,F/5).min(5N), R=min(F/10,B/2,2N), N=10^24. Goal owns meaning/accuracy of F and final forfeiture; Treasury cannot independently verify their economic truth. No multiplication overflow. Zero F => B=1N,R=0.

Decision="QualificationRejected"|"ProofValid"|"ProofInvalidFinal"|"NeutralRefund". Outcome="REFUND"|"SLASH". Decisions immutable after first attestation (same decision replay accepted), before refund_after, while Bound (or Deposited for QualificationRejected). SLASH needs ProofValid + Bound. REFUND needs rejected/final invalid/neutral. No reward for rejection, valid proof, timeout or unbound escrow.

Escrow ID=lowercase SHA256(UTF8 JSON array [goal,case_id,submission_id]); funding hash=SHA256(JSON [goal,funding_id]). All methods after fund_reward use its RETURNED funding hash, not caller funding label. IDs/policy_version nonempty <=128 bytes. proof_id MUST encode immutable Proof Attempt, e.g. milestone-index:proof-hash:attempt. One escrow per (Goal,proof_id) AND (Goal,case_id) permanently; no Multi-Challenger. Separate Goal domains permit same textual case IDs.

Register exact immutable intent again => same ID (for callback recovery); changing stored fields fails. A new payable deposit/funding replay ALWAYS fails, returning attached NEAR by runtime; it is never silently accepted. One funding per escrow, total funding <=cap; partial funding allowed but no topup. Settlement IDs <=128 bytes, globally unique per Goal across bond and reward settlements; use distinct namespaces bond:<case> and reward:<case>. Same escrow/id/outcome replay is no-op; conflict fails. pay_reward same ID is no-op; retry uses dedicated method.

Bond={escrow_id,goal,proof_id,case_id,submission_id,payer:String,amount,reward_cap:U128,deposit_until_ns,bind_until_ns,refund_after_ns:u64,created_at_ns:u64|null,status:Status|null,settlement_id:String|null,outcome:Outcome|null,attempt:u64,decision:Decision|null,demo:bool}. Null status=unfunded intent. Status=Deposited|Bound|RefundPending|Refunded|Slashed|RefundRetryable.

Reward={funding_id (hash),escrow_id,goal,beneficiary:String,amount:U128,status:Funded|PayPending|Paid|PayRetryable,settlement_id:String|null,attempt:u64}. beneficiary always escrow original payer, not caller-selected. Funding requires ProofInvalidFinal AND accepted REFUND settlement; payment requires confirmed principal Refunded. Timely final settlement may be funded/paid after deadline because its decision is already final; an unsettled case instead times out neutrally and loses reward eligibility.

Goal={authorized:bool,policy_version:String}. policy_version is registry metadata, not proof of code behavior. Consumers must query interface_version before registering Goals.

Liabilities={locked,in_flight,buyback,reward_funding,reward_in_flight,storage_required,operating_buffer,account_balance:U128}. locked=Deposited+Bound+RefundRetryable; in_flight=RefundPending; reward_funding=Funded+PayRetryable; reward_in_flight=PayPending. Buyback=all slashed principal per Goal, no withdraw. Storage=runtime storage_usage*bytecost; buffer=0.1N separately sponsored. Admission checks conservatively require balance >= sum(all liabilities, including pending)+storage+buffer; pending transfers may already be debited so this can temporarily reject operations until callbacks, never authorizes excess spend. At quiescence balance must cover all remaining liabilities+storage+buffer. During pending reconcile receipt debits/refunds separately.

Deposits: now<deposit_until<bind_until<refund_after at registration. Bind: now<bind_until. New decision/settlement: now<refund_after. Orphan refund: Deposited and now>=bind_until. Bound timeout: now>=refund_after, including unfinished arbitration => NeutralRefund; no truth implied. Timeouts cannot extend. Revoked Goal cannot prevent permissionless timeout/retry. Pausing does not block exits. Already Slashed or refund Pending/Retryable/Refunded cannot timeout again. Terminal outcomes immutable.

Provide 300 Tgas to Mock orchestration; Treasury settlement/retry at least100 Tgas. Payout checks >=25 Tgas remaining; private callback reserves10 Tgas; bounded empty transfer result. Before sending: ledger Pending and increment attempt; callback matches state+attempt; failure restores reserved liability; stale result no-op. Lost callback stays Pending; never blindly resend.

NEP-297-style logs: EVENT_JSON with standard proofee_bond, version0.2.0, event name, data:[Bond or Reward]. Bond events intent_registered,deposited,bound,decision_authorized,refund_pending,refunded,refund_retryable,slashed; Reward events reward_funded,reward_pending,reward_paid,reward_retryable. Replay no events. Admin changes have no emitted event; inspect receipt/view.

Optional TEST build only: feature demo-registry exposes admin-only register_demo_case(goal,proof_id,case_id,submission_id,payer,amount,deposit_until_ns,bind_until_ns,refund_after_ns)->String. It creates explicitly demo=true records with reward_cap=0, no Reward path, and retains exact payer/amount/state/auth/payout protections. NEVER use this feature for deployed production-interface Treasury; shipped default WASM omits export. This testing ABI differs from versioned production ABI deliberately and is not intended for integration.
