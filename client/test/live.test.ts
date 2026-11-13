// Read-only checks against testnet: goal-4 (docs/DEMO_RUN.md), goal-5 (docs/DEMO_RUN_TREASURY.md).
import { test } from "node:test";
import assert from "node:assert/strict";
import {
  DEMO_GOAL_ID, FREE_CHALLENGE_GOAL_ID, getGoal, getBalances, getBond, balancesToNear,
  milestoneSharePct, verifySpec,
} from "../src/index.ts";

test("goal-4 on testnet (free challenges)", async () => {
  const goal = await getGoal(FREE_CHALLENGE_GOAL_ID);
  assert.equal(goal.goal_title, "Ship ProoFee on NEAR testnet");
  assert.equal(goal.treasury ?? null, null);
  assert.deepEqual(goal.milestones.map((m) => m.status), ["Accepted", "Rejected", "Pending"]);
  assert.deepEqual(goal.milestones.map((m) => milestoneSharePct(m, goal.immediate_bps)), [15, 25, 40]);

  const b = balancesToNear(await getBalances(FREE_CHALLENGE_GOAL_ID));
  assert.equal(b.total_received, "2");
  assert.equal(b.claimed, "0.7");
  assert.equal(b.forfeited, "0.5");
  assert.equal(b.pending, "0.8");

  const spec = await verifySpec(goal);
  assert.equal(spec.ok, true, JSON.stringify(spec));
  assert.equal(spec.actual, "b5b13f8f26ffb94579183ef029c0a9ff1c25d0717a1f213cdc93b0cab1dd9b8a");
});

test("goal-5 on testnet (bonded challenges)", async () => {
  assert.equal(DEMO_GOAL_ID, "goal-5.proofee-anton.testnet");
  const goal = await getGoal(DEMO_GOAL_ID);
  assert.equal(goal.treasury, "treasury.proofee-bond-lab-20261112-1794481933490.testnet");
  assert.deepEqual(goal.milestones.map((m) => m.status), ["Accepted", "Rejected", "Pending"]);
  assert.deepEqual(goal.milestones.map((m) => m.challenge_attempts), [2, 1, 0]);
  const cases = goal.milestones.map((m) => m.case ?? null);
  assert.deepEqual(cases.map((c) => c && [c.case_id, c.stage, c.decision]),
    [["m0-2", "Settled", "ProofValid"], ["m1-1", "Settled", "ProofInvalidFinal"], null]);

  const bonds = await Promise.all(cases.slice(0, 2).map((c) => getBond(goal.treasury!, c!.escrow_id!)));
  assert.deepEqual(bonds.map((x) => [x?.status, x?.outcome]), [["Slashed", "SLASH"], ["Refunded", "REFUND"]]);

  const b = balancesToNear(await getBalances(DEMO_GOAL_ID));
  assert.equal(b.total_received, "1"); // bonds never count as goal income
  assert.equal(b.claimed, "0.35");
  assert.equal(b.forfeited, "0.25");
  assert.equal(b.pending, "0.4");
});
