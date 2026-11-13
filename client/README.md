# ProoFee client

A framework-free TypeScript client for ProoFee goal contracts on NEAR testnet. It reads a goal
over JSON-RPC with plain `fetch` and builds **unsigned** transactions for a wallet. No runtime
dependencies, no keys: signing is the wallet's job. Works in any frontend (Next.js, Vite, plain
HTML) and in Node 18+.

Contract specification: [../docs/SPEC.md](../docs/SPEC.md). Types follow the contract ABI
(`cargo near abi` in `contracts/goal`).

## Install

The client is not published to npm. Either copy `src/index.ts` into your project, or build it and
import `dist/index.js`:

```sh
cd client
npm install
npm run build      # src/index.ts → dist/index.js + dist/index.d.ts
```

`dist/` is committed so that `demo.html` works without a build; rebuild it after changing `src/`.

In a workspace you can also add it as a local dependency: `"@proofee/client": "file:../client"`.

## Read a goal

```ts
import {
  DEMO_GOAL_ID, getGoal, getBalances, balancesToNear, milestoneSharePct, verifySpec,
} from "@proofee/client";

const goal = await getGoal(DEMO_GOAL_ID);           // GoalView, as returned by get_goal
const balances = await getBalances(DEMO_GOAL_ID);   // BalancesView, yoctoNEAR strings
const near = balancesToNear(balances);              // { claimable: "0", claimed: "0.7", ... }

for (const m of goal.milestones) {
  // Share of ALL fees: weight_bps × (10000 − immediate_bps) / 1e6, e.g. 1875 bps at 20% → 15
  console.log(m.title, m.status, milestoneSharePct(m, goal.immediate_bps) + "%");
}

const spec = await verifySpec(goal);   // downloads goal_uri, sha256 via crypto.subtle
spec.ok;        // true if sha256(file) === goal.goal_spec_hash
spec.content;   // the spec file as text
```

- RPC endpoint: `https://rpc.testnet.near.org` by default; pass `{ rpcUrl }` to change it.
- `yoctoToNear("700000000000000000000000") === "0.7"` — exact, string in, string out.
- `verifySpec` never throws: on a download error it returns `ok: false` with `error`.
- In browsers `crypto.subtle` exists only on https or `localhost`.
- On-chain strings (titles, proof links, challenge reasons) are written by users: render them as
  text, and only link `http(s)` URLs.

## Bonded challenges (goals with a Bond Treasury)

A goal deployed with a treasury (release `goal-treasury`, SPEC 6a) returns `treasury` in
`get_goal`, and each milestone has `challenge_attempts` and `case` — the latest bonded challenge
request. Older goals (goal-4) have none of these fields: they are optional in the types.

```ts
import { getGoal, getBond, yoctoToNear, nsToMs, nearblocksUrl, FINAL_CASE_STAGES } from "@proofee/client";

const goal = await getGoal("goal-5.proofee-anton.testnet");
for (const m of goal.milestones) {
  const c = m.case;
  if (!goal.treasury || !c) continue;
  c.stage;                            // "Bound", "Settled", ... (FINAL_CASE_STAGES: nothing more happens)
  yoctoToNear(c.amount);              // bond, NEAR
  new Date(nsToMs(c.refund_after_ns)); // verifier deadline
  const bond = c.escrow_id ? await getBond(goal.treasury, c.escrow_id) : null;
  bond?.status;                       // "Deposited" | "Bound" | "Refunded" | "Slashed" | ... ; null: intent only
}
nearblocksUrl(goal.treasury!);        // https://testnet.nearblocks.io/address/<treasury>
```

Contract times in a case are nanoseconds and exceed `Number.MAX_SAFE_INTEGER`; `nsToMs` rounds
to the nearest millisecond; do not compare raw ns values. The case `stage` is the goal's view;
the bond's money outcome is `getBond(...).status`. Stages, errors and who acts next:
[../release/goal-treasury/errors-events.md](../release/goal-treasury/errors-events.md). The client
builds no bonded-challenge transactions yet (read-only).

## Build transactions

Each function returns `{ receiverId, actions: [FunctionCall] }` with 100 TGas and a zero deposit —
the format of `signAndSendTransaction` in NEAR Wallet Selector and near-connect.

| Function | Contract method | Who may call |
| --- | --- | --- |
| `syncTx(goalId)` | `sync` | anyone |
| `submitProofTx(goalId, index, proofUri, proofHash)` | `submit_proof` | creator, before the deadline |
| `challengeTx(goalId, index, reason)` | `challenge` | anyone except creator and verifier, within the window |
| `finalizeTx(goalId, index)` | `finalize` | anyone, after the window |
| `resolveTx(goalId, index, accept)` | `resolve` | verifier, within 24 h of the challenge |
| `resolveTimeoutTx(goalId, index)` | `resolve_timeout` | anyone, 24 h after the challenge |
| `expireTx(goalId, index)` | `expire` | anyone, after the deadline |
| `claimTx(goalId)` | `claim` | creator |

```ts
import { DEMO_GOAL_ID, claimTx } from "@proofee/client";

const wallet = await selector.wallet();              // or a near-connect wallet
await wallet.signAndSendTransaction(claimTx(DEMO_GOAL_ID)); // the wallet asks the user to sign
```

The contract checks the caller, status and time itself; a wrong call fails with a short error such
as `"Only creator"` or `"Window still open"` (SPEC 7).

## Demo page

`demo.html` reads `goal-4.proofee-anton.testnet` and shows the goal, milestones with statuses and
shares, balances and the spec hash check. For a goal with a treasury it also links the Treasury on
NearBlocks and shows under each milestone its bonded request: stage, challenger, bond, deadlines,
decision and the bond status from `treasury.get_bond`. Read-only. ES modules and `crypto.subtle` need a local server:

```sh
cd client
python3 -m http.server 8000
# open http://localhost:8000/demo.html
# another goal: http://localhost:8000/demo.html?goal=<account>.testnet
# bonded challenges: http://localhost:8000/demo.html?goal=goal-5.proofee-anton.testnet
```

## Tests

```sh
npm test            # offline: formatting, shares, transactions, spec check, get_bond, old/new get_goal
npm run test:live   # read-only: goal-4 on testnet must match docs/DEMO_RUN.md
```
