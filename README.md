# ProoFee

ProoFee is an escrow contract for memecoin fees on NEAR. Token fees go not to the creator directly
but to a goal contract. The creator can claim part of them immediately; the rest unlocks milestone by
milestone: the creator publishes a proof, the community can challenge it, and the verifier resolves the dispute.

NEAR hackathon, track 11. Testnet only.

## How the contract works

Full description: [docs/SPEC.md](docs/SPEC.md); decisions: [docs/DECISIONS.md](docs/DECISIONS.md);
audit: [docs/AUDIT.md](docs/AUDIT.md); what is built and verified on testnet:
[docs/IMPLEMENTATION_STATUS.md](docs/IMPLEMENTATION_STATUS.md).

In short:
- `new` fixes the creator, the verifier, the goal spec hash (`goal_spec_hash`, sha256 of the file at
  `goal_uri`), the immediate share (`immediate_bps`), the challenge window and
  3–5 milestones with weights and deadlines. There are no admin methods and no parameter changes.
- Any NEAR received above the accounted balance is income. `sync` counts it.
- A milestone: `submit_proof` (creator) → challenge window → `finalize` (anyone) or
  `challenge` (anyone except the creator and the verifier) → `resolve` (verifier). If the verifier has not resolved
  the dispute within 24 h, anyone calls `resolve_timeout`, and the milestone is accepted for the creator. A
  milestone not submitted before its deadline is closed with `expire`.
- An accepted milestone's share becomes claimable by the creator; a rejected or expired milestone's
  share is forfeited, including from future fees. `claim` pays out the claimable amount. A failed transfer
  is rolled back in the callback.

## Demo goal

[`goal-4.proofee-anton.testnet`](https://testnet.nearblocks.io/address/goal-4.proofee-anton.testnet)
— "Ship ProoFee on NEAR testnet": 20% immediate, three milestones of 15/25/40% of all fees,
2-minute challenge window, the goal spec ([`specs/demo-goal.json`](specs/demo-goal.json)) pinned
on-chain by `goal_spec_hash`, the account has no keys.

- [docs/DEMO_RUN.md](docs/DEMO_RUN.md) — both demo scenarios run on this goal, every step with
  its transaction: milestone 1 accepted after the window, milestone 2 challenged and rejected,
  `claim` of 0.7 NEAR.
- [docs/IMPLEMENTATION_STATUS.md](docs/IMPLEMENTATION_STATUS.md) — what is verified on testnet,
  implemented, simulated or planned.

```sh
near contract call-function as-read-only goal-4.proofee-anton.testnet get_goal \
  json-args '{}' network-config testnet now
near contract call-function as-read-only goal-4.proofee-anton.testnet get_balances \
  json-args '{}' network-config testnet now
```

The previous contract version is deployed at
[`goal-3.proofee-anton.testnet`](https://testnet.nearblocks.io/address/goal-3.proofee-anton.testnet)
(50% immediate, milestones 40/30/30, no `goal_spec_hash`, the verifier could still challenge).

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
scripts/deploy-testnet.sh                        # draft goal, goal-<first free N>.proofee-anton.testnet
scripts/deploy-testnet.sh scripts/goals/demo.json  # goal parameters from a file
N=5 MASTER=me.testnet scripts/deploy-testnet.sh
DRY_RUN=1 scripts/deploy-testnet.sh scripts/goals/demo.json  # only print the new() arguments
```

The goal spec (`spec_file` in the goal file, e.g. `specs/demo-goal.json`) must be committed and
pushed. The script passes `goal_uri` as a raw GitHub link pinned to the commit and
`goal_spec_hash` as the file's sha256.

The script builds the wasm and creates the goal in one transaction from `MASTER`: account, 3 NEAR, code and
`new`. The goal has no keys; NEAR can leave it only through `claim`.

## Known limitations

Details: the "Fix status" section in [docs/AUDIT.md](docs/AUDIT.md).

- **Do not send tokens (NEP-141) to a goal, including the memecoin itself and wNEAR.** The contract accounts
  for NEAR only. `ft_transfer` to a goal is an irreversible loss; `ft_transfer_call` fails and the
  tokens are returned.
- The verifier cannot challenge from their own account, but can from a second one and then reject
  the milestone. Real protection needs a multisig or DAO verifier and a challenge bond (M-3).
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
