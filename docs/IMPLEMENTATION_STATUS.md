# Implementation status

Updated 13 November 2026. Testnet runs are on
[`goal-4.proofee-anton.testnet`](https://testnet.nearblocks.io/address/goal-4.proofee-anton.testnet);
step numbers refer to [DEMO_RUN.md](DEMO_RUN.md). Unit tests are in `contracts/goal/src/lib.rs`,
integration tests in `contracts/goal/tests/integration.rs`; `cargo test`: 83 unit + 3 integration, green.

Statuses:
- **Testnet verified** — works on testnet, with a transaction;
- **Implemented** — in the contract and covered by tests, not run on testnet;
- **Simulated** — the demo imitates it;
- **Planned** — not built.

| Feature | Status | Evidence |
| --- | --- | --- |
| Fee accounting (`sync`) and `claim` | Testnet verified | `sync` counts 2 NEAR, a second `sync` adds nothing: [3a](https://testnet.nearblocks.io/txns/7c99XdGugTPBt2tJNzDB6UR8XLsV8Skyy7yf8pZSubcD), [3b](https://testnet.nearblocks.io/txns/GHtXg9HhKDaJ7tbk9VbapLLuNpFk8SNq8yinbBpPsWoZ); `claim` pays 0.7 NEAR: [7a](https://testnet.nearblocks.io/txns/23MxUoikemK6hvpmUsfvGvWcDV1cZwZdgEBtbA5E5TrQ); integration test `full_flow_claim` |
| 20% immediate / 80% locked, milestone weights 15/25/40% of all fees | Testnet verified | 2 NEAR → 0.4 immediate, 1.6 locked: [3a](https://testnet.nearblocks.io/txns/7c99XdGugTPBt2tJNzDB6UR8XLsV8Skyy7yf8pZSubcD); milestone 1 unlocks 0.3: [4b](https://testnet.nearblocks.io/txns/7DHjxk7HQjWDWwywHqeqUa3kM42cqKxPd5ubh9pyNfpb); milestone 2 forfeits 0.5: [6b](https://testnet.nearblocks.io/txns/EpskuvXE57uWbbDwzKDevMZ8rRu9TMT7LSMhSdj3HCEY); 0.8 pending. Weights 1875/3125/5000 bps of the locked 80% |
| 3–5 milestones per goal | Implemented | `new_rejects_two_milestones`, `new_accepts_three_and_five_milestones`, `new_rejects_six_milestones`; commit [2b831ea](https://github.com/paonxeth/proofee/commit/2b831ea); goal-4 was created with 3 milestones |
| Goal spec hash on-chain (`goal_spec_hash`) | Testnet verified | `get_goal` of goal-4 returns `b5b13f8f…9b8a`, the sha256 of the [spec at goal_uri](https://raw.githubusercontent.com/paonxeth/proofee/61aacd2355c22ed5f83f3156b5cb63ed7b529c16/specs/demo-goal.json); `new_checks_goal_spec_hash`; commit [24cfa04](https://github.com/paonxeth/proofee/commit/24cfa04) |
| `submit_proof` (link + sha256) | Testnet verified | [4a](https://testnet.nearblocks.io/txns/F4HUTQdqk7nDqS1LRubSXvdLMhLsnDkMWQLeBq69Hxz9), [5a](https://testnet.nearblocks.io/txns/7SStPLG3pmK6VRLXnj8BaFdrrv83vYSURgMVuRyrsnjk) |
| `finalize` without a challenge, after the window | Testnet verified | [4b](https://testnet.nearblocks.io/txns/7DHjxk7HQjWDWwywHqeqUa3kM42cqKxPd5ubh9pyNfpb), 130 s after the proof |
| `challenge` and `resolve` by the verifier | Testnet verified | challenge by a third party: [5c](https://testnet.nearblocks.io/txns/65Jnu3ro4RHT3Nd8ixrSJqaAzukxYdMLhdjAqEZSYDQY); `resolve` by a non-verifier fails with `Only verifier`: [6a](https://testnet.nearblocks.io/txns/4fj3fLSpvJc5xmQfwjxhFEztKqfWD9uzWZbdujdxNKbo); verifier rejects: [6b](https://testnet.nearblocks.io/txns/EpskuvXE57uWbbDwzKDevMZ8rRu9TMT7LSMhSdj3HCEY) |
| The verifier cannot challenge | Testnet verified | fails with `Verifier cannot challenge`: [5b](https://testnet.nearblocks.io/txns/CsTZrERABmKZkymft7MMFUy2mo3Mab9n1hdumuRuMmRU); `challenge_rejects_verifier`; commit [0481518](https://github.com/paonxeth/proofee/commit/0481518) (AUDIT M-3, partial) |
| 24 h dispute timeout (`resolve_timeout` → Accepted) | Implemented | unit tests `resolve_works_until_timeout`, `resolve_rejects_after_timeout`, `resolve_timeout_rejects_before_timeout`, `resolve_timeout_accepts_for_creator`, `resolve_timeout_requires_challenge`; commit [3979e55](https://github.com/paonxeth/proofee/commit/3979e55). Not run on testnet: 24 h does not fit in an evening |
| Double `claim` protection | Testnet verified | second `claim` fails with `Nothing to claim`: [7b](https://testnet.nearblocks.io/txns/2zEsBaDEYGssf96Fv2QiXZyx3B7QFYkYKsSPpyfVtLkU); `claim_twice_fails`, `claim_during_payout_fails` (`Payout in progress` while a payout is in flight) |
| Challenge window | Implemented | a `new` parameter, 60 s – 30 days (`new_rejects_window_below_minute`, `new_rejects_window_above_30_days`, `new_accepts_upper_limits`). Demo: 2 minutes. Product: to be decided by the team |
| Creator fees → goal (Nearly) | Testnet verified | via [launcher](../launcher/REPORT.md) + LP Locker: a Rhea DCL pool for a new token, buy/sell [6GWzhE7X](https://testnet.nearblocks.io/txns/6GWzhE7XUmyRfCYNPduvzBaw1iCWGMDaYi8pWMTRtHeT), [4ZHXdbFb](https://testnet.nearblocks.io/txns/4ZHXdbFbLzzLa7XRXRAypgYiFvYqGG8Q69hUR5acDUhg); the Locker collects LP fees, converts them to NEAR and sends them to `goal-e2e1.pf-launch-mwdyyhj0.testnet`: [Ba8atbQh](https://testnet.nearblocks.io/txns/Ba8atbQh3LR7Fu8e2Z21wFxWQHHqLhZjVxHxJxPGBezo); `sync` [5yw4UUGk](https://testnet.nearblocks.io/txns/5yw4UUGkiphvjdETJcgmmfFqz7ZHioqAQcBKfDpg9P4U), creator `claim` of the 20% immediate share [87itFbHT](https://testnet.nearblocks.io/txns/87itFbHTFWLaoYNQpUguTn7TZ5qoy57YnsykDiz2g7D3); final `sync` / `claim`: [DEGne4vA](https://testnet.nearblocks.io/txns/DEGne4vAaLupoBFJbnh4sv7qBUnzKRtxrSFznQ6w4xcN), [DRobdxz3](https://testnet.nearblocks.io/txns/DRobdxz39jxFSyPsMM6Uj3XdLtXB1AxnnfkqLt8UaLH3). On mainnet the same job is done by Nearly, which pays the NEAR part as plain transfers (launch #3680, SPEC 12). In the goal-4 demo fees were a direct transfer: [2](https://testnet.nearblocks.io/txns/5wfiSn7SbCKeyKmTSqDXG6Q9dc7fmFPF5jEg53goD3eq) |
| Bond Treasury | Testnet verified with Mock Goal | [bond-treasury/](../bond-treasury): refund, slash and a separately funded reward run on testnet against the Mock Goal, every step with its transaction: [TESTNET_RESULTS.md](../bond-treasury/TESTNET_RESULTS.md); 57 + 18 unit tests. Integration with the real goal contract is in progress |
| Launcher + LP Locker | Testnet verified | [launcher/](../launcher): token [41AvUu6e](https://testnet.nearblocks.io/txns/41AvUu6eWBYEhv1FiEajii3GRrjq2WqFo7LoZjfaxqGr), keyless Locker [9udHWdV2](https://testnet.nearblocks.io/txns/9udHWdV2DxjUW4eS2Ljp7Ffz7We1zSNSWvaB9URQW2BX) owning the LP position; principal cannot be withdrawn, upgrade by the parent fails: [1aTfJ1tz](https://testnet.nearblocks.io/txns/1aTfJ1tzGTEVQB3JVkAJgu92JJ7MWzUiZEL2PCKvP2w); a stranger's `claim` fails: [2mALkQhj](https://testnet.nearblocks.io/txns/2mALkQhjZXgeN1c8NH5kqaxnVjcKBk1SibSkL5yfjkPa); 14 Locker unit tests. CLI, not a hosted service; details in [REPORT.md](../launcher/REPORT.md) |
| Daily claim | Planned | — |
| Two proof attempts per milestone | Planned | — |
| Multiple challengers | Planned | — |
| AI qualification | Planned | — |
| Post-Commitment 80/20 | Planned | — |
