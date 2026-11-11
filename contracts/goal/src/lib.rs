// ProoFee goal contract. Specification: docs/SPEC.md.
use near_sdk::serde_json::json;
use near_sdk::{AccountId, Gas, NearToken, PanicOnDefault, Promise, env, near, require};

mod events;
mod math;

use math::BPS_DENOMINATOR;

const MAX_MILESTONES: usize = 5;
const MAX_TITLE_LEN: usize = 100;
const MAX_URI_LEN: usize = 256;
const MAX_REASON_LEN: usize = 280;
const PROOF_HASH_LEN: usize = 64;
const CALLBACK_GAS: Gas = Gas::from_tgas(10);
const DAY_MS: u64 = 86_400_000;
/// Challenge window: at least a minute (otherwise nobody can challenge in time) and at most 30 days.
const MIN_CHALLENGE_WINDOW_MS: u64 = 60_000;
const MAX_CHALLENGE_WINDOW_MS: u64 = 30 * DAY_MS;
/// A milestone deadline is at most a year after `new`.
const MAX_DEADLINE_AHEAD_MS: u64 = 365 * DAY_MS;
/// Storage reserve that `claim` never pays out (SPEC 3, 5.3). Covers the 16 bytes of
/// `payout_in_flight` and state growth: a full goal state is ≈ 15 KB.
const STORAGE_RESERVE_BYTES: u64 = 16_000;
/// How long the verifier has to resolve a dispute; after that the milestone is accepted for the creator (SPEC 6).
const RESOLVE_TIMEOUT_MS: u64 = DAY_MS;

/// A milestone as passed to `new`.
#[near(serializers = [json])]
pub struct MilestoneInput {
    pub title: String,
    pub weight_bps: u16,
    pub deadline_ms: u64,
}

#[near(serializers = [json, borsh])]
#[derive(Clone, Debug, PartialEq)]
pub enum MilestoneStatus {
    Pending,
    Submitted,
    Challenged,
    Accepted,
    Rejected,
    Failed,
}

#[near(serializers = [json, borsh])]
#[derive(Clone)]
pub struct Milestone {
    pub title: String,
    pub weight_bps: u16,
    pub deadline_ms: u64,
    pub status: MilestoneStatus,
    pub proof_uri: Option<String>,
    pub proof_hash: Option<String>,
    pub submitted_at_ms: Option<u64>,
    pub challenger: Option<AccountId>,
    pub challenge_reason: Option<String>,
    pub challenged_at_ms: Option<u64>,
}

/// `get_goal` response: goal parameters and milestones. Money is in `get_balances`.
#[near(serializers = [json])]
pub struct GoalView {
    pub creator: AccountId,
    pub verifier: AccountId,
    pub token: Option<AccountId>,
    pub goal_title: String,
    pub goal_uri: String,
    pub immediate_bps: u16,
    pub challenge_window_ms: u64,
    pub milestones: Vec<Milestone>,
}

/// `get_balances` response. Formulas: SPEC 5.2; amounts in yoctoNEAR (strings in JSON).
#[near(serializers = [json])]
pub struct BalancesView {
    pub total_received: NearToken,
    pub immediate: NearToken,
    pub locked: NearToken,
    pub unlocked: NearToken,
    pub forfeited: NearToken,
    pub pending: NearToken,
    pub claimed: NearToken,
    pub claimable: NearToken,
    /// Received by the account but not yet counted by `sync`.
    pub unsynced: NearToken,
}

#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct Goal {
    creator: AccountId,
    verifier: AccountId,
    token: Option<AccountId>,
    goal_title: String,
    goal_uri: String,
    immediate_bps: u16,
    challenge_window_ms: u64,
    milestones: Vec<Milestone>,
    total_received: NearToken,
    claimed: NearToken,
    last_balance: NearToken,
    /// Payout whose callback has not arrived yet. While set, `sync` and `claim` are blocked.
    payout_in_flight: Option<NearToken>,
}

fn title_ok(title: &str) -> bool {
    (1..=MAX_TITLE_LEN).contains(&title.chars().count())
}

/// sha256 in hex: exactly 64 characters `0-9a-f`.
fn is_valid_proof_hash(hash: &str) -> bool {
    hash.len() == PROOF_HASH_LEN
        && hash
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}

/// The moment the verifier's time to resolve the dispute runs out.
fn resolve_deadline(m: &Milestone) -> u64 {
    m.challenged_at_ms
        .unwrap_or_else(|| env::panic_str("No challenge time"))
        .checked_add(RESOLVE_TIMEOUT_MS)
        .unwrap_or_else(|| math::overflow())
}

/// Storage cost of the current `storage_usage` plus `reserve_bytes`, in yoctoNEAR.
fn storage_cost_with_reserve(reserve_bytes: u64) -> u128 {
    u128::from(env::storage_usage())
        .checked_add(u128::from(reserve_bytes))
        .and_then(|bytes| bytes.checked_mul(env::storage_byte_cost().as_yoctonear()))
        .unwrap_or_else(|| math::overflow())
}

/// The moment the challenge window for a submitted proof closes.
fn window_end(m: &Milestone, challenge_window_ms: u64) -> u64 {
    m.submitted_at_ms
        .unwrap_or_else(|| env::panic_str("No submission time"))
        .checked_add(challenge_window_ms)
        .unwrap_or_else(|| math::overflow())
}

#[near]
impl Goal {
    #[init]
    pub fn new(
        creator: AccountId,
        verifier: AccountId,
        goal_title: String,
        goal_uri: String,
        immediate_bps: u16,
        challenge_window_ms: u64,
        milestones: Vec<MilestoneInput>,
    ) -> Self {
        require!(creator != verifier, "Creator and verifier must differ");
        // The goal has no keys: nobody can call claim or resolve on its behalf.
        let this = env::current_account_id();
        require!(creator != this, "Creator cannot be the contract");
        require!(verifier != this, "Verifier cannot be the contract");
        require!(title_ok(&goal_title), "Goal title must be 1-100 chars");
        require!(goal_uri.chars().count() <= MAX_URI_LEN, "URI too long");
        require!(
            immediate_bps <= BPS_DENOMINATOR,
            "immediate_bps must be <= 10000"
        );
        require!(
            challenge_window_ms >= MIN_CHALLENGE_WINDOW_MS,
            "Window too short"
        );
        require!(
            challenge_window_ms <= MAX_CHALLENGE_WINDOW_MS,
            "Window too long"
        );
        require!(
            (1..=MAX_MILESTONES).contains(&milestones.len()),
            "Need 1-5 milestones"
        );

        let now_ms = env::block_timestamp_ms();
        let latest_deadline_ms = now_ms
            .checked_add(MAX_DEADLINE_AHEAD_MS)
            .unwrap_or_else(|| math::overflow());
        let mut weight_sum: u32 = 0;
        for m in &milestones {
            require!(m.weight_bps > 0, "Weight must be > 0");
            require!(title_ok(&m.title), "Milestone title must be 1-100 chars");
            require!(m.deadline_ms > now_ms, "Deadline must be in future");
            require!(m.deadline_ms <= latest_deadline_ms, "Deadline too far");
            weight_sum = weight_sum
                .checked_add(u32::from(m.weight_bps))
                .unwrap_or_else(|| env::panic_str("Weights must sum to 10000"));
        }
        require!(
            weight_sum == u32::from(BPS_DENOMINATOR),
            "Weights must sum to 10000"
        );

        let milestones = milestones
            .into_iter()
            .map(|m| Milestone {
                title: m.title,
                weight_bps: m.weight_bps,
                deadline_ms: m.deadline_ms,
                status: MilestoneStatus::Pending,
                proof_uri: None,
                proof_hash: None,
                submitted_at_ms: None,
                challenger: None,
                challenge_reason: None,
                challenged_at_ms: None,
            })
            .collect::<Vec<_>>();

        // claim never pays out one reserve; the other covers state growth (SPEC 3).
        require!(
            env::account_balance().as_yoctonear()
                >= storage_cost_with_reserve(2 * STORAGE_RESERVE_BYTES),
            "Deposit too small for storage"
        );

        events::emit(
            "goal_created",
            json!({ "creator": creator, "verifier": verifier, "milestones": milestones.len() }),
        );

        Self {
            creator,
            verifier,
            token: None,
            goal_title,
            goal_uri,
            immediate_bps,
            challenge_window_ms,
            milestones,
            total_received: NearToken::from_yoctonear(0),
            claimed: NearToken::from_yoctonear(0),
            // The initial deposit is not income (SPEC 5.1).
            last_balance: env::account_balance(),
            payout_in_flight: None,
        }
    }

    /// Token address, informational only. Set by the creator, once.
    pub fn set_token(&mut self, token: AccountId) {
        require!(
            env::predecessor_account_id() == self.creator,
            "Only creator"
        );
        require!(self.token.is_none(), "Token already set");
        self.token = Some(token);
    }

    /// Accepts NEAR. The deposit is already in the balance when the call starts; `sync` counts it as income.
    #[payable]
    pub fn deposit(&mut self) {
        self.sync();
    }

    /// Anything above `last_balance` is new income (SPEC 5.1).
    /// `last_balance` never decreases, and while a payout is in flight sync is skipped,
    /// so the refund of a failed payout is never counted as income.
    pub fn sync(&mut self) {
        if self.payout_in_flight.is_some() {
            return;
        }
        let balance = env::account_balance();
        if balance > self.last_balance {
            let income = balance
                .checked_sub(self.last_balance)
                .unwrap_or_else(|| math::overflow());
            self.total_received = self
                .total_received
                .checked_add(income)
                .unwrap_or_else(|| math::overflow());
            events::emit(
                "fees_synced",
                json!({ "income": income, "total_received": self.total_received }),
            );
            self.last_balance = balance;
        }
    }

    /// Pending → Submitted. Creator only, before the milestone deadline.
    pub fn submit_proof(&mut self, index: u8, proof_uri: String, proof_hash: String) {
        self.check_index(index);
        require!(
            env::predecessor_account_id() == self.creator,
            "Only creator"
        );
        let now_ms = env::block_timestamp_ms();
        let m = &mut self.milestones[usize::from(index)];
        require!(m.status == MilestoneStatus::Pending, "Wrong status");
        require!(now_ms < m.deadline_ms, "Deadline passed");
        require!(proof_uri.chars().count() <= MAX_URI_LEN, "URI too long");
        require!(is_valid_proof_hash(&proof_hash), "Invalid proof hash");

        m.status = MilestoneStatus::Submitted;
        m.proof_uri = Some(proof_uri.clone());
        m.proof_hash = Some(proof_hash.clone());
        m.submitted_at_ms = Some(now_ms);
        events::emit(
            "proof_submitted",
            json!({ "index": index, "proof_uri": proof_uri, "proof_hash": proof_hash }),
        );
    }

    /// Submitted → Challenged. Anyone except the creator, while the challenge window is open.
    pub fn challenge(&mut self, index: u8, reason: String) {
        self.check_index(index);
        let challenger = env::predecessor_account_id();
        require!(challenger != self.creator, "Creator cannot challenge");
        let now_ms = env::block_timestamp_ms();
        let window_ms = self.challenge_window_ms;
        let m = &mut self.milestones[usize::from(index)];
        require!(m.status == MilestoneStatus::Submitted, "Wrong status");
        require!(now_ms < window_end(m, window_ms), "Window closed");
        require!(reason.chars().count() <= MAX_REASON_LEN, "Reason too long");

        m.status = MilestoneStatus::Challenged;
        m.challenger = Some(challenger.clone());
        m.challenge_reason = Some(reason.clone());
        m.challenged_at_ms = Some(now_ms);
        events::emit(
            "proof_challenged",
            json!({ "index": index, "challenger": challenger, "reason": reason }),
        );
    }

    /// Submitted → Accepted. Anyone, after the challenge window. The deadline does not matter: the proof was submitted in time.
    pub fn finalize(&mut self, index: u8) {
        self.check_index(index);
        let now_ms = env::block_timestamp_ms();
        let window_ms = self.challenge_window_ms;
        let m = &mut self.milestones[usize::from(index)];
        require!(m.status == MilestoneStatus::Submitted, "Wrong status");
        require!(now_ms >= window_end(m, window_ms), "Window still open");

        m.status = MilestoneStatus::Accepted;
        emit_resolved(index, &m.status);
    }

    /// Challenged → Accepted / Rejected. Verifier only, within 24 h of the challenge.
    pub fn resolve(&mut self, index: u8, accept: bool) {
        self.check_index(index);
        require!(
            env::predecessor_account_id() == self.verifier,
            "Only verifier"
        );
        let now_ms = env::block_timestamp_ms();
        let m = &mut self.milestones[usize::from(index)];
        require!(m.status == MilestoneStatus::Challenged, "Wrong status");
        require!(now_ms < resolve_deadline(m), "Resolve time passed");

        m.status = if accept {
            MilestoneStatus::Accepted
        } else {
            MilestoneStatus::Rejected
        };
        emit_resolved(index, &m.status);
    }

    /// Challenged → Accepted. Anyone, if the verifier has not resolved the dispute within 24 h (AUDIT M-1).
    pub fn resolve_timeout(&mut self, index: u8) {
        self.check_index(index);
        let now_ms = env::block_timestamp_ms();
        let m = &mut self.milestones[usize::from(index)];
        require!(m.status == MilestoneStatus::Challenged, "Wrong status");
        require!(now_ms >= resolve_deadline(m), "Resolve time not passed");

        m.status = MilestoneStatus::Accepted;
        emit_resolved(index, &m.status);
    }

    /// Pending → Failed. Anyone, after the deadline.
    pub fn expire(&mut self, index: u8) {
        self.check_index(index);
        let now_ms = env::block_timestamp_ms();
        let m = &mut self.milestones[usize::from(index)];
        require!(m.status == MilestoneStatus::Pending, "Wrong status");
        require!(now_ms >= m.deadline_ms, "Deadline not passed");

        m.status = MilestoneStatus::Failed;
        emit_resolved(index, &m.status);
    }

    /// Pays the creator everything claimable (SPEC 5.3).
    pub fn claim(&mut self) -> Promise {
        require!(
            env::predecessor_account_id() == self.creator,
            "Only creator"
        );
        require!(self.payout_in_flight.is_none(), "Payout in progress");
        self.sync();

        let claimable = self.breakdown().claimable;
        // Money that pays for storage is never paid out. The reserve covers the 16 bytes of
        // payout_in_flight: near-sdk writes them after this calculation (AUDIT L-1).
        let storage_cost = storage_cost_with_reserve(STORAGE_RESERVE_BYTES);
        // Storage costs more than the balance → 0 is free (saturating subtraction).
        let free = env::account_balance()
            .as_yoctonear()
            .saturating_sub(storage_cost);
        let amount = NearToken::from_yoctonear(claimable.min(free));
        require!(!amount.is_zero(), "Nothing to claim");

        // State first, then the transfer.
        self.claimed = self
            .claimed
            .checked_add(amount)
            .unwrap_or_else(|| math::overflow());
        self.last_balance = self
            .last_balance
            .checked_sub(amount)
            .unwrap_or_else(|| math::overflow());
        self.payout_in_flight = Some(amount);

        Promise::new(self.creator.clone()).transfer(amount).then(
            Self::ext(env::current_account_id())
                .with_static_gas(CALLBACK_GAS)
                .on_claim_transfer(amount),
        )
    }

    /// Result of the transfer from `claim`. Rolls the counters back on failure.
    /// There are no checks here that can fail: a failed callback would leave the payout
    /// flag set forever.
    #[private]
    pub fn on_claim_transfer(&mut self, amount: NearToken) {
        self.payout_in_flight = None;
        // A transfer has an empty result, hence max_len = 0.
        let transferred = env::promise_result_checked(0, 0).is_ok();
        if transferred {
            events::emit(
                "claimed",
                json!({ "amount": amount, "total_claimed": self.claimed }),
            );
        } else {
            self.claimed = self
                .claimed
                .checked_sub(amount)
                .unwrap_or_else(|| math::overflow());
            self.last_balance = self
                .last_balance
                .checked_add(amount)
                .unwrap_or_else(|| math::overflow());
            events::emit("claim_failed", json!({ "amount": amount }));
        }
    }

    pub fn get_balances(&self) -> BalancesView {
        let b = self.breakdown();
        let unsynced = env::account_balance()
            .checked_sub(self.last_balance)
            .unwrap_or(NearToken::from_yoctonear(0));
        BalancesView {
            total_received: self.total_received,
            immediate: NearToken::from_yoctonear(b.immediate),
            locked: NearToken::from_yoctonear(b.locked),
            unlocked: NearToken::from_yoctonear(b.unlocked),
            forfeited: NearToken::from_yoctonear(b.forfeited),
            pending: NearToken::from_yoctonear(b.pending),
            claimed: self.claimed,
            claimable: NearToken::from_yoctonear(b.claimable),
            unsynced,
        }
    }

    pub fn get_goal(&self) -> GoalView {
        GoalView {
            creator: self.creator.clone(),
            verifier: self.verifier.clone(),
            token: self.token.clone(),
            goal_title: self.goal_title.clone(),
            goal_uri: self.goal_uri.clone(),
            immediate_bps: self.immediate_bps,
            challenge_window_ms: self.challenge_window_ms,
            milestones: self.milestones.clone(),
        }
    }
}

/// `milestone_resolved` event: `by` is the account that called the method.
fn emit_resolved(index: u8, status: &MilestoneStatus) {
    events::emit(
        "milestone_resolved",
        json!({ "index": index, "status": status, "by": env::predecessor_account_id() }),
    );
}

impl Goal {
    fn check_index(&self, index: u8) {
        require!(
            usize::from(index) < self.milestones.len(),
            "No such milestone"
        );
    }

    /// Breakdown of counted income by the SPEC 5.2 formulas.
    fn breakdown(&self) -> math::Breakdown {
        let (w_acc, w_lost) = self.weights();
        math::compute(
            self.total_received.as_yoctonear(),
            self.immediate_bps,
            w_acc,
            w_lost,
            self.claimed.as_yoctonear(),
        )
    }

    /// (W_acc, W_lost): total weight of accepted and of rejected/failed milestones.
    fn weights(&self) -> (u32, u32) {
        let mut w_acc: u32 = 0;
        let mut w_lost: u32 = 0;
        for m in &self.milestones {
            let w = u32::from(m.weight_bps);
            match m.status {
                MilestoneStatus::Accepted => {
                    w_acc = w_acc.checked_add(w).unwrap_or_else(|| math::overflow())
                }
                MilestoneStatus::Rejected | MilestoneStatus::Failed => {
                    w_lost = w_lost.checked_add(w).unwrap_or_else(|| math::overflow())
                }
                _ => {}
            }
        }
        (w_acc, w_lost)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use near_sdk::test_utils::{VMContextBuilder, get_logs};
    use near_sdk::{PromiseResult, testing_env};

    const NOW_MS: u64 = 1_000_000;

    fn creator() -> AccountId {
        "creator.testnet".parse().unwrap()
    }

    fn verifier() -> AccountId {
        "verifier.testnet".parse().unwrap()
    }

    fn stranger() -> AccountId {
        "stranger.testnet".parse().unwrap()
    }

    /// Context: `predecessor` calls at NOW_MS, the contract balance is 5 NEAR.
    fn set_context(predecessor: AccountId) {
        set_context_with(predecessor, NearToken::from_near(5), NearToken::from_near(0));
    }

    /// `balance` is the balance before the deposit. The runtime (and the near-sdk mock) adds
    /// `deposit` itself, so `env::account_balance()` inside the call = `balance + deposit`.
    fn set_context_with(predecessor: AccountId, balance: NearToken, deposit: NearToken) {
        set_env(predecessor, NOW_MS, balance, deposit);
    }

    /// Full context: caller, time in ms, balance before the deposit, deposit.
    fn set_env(predecessor: AccountId, now_ms: u64, balance: NearToken, deposit: NearToken) {
        let mut builder = VMContextBuilder::new();
        builder
            .current_account_id("goal.testnet".parse().unwrap())
            .predecessor_account_id(predecessor)
            .block_timestamp(now_ms * 1_000_000)
            .account_balance(balance)
            .attached_deposit(deposit);
        testing_env!(builder.build());
    }

    /// Call from `predecessor` at `now_ms`, balance 5 NEAR.
    fn set_time(predecessor: AccountId, now_ms: u64) {
        set_env(predecessor, now_ms, NearToken::from_near(5), NearToken::from_near(0));
    }

    fn milestones(weights: &[u16]) -> Vec<MilestoneInput> {
        weights
            .iter()
            .enumerate()
            .map(|(i, &weight_bps)| MilestoneInput {
                title: format!("Milestone {i}"),
                weight_bps,
                deadline_ms: NOW_MS + DAY_MS,
            })
            .collect()
    }

    fn new_goal(verifier: AccountId, milestones: Vec<MilestoneInput>) -> Goal {
        Goal::new(
            creator(),
            verifier,
            "Ship the app".to_string(),
            "https://proofee.example/goal".to_string(),
            5000,
            120_000,
            milestones,
        )
    }

    /// Like `new_goal`, but with arbitrary strings, immediate share and window.
    fn new_goal_with(
        goal_title: &str,
        goal_uri: &str,
        immediate_bps: u16,
        challenge_window_ms: u64,
        milestones: Vec<MilestoneInput>,
    ) -> Goal {
        Goal::new(
            creator(),
            verifier(),
            goal_title.to_string(),
            goal_uri.to_string(),
            immediate_bps,
            challenge_window_ms,
            milestones,
        )
    }

    // SPEC 10, test 1
    #[test]
    #[should_panic(expected = "Weights must sum to 10000")]
    fn new_rejects_weights_not_summing_to_10000() {
        set_context(creator());
        new_goal(verifier(), milestones(&[4000, 3000, 2000]));
    }

    // SPEC 10, test 2
    #[test]
    #[should_panic(expected = "Need 1-5 milestones")]
    fn new_rejects_zero_milestones() {
        set_context(creator());
        new_goal(verifier(), milestones(&[]));
    }

    #[test]
    #[should_panic(expected = "Need 1-5 milestones")]
    fn new_rejects_six_milestones() {
        set_context(creator());
        new_goal(
            verifier(),
            milestones(&[2000, 2000, 2000, 2000, 1000, 1000]),
        );
    }

    #[test]
    #[should_panic(expected = "Creator and verifier must differ")]
    fn new_rejects_verifier_equal_to_creator() {
        set_context(creator());
        new_goal(creator(), milestones(&[4000, 3000, 3000]));
    }

    #[test]
    #[should_panic(expected = "Deadline must be in future")]
    fn new_rejects_deadline_in_past() {
        set_context(creator());
        let mut ms = milestones(&[5000, 5000]);
        ms[1].deadline_ms = NOW_MS - 1;
        new_goal(verifier(), ms);
    }

    #[test]
    #[should_panic(expected = "Deadline must be in future")]
    fn new_rejects_deadline_equal_to_now() {
        set_context(creator());
        let mut ms = milestones(&[10_000]);
        ms[0].deadline_ms = NOW_MS;
        new_goal(verifier(), ms);
    }

    #[test]
    #[should_panic(expected = "Weight must be > 0")]
    fn new_rejects_zero_weight() {
        set_context(creator());
        new_goal(verifier(), milestones(&[10_000, 0]));
    }

    #[test]
    #[should_panic(expected = "Goal title must be 1-100 chars")]
    fn new_rejects_empty_goal_title() {
        set_context(creator());
        new_goal_with("", "", 5000, 120_000, milestones(&[10_000]));
    }

    #[test]
    #[should_panic(expected = "Goal title must be 1-100 chars")]
    fn new_rejects_long_goal_title() {
        set_context(creator());
        new_goal_with(&"a".repeat(101), "", 5000, 120_000, milestones(&[10_000]));
    }

    #[test]
    #[should_panic(expected = "URI too long")]
    fn new_rejects_long_goal_uri() {
        set_context(creator());
        new_goal_with("Goal", &"a".repeat(257), 5000, 120_000, milestones(&[10_000]));
    }

    #[test]
    #[should_panic(expected = "Milestone title must be 1-100 chars")]
    fn new_rejects_long_milestone_title() {
        set_context(creator());
        let mut ms = milestones(&[10_000]);
        ms[0].title = "a".repeat(101);
        new_goal(verifier(), ms);
    }

    #[test]
    #[should_panic(expected = "immediate_bps must be <= 10000")]
    fn new_rejects_immediate_bps_above_10000() {
        set_context(creator());
        new_goal_with("Goal", "", 10_001, 120_000, milestones(&[10_000]));
    }

    #[test]
    #[should_panic(expected = "Window too short")]
    fn new_rejects_zero_window() {
        set_context(creator());
        new_goal_with("Goal", "", 5000, 0, milestones(&[10_000]));
    }

    // AUDIT M-2: a 1 ms window made challenging impossible.
    #[test]
    #[should_panic(expected = "Window too short")]
    fn new_rejects_window_below_minute() {
        set_context(creator());
        new_goal_with("Goal", "", 5000, 59_999, milestones(&[10_000]));
    }

    // AUDIT L-3: a u64::MAX window broke window_end and locked the milestone in Submitted.
    #[test]
    #[should_panic(expected = "Window too long")]
    fn new_rejects_window_above_30_days() {
        set_context(creator());
        new_goal_with("Goal", "", 5000, 30 * DAY_MS + 1, milestones(&[10_000]));
    }

    // AUDIT L-3: a deadline far in the future kept the milestone from ever failing.
    #[test]
    #[should_panic(expected = "Deadline too far")]
    fn new_rejects_deadline_beyond_year() {
        set_context(creator());
        let mut ms = milestones(&[10_000]);
        ms[0].deadline_ms = NOW_MS + 365 * DAY_MS + 1;
        new_goal_with("Goal", "", 5000, 120_000, ms);
    }

    #[test]
    fn new_accepts_upper_limits() {
        set_context(creator());
        let mut ms = milestones(&[10_000]);
        ms[0].deadline_ms = NOW_MS + 365 * DAY_MS;
        let goal = new_goal_with("Goal", "", 5000, 30 * DAY_MS, ms);
        assert_eq!(goal.get_goal().challenge_window_ms, 30 * DAY_MS);
    }

    /// `new` with balance `balance` and the mock's default storage (307 200 bytes).
    fn new_with_balance(balance: NearToken) -> Goal {
        set_context_with(creator(), balance, NearToken::from_near(0));
        new_goal(verifier(), milestones(&[10_000]))
    }

    /// Minimum deposit: (307 200 + 2 × 16 000) bytes × 10^19 yocto.
    const MIN_DEPOSIT_YOCTO: u128 = 339_200 * 10_000_000_000_000_000_000;

    // AUDIT L-2: without a reserve, storage would be paid from the creator's income.
    #[test]
    #[should_panic(expected = "Deposit too small for storage")]
    fn new_rejects_deposit_below_storage_reserve() {
        new_with_balance(NearToken::from_yoctonear(MIN_DEPOSIT_YOCTO - 1));
    }

    #[test]
    fn new_accepts_exact_storage_reserve() {
        let goal = new_with_balance(NearToken::from_yoctonear(MIN_DEPOSIT_YOCTO));
        assert_eq!(goal.total_received, NearToken::from_yoctonear(0));
    }

    // AUDIT L-4: the goal has no keys, nobody can call claim on its behalf.
    #[test]
    #[should_panic(expected = "Creator cannot be the contract")]
    fn new_rejects_creator_equal_to_contract() {
        set_context(creator());
        Goal::new(
            "goal.testnet".parse().unwrap(),
            verifier(),
            "Goal".to_string(),
            String::new(),
            5000,
            120_000,
            milestones(&[10_000]),
        );
    }

    // AUDIT L-4: nobody can call resolve on the goal's behalf, disputes would hang forever.
    #[test]
    #[should_panic(expected = "Verifier cannot be the contract")]
    fn new_rejects_verifier_equal_to_contract() {
        set_context(creator());
        new_goal("goal.testnet".parse().unwrap(), milestones(&[10_000]));
    }

    #[test]
    fn new_accepts_limits() {
        set_context(creator());
        // Length is counted in characters, not bytes: "é" is 2 bytes but 1 character.
        let mut ms = milestones(&[10_000]);
        ms[0].title = "é".repeat(100);
        ms[0].deadline_ms = NOW_MS + 1;
        let goal = new_goal_with(&"é".repeat(100), &"a".repeat(256), 10_000, 60_000, ms);
        assert_eq!(goal.get_goal().immediate_bps, 10_000);
    }

    #[test]
    fn new_emits_goal_created() {
        set_context(creator());
        new_goal(verifier(), milestones(&[4000, 3000, 3000]));

        let logs = get_logs();
        assert_eq!(logs.len(), 1);
        assert_eq!(
            logs[0],
            r#"EVENT_JSON:{"standard":"proofee","version":"1.0.0","event":"goal_created","data":[{"creator":"creator.testnet","verifier":"verifier.testnet","milestones":3}]}"#
        );
    }

    #[test]
    fn new_sets_initial_state() {
        set_context(creator());
        let goal = new_goal(verifier(), milestones(&[4000, 3000, 3000]));

        assert_eq!(goal.last_balance, NearToken::from_near(5));
        assert_eq!(goal.total_received, NearToken::from_yoctonear(0));
        assert_eq!(goal.claimed, NearToken::from_yoctonear(0));

        let view = goal.get_goal();
        assert_eq!(view.creator, creator());
        assert_eq!(view.verifier, verifier());
        assert_eq!(view.token, None);
        assert_eq!(view.immediate_bps, 5000);
        assert_eq!(view.challenge_window_ms, 120_000);
        assert_eq!(view.milestones.len(), 3);
        assert!(
            view.milestones
                .iter()
                .all(|m| m.status == MilestoneStatus::Pending && m.proof_uri.is_none())
        );
    }

    #[test]
    fn set_token_by_creator() {
        set_context(creator());
        let mut goal = new_goal(verifier(), milestones(&[10_000]));
        let token: AccountId = "meme.testnet".parse().unwrap();
        goal.set_token(token.clone());
        assert_eq!(goal.get_goal().token, Some(token));
    }

    #[test]
    #[should_panic(expected = "Token already set")]
    fn set_token_only_once() {
        set_context(creator());
        let mut goal = new_goal(verifier(), milestones(&[10_000]));
        goal.set_token("meme.testnet".parse().unwrap());
        goal.set_token("other.testnet".parse().unwrap());
    }

    #[test]
    #[should_panic(expected = "Only creator")]
    fn set_token_rejects_stranger() {
        set_context(creator());
        let mut goal = new_goal(verifier(), milestones(&[10_000]));
        set_context(stranger());
        goal.set_token("meme.testnet".parse().unwrap());
    }

    // SPEC 10, test 3
    #[test]
    fn sync_ignores_initial_deposit() {
        set_context(creator());
        let mut goal = new_goal(verifier(), milestones(&[10_000]));
        goal.sync();
        assert_eq!(goal.total_received, NearToken::from_near(0));
        assert_eq!(goal.last_balance, NearToken::from_near(5));
        assert!(!get_logs().iter().any(|l| l.contains("fees_synced")));
    }

    #[test]
    fn sync_counts_incoming_transfer() {
        set_context(creator());
        let mut goal = new_goal(verifier(), milestones(&[10_000]));

        // 10 NEAR arrived at the contract as a plain transfer.
        set_context_with(stranger(), NearToken::from_near(15), NearToken::from_near(0));
        assert_eq!(goal.get_balances().unsynced, NearToken::from_near(10));

        goal.sync();
        assert_eq!(goal.total_received, NearToken::from_near(10));
        assert_eq!(goal.last_balance, NearToken::from_near(15));

        let balances = goal.get_balances();
        assert_eq!(balances.unsynced, NearToken::from_near(0));
        assert_eq!(balances.immediate, NearToken::from_near(5));
        assert_eq!(balances.locked, NearToken::from_near(5));
        assert_eq!(balances.pending, NearToken::from_near(5));
        assert_eq!(balances.claimable, NearToken::from_near(5));

        let logs = get_logs();
        assert_eq!(logs.len(), 1);
        assert!(logs[0].starts_with("EVENT_JSON:"));
        assert!(logs[0].contains(r#""event":"fees_synced""#));
        assert!(logs[0].contains(r#""income":"10000000000000000000000000""#));
    }

    #[test]
    fn deposit_counts_attached_near() {
        set_context(creator());
        let mut goal = new_goal(verifier(), milestones(&[10_000]));

        // 5 NEAR before, 2 attached: the balance inside the call is 7.
        set_context_with(stranger(), NearToken::from_near(5), NearToken::from_near(2));
        assert_eq!(env::account_balance(), NearToken::from_near(7));
        goal.deposit();
        assert_eq!(goal.total_received, NearToken::from_near(2));
        assert_eq!(goal.last_balance, NearToken::from_near(7));
    }

    // ---- Milestones (SPEC 6) ----

    const WINDOW_MS: u64 = 120_000;
    const DEADLINE_MS: u64 = NOW_MS + DAY_MS;
    // sha256 of the empty string.
    const HASH: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
    const URI: &str = "https://proofee.example/proof/1";

    /// Goal with milestones 40/30/30 and a 2-minute window.
    fn goal3() -> Goal {
        set_context(creator());
        new_goal(verifier(), milestones(&[4000, 3000, 3000]))
    }

    fn submit_at(goal: &mut Goal, index: u8, now_ms: u64) {
        set_time(creator(), now_ms);
        goal.submit_proof(index, URI.to_string(), HASH.to_string());
    }

    /// The balance becomes 5 + `total_near` NEAR (`total_near` received in total), then sync.
    fn receive_and_sync(goal: &mut Goal, total_near: u128) {
        set_context_with(
            stranger(),
            NearToken::from_near(5 + total_near),
            NearToken::from_near(0),
        );
        goal.sync();
    }

    fn status(goal: &Goal, index: usize) -> MilestoneStatus {
        goal.get_goal().milestones[index].status.clone()
    }

    #[test]
    #[should_panic(expected = "No such milestone")]
    fn rejects_unknown_index() {
        let mut goal = goal3();
        submit_at(&mut goal, 3, NOW_MS);
    }

    // SPEC 10, test 5
    #[test]
    fn submit_proof_by_creator() {
        let mut goal = goal3();
        submit_at(&mut goal, 1, NOW_MS + 10);

        let m = &goal.get_goal().milestones[1];
        assert_eq!(m.status, MilestoneStatus::Submitted);
        assert_eq!(m.proof_uri.as_deref(), Some(URI));
        assert_eq!(m.proof_hash.as_deref(), Some(HASH));
        assert_eq!(m.submitted_at_ms, Some(NOW_MS + 10));
        assert_eq!(status(&goal, 0), MilestoneStatus::Pending);

        let logs = get_logs();
        assert_eq!(logs.len(), 1);
        assert!(logs[0].contains(r#""event":"proof_submitted""#));
        assert!(logs[0].contains(r#""index":1"#));
    }

    #[test]
    #[should_panic(expected = "Only creator")]
    fn submit_proof_rejects_stranger() {
        let mut goal = goal3();
        set_time(stranger(), NOW_MS);
        goal.submit_proof(0, URI.to_string(), HASH.to_string());
    }

    #[test]
    #[should_panic(expected = "Wrong status")]
    fn submit_proof_only_from_pending() {
        let mut goal = goal3();
        submit_at(&mut goal, 0, NOW_MS);
        submit_at(&mut goal, 0, NOW_MS + 1);
    }

    #[test]
    #[should_panic(expected = "Deadline passed")]
    fn submit_proof_rejects_at_deadline() {
        let mut goal = goal3();
        submit_at(&mut goal, 0, DEADLINE_MS);
    }

    #[test]
    fn submit_proof_checks_hash() {
        let bad = [
            HASH.to_uppercase(),           // upper case
            HASH[..63].to_string(),        // 63 characters
            format!("{}g", &HASH[..63]),   // not hex
            format!("{HASH}0"),            // 65 characters
        ];
        for hash in bad {
            let mut goal = goal3();
            set_time(creator(), NOW_MS);
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                goal.submit_proof(0, URI.to_string(), hash.clone())
            }));
            let err = result.expect_err(&format!("hash {hash} must be rejected"));
            let msg = err.downcast_ref::<String>().map(String::as_str).unwrap_or("");
            assert!(msg.contains("Invalid proof hash"), "{hash}: {msg}");
        }
    }

    #[test]
    #[should_panic(expected = "URI too long")]
    fn submit_proof_rejects_long_uri() {
        let mut goal = goal3();
        set_time(creator(), NOW_MS);
        goal.submit_proof(0, "a".repeat(257), HASH.to_string());
    }

    // SPEC 10, test 6
    #[test]
    fn challenge_within_window() {
        let mut goal = goal3();
        submit_at(&mut goal, 0, NOW_MS);
        set_time(stranger(), NOW_MS + WINDOW_MS - 1);
        goal.challenge(0, "Fake screenshot".to_string());

        let m = &goal.get_goal().milestones[0];
        assert_eq!(m.status, MilestoneStatus::Challenged);
        assert_eq!(m.challenger, Some(stranger()));
        assert_eq!(m.challenge_reason.as_deref(), Some("Fake screenshot"));
        assert_eq!(m.challenged_at_ms, Some(NOW_MS + WINDOW_MS - 1));
        assert!(get_logs()[0].contains(r#""event":"proof_challenged""#));
    }

    #[test]
    #[should_panic(expected = "Creator cannot challenge")]
    fn challenge_rejects_creator() {
        let mut goal = goal3();
        submit_at(&mut goal, 0, NOW_MS);
        goal.challenge(0, "self".to_string());
    }

    #[test]
    #[should_panic(expected = "Window closed")]
    fn challenge_rejects_after_window() {
        let mut goal = goal3();
        submit_at(&mut goal, 0, NOW_MS);
        set_time(stranger(), NOW_MS + WINDOW_MS);
        goal.challenge(0, "late".to_string());
    }

    #[test]
    #[should_panic(expected = "Wrong status")]
    fn challenge_rejects_pending() {
        let mut goal = goal3();
        set_time(stranger(), NOW_MS);
        goal.challenge(0, "nothing to challenge".to_string());
    }

    #[test]
    #[should_panic(expected = "Wrong status")]
    fn challenge_only_once() {
        let mut goal = goal3();
        submit_at(&mut goal, 0, NOW_MS);
        set_time(stranger(), NOW_MS + 1);
        goal.challenge(0, "first".to_string());
        set_time(verifier(), NOW_MS + 2);
        goal.challenge(0, "second".to_string());
    }

    #[test]
    #[should_panic(expected = "Reason too long")]
    fn challenge_rejects_long_reason() {
        let mut goal = goal3();
        submit_at(&mut goal, 0, NOW_MS);
        set_time(stranger(), NOW_MS + 1);
        goal.challenge(0, "a".repeat(281));
    }

    // SPEC 10, test 7
    #[test]
    #[should_panic(expected = "Window still open")]
    fn finalize_rejects_open_window() {
        let mut goal = goal3();
        submit_at(&mut goal, 0, NOW_MS);
        set_time(stranger(), NOW_MS + WINDOW_MS - 1);
        goal.finalize(0);
    }

    #[test]
    fn finalize_after_window_unlocks_share() {
        let mut goal = goal3();
        receive_and_sync(&mut goal, 10);
        submit_at(&mut goal, 0, NOW_MS);
        set_time(stranger(), NOW_MS + WINDOW_MS);
        goal.finalize(0);

        assert_eq!(status(&goal, 0), MilestoneStatus::Accepted);
        let log = &get_logs()[0];
        assert!(log.contains(r#""event":"milestone_resolved""#));
        assert!(log.contains(r#""status":"Accepted""#));
        assert!(log.contains(r#""by":"stranger.testnet""#));

        // 5 NEAR locked, milestone 40% → 2 unlocked, 5 + 2 claimable.
        let b = goal.get_balances();
        assert_eq!(b.unlocked, NearToken::from_near(2));
        assert_eq!(b.forfeited, NearToken::from_near(0));
        assert_eq!(b.pending, NearToken::from_near(3));
        assert_eq!(b.claimable, NearToken::from_near(7));
    }

    #[test]
    fn finalize_after_deadline_if_submitted_in_time() {
        let mut goal = goal3();
        submit_at(&mut goal, 0, DEADLINE_MS - 1);
        set_time(stranger(), DEADLINE_MS - 1 + WINDOW_MS);
        goal.finalize(0);
        assert_eq!(status(&goal, 0), MilestoneStatus::Accepted);
    }

    // SPEC 10, test 8
    fn challenged(index: u8) -> Goal {
        let mut goal = goal3();
        receive_and_sync(&mut goal, 10);
        submit_at(&mut goal, index, NOW_MS);
        set_time(stranger(), NOW_MS + 1);
        goal.challenge(index, "doubt".to_string());
        goal
    }

    #[test]
    #[should_panic(expected = "Only verifier")]
    fn resolve_rejects_non_verifier() {
        let mut goal = challenged(1);
        set_time(creator(), NOW_MS + 2);
        goal.resolve(1, true);
    }

    #[test]
    #[should_panic(expected = "Wrong status")]
    fn resolve_requires_challenge() {
        let mut goal = goal3();
        submit_at(&mut goal, 1, NOW_MS);
        set_time(verifier(), NOW_MS + 1);
        goal.resolve(1, true);
    }

    #[test]
    fn resolve_accept() {
        let mut goal = challenged(1);
        set_time(verifier(), NOW_MS + 2);
        goal.resolve(1, true);
        assert_eq!(status(&goal, 1), MilestoneStatus::Accepted);
        assert_eq!(goal.get_balances().unlocked, NearToken::from_millinear(1500));
    }

    #[test]
    fn resolve_reject_forfeits_share() {
        let mut goal = challenged(1);
        set_time(verifier(), NOW_MS + 2);
        goal.resolve(1, false);
        assert_eq!(status(&goal, 1), MilestoneStatus::Rejected);
        assert!(get_logs()[0].contains(r#""by":"verifier.testnet""#));

        // 5 NEAR locked, milestone 30% → 1.5 forfeited.
        let b = goal.get_balances();
        assert_eq!(b.forfeited, NearToken::from_millinear(1500));
        assert_eq!(b.unlocked, NearToken::from_near(0));
        assert_eq!(b.claimable, NearToken::from_near(5));
    }

    #[test]
    #[should_panic(expected = "Wrong status")]
    fn resolve_is_final() {
        let mut goal = challenged(1);
        set_time(verifier(), NOW_MS + 2);
        goal.resolve(1, false);
        goal.resolve(1, true);
    }

    // ---- Dispute timeout (SPEC 6, AUDIT M-1). challenged() challenges at NOW_MS + 1. ----

    #[test]
    fn resolve_works_until_timeout() {
        let mut goal = challenged(1);
        set_time(verifier(), NOW_MS + 1 + DAY_MS - 1);
        goal.resolve(1, false);
        assert_eq!(status(&goal, 1), MilestoneStatus::Rejected);
    }

    #[test]
    #[should_panic(expected = "Resolve time passed")]
    fn resolve_rejects_after_timeout() {
        let mut goal = challenged(1);
        set_time(verifier(), NOW_MS + 1 + DAY_MS);
        goal.resolve(1, false);
    }

    #[test]
    #[should_panic(expected = "Resolve time not passed")]
    fn resolve_timeout_rejects_before_timeout() {
        let mut goal = challenged(0);
        set_time(stranger(), NOW_MS + 1 + DAY_MS - 1);
        goal.resolve_timeout(0);
    }

    #[test]
    fn resolve_timeout_accepts_for_creator() {
        let mut goal = challenged(0);
        set_time(stranger(), NOW_MS + 1 + DAY_MS);
        goal.resolve_timeout(0);

        assert_eq!(status(&goal, 0), MilestoneStatus::Accepted);
        // 10 NEAR: 5 immediate, milestone 0 (40%) unlocks 2.
        assert_eq!(goal.get_balances().claimable, NearToken::from_near(7));
        let logs = get_logs();
        assert!(logs[0].contains(r#""event":"milestone_resolved""#));
        assert!(logs[0].contains(r#""status":"Accepted""#));
        assert!(logs[0].contains(r#""by":"stranger.testnet""#));
    }

    #[test]
    #[should_panic(expected = "Wrong status")]
    fn resolve_timeout_requires_challenge() {
        let mut goal = goal3();
        submit_at(&mut goal, 0, NOW_MS);
        set_time(stranger(), NOW_MS + 2 * DAY_MS);
        goal.resolve_timeout(0);
    }

    #[test]
    fn rejected_share_burns_from_future_fees() {
        let mut goal = challenged(1);
        set_time(verifier(), NOW_MS + 2);
        goal.resolve(1, false);
        assert_eq!(goal.get_balances().forfeited, NearToken::from_millinear(1500));

        // After the rejection another 10 NEAR arrived: 20 in total, 10 locked.
        receive_and_sync(&mut goal, 20);
        let b = goal.get_balances();
        assert_eq!(b.total_received, NearToken::from_near(20));
        assert_eq!(b.locked, NearToken::from_near(10));
        assert_eq!(b.forfeited, NearToken::from_near(3));
        assert_eq!(b.pending, NearToken::from_near(7));
        assert_eq!(b.claimable, NearToken::from_near(10));
    }

    // SPEC 10, test 9
    #[test]
    #[should_panic(expected = "Deadline not passed")]
    fn expire_rejects_before_deadline() {
        let mut goal = goal3();
        set_time(stranger(), DEADLINE_MS - 1);
        goal.expire(2);
    }

    #[test]
    fn expire_after_deadline_fails_milestone() {
        let mut goal = goal3();
        receive_and_sync(&mut goal, 10);
        set_time(stranger(), DEADLINE_MS);
        goal.expire(2);
        assert_eq!(status(&goal, 2), MilestoneStatus::Failed);
        assert!(get_logs()[0].contains(r#""status":"Failed""#));
        assert_eq!(goal.get_balances().forfeited, NearToken::from_millinear(1500));
    }

    #[test]
    #[should_panic(expected = "Wrong status")]
    fn expire_rejects_submitted() {
        let mut goal = goal3();
        submit_at(&mut goal, 2, DEADLINE_MS - 1);
        set_time(stranger(), DEADLINE_MS);
        goal.expire(2);
    }

    // ---- Payout (SPEC 5.3) ----

    /// Calls `claim` with an explicit balance and storage usage in bytes.
    fn set_claim_env(predecessor: AccountId, balance: NearToken, storage_usage: u64) {
        let mut builder = VMContextBuilder::new();
        builder
            .current_account_id("goal.testnet".parse().unwrap())
            .predecessor_account_id(predecessor)
            .block_timestamp(NOW_MS * 1_000_000)
            .account_balance(balance)
            .storage_usage(storage_usage);
        testing_env!(builder.build());
    }

    /// Calls the callback: the contract calls it itself, the transfer result is `result`.
    fn set_callback_env(balance: NearToken, result: PromiseResult) {
        let mut builder = VMContextBuilder::new();
        builder
            .current_account_id("goal.testnet".parse().unwrap())
            .predecessor_account_id("goal.testnet".parse().unwrap())
            .block_timestamp(NOW_MS * 1_000_000)
            .account_balance(balance);
        testing_env!(
            builder.build(),
            near_sdk::test_vm_config(),
            near_sdk::RuntimeFeesConfig::test(),
            Default::default(),
            vec![result],
        );
    }

    /// Goal 40/30/30, 10 NEAR received (balance 15), the creator has already called `claim` for 5 NEAR.
    fn claimed_goal() -> Goal {
        let mut goal = goal3();
        receive_and_sync(&mut goal, 10);
        set_context_with(creator(), NearToken::from_near(15), NearToken::from_near(0));
        let _ = goal.claim();
        goal
    }

    /// The callback for the 5 NEAR payout arrives; `balance_near` is the balance at that moment.
    fn callback(goal: &mut Goal, balance_near: u128, result: PromiseResult) {
        set_callback_env(NearToken::from_near(balance_near), result);
        goal.on_claim_transfer(NearToken::from_near(5));
    }

    /// `sync` from a third-party account at balance `balance_near`.
    fn sync_at(goal: &mut Goal, balance_near: u128) {
        set_context_with(
            stranger(),
            NearToken::from_near(balance_near),
            NearToken::from_near(0),
        );
        goal.sync();
    }

    // SPEC 10, test 10
    #[test]
    fn claim_pays_exactly_claimable() {
        let mut goal = claimed_goal();
        assert_eq!(goal.claimed, NearToken::from_near(5));
        // The mock, like the network, deducted the transfer from the balance right away.
        assert_eq!(env::account_balance(), NearToken::from_near(10));
        assert_eq!(goal.last_balance, NearToken::from_near(10));
        assert_eq!(goal.get_balances().claimable, NearToken::from_near(0));
        assert_eq!(goal.payout_in_flight, Some(NearToken::from_near(5)));

        // A payout is not "negative income": after the callback sync sees nothing new.
        callback(&mut goal, 10, PromiseResult::Successful(vec![]));
        sync_at(&mut goal, 10);
        assert_eq!(goal.total_received, NearToken::from_near(10));
    }

    #[test]
    #[should_panic(expected = "Nothing to claim")]
    fn claim_twice_fails() {
        let mut goal = claimed_goal();
        callback(&mut goal, 10, PromiseResult::Successful(vec![]));
        set_context_with(creator(), NearToken::from_near(10), NearToken::from_near(0));
        let _ = goal.claim();
    }

    #[test]
    #[should_panic(expected = "Nothing to claim")]
    fn claim_without_income_fails() {
        let mut goal = goal3();
        let _ = goal.claim();
    }

    #[test]
    #[should_panic(expected = "Only creator")]
    fn claim_rejects_stranger() {
        let mut goal = goal3();
        receive_and_sync(&mut goal, 10);
        let _ = goal.claim(); // receive_and_sync left stranger as the caller
    }

    #[test]
    #[should_panic(expected = "Only creator")]
    fn claim_rejects_verifier() {
        let mut goal = goal3();
        receive_and_sync(&mut goal, 10);
        set_context_with(verifier(), NearToken::from_near(15), NearToken::from_near(0));
        let _ = goal.claim();
    }

    #[test]
    fn claim_callback_rolls_back_on_failure() {
        let mut goal = claimed_goal();
        callback(&mut goal, 10, PromiseResult::Failed);

        assert_eq!(goal.payout_in_flight, None);
        assert_eq!(goal.claimed, NearToken::from_near(0));
        assert_eq!(goal.last_balance, NearToken::from_near(15));
        assert_eq!(goal.get_balances().claimable, NearToken::from_near(5));
        let logs = get_logs();
        assert_eq!(logs.len(), 1);
        assert!(logs[0].contains(r#""event":"claim_failed""#));
    }

    #[test]
    fn claim_callback_keeps_counters_on_success() {
        let mut goal = claimed_goal();
        callback(&mut goal, 10, PromiseResult::Successful(vec![]));

        assert_eq!(goal.payout_in_flight, None);
        assert_eq!(goal.claimed, NearToken::from_near(5));
        assert_eq!(goal.last_balance, NearToken::from_near(10));
        let logs = get_logs();
        assert_eq!(logs.len(), 1);
        assert!(logs[0].contains(r#""event":"claimed""#));
        assert!(logs[0].contains(r#""total_claimed":"5000000000000000000000000""#));
    }

    #[test]
    #[should_panic(expected = "Nothing to claim")]
    fn claim_spec_example_through_contract() {
        // 100 NEAR, milestone 0 accepted, milestone 1 rejected: 50 + 20 claimable.
        let mut goal = goal3();
        receive_and_sync(&mut goal, 100);
        submit_at(&mut goal, 0, NOW_MS);
        set_time(stranger(), NOW_MS + WINDOW_MS);
        goal.finalize(0);
        submit_at(&mut goal, 1, NOW_MS + WINDOW_MS);
        set_time(stranger(), NOW_MS + WINDOW_MS + 1);
        goal.challenge(1, "doubt".to_string());
        set_time(verifier(), NOW_MS + WINDOW_MS + 2);
        goal.resolve(1, false);

        set_context_with(creator(), NearToken::from_near(105), NearToken::from_near(0));
        let _ = goal.claim();
        assert_eq!(goal.claimed, NearToken::from_near(70));
        assert_eq!(goal.get_balances().forfeited, NearToken::from_near(15));

        set_callback_env(NearToken::from_near(35), PromiseResult::Successful(vec![]));
        goal.on_claim_transfer(NearToken::from_near(70));
        set_context_with(creator(), NearToken::from_near(35), NearToken::from_near(0));
        let _ = goal.claim();
    }

    #[test]
    fn claim_keeps_storage_reserve() {
        let mut goal = goal3();
        receive_and_sync(&mut goal, 10);
        // Storage 1 084 000 bytes + reserve 16 000 = 11 NEAR. Free: 15 − 11 = 4 < 5.
        // Without the reserve (AUDIT L-1) the payout would be 4.16 NEAR.
        set_claim_env(creator(), NearToken::from_near(15), 1_084_000);
        let _ = goal.claim();
        assert_eq!(goal.claimed, NearToken::from_near(4));
        assert_eq!(goal.get_balances().claimable, NearToken::from_near(1));
    }

    #[test]
    #[should_panic(expected = "Nothing to claim")]
    fn claim_nothing_when_storage_takes_all() {
        let mut goal = goal3();
        receive_and_sync(&mut goal, 10);
        // Storage 1 600 000 bytes = 16 NEAR, more than the balance.
        set_claim_env(creator(), NearToken::from_near(15), 1_600_000);
        let _ = goal.claim();
    }

    // ---- Refund of a failed payout (SPEC 5.1, 5.3) ----

    #[test]
    fn sync_never_lowers_last_balance() {
        let mut goal = goal3();
        receive_and_sync(&mut goal, 10);
        sync_at(&mut goal, 12);
        assert_eq!(goal.last_balance, NearToken::from_near(15));
        assert_eq!(goal.total_received, NearToken::from_near(10));
        assert!(get_logs().is_empty());
    }

    // (a) the refund arrived after the callback, with a sync in between.
    #[test]
    fn refund_after_callback_is_not_income() {
        let mut goal = claimed_goal();
        callback(&mut goal, 10, PromiseResult::Failed);
        assert_eq!(goal.last_balance, NearToken::from_near(15));

        // The money is not back yet: the balance is below last_balance, sync does not lower it.
        sync_at(&mut goal, 10);
        assert_eq!(goal.last_balance, NearToken::from_near(15));
        assert!(get_logs().is_empty());

        // The refund arrived.
        sync_at(&mut goal, 15);
        assert_eq!(goal.total_received, NearToken::from_near(10));
        assert!(get_logs().is_empty());

        // The creator can claim their 5 again, and no more.
        set_context_with(creator(), NearToken::from_near(15), NearToken::from_near(0));
        let _ = goal.claim();
        assert_eq!(goal.claimed, NearToken::from_near(5));
    }

    // (b) the refund arrived before the callback, with a sync in between.
    #[test]
    fn refund_before_callback_is_not_income() {
        let mut goal = claimed_goal();

        // The balance is back at 15, but a payout is in flight: sync is skipped.
        sync_at(&mut goal, 15);
        assert_eq!(goal.last_balance, NearToken::from_near(10));
        assert_eq!(goal.total_received, NearToken::from_near(10));
        assert!(get_logs().is_empty());

        callback(&mut goal, 15, PromiseResult::Failed);
        assert_eq!(goal.last_balance, NearToken::from_near(15));

        sync_at(&mut goal, 15);
        assert_eq!(goal.total_received, NearToken::from_near(10));
        assert_eq!(goal.get_balances().claimable, NearToken::from_near(5));
    }

    // (c) real income during a payout is counted exactly once. The payout succeeded.
    #[test]
    fn income_during_successful_payout_counted_once() {
        let mut goal = claimed_goal();
        sync_at(&mut goal, 13); // 3 NEAR arrived, sync skipped
        assert_eq!(goal.total_received, NearToken::from_near(10));

        callback(&mut goal, 13, PromiseResult::Successful(vec![]));
        sync_at(&mut goal, 13);
        assert_eq!(goal.total_received, NearToken::from_near(13));
        sync_at(&mut goal, 13);
        assert_eq!(goal.total_received, NearToken::from_near(13));
    }

    // (c) same, but the payout failed and the refund arrived last.
    #[test]
    fn income_during_failed_payout_counted_once() {
        let mut goal = claimed_goal();
        callback(&mut goal, 13, PromiseResult::Failed); // 3 NEAR already arrived, no refund yet
        assert_eq!(goal.last_balance, NearToken::from_near(15));

        sync_at(&mut goal, 13);
        assert_eq!(goal.total_received, NearToken::from_near(10));

        sync_at(&mut goal, 18); // the refund arrived
        assert_eq!(goal.total_received, NearToken::from_near(13));
        sync_at(&mut goal, 18);
        assert_eq!(goal.total_received, NearToken::from_near(13));
    }

    // (d)
    #[test]
    #[should_panic(expected = "Payout in progress")]
    fn claim_during_payout_fails() {
        let mut goal = claimed_goal();
        set_context_with(creator(), NearToken::from_near(20), NearToken::from_near(0));
        let _ = goal.claim();
    }
}
