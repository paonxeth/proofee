# ProoFee — goal contract specification (MVP)

Version 1.0 · 8 November 2026 · the source of truth for `contracts/goal`.
Any change in contract behavior starts with an edit to this file.

## 1. What it is

One contract instance = one goal. The contract address is set as the recipient of the token's
creator fees (the `fee_to` field in Nearly). The contract accepts NEAR, keeps the accounting and pays
the creator: part immediately, the rest as milestones are accepted. The share of milestones that are
rejected or failed stays in the contract forever.

The MVP runs on testnet only. In the demo, fees are simulated with plain NEAR transfers to the
contract. For the contract this is the same as a payout from Nearly: Nearly sends the NEAR part of the
fees as plain transfers (verified on mainnet with launch #3680).

## 2. Out of scope for the MVP

- Fees in tokens (NEP-141): the contract accounts for neither the memecoin itself nor wNEAR.
- Challenge bond.
- Resubmitting a proof after rejection.
- A 3-of-5 committee: in the MVP a single verifier decides. A committee can be added later without
  changing the contract: a multisig or DAO account is set as the verifier.
- Milestone ordering: milestones are independent of each other.
- Code upgrades: the full access key is deleted after deploy.

## 3. Parameters (set in `new`, never changed afterwards)

| Field | Type | Check in `new` | Demo |
| --- | --- | --- | --- |
| `creator` | AccountId | differs from `verifier` and from the contract address | creator account |
| `verifier` | AccountId | differs from the contract address | team account |
| `goal_title` | String | 1–100 characters | |
| `goal_uri` | String | up to 256 characters | link to the goal description |
| `immediate_bps` | u16 | ≤ 10000 | 5000 (50%) |
| `challenge_window_ms` | u64 | from 60000 (1 min) to 2592000000 (30 days) inclusive | 120000 (2 minutes); default 86400000 (24 h) |
| `milestones` | Vec<MilestoneInput> | 3–5 milestones, each with `weight_bps` > 0, weights sum to 10000, `deadline_ms` > current time and ≤ current time + 31536000000 (365 days), `title` 1–100 characters | |

`MilestoneInput { title: String, weight_bps: u16, deadline_ms: u64 }`

Deposit: at the end of `new` the account balance must be at least
`(storage_usage + 2 × STORAGE_RESERVE_BYTES) × storage_byte_cost`, where
`STORAGE_RESERVE_BYTES = 16000` (≈ 0.32 NEAR on top of the code and the account), otherwise
`"Deposit too small for storage"`. `claim` never pays out one reserve (5.3); the other covers
state growth (proofs, challenges). This way storage is never paid from the creator's income, and
`challenge` and `submit_proof` can always write their data.

## 4. State

```rust
pub struct Goal {
    creator: AccountId,
    verifier: AccountId,
    token: Option<AccountId>,      // informational only, set once
    goal_title: String,
    goal_uri: String,
    immediate_bps: u16,
    challenge_window_ms: u64,
    milestones: Vec<Milestone>,
    total_received: NearToken,     // all income ever
    claimed: NearToken,            // already paid to the creator
    last_balance: NearToken,       // account balance after the last sync or payout
    payout_in_flight: Option<NearToken>, // payout whose callback has not arrived yet
}

pub struct Milestone {
    title: String,
    weight_bps: u16,
    deadline_ms: u64,
    status: MilestoneStatus,
    proof_uri: Option<String>,
    proof_hash: Option<String>,    // sha256, 64 lowercase hex characters
    submitted_at_ms: Option<u64>,
    challenger: Option<AccountId>,
    challenge_reason: Option<String>,
    challenged_at_ms: Option<u64>, // when challenged; the dispute timeout counts from it
}

pub enum MilestoneStatus { Pending, Submitted, Challenged, Accepted, Rejected, Failed }
```

## 5. Accounting

### 5.1 Sync

- At the end of `new`: `last_balance = env::account_balance()`. The initial deposit is not income.
- `sync()`:
  - if `payout_in_flight` is set, do nothing;
  - `b = env::account_balance()`;
  - if `b > last_balance`: `income = b - last_balance`, `total_received += income`,
    `last_balance = b`, event `fees_synced`;
  - otherwise do nothing: `sync` never lowers `last_balance`.
- The balance decreases only through our payouts, and `claim` lowers `last_balance` itself.
  Therefore anything received above `last_balance` counts as income.
- If the transfer to the creator fails, the NEAR come back in a separate refund receipt, before
  or after the callback. Both rules above are needed so that the refund never counts as income in
  any order: while a payout is in flight, `sync` is skipped; after a callback with rollback the
  balance is temporarily below `last_balance`, and `sync` does not lower it.

### 5.2 Formulas

All divisions are integer divisions rounding down. Multiplications use `checked_mul`.

```
R      = total_received
I      = R * immediate_bps / 10000          // immediate
L      = R - I                              // locked for milestones
W_acc  = sum of weight_bps of Accepted milestones
W_lost = sum of weight_bps of Rejected and Failed milestones
U      = L * W_acc / 10000                  // unlocked
F      = L * W_lost / 10000                 // forfeited
P      = L - U - F                          // still pending
claimable = I + U - claimed
```

The formulas are computed from all income `R`, so they also work for future fees: after a
milestone fails, its share of future income is forfeited too, in proportion to its weight.

Example. 100 NEAR received, `immediate_bps = 5000`, milestones 40/30/30.
50 immediate, 50 locked. Milestone 1 accepted: 20 unlocked. Milestone 2 rejected: 15 forfeited.
Milestone 3 pending: 15. The creator already claimed 30 → claimable 50 + 20 − 30 = 40 NEAR.

### 5.3 Payout

`claim()`, `creator` only:

0. If `payout_in_flight` is set → error `"Payout in progress"`.
1. Call `sync()`.
2. `amount = claimable`.
3. Cap it: `amount ≤ account_balance − (storage_usage + STORAGE_RESERVE_BYTES) ×
   storage_byte_cost`, so the money that pays for storage is untouched. The reserve is needed
   because `storage_usage` does not include the 16 bytes of `payout_in_flight` that `claim`
   writes itself, nor future state growth (a full goal state is ≈ 15 KB).
4. If `amount == 0` → error `"Nothing to claim"`.
5. `claimed += amount`, `last_balance -= amount`, `payout_in_flight = Some(amount)`.
6. `Promise::new(creator).transfer(amount)`, then the callback `on_claim_transfer(amount)`
   with 10 TGas.
7. Callback (`#[private]`): in both outcomes `payout_in_flight = None`. If the transfer
   failed: `claimed -= amount`, `last_balance += amount`, event `claim_failed`; otherwise
   event `claimed`. The callback has no checks that can fail: if it did not run,
   the flag would stay set and `claim` and `sync` would stop forever.

## 6. Milestones and statuses

| Transition | Method | Who | Condition |
| --- | --- | --- | --- |
| Pending → Submitted | `submit_proof` | `creator` only | `now < deadline_ms` |
| Submitted → Challenged | `challenge` | anyone except `creator` | `now < submitted_at_ms + challenge_window_ms` |
| Submitted → Accepted | `finalize` | anyone | `now ≥ submitted_at_ms + challenge_window_ms` |
| Challenged → Accepted / Rejected | `resolve` | `verifier` only | `now < challenged_at_ms + RESOLVE_TIMEOUT_MS` |
| Challenged → Accepted | `resolve_timeout` | anyone | `now ≥ challenged_at_ms + RESOLVE_TIMEOUT_MS` |
| Pending → Failed | `expire` | anyone | `now ≥ deadline_ms` |

- Accepted, Rejected and Failed are final statuses.
- A proof submitted before the deadline goes through even if the challenge window ends after the deadline.
- A proof can be challenged only once: after `challenge` the status is already Challenged.
- `RESOLVE_TIMEOUT_MS = 86400000` (24 h), a constant. If the verifier has not resolved the dispute
  within 24 hours, the milestone is accepted for the creator: a challenge is free, and "reject" as
  the default outcome would let anyone forfeit a milestone's share. After the timeout `resolve` no
  longer works, even if nobody has called `resolve_timeout` yet.
- Time is `env::block_timestamp_ms()`.

## 7. Methods

```rust
#[init]
pub fn new(creator: AccountId, verifier: AccountId, goal_title: String, goal_uri: String,
           immediate_bps: u16, challenge_window_ms: u64,
           milestones: Vec<MilestoneInput>) -> Self

pub fn set_token(&mut self, token: AccountId)                  // creator, once
#[payable] pub fn deposit(&mut self)                            // anyone, then sync
pub fn sync(&mut self)                                          // anyone
pub fn submit_proof(&mut self, index: u8, proof_uri: String, proof_hash: String) // creator
pub fn challenge(&mut self, index: u8, reason: String)          // anyone except creator
pub fn finalize(&mut self, index: u8)                           // anyone
pub fn resolve(&mut self, index: u8, accept: bool)              // verifier
pub fn resolve_timeout(&mut self, index: u8)                    // anyone
pub fn expire(&mut self, index: u8)                             // anyone
pub fn claim(&mut self) -> Promise                              // creator
#[private] pub fn on_claim_transfer(&mut self, amount: NearToken)

// view
pub fn get_goal(&self) -> GoalView          // parameters, milestones, statuses
pub fn get_balances(&self) -> BalancesView  // total_received, immediate, locked, unlocked,
                                            // forfeited, pending, claimed, claimable,
                                            // unsynced = account_balance − last_balance
```

- All amounts in JSON are yoctoNEAR strings (this is how `NearToken` serializes).
- Methods without `#[payable]` reject an attached deposit; this is the near-sdk default.
- There is not and will never be a method that changes the creator, verifier, weights, `immediate_bps` or the window.

String limits: `goal_title` and milestone `title` ≤ 100, `goal_uri` and `proof_uri` ≤ 256,
`reason` ≤ 280, `proof_hash` exactly 64 hex characters.

Error messages are short and clear: `"Only creator"`, `"Only verifier"`,
`"Wrong status"`, `"Deadline passed"`, `"Window closed"`, `"Window still open"`,
`"Nothing to claim"`, `"Weights must sum to 10000"`, `"No such milestone"`,
`"Invalid proof hash"`, `"Creator cannot challenge"`, `"Reason too long"`,
`"Deadline not passed"`, `"Payout in progress"`, `"Resolve time passed"`,
`"Resolve time not passed"`.

`new` validation and internal errors: `"Creator and verifier must differ"`,
`"Creator cannot be the contract"`, `"Verifier cannot be the contract"`,
`"Goal title must be 1-100 chars"`, `"Milestone title must be 1-100 chars"`, `"URI too long"`,
`"immediate_bps must be <= 10000"`, `"Window too short"`, `"Window too long"`,
`"Need 3-5 milestones"`, `"Weight must be > 0"`, `"Deadline must be in future"`,
`"Deadline too far"`, `"Deposit too small for storage"`, `"Token already set"`, `"Math overflow"`, `"No submission time"`.

## 8. Events (NEP-297)

Format: `EVENT_JSON:{"standard":"proofee","version":"1.0.0","event":"<name>","data":[{...}]}`

| Event | Data |
| --- | --- |
| `goal_created` | creator, verifier, milestones (count) |
| `fees_synced` | income, total_received |
| `proof_submitted` | index, proof_uri, proof_hash |
| `proof_challenged` | index, challenger, reason |
| `milestone_resolved` | index, status, by |
| `claimed` | amount, total_claimed |
| `claim_failed` | amount |

## 9. Security

- Change the state first, then send the money; if the transfer fails, the callback
  rolls the counters back.
- Rounding down: the contract can never pay out more than it received.
- Every method checks who called (`env::predecessor_account_id()`), the milestone status
  and the time.
- Money that pays for storage is never paid out (5.3).
- After deploy, delete the full access key and check that no keys are left.

## 10. Tests

Unit tests (`near-sdk` with `VMContextBuilder`):

1. `new` rejects weights that do not sum to 10000.
2. `new` rejects fewer than 3 or more than 5 milestones, and `verifier == creator`.
3. `sync` counts an incoming transfer as income, but not the initial deposit.
4. The split into immediate and locked matches the formula (the example from 5.2).
5. `submit_proof`: creator only, only from Pending, only before the deadline, the hash is validated.
6. `challenge`: only while the window is open, not from the creator.
7. `finalize` after the window → Accepted, the right amount is unlocked.
8. `resolve`: verifier only; a rejection forfeits the milestone's share, including from future fees.
9. `expire` after the deadline → Failed.
9a. `resolve_timeout`: within 24 h after `challenge` → error, after → Accepted by anyone;
    `resolve` after 24 h → `"Resolve time passed"`.
10. `claim`: pays exactly the claimable amount, a repeated claim → `"Nothing to claim"`, another
    account cannot claim, the callback rolls the counters back on failure.

Integration test (`near-sandbox` + `near-api`, sandbox):

11. Deploy → transfer 10 NEAR → `sync` → proof → move time past the window (`fast_forward`) →
    `finalize` → `claim` → check the creator and contract balances.

## 11. Testnet demo scenario

Check near-cli-rs syntax against `near --help`.

1. Deploy a goal: window 120000 ms, three milestones 40/30/30, deadlines a few days out.
2. Simulate fees: transfer 1 NEAR to the contract
   (`near tokens <sender>.testnet send-near <goal>.testnet '1 NEAR' network-config testnet sign-with-keychain send`).
3. `sync` → `get_balances`: claimable 0.5 NEAR.
4. `submit_proof` for milestone 0 → wait 2 minutes → `finalize` → claimable 0.7 NEAR.
5. `submit_proof` for milestone 1 → `challenge` from a third account → `resolve(1, false)` →
   forfeited 0.15 NEAR.
6. `claim` → the NEAR arrive at the creator.
7. Transfer another 1 NEAR → `sync` → the rejected milestone's share is forfeited from the
   new fees too.

## 12. Nearly integration (for mainnet, after the hackathon)

- `fee_to` = the goal contract address.
- The NEAR part of the fees arrives as plain transfers from `nearlytrade.near`; `sync` sees them.
- Nearly keeps part of the fees in the token itself (`creator_token_fees`); the payout via
  `push_creator_token_fees` has not been verified. The MVP contract does not account for tokens;
  before mainnet it needs `sync_ft` and token payouts.
- Not verified whether the creator can change `fee_to` after launch
  (`set_launch_recipients`, `set_launch_hook`). If they can, they bypass the contract.
  Verify before mainnet.
- Buy/Sell tax in demo launches is 0.
