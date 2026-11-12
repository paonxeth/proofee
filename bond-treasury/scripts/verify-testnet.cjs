// Independent public RPC readback: code hashes, no keys, and final liabilities.
const fs = require("fs"),
  path = require("path"),
  crypto = require("crypto"),
  assert = require("node:assert/strict"),
  { spawnSync } = require("child_process"),
  near = require("near-api-js");
const root = path.resolve(__dirname, "..");
const evidence = JSON.parse(fs.readFileSync(process.argv[2], "utf8"));
function rpc(params) {
  const r = spawnSync(
    "curl",
    [
      "-fsS",
      "--max-time",
      "30",
      "https://rpc.testnet.near.org",
      "-H",
      "Content-Type: application/json",
      "--data-binary",
      "@-",
    ],
    {
      input: JSON.stringify({
        jsonrpc: "2.0",
        id: "readback",
        method: "query",
        params,
      }),
      encoding: "utf8",
      maxBuffer: 4 * 1024 * 1024,
    },
  );
  if (r.status !== 0) throw new Error(r.stderr);
  const x = JSON.parse(r.stdout);
  if (x.error) throw new Error(JSON.stringify(x.error));
  return x.result;
}
const report = { network: "testnet", contracts: {} };
for (const [role, name] of [
  ["treasury", "bond_treasury"],
  ["mock", "mock_goal"],
]) {
  const id = evidence.accounts[role];
  assert(id.endsWith(".testnet"));
  const state = rpc({
    request_type: "view_account",
    account_id: id,
    finality: "final",
  });
  const local = crypto
    .createHash("sha256")
    .update(fs.readFileSync(path.join(root, "release", name + ".wasm")))
    .digest();
  const localBase58 = near.utils.serialize.base_encode(local);
  const keys = rpc({
    request_type: "view_access_key_list",
    account_id: id,
    finality: "final",
  });
  report.contracts[role] = {
    account_id: id,
    local_wasm_sha256: local.toString("hex"),
    local_code_hash: localBase58,
    on_chain_code_hash: state.code_hash,
    matches: state.code_hash === localBase58,
    keys: keys.keys,
  };
  assert.equal(keys.keys.length, 0);
  assert.equal(state.code_hash, localBase58, role + " code hash differs");
}
const l = rpc({
  request_type: "call_function",
  account_id: evidence.accounts.treasury,
  method_name: "get_liabilities",
  args_base64: "e30=",
  finality: "final",
});
report.liabilities = JSON.parse(Buffer.from(l.result).toString());
report.account = rpc({
  request_type: "view_account",
  account_id: evidence.accounts.treasury,
  finality: "final",
});
report.block_height = l.block_height;
assert(
  BigInt(report.account.amount) >=
    BigInt(report.liabilities.buyback) +
      BigInt(report.liabilities.locked) +
      BigInt(report.liabilities.reward_funding) +
      BigInt(report.liabilities.storage_required) +
      BigInt(report.liabilities.operating_buffer),
);
report.pass = true;
console.log(JSON.stringify(report, null, 2));
