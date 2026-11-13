// Testnet ONLY. Read signer JSON locally; never prints or uploads private keys.
// Usage: NODE_PATH=tests/sandbox/node_modules node scripts/testnet-demo.cjs /secure/admin.json
const fs = require("fs"),
  path = require("path"),
  assert = require("node:assert/strict"),
  { spawnSync } = require("child_process");
const near = require("near-api-js");
const N = 10n ** 24n;
const ROOT = path.resolve(__dirname, "..");
const out = path.join(ROOT, "work/testnet-evidence");
fs.mkdirSync(out, { recursive: true, mode: 0o700 });
class CurlProvider extends near.providers.JsonRpcProvider {
  async sendJsonRpc(method, params) {
    for (let attempt = 0; attempt < 4; attempt++) {
      const headers = path.join(out, "rpc-headers.txt");
      const r = spawnSync(
        "curl",
        [
          "-sS",
          "--max-time",
          "90",
          "-D",
          headers,
          "-w",
          "\n%{http_code}",
          "https://rpc.testnet.near.org",
          "-H",
          "Content-Type: application/json",
          "--data-binary",
          "@-",
        ],
        {
          input: JSON.stringify({
            jsonrpc: "2.0",
            id: "proofee-testnet",
            method,
            params,
          }),
          encoding: "utf8",
          maxBuffer: 20 * 1024 * 1024,
        },
      );
      if (r.status !== 0)
        throw new Error(
          `Testnet RPC curl exit ${r.status}: ${r.stderr}; reconcile signed transaction before any new financial retry`,
        );
      const split = r.stdout.lastIndexOf("\n"),
        status = Number(r.stdout.slice(split + 1));
      if (status === 429 && attempt < 3) {
        const h = fs.readFileSync(headers, "utf8");
        const m = h.match(/retry-after:\s*(\d+)/i);
        const wait = m ? Number(m[1]) : 30 * (attempt + 1);
        console.log(
          "Testnet RPC rate limit; respecting retry-after",
          wait,
          "seconds",
        );
        await new Promise((resolve) => setTimeout(resolve, wait * 1000));
        continue;
      }
      if (status !== 200) throw new Error("Testnet RPC HTTP " + status);
      const x = JSON.parse(r.stdout.slice(0, split));
      if (x.error) {
        if (typeof x.error.data === "object")
          throw near.utils.rpc_errors.parseRpcError(x.error.data);
        throw new Error(JSON.stringify(x.error));
      }
      return x.result;
    }
  }
}
(async () => {
  const credential = JSON.parse(fs.readFileSync(process.argv[2], "utf8"));
  assert(credential.account_id.endsWith(".testnet"));
  const ks = new near.keyStores.InMemoryKeyStore();
  await ks.setKey(
    "testnet",
    credential.account_id,
    near.KeyPair.fromString(credential.private_key),
  );
  const provider = new CurlProvider({ url: "https://rpc.testnet.near.org" });
  const signer = new near.InMemorySigner(ks);
  const connection = new near.Connection("testnet", provider, signer);
  const root = new near.Account(connection, credential.account_id);
  const accounts = {
    admin: root.accountId,
    treasury: "treasury." + root.accountId,
    mock: "mock." + root.accountId,
    payer: "payer." + root.accountId,
    verifier: "verifier." + root.accountId,
  };
  assert(Object.values(accounts).every((s) => s.endsWith(".testnet")));
  const previous =
    process.env.PROOFEE_RESUME === "1" &&
    fs.existsSync(path.join(out, "transactions.json"))
      ? JSON.parse(fs.readFileSync(path.join(out, "transactions.json"), "utf8"))
      : null;
  if (previous) assert.deepEqual(previous.accounts, accounts);
  const evidence = previous || {
    network: "testnet",
    accounts,
    transactions: [],
    views: {},
  };
  const save = () =>
    fs.writeFileSync(
      path.join(out, "transactions.json"),
      JSON.stringify(evidence, null, 2),
    );
  function record(label, r) {
    evidence.transactions.push({
      label,
      hash: r.transaction.hash,
      explorer: "https://testnet.nearblocks.io/txns/" + r.transaction.hash,
      result: r,
    });
    save();
    console.log(label, r.transaction.hash);
    if (r.status.Failure) throw new Error(label + " failed");
    return r;
  }
  async function send(who, receiverId, method, args = {}, amount = 0n) {
    const raw = Buffer.isBuffer(args)
      ? args
      : Buffer.from(JSON.stringify(args));
    return record(
      method,
      await who.signAndSendTransaction({
        receiverId,
        actions: [
          near.transactions.functionCall(method, raw, 300000000000000n, amount),
        ],
      }),
    );
  }
  async function view(receiver_id, method_name, args = {}) {
    const x = await provider.query({
      request_type: "call_function",
      account_id: receiver_id,
      method_name,
      args_base64: Buffer.from(JSON.stringify(args)).toString("base64"),
      finality: "final",
    });
    return JSON.parse(Buffer.from(x.result).toString());
  }
  async function create(id, amount, wasm, init, keyed) {
    if (
      process.env.PROOFEE_RESUME === "1" &&
      evidence.transactions.some((t) => t.label === "create " + id)
    ) {
      if (keyed) {
        const saved = JSON.parse(
          fs.readFileSync(
            path.join(path.dirname(process.argv[2]), id + ".json"),
            "utf8",
          ),
        );
        await ks.setKey(
          "testnet",
          id,
          near.KeyPair.fromString(saved.private_key),
        );
      }
      if (wasm) {
        const state = await provider.query({
          request_type: "view_account",
          account_id: id,
          finality: "final",
        });
        const expected = near.utils.serialize.base_encode(
          require("crypto")
            .createHash("sha256")
            .update(
              fs.readFileSync(
                path.join(ROOT, "target/wasm32-unknown-unknown/release", wasm),
              ),
            )
            .digest(),
        );
        assert.equal(state.code_hash, expected, "Deployed WASM differs");
      }
      return new near.Account(connection, id);
    }
    const actions = [
      near.transactions.createAccount(),
      near.transactions.transfer(amount),
    ];
    if (keyed) {
      const k = near.KeyPair.fromRandom("ed25519");
      await ks.setKey("testnet", id, k);
      fs.writeFileSync(
        path.join(path.dirname(process.argv[2]), id + ".json"),
        JSON.stringify({
          account_id: id,
          public_key: k.getPublicKey().toString(),
          private_key: k.toString(),
        }),
        { mode: 0o600 },
      );
      actions.push(
        near.transactions.addKey(
          k.getPublicKey(),
          near.transactions.fullAccessKey(),
        ),
      );
    }
    if (wasm) {
      actions.push(
        near.transactions.deployContract(
          fs.readFileSync(
            path.join(ROOT, "target/wasm32-unknown-unknown/release", wasm),
          ),
        ),
      );
      actions.push(
        near.transactions.functionCall(
          "new",
          Buffer.from(JSON.stringify(init)),
          100000000000000n,
          0n,
        ),
      );
    }
    record(
      "create " + id,
      await root.signAndSendTransaction({ receiverId: id, actions }),
    );
    return new near.Account(connection, id);
  }
  // Optional separate faucet-funded TEST account supplies sufficient storage endowment.
  if (
    process.argv[3] &&
    !evidence.transactions.some(
      (t) => t.label === "test faucet support transfer",
    )
  ) {
    const support = JSON.parse(fs.readFileSync(process.argv[3], "utf8"));
    assert(support.account_id.endsWith(".testnet"));
    await ks.setKey(
      "testnet",
      support.account_id,
      near.KeyPair.fromString(support.private_key),
    );
    const sa = new near.Account(connection, support.account_id);
    record(
      "test faucet support transfer",
      await sa.signAndSendTransaction({
        receiverId: root.accountId,
        actions: [near.transactions.transfer((98n * N) / 10n)],
      }),
    );
  }
  // Refuse any overwrite; intended as a once-only deployment/demo on fresh subaccounts.
  for (const id of Object.values(accounts).filter(
    (x) => x !== accounts.admin,
  )) {
    if (
      process.env.PROOFEE_RESUME === "1" &&
      evidence.transactions.some((t) => t.label === "create " + id)
    )
      continue;
    try {
      await provider.query({
        request_type: "view_account",
        account_id: id,
        finality: "final",
      });
      throw new Error("ACCOUNT_ALREADY_EXISTS " + id);
    } catch (e) {
      if (!String(e).includes("UNKNOWN_ACCOUNT")) throw e;
    }
  }
  const t = await create(
    accounts.treasury,
    6n * N,
    "bond_treasury.wasm",
    { admin: accounts.admin },
    false,
  );
  const g = await create(
    accounts.mock,
    6n * N,
    "mock_goal.wasm",
    {
      admin: accounts.admin,
      verifier: accounts.verifier,
      treasury: accounts.treasury,
    },
    false,
  );
  const payer = await create(accounts.payer, 3n * N, null, null, true);
  const verifier = await create(accounts.verifier, N, null, null, true);
  await send(root, t.accountId, "register_goal", {
    goal: g.accountId,
    policy_version: "proofee-treasury/0.2.0",
  });
  async function prepare(case_id) {
    const block = await provider.block({ finality: "final" });
    const now = BigInt(block.header.timestamp_nanosec);
    const intent = {
      proof_id: "proof:" + case_id,
      case_id,
      submission_id: "0",
      payer: payer.accountId,
      fee_snapshot: (10n * N).toString(),
      amount: (2n * N).toString(),
      reward_cap: N.toString(),
      deposit_until_ns: "D",
      bind_until_ns: "B",
      refund_after_ns: "R",
    };
    const raw = JSON.stringify({ intent })
      .replace('"D"', String(now + 3600n * 1000000000n))
      .replace('"B"', String(now + 7200n * 1000000000n))
      .replace('"R"', String(now + 10800n * 1000000000n));
    await send(root, g.accountId, "prepare_challenge", Buffer.from(raw));
    let c = await view(g.accountId, "get_case", { case_id });
    assert.equal(c.stage, "Ready");
    await send(
      payer,
      t.accountId,
      "deposit_bond",
      { escrow_id: c.escrow_id },
      2n * N,
    );
    await send(payer, g.accountId, "confirm_escrow", { case_id });
    assert.equal(
      (await view(g.accountId, "get_case", { case_id })).stage,
      "Bound",
    );
    return c;
  }
  let c = await prepare("qualification-rejected");
  const before = BigInt(
    (
      await provider.query({
        request_type: "view_account",
        account_id: payer.accountId,
        finality: "final",
      })
    ).amount,
  );
  await send(root, g.accountId, "qualify", {
    case_id: c.intent.case_id,
    qualified: false,
  });
  let b = await view(t.accountId, "get_bond", { escrow_id: c.escrow_id });
  assert.equal(b.status, "Refunded");
  const after = BigInt(
    (
      await provider.query({
        request_type: "view_account",
        account_id: payer.accountId,
        finality: "final",
      })
    ).amount,
  );
  assert.equal(after - before, 2n * N);
  evidence.views.rejected = b;
  save();
  c = await prepare("winning-challenge");
  await send(root, g.accountId, "qualify", {
    case_id: c.intent.case_id,
    qualified: true,
  });
  await send(verifier, g.accountId, "arbitrate", {
    case_id: c.intent.case_id,
    valid: false,
  });
  assert.equal(
    (await view(t.accountId, "get_bond", { escrow_id: c.escrow_id })).status,
    "Refunded",
  );
  await send(root, g.accountId, "refresh_bond", { case_id: c.intent.case_id });
  await send(root, g.accountId, "fund_case", {
    case_id: c.intent.case_id,
    amount: N.toString(),
  });
  let wc = await view(g.accountId, "get_case", { case_id: c.intent.case_id });
  assert.equal(wc.stage, "Funded");
  await send(root, g.accountId, "claim_reward", { case_id: c.intent.case_id });
  let r = await view(t.accountId, "get_reward", { funding_id: wc.funding_id });
  assert.equal(r.status, "Paid");
  evidence.views.reward = r;
  save();
  c = await prepare("losing-challenge");
  await send(root, g.accountId, "qualify", {
    case_id: c.intent.case_id,
    qualified: true,
  });
  await send(verifier, g.accountId, "arbitrate", {
    case_id: c.intent.case_id,
    valid: true,
  });
  b = await view(t.accountId, "get_bond", { escrow_id: c.escrow_id });
  assert.equal(b.status, "Slashed");
  evidence.views.slashed = b;
  const liabilities = await view(t.accountId, "get_liabilities");
  assert.equal(liabilities.locked, "0");
  assert.equal(liabilities.in_flight, "0");
  assert.equal(liabilities.reward_funding, "0");
  assert.equal(liabilities.reward_in_flight, "0");
  assert.equal(liabilities.buyback, (2n * N).toString());
  const balance = await provider.query({
    request_type: "view_account",
    account_id: t.accountId,
    finality: "final",
  });
  assert(
    BigInt(balance.amount) >=
      2n * N +
        BigInt(liabilities.storage_required) +
        BigInt(liabilities.operating_buffer),
  );
  evidence.views.liabilities = liabilities;
  evidence.views.physical_balance = balance;
  evidence.views.treasury_keys = await provider.query({
    request_type: "view_access_key_list",
    account_id: t.accountId,
    finality: "final",
  });
  evidence.views.mock_keys = await provider.query({
    request_type: "view_access_key_list",
    account_id: g.accountId,
    finality: "final",
  });
  evidence.complete = true;
  save();
  console.log("TESTNET_DEMO_PASS", JSON.stringify({ accounts, liabilities }));
})().catch((e) => {
  console.error(String(e));
  process.exitCode = 1;
});
