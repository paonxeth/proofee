# Testnet demo checklist — NOT EXECUTED

Only separate fake test cases/test NEAR. Deployment deferred by user. Every transaction hash below is intentionally NOT RUN; do not populate fabricated explorer links.

| Step | Caller | Expected final state / money | Tx hash |
|---|---|---|---|
| Atomic account/create/deploy/new | funded testnet parent | no keys; zero principal liabilities | NOT RUN |
| register_goal + register_demo_case | admin | fixed payer, 2 NEAR, explicit three deadlines | NOT RUN |
| deposit_bond | exact payer, exactly 2 NEAR | Deposited; locked +2 | NOT RUN |
| Qualification rejected: settle REFUND | owning demo Goal | Pending -> Refunded; payer gets exactly 2 NEAR; rewards 0 | NOT RUN |
| Replay same settlement | owning demo Goal | no extra transfer | NOT RUN |
| New case/deposit/bind/settle SLASH | payer then Goal | Slashed; locked -2; goal Buyback +2; no wallet transfer | NOT RUN |
| Outsider settle / private callback | outsider | failed receipt; no money changes | NOT RUN |
| New unbound case past bind_until | anyone | timeout REFUND to original payer | NOT RUN |
| New bound case during unfinished arbitration past refund_after | anyone | safe REFUND; late Goal SLASH rejected | NOT RUN |
| Actual failed refund recipient + retry | isolated disposable payer | Retryable -> Pending -> Refunded; never double-paid | NOT RUN |

Before/after each signed operation record: block timestamp; transaction hash plus ALL child receipt outcomes; get_bond; get_liabilities; get_buyback_balance for both Goals; payer/Treasury account balances and storage. Wallet deposit sender pays tx gas, so compare principal separately from caller gas. For refund, have Goal (not payer) trigger settlement so payer balance difference is exactly B. Slash has no outgoing transfer. Buffer/storage and tx gas are not principal.

Use README exact CLI calls. Explorer link format https://testnet.nearblocks.io/txns/HASH . `settle_bond` successful receipt does NOT prove final transfer succeeded; require get_bond.status==Refunded. If Retryable, permissionless retry_failed_refund with >=100 Tgas. If Pending, inspect callback receipt: never reset or resend manually. If funding check fails, donate through fund_storage then retry; never reduce bond amount. If adapter unavailable, keep app P0 without monetary Challenge and label Treasury independent centralized demo.

Gate: all unit tests + actual sandbox execution + explicit timeout decisions + testnet balances/receipts + review. Phase B must not be claimed complete before full adapter/funding/reward workflow evidence.
