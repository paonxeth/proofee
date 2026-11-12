// Reads goal-4 on testnet (read-only) and checks it against docs/DEMO_RUN.md.
import { test } from "node:test";
import assert from "node:assert/strict";
import { DEMO_GOAL_ID, getGoal, getBalances, balancesToNear, milestoneSharePct, verifySpec } from "../src/index.ts";

test("goal-4 on testnet", async () => {
  const goal = await getGoal(DEMO_GOAL_ID);
  assert.equal(goal.goal_title, "Ship ProoFee on NEAR testnet");
  assert.deepEqual(goal.milestones.map((m) => m.status), ["Accepted", "Rejected", "Pending"]);
  assert.deepEqual(goal.milestones.map((m) => milestoneSharePct(m, goal.immediate_bps)), [15, 25, 40]);

  const b = balancesToNear(await getBalances(DEMO_GOAL_ID));
  assert.equal(b.total_received, "2");
  assert.equal(b.claimed, "0.7");
  assert.equal(b.forfeited, "0.5");
  assert.equal(b.pending, "0.8");

  const spec = await verifySpec(goal);
  assert.equal(spec.ok, true, JSON.stringify(spec));
  assert.equal(spec.actual, "b5b13f8f26ffb94579183ef029c0a9ff1c25d0717a1f213cdc93b0cab1dd9b8a");
});
