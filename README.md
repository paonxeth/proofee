# ProoFee

ProoFee is an escrow contract for memecoin fees on NEAR. Token fees go not to the creator directly
but to a goal contract. The creator can claim part of them immediately; the rest unlocks milestone by
milestone: the creator publishes a proof, the community can challenge it, and the verifier resolves the dispute.

NEAR hackathon, track 11. Testnet only.

## How the contract works

Full description: [docs/SPEC.md](docs/SPEC.md); decisions: [docs/DECISIONS.md](docs/DECISIONS.md);
audit: [docs/AUDIT.md](docs/AUDIT.md).

In short:
- `new` fixes the creator, the verifier, the immediate share (`immediate_bps`), the challenge window and
  3–5 milestones with weights and deadlines. There are no admin methods and no parameter changes.
- Any NEAR received above the accounted balance is income. `sync` counts it.
- A milestone: `submit_proof` (creator) → challenge window → `finalize` (anyone) or
  `challenge` (anyone except the creator) → `resolve` (verifier). If the verifier has not resolved
  the dispute within 24 h, anyone calls `resolve_timeout`, and the milestone is accepted for the creator. A
  milestone not submitted before its deadline is closed with `expire`.
- An accepted milestone's share becomes claimable by the creator; a rejected or expired milestone's
  share is forfeited, including from future fees. `claim` pays out the claimable amount. A failed transfer
  is rolled back in the callback.

## Demo goal

[`goal-3.proofee-anton.testnet`](https://testnet.nearblocks.io/address/goal-3.proofee-anton.testnet)
— 2-minute challenge window, three milestones 40/30/30, the account has no keys.

```sh
near contract call-function as-read-only goal-3.proofee-anton.testnet get_balances \
  json-args '{}' network-config testnet now
```

## Tests

```sh
cd contracts/goal && cargo test
```

Unit tests run in the near-sdk mock. Integration tests (`contracts/goal/tests/integration.rs`)
start a local sandbox via near-sandbox; the first run downloads its binary.

## Deploying to testnet

You need Rust (the version is pinned in `contracts/goal/rust-toolchain.toml`), `cargo-near` and
`near-cli-rs`. The `MASTER` key must be in the keychain, and the verifier account
`verifier.$MASTER` must exist.

```sh
scripts/deploy-testnet.sh                        # goal-<first free N>.proofee-anton.testnet
N=5 MASTER=me.testnet scripts/deploy-testnet.sh
```

The script builds the wasm and creates the goal in one transaction from `MASTER`: account, 3 NEAR, code and
`new`. The goal has no keys; NEAR can leave it only through `claim`.

## Known limitations

Details: the "Fix status" section in [docs/AUDIT.md](docs/AUDIT.md).

- **Do not send tokens (NEP-141) to a goal, including the memecoin itself and wNEAR.** The contract accounts
  for NEAR only. `ft_transfer` to a goal is an irreversible loss; `ft_transfer_call` fails and the
  tokens are returned.
- The verifier can challenge a milestone themselves and reject it. There is no protection; this needs a multisig or DAO
  and a challenge bond (M-3).
- If the creator deletes their top-level account, someone else can take the name and
  claim the claimable amount (L-5).
- In the demo the creator and the verifier belong to the same person: the verifier is a
  subaccount of `MASTER`. Real goals must not do this (L-5).
- Methods do not require 1 yoctoNEAR, so they can be called with a function-call key without
  wallet confirmation. A leaked function-call key of the verifier would allow rejecting milestones (L-6).
- While a payout is in flight, `unsynced` in `get_balances` may show an arrived refund as
  income (I-2). A challenge sent in the last seconds of the window may not make it in time (I-7).

## License

MIT, see [LICENSE](LICENSE).
