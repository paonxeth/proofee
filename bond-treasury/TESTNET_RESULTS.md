# Public NEAR Testnet results — 12 November 2026

Status: COMPLETE: refund, slash and separately funded reward confirmed. No Mainnet operations.

## Dedicated demo accounts

- admin: [proofee-bond-lab-20261112-1794481933490.testnet](https://testnet.nearblocks.io/address/proofee-bond-lab-20261112-1794481933490.testnet)
- treasury: [treasury.proofee-bond-lab-20261112-1794481933490.testnet](https://testnet.nearblocks.io/address/treasury.proofee-bond-lab-20261112-1794481933490.testnet)
- mock: [mock.proofee-bond-lab-20261112-1794481933490.testnet](https://testnet.nearblocks.io/address/mock.proofee-bond-lab-20261112-1794481933490.testnet)
- payer: [payer.proofee-bond-lab-20261112-1794481933490.testnet](https://testnet.nearblocks.io/address/payer.proofee-bond-lab-20261112-1794481933490.testnet)
- verifier: [verifier.proofee-bond-lab-20261112-1794481933490.testnet](https://testnet.nearblocks.io/address/verifier.proofee-bond-lab-20261112-1794481933490.testnet)

Treasury and Mock are atomic no-key deployments. Parent/payer/verifier TEST-only keys were generated locally and remain protected in ignored work directory; never included in this report/archive. New user-owned deployment should use locally controlled signer/OS keychain. Parent and support accounts were each funded10 test NEAR by official Testnet helper; support transferred9.8 to parent for code/storage sponsorship. This is centralized Mock demonstration, not actual Goal fees. Existing Goal remains untouched.

## Confirmed signed transactions

| Operation | Case | Transaction / explorer |
|---|---|---|
| test faucet support transfer | — | [3L73kyhmeXCV1cgGCNPNS4UxSCMoNtMXDvjEvooR6LFe](https://testnet.nearblocks.io/txns/3L73kyhmeXCV1cgGCNPNS4UxSCMoNtMXDvjEvooR6LFe) |
| create treasury.proofee-bond-lab-20261112-1794481933490.testnet | — | [2ekiVg8Wj8WmrPtwo5NLrotQYV2ubdT3xpXv13VHnVU2](https://testnet.nearblocks.io/txns/2ekiVg8Wj8WmrPtwo5NLrotQYV2ubdT3xpXv13VHnVU2) |
| create mock.proofee-bond-lab-20261112-1794481933490.testnet | — | [DDNnSP3KF6yXEabK4RZXmEtfbMe98t9wHspaa2Rxwd3Z](https://testnet.nearblocks.io/txns/DDNnSP3KF6yXEabK4RZXmEtfbMe98t9wHspaa2Rxwd3Z) |
| create payer.proofee-bond-lab-20261112-1794481933490.testnet | — | [DfMgKiki7oUhctZWHkip38yadcacT1BAamzXbBHGk3b8](https://testnet.nearblocks.io/txns/DfMgKiki7oUhctZWHkip38yadcacT1BAamzXbBHGk3b8) |
| create verifier.proofee-bond-lab-20261112-1794481933490.testnet | — | [3KdbxFv9xGcot7ebaRgfPipG8nqNGzK8joDpvqMNYUmf](https://testnet.nearblocks.io/txns/3KdbxFv9xGcot7ebaRgfPipG8nqNGzK8joDpvqMNYUmf) |
| register_goal | — | [CiJXNWyuLfVv5qXZ76PqNunbBPqUp3nYaztABi4NMW9p](https://testnet.nearblocks.io/txns/CiJXNWyuLfVv5qXZ76PqNunbBPqUp3nYaztABi4NMW9p) |
| prepare_challenge | qualification-rejected | [Eev8RKi4UmZcXcsTWKsSNy9Zh2YycyUwZeeLL122mWis](https://testnet.nearblocks.io/txns/Eev8RKi4UmZcXcsTWKsSNy9Zh2YycyUwZeeLL122mWis) |
| deposit_bond | — | [76bUvJ3MMgezDrWGx9wtq6WXgCpCcVZ8xFvFhZegMMxa](https://testnet.nearblocks.io/txns/76bUvJ3MMgezDrWGx9wtq6WXgCpCcVZ8xFvFhZegMMxa) |
| confirm_escrow | qualification-rejected | [34PNW4LLE9sBMc1dhXhKc5Mgx5PhRggDXzgSJNJmzQct](https://testnet.nearblocks.io/txns/34PNW4LLE9sBMc1dhXhKc5Mgx5PhRggDXzgSJNJmzQct) |
| qualify | qualification-rejected | [9P7ZcCK5771duimRfks1jHCzuvUy75LyXxUVE4Ka2VXx](https://testnet.nearblocks.io/txns/9P7ZcCK5771duimRfks1jHCzuvUy75LyXxUVE4Ka2VXx) |
| prepare_challenge | winning-challenge | [FYyX1DwHnMPFmGsrsfaj3Tabi4uhYvgqi1wGZQSAmSuC](https://testnet.nearblocks.io/txns/FYyX1DwHnMPFmGsrsfaj3Tabi4uhYvgqi1wGZQSAmSuC) |
| deposit_bond | — | [9j8TjVY5ss1V4ACzehDLCfmaiarnbPNkM5SqZfUSdUCq](https://testnet.nearblocks.io/txns/9j8TjVY5ss1V4ACzehDLCfmaiarnbPNkM5SqZfUSdUCq) |
| confirm_escrow | winning-challenge | [ECKqrnuunWLP4y91bxgE2SdVNpkwvGS4PYgzsaVTCaTi](https://testnet.nearblocks.io/txns/ECKqrnuunWLP4y91bxgE2SdVNpkwvGS4PYgzsaVTCaTi) |
| qualify | winning-challenge | [Ghd5TF8UGx3x73pwQsCUwHmrXyg6FN1EcgrGcmj6HVL9](https://testnet.nearblocks.io/txns/Ghd5TF8UGx3x73pwQsCUwHmrXyg6FN1EcgrGcmj6HVL9) |
| verifier gas topup | — | [CuBfvcnu3i3w86nPbcQyHmAHjqHkh4MfCtYZX5Ewtd6M](https://testnet.nearblocks.io/txns/CuBfvcnu3i3w86nPbcQyHmAHjqHkh4MfCtYZX5Ewtd6M) |
| arbitrate | winning-challenge | [AvoGWqYnQbEso4a8JYpW5bXKaf6YxbNzCJS56kXum9mA](https://testnet.nearblocks.io/txns/AvoGWqYnQbEso4a8JYpW5bXKaf6YxbNzCJS56kXum9mA) |
| refresh_bond | winning-challenge | [Gtf7svWD77NUs2fwxeFfgNfZe6xNTtiPkf4Ewespkho4](https://testnet.nearblocks.io/txns/Gtf7svWD77NUs2fwxeFfgNfZe6xNTtiPkf4Ewespkho4) |
| fund_case | winning-challenge | [7fDMgjrfLN5d7fFxG5nhTewzWxCwYuqPudWKSTS8tPJ5](https://testnet.nearblocks.io/txns/7fDMgjrfLN5d7fFxG5nhTewzWxCwYuqPudWKSTS8tPJ5) |
| claim_reward | winning-challenge | [9LPaW74sDE3xY4Jz31uzc8mS3yc3sqebvXaMhbngxTMo](https://testnet.nearblocks.io/txns/9LPaW74sDE3xY4Jz31uzc8mS3yc3sqebvXaMhbngxTMo) |
| prepare_challenge | losing-challenge | [HZJQomiTeL6qRvn83aSg9HueaC23aRz5QJRjZmdn3SV1](https://testnet.nearblocks.io/txns/HZJQomiTeL6qRvn83aSg9HueaC23aRz5QJRjZmdn3SV1) |
| deposit_bond | — | [BkYCqprGVUW2ud7VF9wQ1CcEqTsb3nLVQ9RsPAeG32nx](https://testnet.nearblocks.io/txns/BkYCqprGVUW2ud7VF9wQ1CcEqTsb3nLVQ9RsPAeG32nx) |
| confirm_escrow | losing-challenge | [XycLzVVLXTRWNGW8D2MtYayCi7YDDqgmxfttjaHk5Wr](https://testnet.nearblocks.io/txns/XycLzVVLXTRWNGW8D2MtYayCi7YDDqgmxfttjaHk5Wr) |
| qualify | losing-challenge | [CHnjfakDveKcQoqZfp86C1KFkN4rknqnTX86stMqkYhZ](https://testnet.nearblocks.io/txns/CHnjfakDveKcQoqZfp86C1KFkN4rknqnTX86stMqkYhZ) |
| arbitrate | losing-challenge | [J1RKsEoRDyJoAkM5wwPYQRqZMWXwxH5hseYaeErNSmCU](https://testnet.nearblocks.io/txns/J1RKsEoRDyJoAkM5wwPYQRqZMWXwxH5hseYaeErNSmCU) |

Full receipt results and final ledger views: evidence/testnet-transactions.json. Independent code hash / key / balance readback: evidence/testnet-readback.json when completed. Local sandbox tx hashes must not be confused with these public hashes.

## Final recorded views

```json
{
  "rejected": {
    "escrow_id": "ba58d3b0f4469913997135bb4e7737b49048e8386543b2701948cc7c4b0d437b",
    "goal": "mock.proofee-bond-lab-20261112-1794481933490.testnet",
    "proof_id": "proof:qualification-rejected",
    "case_id": "qualification-rejected",
    "submission_id": "0",
    "payer": "payer.proofee-bond-lab-20261112-1794481933490.testnet",
    "amount": "2000000000000000000000000",
    "deposit_until_ns": 1794486194485221000,
    "bind_until_ns": 1794489794485221000,
    "refund_after_ns": 1794493394485221000,
    "created_at_ns": 1794482632651674600,
    "status": "Refunded",
    "settlement_id": "bond:qualification-rejected",
    "outcome": "REFUND",
    "attempt": 1,
    "reward_cap": "1000000000000000000000000",
    "decision": "QualificationRejected",
    "demo": false
  },
  "reward": {
    "funding_id": "0af80a807505e069c891f5f967a16e9edcd4ef89929942775566b448aecf4c7c",
    "escrow_id": "0395814941e64a0f9715b0146efe886b57a992b2d214e84c27607980df1351c0",
    "goal": "mock.proofee-bond-lab-20261112-1794481933490.testnet",
    "beneficiary": "payer.proofee-bond-lab-20261112-1794481933490.testnet",
    "amount": "1000000000000000000000000",
    "status": "Paid",
    "settlement_id": "reward:winning-challenge",
    "attempt": 1
  },
  "slashed": {
    "escrow_id": "307bd156da1a2f90c5acd4dc9dad73f7b67d45845b1049569818ad287e396931",
    "goal": "mock.proofee-bond-lab-20261112-1794481933490.testnet",
    "proof_id": "proof:losing-challenge",
    "case_id": "losing-challenge",
    "submission_id": "0",
    "payer": "payer.proofee-bond-lab-20261112-1794481933490.testnet",
    "amount": "2000000000000000000000000",
    "deposit_until_ns": 1794487167189174000,
    "bind_until_ns": 1794490767189174000,
    "refund_after_ns": 1794494367189174000,
    "created_at_ns": 1794483603606652700,
    "status": "Slashed",
    "settlement_id": "bond:losing-challenge",
    "outcome": "SLASH",
    "attempt": 0,
    "reward_cap": "1000000000000000000000000",
    "decision": "ProofValid",
    "demo": false
  },
  "liabilities": {
    "locked": "0",
    "in_flight": "0",
    "buyback": "2000000000000000000000000",
    "reward_funding": "0",
    "reward_in_flight": "0",
    "storage_required": "4070450000000000000000000",
    "operating_buffer": "100000000000000000000000",
    "account_balance": "8000000000000000000000000"
  },
  "physical_balance": {
    "amount": "8000000000000000000000000",
    "block_hash": "8pvvaUSQSHZzvhaCRVXg4AJfU8M4rKZMKxTbBBfQ3n3C",
    "block_height": 277358001,
    "code_hash": "7KrvcYZuh2cA9yoq7LHUZFMTCzpkmG3Yq4wFCSbj9FhU",
    "locked": "0",
    "storage_paid_at": 0,
    "storage_usage": 407045
  },
  "treasury_keys": {
    "block_hash": "CjBS671373uhFJNEcmFxBcRzSrWpDC3z6hAWHSruFzzM",
    "block_height": 277358007,
    "keys": []
  },
  "mock_keys": {
    "block_hash": "9rwxaSWbN4n7p5TEUxPZ2FzFxePPFGAthQoArrrmrywh",
    "block_height": 277358013,
    "keys": []
  }
}
```

## Scope and limitations

Public demo covers deposit, bind, rejection refund, winning refund, actual Goal reward funding/payment, losing slash and quiescent balance coverage. Runtime failure/retry, timeout and replay tests are demonstrated in local sandbox, not claimed as public Testnet transactions unless separately recorded. RPC InvalidNonce and429 interrupted tooling; authoritative views and known confirmed receipts were checked before continuing. Initial verifier0.2NEAR could not reserve300Tgas; topped up1NEAR; fresh deployment script now provisions1NEAR upfront. No previously confirmed payment was resent.

Mainnet not supported or claimed ready. Real Goal adapter, production timeout policy and real fee/forfeiture accounting remain another implementation/review gate. No independent security audit.
