# Frontend handoff — Testnet only

## Launch Wizard

The MVP Launcher is an off-chain Node.js orchestration module (`scripts/launch.cjs`),
not a deployed multi-tenant factory or hosted HTTP service. Import `launch(config)`
in a trusted deployment worker or use its CLI; never expose NEAR_SIGNER_FILE to the
browser. An application authentication/funding service is not included.

Inputs: `network:"testnet"`, immutable unique `launch_id`, `name`, `symbol`, `decimals`,
`image_uri` (NEP-148 icon URI), optional `reference` and base64 `reference_hash`,
`creator_id`, `goal_id`, `init_point`, `left_point`, `right_point`.
Supply is not an input: 1,000,000,000 whole tokens. DCL/wNEAR/fee are fixed.
Omitted creator/Goal are only for isolated demonstration account creation.

Before accepting a price, calculate its raw point with token decimals. The example
uses decimals=18, init=-69400, range=[-69200,-59400]. This creates an X-only position
without initial wNEAR; the first actual trading point is inside the range, not the
empty initial spot. Do not present init_point as a guaranteed execution price.
Exact all-supply inclusion is limited by DCL integer rounding. Show the locked dust
and actual pool.total_x explicitly. Supply sent to DCL is not the same as LP principal.

The orchestration worker emits creation/registration labels and hashes. Persist its
registry outside source control; poll `get_launch({})` on Locker after deployment.
Never mark a stage complete from a submitted transaction alone. The source of truth
is FINAL receipts plus contract state and DCL get_liquidity.

## Locker ABI

Generated full ABI: `abi/locker.abi.json`. There are four normal methods:

| Method | Arguments | Deposit | Gas | Caller |
|---|---|---:|---:|---|
| new | creator_id:AccountId, goal_id:AccountId, token_id:AccountId, decimals:u8, init_point:i32, left_point:i32, right_point:i32 | 0 | 100 TGas | account-creation batch, once |
| get_launch | {} | view | — | anyone |
| advance | {} | 0 | 300 TGas | anyone |
| collect | {} | 0 | 300 TGas | anyone, Active only |

`on_complete({})` is a private callback; a wallet must never call it.
No method accepts fee receiver, principal amount or replacement Goal.

get_launch returns creator_id, goal_id, token_id, pool_id, lpt_id, supply (raw decimal
string), phase, pending, principal_liquidity, dust_x/y, fee_x/y, swapped_wnear,
native_due, native_paid, last_error. All amount fields are strings; use BigInt.
principal_liquidity is DCL liquidity units, not whole tokens or market value.

| Phase | UI operation |
|---|---|
| Pool | advance: create DCL pool |
| Seed | advance: full-supply HotZap |
| Verify | advance: verify sole LP owner/pool/range |
| Baseline | advance: record initial DCL principal dust |
| Active | launch complete; collect available |
| ReadFees | advance: distinguish fees from principal dust |
| WithdrawX, CheckX | advance: withdraw and verify token fee |
| SwapBaseline, Quote, Swap, CheckSwap | advance: token fee → wNEAR, verify actual amount |
| WithdrawY, CheckY | advance: withdraw and verify original wNEAR fee |
| Unwrap | advance: unwrap exact combined wNEAR fee amount |
| Pay | advance: transfer native_due only to bound Goal |
| Busy | wait; if last_error exists show frozen/needs inspection, never auto-reset |

## Trade UI

Reuse `abi/dcl-testnet.abi.json` and `abi/standard-ft.abi.json` from the verified PoC.
Register buyer in token and wrap.testnet. Call DCL.quote({pool_ids:[pool_id],
input_token,output_token,input_amount}) and display the positive minimum output.
Call the **input FT** ft_transfer_call({receiver_id:"dclv2.ref-dev.testnet",amount,
msg:JSON.stringify({Swap:{pool_ids:[pool_id],output_token,min_output_amount}})}),
1 yoctoNEAR, 250 TGas. Buy input is wNEAR; wrap via near_deposit first.
DCL's default wrap output may be settled as native NEAR. Inspect actual receipts;
do not unwrap already-native Sell proceeds. No additional Buy/Sell tax exists.

## Dashboard and fee collection

Show swap total fees, protocol allocation and this position's LP fees separately.
In the tested DCL deployment, pool.total_fee_*_charged matched protocol_fee in the
swap events, not total_fee. Use the actual event fields, not the counter's name.
Use get_liquidity({lpt_id}) and Locker accounting, never pool-wide fees as revenue.
A fee conversion trade itself pays DCL trading fees and can accrue new LP fees for
the same position; a following collect can legitimately return additional revenue.
Distinguish that from a repeated payout of the previous cycle.

For a lock badge verify all of:
1. DCL get_liquidity.owner_id == locker_id, matching pool/range and expected principal.
2. Locker code_hash matches the published artifact; token and Locker access-key lists empty.
3. Locker goal/token/pool/lpt bindings match the launch registry.
4. No outstanding error is hidden behind a success label.
The creator and deployment root cannot redeploy to a keyless child. This relies on
NEAR action authorization plus Rhea's ownership semantics; external DCL upgrades
remain an explicit trust assumption.

Collect: collect({}), then repeatedly advance({}) while phase is neither Active nor
Busy. Each is a separate wallet/relayer transaction. There is no single-call auto-
keeper in this MVP. Users can sponsor gas but cannot pick payout recipients.
Do not call collect again while another cycle runs.

## Goal ABI and Rust ownership boundary

Use `abi/goal.abi.json` generated from the unchanged repository source. Never send FT
or wNEAR to Goal. Locker pays a plain native NEAR transfer. Then anyone calls
`sync({})`, deposit=0, 50 TGas. Read `get_goal({})` and `get_balances({})`.
Only the configured creator calls `claim({})`, deposit=0, 100 TGas. Goal owns its 20/80
accounting and callback rollback. No Treasury interface is added or changed.

A future Goal version must expose equivalent native receipt accounting, immutable
creator/binding readback, permissionless idempotent sync and creator-only claim
with failed-transfer rollback. A changed ABI requires an explicit adapter decision;
do not silently deploy or replace a Rust developer's Goal.

## Operational limitations

MVP, not audited. Fixed slippage against the pool's quote does not prevent price
manipulation. No admin escape is provided for ambiguous callbacks. Progress is
resumable for known states; unknown partial executions remain frozen. Check the
report for live results and limitations rather than assuming all acceptance items
passed. Keep signing files, caches and node_modules outside release archives.
