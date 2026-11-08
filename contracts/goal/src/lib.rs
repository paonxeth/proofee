// Контракт цели ProoFee. Спецификация — docs/SPEC.md.
use near_sdk::serde_json::json;
use near_sdk::{AccountId, NearToken, PanicOnDefault, env, near, require};

mod events;
mod math;

use math::BPS_DENOMINATOR;

const MAX_MILESTONES: usize = 5;
const MAX_TITLE_LEN: usize = 100;
const MAX_URI_LEN: usize = 256;

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

impl Goal {
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
        let mut builder = VMContextBuilder::new();
        builder
            .current_account_id("goal.testnet".parse().unwrap())
            .predecessor_account_id(predecessor)
            .block_timestamp(NOW_MS * 1_000_000)
            .account_balance(balance)
            .attached_deposit(deposit);
        testing_env!(builder.build());
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
}
