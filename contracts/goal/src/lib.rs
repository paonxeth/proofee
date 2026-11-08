// Контракт цели ProoFee. Спецификация — docs/SPEC.md.
use near_sdk::serde_json::json;
use near_sdk::{AccountId, NearToken, PanicOnDefault, env, near, require};

mod events;
mod math;

use math::BPS_DENOMINATOR;

const MAX_MILESTONES: usize = 5;
const MAX_TITLE_LEN: usize = 100;
const MAX_URI_LEN: usize = 256;
const MAX_REASON_LEN: usize = 280;
const PROOF_HASH_LEN: usize = 64;

/// Этап, как его передают в `new`.
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
}

/// Ответ `get_goal`: параметры цели и этапы. Деньги — в `get_balances`.
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

/// Ответ `get_balances`. Формулы — SPEC 5.2, суммы в yoctoNEAR (строки в JSON).
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
    /// Пришло на аккаунт, но ещё не учтено `sync`.
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
}

fn title_ok(title: &str) -> bool {
    (1..=MAX_TITLE_LEN).contains(&title.chars().count())
}

/// sha256 в hex: ровно 64 символа `0-9a-f`.
fn is_valid_proof_hash(hash: &str) -> bool {
    hash.len() == PROOF_HASH_LEN
        && hash
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}

/// Момент, когда закрывается окно спора по поданному пруфу.
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
        require!(title_ok(&goal_title), "Goal title must be 1-100 chars");
        require!(goal_uri.chars().count() <= MAX_URI_LEN, "URI too long");
        require!(
            immediate_bps <= BPS_DENOMINATOR,
            "immediate_bps must be <= 10000"
        );
        require!(challenge_window_ms > 0, "Window must be > 0");
        require!(
            (1..=MAX_MILESTONES).contains(&milestones.len()),
            "Need 1-5 milestones"
        );

        let now_ms = env::block_timestamp_ms();
        let mut weight_sum: u32 = 0;
        for m in &milestones {
            require!(m.weight_bps > 0, "Weight must be > 0");
            require!(title_ok(&m.title), "Milestone title must be 1-100 chars");
            require!(m.deadline_ms > now_ms, "Deadline must be in future");
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
            })
            .collect();

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
            // Стартовый депозит — не доход (SPEC 5.1).
            last_balance: env::account_balance(),
        }
    }

    /// Адрес токена — только для информации. Ставит креатор, один раз.
    pub fn set_token(&mut self, token: AccountId) {
        require!(
            env::predecessor_account_id() == self.creator,
            "Only creator"
        );
        require!(self.token.is_none(), "Token already set");
        self.token = Some(token);
    }

    /// Приём NEAR. Депозит к началу вызова уже на балансе, `sync` учтёт его как доход.
    #[payable]
    pub fn deposit(&mut self) {
        self.sync();
    }

    /// Всё, что выросло сверх `last_balance`, — новый доход (SPEC 5.1).
    pub fn sync(&mut self) {
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
        }
        self.last_balance = balance;
    }

    /// Pending → Submitted. Только creator, до дедлайна этапа.
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

    /// Submitted → Challenged. Любой, кроме creator, пока окно спора открыто.
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
        events::emit(
            "proof_challenged",
            json!({ "index": index, "challenger": challenger, "reason": reason }),
        );
    }

    /// Submitted → Accepted. Любой, после окна спора. Дедлайн не важен: пруф подан вовремя.
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

    /// Challenged → Accepted / Rejected. Только verifier.
    pub fn resolve(&mut self, index: u8, accept: bool) {
        self.check_index(index);
        require!(
            env::predecessor_account_id() == self.verifier,
            "Only verifier"
        );
        let m = &mut self.milestones[usize::from(index)];
        require!(m.status == MilestoneStatus::Challenged, "Wrong status");

        m.status = if accept {
            MilestoneStatus::Accepted
        } else {
            MilestoneStatus::Rejected
        };
        emit_resolved(index, &m.status);
    }

    /// Pending → Failed. Любой, после дедлайна.
    pub fn expire(&mut self, index: u8) {
        self.check_index(index);
        let now_ms = env::block_timestamp_ms();
        let m = &mut self.milestones[usize::from(index)];
        require!(m.status == MilestoneStatus::Pending, "Wrong status");
        require!(now_ms >= m.deadline_ms, "Deadline not passed");

        m.status = MilestoneStatus::Failed;
        emit_resolved(index, &m.status);
    }

    pub fn get_balances(&self) -> BalancesView {
        let (w_acc, w_lost) = self.weights();
        let b = math::compute(
            self.total_received.as_yoctonear(),
            self.immediate_bps,
            w_acc,
            w_lost,
            self.claimed.as_yoctonear(),
        );
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

/// Событие `milestone_resolved`: `by` — тот, кто вызвал метод.
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

    /// (W_acc, W_lost): сумма весов принятых и отклонённых/проваленных этапов.
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
    use near_sdk::testing_env;

    const NOW_MS: u64 = 1_000_000;
    const DAY_MS: u64 = 86_400_000;

    fn creator() -> AccountId {
        "creator.testnet".parse().unwrap()
    }

    fn verifier() -> AccountId {
        "verifier.testnet".parse().unwrap()
    }

    fn stranger() -> AccountId {
        "stranger.testnet".parse().unwrap()
    }

    /// Окружение: вызывает `predecessor`, время NOW_MS, на балансе контракта 5 NEAR.
    fn set_context(predecessor: AccountId) {
        set_context_with(predecessor, NearToken::from_near(5), NearToken::from_near(0));
    }

    /// `balance` — баланс до зачисления депозита. Рантайм (и мок near-sdk) сам прибавляет
    /// `deposit`, поэтому `env::account_balance()` внутри вызова = `balance + deposit`.
    fn set_context_with(predecessor: AccountId, balance: NearToken, deposit: NearToken) {
        set_env(predecessor, NOW_MS, balance, deposit);
    }

    /// Окружение целиком: кто вызывает, время в мс, баланс до депозита, депозит.
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

    /// Вызов от `predecessor` в момент `now_ms`, баланс 5 NEAR.
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

    /// Как `new_goal`, но с произвольными строками, долей и окном.
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

    // SPEC 10, тест 1
    #[test]
    #[should_panic(expected = "Weights must sum to 10000")]
    fn new_rejects_weights_not_summing_to_10000() {
        set_context(creator());
        new_goal(verifier(), milestones(&[4000, 3000, 2000]));
    }

    // SPEC 10, тест 2
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
    #[should_panic(expected = "Window must be > 0")]
    fn new_rejects_zero_window() {
        set_context(creator());
        new_goal_with("Goal", "", 5000, 0, milestones(&[10_000]));
    }

    #[test]
    fn new_accepts_limits() {
        set_context(creator());
        // Длина считается в символах, не в байтах: «я» — 2 байта, но 1 символ.
        let mut ms = milestones(&[10_000]);
        ms[0].title = "я".repeat(100);
        ms[0].deadline_ms = NOW_MS + 1;
        let goal = new_goal_with(&"я".repeat(100), &"a".repeat(256), 10_000, 1, ms);
        assert_eq!(goal.get_goal().immediate_bps, 10_000);
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

    // SPEC 10, тест 3
    #[test]
    fn sync_ignores_initial_deposit() {
        set_context(creator());
        let mut goal = new_goal(verifier(), milestones(&[10_000]));
        goal.sync();
        assert_eq!(goal.total_received, NearToken::from_near(0));
        assert_eq!(goal.last_balance, NearToken::from_near(5));
        assert!(get_logs().is_empty());
    }

    #[test]
    fn sync_counts_incoming_transfer() {
        set_context(creator());
        let mut goal = new_goal(verifier(), milestones(&[10_000]));

        // На контракт пришло 10 NEAR обычным переводом.
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

        // Было 5 NEAR, приложено 2: внутри вызова баланс 7.
        set_context_with(stranger(), NearToken::from_near(5), NearToken::from_near(2));
        assert_eq!(env::account_balance(), NearToken::from_near(7));
        goal.deposit();
        assert_eq!(goal.total_received, NearToken::from_near(2));
        assert_eq!(goal.last_balance, NearToken::from_near(7));
    }

    // ---- Этапы (SPEC 6) ----

    const WINDOW_MS: u64 = 120_000;
    const DEADLINE_MS: u64 = NOW_MS + DAY_MS;
    // sha256 пустой строки.
    const HASH: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
    const URI: &str = "https://proofee.example/proof/1";

    /// Цель с этапами 40/30/30 и окном 2 минуты.
    fn goal3() -> Goal {
        set_context(creator());
        new_goal(verifier(), milestones(&[4000, 3000, 3000]))
    }

    fn submit_at(goal: &mut Goal, index: u8, now_ms: u64) {
        set_time(creator(), now_ms);
        goal.submit_proof(index, URI.to_string(), HASH.to_string());
    }

    /// Баланс стал 5 + `total_near` NEAR (всего пришло `total_near`) — и sync.
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

    // SPEC 10, тест 5
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
            HASH.to_uppercase(),           // верхний регистр
            HASH[..63].to_string(),        // 63 символа
            format!("{}g", &HASH[..63]),   // не hex
            format!("{HASH}0"),            // 65 символов
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

    // SPEC 10, тест 6
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

    // SPEC 10, тест 7
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

        // Заложено 5 NEAR, этап 40% → разлочено 2, доступно 5 + 2.
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

    // SPEC 10, тест 8
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

        // Заложено 5 NEAR, этап 30% → сгорело 1,5.
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

    #[test]
    fn rejected_share_burns_from_future_fees() {
        let mut goal = challenged(1);
        set_time(verifier(), NOW_MS + 2);
        goal.resolve(1, false);
        assert_eq!(goal.get_balances().forfeited, NearToken::from_millinear(1500));

        // После отказа пришло ещё 10 NEAR: всего 20, заложено 10.
        receive_and_sync(&mut goal, 20);
        let b = goal.get_balances();
        assert_eq!(b.total_received, NearToken::from_near(20));
        assert_eq!(b.locked, NearToken::from_near(10));
        assert_eq!(b.forfeited, NearToken::from_near(3));
        assert_eq!(b.pending, NearToken::from_near(7));
        assert_eq!(b.claimable, NearToken::from_near(10));
    }

    // SPEC 10, тест 9
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
}
