# ProoFee Launcher + LP Locker — Testnet E2E

Date: 12 November 2026. NEAR Testnet only. P0 and the on-chain P1 checks are done.
This is an MVP without an independent audit, not a ready Mainnet deployment.

## Implemented

A sequential Node.js Launcher based on the verified PoC, a separate Rust Locker
per token, NEP-141 without additional minting, pool token/wrap.testnet fee=10000,
HotZap of the entire initial supply, Buy/Sell, LP fee collection, conversion of the token fee to wNEAR,
exact unwrap, native NEAR into a new Goal, sync and Creator Claim.
This is CLI/library orchestration, not a multi-tenant on-chain Factory and not a hosted web API.
The existing Goal and Treasury are not changed. The Goal WASM was built from the source of the main
repository without changes and deployed to a new isolated account.

## Accounts and position

| Purpose | ID |
|---|---|
| Isolated root / creator | `pf-launch-mwdyyhj0.testnet` |
| Token | `token-e2e1.pf-launch-mwdyyhj0.testnet` |
| Locker / LP owner | `lock-e2e1.pf-launch-mwdyyhj0.testnet` |
| New Goal | `goal-e2e1.pf-launch-mwdyyhj0.testnet` |
| Permissionless caller | `caller.pf-launch-mwdyyhj0.testnet` |
| DCL | `dclv2.ref-dev.testnet` |
| wNEAR | `wrap.testnet` |

Pool: `token-e2e1.pf-launch-mwdyyhj0.testnet|wrap.testnet|10000`.

LPT: `token-e2e1.pf-launch-mwdyyhj0.testnet|wrap.testnet|10000#2583`.

Token decimals=18, supply=1000000000000000000000000000 raw = 1 000 000 000 tokens.
Init=-69400, range=[-69200,-59400], no initial wNEAR. The LP is created directly by the Locker.

The whole supply was sent to DCL; 999999999.99999999999987968 tokens went directly into the LP.
The remainder of 120320 raw = 0.00000000000012032
tokens is DCL integer-rounding principal dust that permanently stays on the Locker's internal
balance and is excluded from collections. The strict "every raw unit is in the LP"
is not met because of DCL rounding; this remainder is not called LP liquidity.

## Verified lock

Token, Locker and Goal have empty access key lists. The code hash of each matched the
deployed WASM; details in evidence/onchain-final.json. The Locker has no upgrade method,
no positive remove_liquidity, no arbitrary call, no NFT transfer/approval,
no Goal change and no withdrawal of the whole native balance.

DCL owner_id of the position = Locker. Principal liquidity amount before and after all trades and
three collect cycles: `4057331146356777986151`. The creator and the deployment root could not withdraw
the principal. An attempt by the deployment root to redeploy the same WASM on the keyless Locker
was rejected by NEAR. Dependence on the administration and upgrades of the external DCL remains.

## Fees and the Goal

After the initial Buy/Sell:

| Asset | Total swap fee | DCL protocol fee | Our LP fee |
|---|---:|---:|---:|
| Token, raw | 482234820568394840715492 | 96446964113678968143098 | 385787856454715872572393 |
| wNEAR, raw | 1000000000000000000000 | 200000000000000000000 | 799999999999999999999 |

The protocol share = 20% of the swap fee, the LP about 80%; 1 more raw unit each goes to rounding.
In this deployment pool.total_fee_*_charged equals protocol_fee of the swap event;
it is not the whole pool fee and not ProoFee income. The shares are confirmed by events, not by the field name.

LP token fee: 385787.856454715872572393 tokens. LP wNEAR fee:
0.000799999999999999999999 wNEAR. The swap event of the first fee conversion reports
amount_in=385787856454715872572383, 10 raw units
less than the transferred FT amount; this DCL rounding quirk is kept in the evidence,
and is not declared as additional income or a confirmed refund.
Final Locker wallet token balance = 0; DCL internal assets = the original principal dust.

The first cycle delivered exactly **0.001191987982122365775599 NEAR** to the Goal:
the actual wNEAR output of the fee swap plus the separate wNEAR LP fee. After sync the Goal accrued
20% immediate, 80% milestones; the first Creator Claim = **0.000238397596424473155119 NEAR**.

After a repeated collect and a permissionless collect from another account, final sync/claim:

| Metric | NEAR |
|---|---:|
| Delivered by the Locker / counted by the Goal | 0.0011951483583858763164 |
| Immediate 20%, paid to the creator | 0.00023902967167717526328 |
| Locked milestones 80% | 0.00095611868670870105312 |
| Unsynced / claimable | 0 / 0 |

Repeated collects receive new fees from the previous fee-conversion trades.
This is not a repeated payout of the original income: the new unclaimed_fee and the separate
wNEAR settlement amounts were reconciled. A small new token fee from the last
conversion stays in the LP: 197523382504814527 raw, not yet collected and not recorded as native income.

## Checks and errors

14 unit tests and 25 evidence checks passed. Finality: 62/62 included transactions FINAL,
54 successful, 8 failed. Failed: the initial insufficient storage deposit of the new Goal
(atomic rollback, then a successful launch with 2.1 NEAR) and 7 negative test transactions,
including a repeat of the parent-upgrade ban check. No operations were sent to the existing goal-4.

Negative checks: absence of principal-withdraw and goal-setter in the Locker;
a private callback from an outsider; DCL remove_liquidity from a non-owner;
a parent deploy without keys; someone else's claim. An outside caller ran collect with extra
receiver_id/goal_id: they did not change the immutable Goal, the caller only spent gas.

Separately from the included transactions:
- One attempt was rejected before inclusion: the caller's balance was insufficient for the 300 TGas reserve.
- One signed attempt got RPC 429 on the deprecated rpc.testnet.near.org and was not
  included. Non-inclusion is confirmed: the same signer/public key/nonce was later consumed
  by another FINAL transaction. Proof — evidence/superseded-transactions.json.

Switching the RPC and resuming from the saved on-chain stage did not mint the supply again.
Waiting for FINAL and a chain_id=testnet check before signing were added to the client.

Total tokens_burnt of all 62 transactions, including faucet and negative tests:
**0.0891601768188389 NEAR**. This is gas, not deployment/storage funding,
and not the 300 TGas gas limit. Per-operation breakdown — evidence/transactions.csv.

## Main transactions

| Operation | Testnet transaction |
|---|---|
| create-token | [41AvUu6eWBYEhv1FiEajii3GRrjq2WqFo7LoZjfaxqGr](https://testnet.nearblocks.io/txns/41AvUu6eWBYEhv1FiEajii3GRrjq2WqFo7LoZjfaxqGr) |
| create-locker | [9udHWdV2DxjUW4eS2Ljp7Ffz7We1zSNSWvaB9URQW2BX](https://testnet.nearblocks.io/txns/9udHWdV2DxjUW4eS2Ljp7Ffz7We1zSNSWvaB9URQW2BX) |
| e2e-buy | [6GWzhE7XUmyRfCYNPduvzBaw1iCWGMDaYi8pWMTRtHeT](https://testnet.nearblocks.io/txns/6GWzhE7XUmyRfCYNPduvzBaw1iCWGMDaYi8pWMTRtHeT) |
| e2e-sell | [4ZHXdbFbLzzLa7XRXRAypgYiFvYqGG8Q69hUR5acDUhg](https://testnet.nearblocks.io/txns/4ZHXdbFbLzzLa7XRXRAypgYiFvYqGG8Q69hUR5acDUhg) |
| e2e-collect | [Ba8atbQh3LR7Fu8e2Z21wFxWQHHqLhZjVxHxJxPGBezo](https://testnet.nearblocks.io/txns/Ba8atbQh3LR7Fu8e2Z21wFxWQHHqLhZjVxHxJxPGBezo) |
| e2e-goal-sync | [5yw4UUGkiphvjdETJcgmmfFqz7ZHioqAQcBKfDpg9P4U](https://testnet.nearblocks.io/txns/5yw4UUGkiphvjdETJcgmmfFqz7ZHioqAQcBKfDpg9P4U) |
| e2e-creator-claim | [87itFbHTFWLaoYNQpUguTn7TZ5qoy57YnsykDiz2g7D3](https://testnet.nearblocks.io/txns/87itFbHTFWLaoYNQpUguTn7TZ5qoy57YnsykDiz2g7D3) |
| final-goal-sync | [DEGne4vAaLupoBFJbnh4sv7qBUnzKRtxrSFznQ6w4xcN](https://testnet.nearblocks.io/txns/DEGne4vAaLupoBFJbnh4sv7qBUnzKRtxrSFznQ6w4xcN) |
| final-creator-claim | [DRobdxz39jxFSyPsMM6Uj3XdLtXB1AxnnfkqLt8UaLH3](https://testnet.nearblocks.io/txns/DRobdxz39jxFSyPsMM6Uj3XdLtXB1AxnnfkqLt8UaLH3) |
| negative-parent-upgrade | [1aTfJ1tzGTEVQB3JVkAJgu92JJ7MWzUiZEL2PCKvP2w](https://testnet.nearblocks.io/txns/1aTfJ1tzGTEVQB3JVkAJgu92JJ7MWzUiZEL2PCKvP2w) |
| negative-stranger-claim | [2mALkQhjZXgeN1c8NH5kqaxnVjcKBk1SibSkL5yfjkPa](https://testnet.nearblocks.io/txns/2mALkQhjZXgeN1c8NH5kqaxnVjcKBk1SibSkL5yfjkPa) |

Full list — evidence/final-transactions.json and transactions.csv. All raw RPC,
child receipt outcomes, balances and transaction signatures are available in the release archive
in full-evidence/. There are no private signing keys there.

## Limitations and handoff

- The Locker is immutable/keyless. On an ambiguous state-changing receipt or a lost
  callback it stops without an admin reset; recovery of all such cases is not
  guaranteed. Known read/pay failures and a fully returned seed/swap allow a retry.
- The fee swap has a 1% floor from the current DCL quote, without oracle/MEV protection.
- The whole supply was sent to DCL, but mathematical rounding leaves the locked dust stated above.
- The Launcher is a CLI/library; no frontend/backend service is deployed. The handoff describes the integration.
- The second PoC archive is unavailable. The first archive and the current Goal ABI from the repository were used.
- The branch `feat/testnet-launcher-locker` is local. The GitHub connection grants pull=true,
  push=false; no remote branch/PR was created. The patch adds only launcher/.
- The generated Testnet signer files are outside the repository and the release; they must be kept
  in secret storage for a future Creator Claim. Key values are not published.

Run, ABI and frontend instructions: README.md, INTEGRATION_HANDOFF.md, abi/.
WASM provenance and hashes: SOURCE_PROVENANCE.json. Treasury and the working team accounts are not affected.
