const fs = require('node:fs');
const path = require('node:path');
const assert = require('node:assert/strict');

// The checked-in file contains projections of archival RPC results, indexed by hash.
module.exports = function loadEvidence(directory) {
  const read = name => JSON.parse(fs.readFileSync(path.join(directory, name + '.json'), 'utf8'));
  const bundled = fs.existsSync(path.join(directory, 'historical-receipts.json'))
    ? read('historical-receipts') : null;
  return {
    read,
    receipt(row) {
      const result = bundled
        ? bundled[row.hash]
        : read('final-' + row.label + '-' + row.hash).response.result;
      assert(result, 'Missing archival receipt for ' + row.label + ' ' + row.hash);
      assert.equal(result.transaction.hash, row.hash);
      assert.equal(result.transaction.signer_id, row.signer);
      assert.equal(result.final_execution_status, 'FINAL');
      assert.deepEqual(result.status, row.status);
      assert.deepEqual(
        result.receipts_outcome.map(receipt => receipt.outcome.status).filter(status => status.Failure),
        row.receipt_failures,
      );
      return result;
    },
  };
};
