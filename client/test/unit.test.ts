// Offline tests: formatting, shares, transactions, spec check with a stub fetch.
import { test } from "node:test";
import assert from "node:assert/strict";
import {
  yoctoToNear, balancesToNear, milestoneSharePct, immediateSharePct, sha256Hex, verifySpec,
  viewFunction, syncTx, submitProofTx, challengeTx, finalizeTx, resolveTx, resolveTimeoutTx,
  expireTx, claimTx, getGoal, getBond, nearblocksUrl, nsToMs, FINAL_CASE_STAGES,
  type GoalView, type Milestone, type ChallengeCase,
} from "../src/index.ts";

const milestone = (weight_bps: number): Milestone => ({
  title: "M", weight_bps, deadline_ms: 0, status: "Pending", proof_uri: null, proof_hash: null,
  submitted_at_ms: null, challenger: null, challenge_reason: null, challenged_at_ms: null,
});

test("yoctoToNear is exact", () => {
  assert.equal(yoctoToNear("0"), "0");
  assert.equal(yoctoToNear("1"), "0.000000000000000000000001");
  assert.equal(yoctoToNear("700000000000000000000000"), "0.7");
  assert.equal(yoctoToNear("2000000000000000000000000"), "2");
  assert.equal(yoctoToNear("123456789000000000000000000000"), "123456.789");
  assert.throws(() => yoctoToNear("1.5"));
});

test("balancesToNear converts every field", () => {
  const b = balancesToNear({
    total_received: "2000000000000000000000000", immediate: "400000000000000000000000",
    locked: "1600000000000000000000000", unlocked: "300000000000000000000000",
    forfeited: "500000000000000000000000", pending: "800000000000000000000000",
    claimed: "700000000000000000000000", claimable: "0", unsynced: "0",
  });
  assert.deepEqual(b, {
    total_received: "2", immediate: "0.4", locked: "1.6", unlocked: "0.3", forfeited: "0.5",
    pending: "0.8", claimed: "0.7", claimable: "0", unsynced: "0",
  });
});

test("shares: 20% immediate, 1875/3125/5000 bps → 15/25/40% of all fees", () => {
  const goal = { immediate_bps: 2000 } as GoalView;
  assert.equal(immediateSharePct(goal), 20);
  assert.deepEqual([1875, 3125, 5000].map((w) => milestoneSharePct(milestone(w), 2000)), [15, 25, 40]);
  assert.equal(milestoneSharePct(milestone(4000), 5000), 20);
});

test("sha256Hex", async () => {
  assert.equal(
    await sha256Hex(new TextEncoder().encode("spec")),
    "d4f02eaafd1a9e9de7d10972ca8e47fa7a985825c3c9c1e249c72683cb3e4f19",
  );
});

const stubFetch = (body: string, status = 200) =>
  (async () => new Response(body, { status })) as unknown as typeof fetch;

test("verifySpec: match, mismatch, download error", async () => {
  const goal = {
    goal_uri: "https://example/spec.json",
    goal_spec_hash: "d4f02eaafd1a9e9de7d10972ca8e47fa7a985825c3c9c1e249c72683cb3e4f19",
  } as GoalView;
  const ok = await verifySpec(goal, { fetch: stubFetch("spec") });
  assert.equal(ok.ok, true);
  assert.equal(ok.content, "spec");
  const bad = await verifySpec(goal, { fetch: stubFetch("spec!") });
  assert.equal(bad.ok, false);
  assert.notEqual(bad.actual, goal.goal_spec_hash);
  const missing = await verifySpec(goal, { fetch: stubFetch("", 404) });
  assert.equal(missing.ok, false);
  assert.equal(missing.actual, null);
  assert.match(missing.error ?? "", /404/);
});

test("viewFunction surfaces contract errors", async () => {
  const fetchErr = stubFetch(JSON.stringify({ result: { error: "wasm execution failed" } }));
  await assert.rejects(viewFunction("g.testnet", "get_goal", {}, { fetch: fetchErr }), /wasm execution failed/);
});

test("transactions: receiver, method, args, 100 TGas, zero deposit", () => {
  const g = "goal.testnet";
  const cases: [ReturnType<typeof syncTx>, string, object][] = [
    [syncTx(g), "sync", {}],
    [submitProofTx(g, 1, "https://p", "ab"), "submit_proof", { index: 1, proof_uri: "https://p", proof_hash: "ab" }],
    [challengeTx(g, 1, "why"), "challenge", { index: 1, reason: "why" }],
    [finalizeTx(g, 0), "finalize", { index: 0 }],
    [resolveTx(g, 1, false), "resolve", { index: 1, accept: false }],
    [resolveTimeoutTx(g, 2), "resolve_timeout", { index: 2 }],
    [expireTx(g, 2), "expire", { index: 2 }],
    [claimTx(g), "claim", {}],
  ];
  for (const [tx, methodName, args] of cases) {
    assert.deepEqual(tx, {
      receiverId: g,
      actions: [{ type: "FunctionCall", params: { methodName, args, gas: "100000000000000", deposit: "0" } }],
    });
  }
});

// ---- Treasury mode (goal-treasury release) ----

/** Stub RPC: records the request, answers a view call with `result` as JSON bytes. */
const rpcStub = (result: unknown, seen: { body?: any } = {}) =>
  (async (_url: string, init: RequestInit) => {
    seen.body = JSON.parse(String(init.body));
    const bytes = Array.from(new TextEncoder().encode(JSON.stringify(result)));
    return new Response(JSON.stringify({ result: { result: bytes } }));
  }) as unknown as typeof fetch;

test("getBond: calls treasury.get_bond with escrow_id, returns the bond or null", async () => {
  const bond = { escrow_id: "e1", status: "Slashed", outcome: "SLASH", amount: "1000000000000000000000000" };
  const seen: { body?: any } = {};
  const got = await getBond("treasury.testnet", "e1", { fetch: rpcStub(bond, seen) });
  assert.deepEqual(got, bond);
  assert.equal(seen.body.params.account_id, "treasury.testnet");
  assert.equal(seen.body.params.method_name, "get_bond");
  assert.deepEqual(JSON.parse(atob(seen.body.params.args_base64)), { escrow_id: "e1" });
  assert.equal(await getBond("treasury.testnet", "missing", { fetch: rpcStub(null) }), null);
});

test("getGoal: old goals (no treasury fields) and treasury goals both parse", async () => {
  const old = { milestones: [milestone(10000)] };
  const g4 = await getGoal("goal-4.testnet", { fetch: rpcStub(old) });
  assert.equal(g4.treasury ?? null, null);
  assert.equal(g4.milestones[0].case ?? null, null);

  const c: ChallengeCase = {
    proof_id: "0:aa:2", case_id: "m0-2", payer: "c.testnet", reason: "fake", fee_snapshot: "0",
    amount: "1000000000000000000000000", reward_cap: "0", deposit_until_ns: 1, bind_until_ns: 2,
    refund_after_ns: 3, escrow_id: "e2", stage: "Settled", decision: "ProofValid", nonce: 4,
  };
  const g5 = await getGoal("goal-5.testnet", {
    fetch: rpcStub({ treasury: "t.testnet", milestones: [{ ...milestone(10000), challenge_attempts: 2, case: c }] }),
  });
  assert.equal(g5.treasury, "t.testnet");
  assert.deepEqual(g5.milestones[0].case, c);
  assert.ok(FINAL_CASE_STAGES.includes(g5.milestones[0].case!.stage));
});

test("nearblocksUrl and nsToMs", () => {
  assert.equal(nearblocksUrl("goal-5.proofee-anton.testnet"),
    "https://testnet.nearblocks.io/address/goal-5.proofee-anton.testnet");
  assert.equal(nsToMs(1_794_481_933_490_000_000), 1_794_481_933_490);
  assert.equal(nsToMs(1_999_999), 2);
  assert.deepEqual([...FINAL_CASE_STAGES], ["Settled", "TimeoutRequested", "Orphaned"]);
});
