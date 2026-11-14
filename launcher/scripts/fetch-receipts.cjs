// Public archival RPC only. No accounts, keys, transaction creation, or broadcasting.
const fs = require('node:fs');
const rows = require('../evidence/final-transactions.json');
const endpoints = [
  'https://archival-rpc.testnet.fastnear.com',
  'https://archival-rpc.testnet.near.org',
];
const output = process.argv[2];

function project(result) {
  const { transaction, transaction_outcome, receipts_outcome, receipts, status, final_execution_status } = result;
  return {
    final_execution_status, status,
    transaction: {
      hash: transaction.hash, signer_id: transaction.signer_id,
      receiver_id: transaction.receiver_id, actions: transaction.actions,
    },
    transaction_outcome: { outcome: transaction_outcome.outcome },
    receipts_outcome: receipts_outcome.map(({ id, outcome }) => ({ id, outcome })),
    receipts: receipts.flatMap(({ receipt_id, predecessor_id, receiver_id, receipt }) => {
      const actions = receipt.Action?.actions.filter(action => action.Transfer);
      return actions?.length ? [{ receipt_id, predecessor_id, receiver_id, receipt: { Action: { actions } } }] : [];
    }),
  };
}

async function query({ hash, signer }) {
  const errors = [];
  for (const endpoint of endpoints) {
    try {
      const response = await fetch(endpoint, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        signal: AbortSignal.timeout(12000),
        body: JSON.stringify({
          jsonrpc: '2.0', id: hash, method: 'EXPERIMENTAL_tx_status', params: [hash, signer],
        }),
      });
      if (!response.ok) throw Error('HTTP ' + response.status);
      const { result, error } = await response.json();
      if (result?.final_execution_status !== 'FINAL' || result.transaction?.hash !== hash) {
        throw Error(error?.cause?.name ?? 'missing final result');
      }
      return project(result);
    } catch (error) {
      errors.push(new URL(endpoint).host + ': ' + error.message);
    }
  }
  throw Error(hash + ' (' + errors.join('; ') + ')');
}

async function main() {
  if (!output) throw new Error('Usage: node scripts/fetch-receipts.cjs <output.json>');
  const results = fs.existsSync(output) ? JSON.parse(fs.readFileSync(output, 'utf8')) : {};
  let failures = 0;
  for (const row of rows) {
    if (results[row.hash]?.transaction?.hash === row.hash
      && results[row.hash].final_execution_status === 'FINAL') continue;
    try {
      results[row.hash] = await query(row);
      fs.writeFileSync(output + '.tmp', JSON.stringify(results) + '\n');
      fs.renameSync(output + '.tmp', output);
    } catch (error) {
      console.error('FAIL', row.label, error.message);
      failures++;
    }
  }
  const complete = rows.filter(({ hash }) => results[hash]?.transaction?.hash === hash).length;
  console.log(failures ? 'FAIL' : 'PASS', complete + '/' + rows.length + ' receipts');
  if (failures) process.exitCode = 1;
}

module.exports = { project };
if (require.main === module) main().catch(error => { console.error(error.message); process.exitCode = 1; });
