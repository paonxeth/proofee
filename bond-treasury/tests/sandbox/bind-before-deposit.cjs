const { Worker } = require("near-workspaces"),
  assert = require("node:assert/strict"),
  path = require("path");
(async () => {
  const w = await Worker.init({ rootAccountId: "testnet" });
  try {
    const root = w.rootAccount,
      t = await root.createSubAccount("bindingtreasury", {
        initialBalance: "10000000000000000000000000",
      }),
      g = await root.createSubAccount("bindingmock", {
        initialBalance: "10000000000000000000000000",
      }),
      p = await root.createSubAccount("bindingpayer");
    const tx = (who, to, m, args = {}, amount = "0") =>
      who.call(to, m, args, {
        gas: "300000000000000",
        attachedDeposit: amount,
      });
    await t.deploy(path.resolve(__dirname, "../../release/bond_treasury.wasm"));
    await tx(root, t, "new", { admin: root.accountId });
    await g.deploy(path.resolve(__dirname, "../../release/mock_goal.wasm"));
    await tx(root, g, "new", {
      admin: root.accountId,
      verifier: root.accountId,
      treasury: t.accountId,
    });
    await tx(root, t, "register_goal", {
      goal: g.accountId,
      policy_version: "proofee-treasury/0.2.0",
    });
    const b = await w.provider.block({ finality: "final" }),
      now = BigInt(b.header.timestamp_nanosec);
    const i = {
      proof_id: "unfunded-proof",
      case_id: "unfunded",
      submission_id: "0",
      payer: p.accountId,
      fee_snapshot: "0",
      amount: "1000000000000000000000000",
      reward_cap: "0",
      deposit_until_ns: "D",
      bind_until_ns: "B",
      refund_after_ns: "R",
    };
    const raw = JSON.stringify({ intent: i })
      .replace('"D"', String(now + 1000n * 10n ** 9n))
      .replace('"B"', String(now + 2000n * 10n ** 9n))
      .replace('"R"', String(now + 3000n * 10n ** 9n));
    await tx(root, g, "prepare_challenge", Buffer.from(raw));
    await tx(p, g, "confirm_escrow", { case_id: "unfunded" });
    let c = await g.view("get_case", { case_id: "unfunded" });
    assert.equal(c.stage, "BindRetryable");
    await tx(
      p,
      t,
      "deposit_bond",
      { escrow_id: c.escrow_id },
      "1000000000000000000000000",
    );
    await tx(p, g, "confirm_escrow", { case_id: "unfunded" });
    c = await g.view("get_case", { case_id: "unfunded" });
    assert.equal(c.stage, "Bound");
    await tx(root, g, "qualify", { case_id: "unfunded", qualified: false });
    assert.equal(
      (await t.view("get_bond", { escrow_id: c.escrow_id })).status,
      "Refunded",
    );
    console.log(
      "PASS bind before deposit rejects verification, stays retryable, then real deposit/bind/refund succeeds",
    );
  } finally {
    await w.tearDown();
  }
})().catch((e) => {
  console.error(e);
  process.exitCode = 1;
});
