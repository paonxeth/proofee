//! Testnet Treasury v0.2. Trusted Goal decisions; segregated principal and funded rewards.
use near_sdk::collections::LookupMap;
use near_sdk::json_types::U128;
use near_sdk::{env, near, AccountId, Gas, NearToken, PanicOnDefault, Promise};
const BUFFER: u128 = 10u128.pow(23); // operator-funded 0.1 NEAR, never principal
const CALLBACK_GAS: Gas = Gas::from_tgas(10);
#[near(serializers=[borsh,json])]
#[derive(Clone, PartialEq, Debug)]
pub enum Status {
    Deposited,
    Bound,
    RefundPending,
    Refunded,
    Slashed,
    RefundRetryable,
}
#[near(serializers=[borsh,json])]
#[derive(Clone, PartialEq, Debug)]
pub enum Decision {
    QualificationRejected,
    ProofValid,
    ProofInvalidFinal,
    NeutralRefund,
}
#[near(serializers=[borsh,json])]
#[derive(Clone, PartialEq, Debug)]
pub enum Outcome {
    REFUND,
    SLASH,
}
#[near(serializers=[borsh,json])]
#[derive(Clone)]
pub struct Goal {
    pub authorized: bool,
    pub policy_version: String,
}
#[near(serializers=[borsh,json])]
#[derive(Clone)]
pub struct Bond {
    pub escrow_id: String,
    pub goal: AccountId,
    pub proof_id: String,
    pub case_id: String,
    pub submission_id: String,
    pub payer: AccountId,
    pub amount: U128,
    pub deposit_until_ns: u64,
    pub bind_until_ns: u64,
    pub refund_after_ns: u64,
    pub created_at_ns: Option<u64>,
    pub status: Option<Status>,
    pub settlement_id: Option<String>,
    pub outcome: Option<Outcome>,
    pub attempt: u64,
    pub reward_cap: U128,
    pub decision: Option<Decision>,
    pub demo: bool,
}
#[near(serializers=[borsh,json])]
#[derive(Clone, PartialEq, Debug)]
pub enum RewardStatus {
    Funded,
    PayPending,
    Paid,
    PayRetryable,
}
#[near(serializers=[borsh,json])]
#[derive(Clone)]
pub struct Reward {
    pub funding_id: String,
    pub escrow_id: String,
    pub goal: AccountId,
    pub beneficiary: AccountId,
    pub amount: U128,
    pub status: RewardStatus,
    pub settlement_id: Option<String>,
    pub attempt: u64,
}
#[near(serializers=[json])]
pub struct Intent {
    pub proof_id: String,
    pub case_id: String,
    pub submission_id: String,
    pub payer: AccountId,
    pub fee_snapshot: U128,
    pub amount: U128,
    pub reward_cap: U128,
    pub deposit_until_ns: u64,
    pub bind_until_ns: u64,
    pub refund_after_ns: u64,
}
#[near(serializers=[json])]
pub struct Liabilities {
    pub locked: U128,
    pub in_flight: U128,
    pub buyback: U128,
    pub reward_funding: U128,
    pub reward_in_flight: U128,
    pub storage_required: U128,
    pub operating_buffer: U128,
    pub account_balance: U128,
}
#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct Treasury {
    admin: AccountId,
    paused: bool,
    goals: LookupMap<AccountId, Goal>,
    bonds: LookupMap<String, Bond>,
    reserves: LookupMap<AccountId, u128>,
    settlements: LookupMap<String, String>,
    rewards: LookupMap<String, Reward>,
    funded_escrows: LookupMap<String, String>,
    proofs: LookupMap<String, String>,
    case_ids: LookupMap<String, String>,
    reward_locked: u128,
    reward_pending: u128,
    locked: u128,
    pending: u128,
    buyback: u128,
}
fn bounded(s: &str) {
    assert!(!s.is_empty() && s.len() <= 128, "ID_LENGTH");
}
fn key(parts: &[&str]) -> String {
    let bytes = near_sdk::serde_json::to_vec(parts).unwrap();
    env::sha256(&bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
#[near]
impl Treasury {
    #[init]
    pub fn new(admin: AccountId) -> Self {
        assert!(
            env::current_account_id().as_str().ends_with(".testnet"),
            "TESTNET_ONLY"
        );
        Self {
            admin,
            paused: false,
            goals: LookupMap::new(b"g"),
            bonds: LookupMap::new(b"b"),
            reserves: LookupMap::new(b"r"),
            settlements: LookupMap::new(b"s"),
            rewards: LookupMap::new(b"w"),
            funded_escrows: LookupMap::new(b"f"),
            proofs: LookupMap::new(b"p"),
            case_ids: LookupMap::new(b"c"),
            reward_locked: 0,
            reward_pending: 0,
            locked: 0,
            pending: 0,
            buyback: 0,
        }
    }
    pub fn register_goal(&mut self, goal: AccountId, policy_version: String) {
        self.admin();
        bounded(&policy_version);
        assert!(goal != env::current_account_id(), "SELF_GOAL");
        assert!(self.goals.get(&goal).is_none(), "GOAL_EXISTS");
        self.goals.insert(
            &goal,
            &Goal {
                authorized: true,
                policy_version,
            },
        );
        self.solvent();
    }
    pub fn set_goal_authorized(&mut self, goal: AccountId, authorized: bool) {
        self.admin();
        let mut g = self.goals.get(&goal).expect("UNKNOWN_GOAL");
        g.authorized = authorized;
        self.goals.insert(&goal, &g);
    }
    pub fn set_paused(&mut self, paused: bool) {
        self.admin();
        self.paused = paused;
    }
    /// Explicit demo authority. No caller-controlled amount/payer at deposit time.
    #[cfg(any(test, feature = "demo-registry"))]
    pub fn register_demo_case(
        &mut self,
        goal: AccountId,
        proof_id: String,
        case_id: String,
        submission_id: String,
        payer: AccountId,
        amount: U128,
        deposit_until_ns: u64,
        bind_until_ns: u64,
        refund_after_ns: u64,
    ) -> String {
        self.admin();
        self.authorized(&goal);
        bounded(&proof_id);
        bounded(&case_id);
        bounded(&submission_id);
        assert!(
            amount.0 >= 10u128.pow(24) && amount.0 <= 5 * 10u128.pow(24),
            "BOND_RANGE"
        );
        assert!(
            env::block_timestamp() < deposit_until_ns
                && deposit_until_ns < bind_until_ns
                && bind_until_ns < refund_after_ns,
            "DEADLINES"
        );
        let id = key(&[goal.as_str(), &case_id, &submission_id]);
        assert!(self.bonds.get(&id).is_none(), "CASE_EXISTS");
        self.bonds.insert(
            &id,
            &Bond {
                escrow_id: id.clone(),
                goal,
                proof_id,
                case_id,
                submission_id,
                payer,
                amount,
                deposit_until_ns,
                bind_until_ns,
                refund_after_ns,
                created_at_ns: None,
                status: None,
                settlement_id: None,
                outcome: None,
                attempt: 0,
                reward_cap: U128(0),
                decision: None,
                demo: true,
            },
        );
        self.solvent();
        id
    }
    #[payable]
    pub fn fund_storage(&mut self) {
        assert!(env::attached_deposit().as_yoctonear() > 0, "ZERO");
    }
    #[payable]
    pub fn deposit_bond(&mut self, escrow_id: String) {
        assert!(!self.paused, "PAUSED");
        let mut b = self.bond(&escrow_id);
        self.authorized(&b.goal);
        assert!(b.status.is_none(), "ALREADY_DEPOSITED");
        assert!(env::block_timestamp() < b.deposit_until_ns, "EXPIRED");
        assert_eq!(env::predecessor_account_id(), b.payer, "PAYER");
        assert_eq!(env::attached_deposit().as_yoctonear(), b.amount.0, "AMOUNT");
        b.status = Some(Status::Deposited);
        b.created_at_ns = Some(env::block_timestamp());
        self.locked += b.amount.0;
        self.bonds.insert(&escrow_id, &b);
        self.solvent();
        self.event("deposited", &b);
    }
    pub fn bind_bond(&mut self, escrow_id: String) {
        let mut b = self.bond(&escrow_id);
        self.goal_caller(&b);
        assert!(env::block_timestamp() < b.bind_until_ns, "BIND_EXPIRED");
        if b.status == Some(Status::Bound) {
            return;
        }
        assert!(b.status == Some(Status::Deposited), "STATE");
        b.status = Some(Status::Bound);
        self.bonds.insert(&escrow_id, &b);
        self.event("bound", &b);
    }
    pub fn settle_bond(&mut self, escrow_id: String, settlement_id: String, outcome: Outcome) {
        bounded(&settlement_id);
        let mut b = self.bond(&escrow_id);
        self.goal_caller(&b);
        if let Some(old) = &b.settlement_id {
            assert!(
                old == &settlement_id && b.outcome == Some(outcome),
                "CONFLICT"
            );
            return;
        }
        assert!(
            env::block_timestamp() < b.refund_after_ns,
            "SETTLEMENT_EXPIRED"
        );
        assert!(
            b.status == Some(Status::Bound)
                || (outcome == Outcome::REFUND && b.status == Some(Status::Deposited)),
            "STATE"
        );
        if !b.demo {
            assert!(
                match (&outcome, &b.decision) {
                    (Outcome::SLASH, Some(Decision::ProofValid)) => true,
                    (
                        Outcome::REFUND,
                        Some(
                            Decision::QualificationRejected
                            | Decision::ProofInvalidFinal
                            | Decision::NeutralRefund,
                        ),
                    ) => true,
                    _ => false,
                },
                "DECISION_REQUIRED"
            );
        }
        let sk = key(&[b.goal.as_str(), &settlement_id]);
        assert!(self.settlements.get(&sk).is_none(), "SETTLEMENT_REPLAY");
        self.settlements.insert(&sk, &escrow_id);
        b.settlement_id = Some(settlement_id);
        b.outcome = Some(outcome.clone());
        match outcome {
            Outcome::REFUND => {
                self.start_refund(b);
            }
            Outcome::SLASH => {
                self.locked -= b.amount.0;
                self.buyback += b.amount.0;
                let v = self.reserves.get(&b.goal).unwrap_or(0) + b.amount.0;
                self.reserves.insert(&b.goal, &v);
                b.status = Some(Status::Slashed);
                self.bonds.insert(&escrow_id, &b);
                self.solvent();
                self.event("slashed", &b);
            }
        }
    }
    /// Demo deadlines are explicit immutable policy, not approved production timeout values.
    pub fn refund_expired(&mut self, escrow_id: String) {
        let mut b = self.bond(&escrow_id);
        let now = env::block_timestamp();
        assert!(
            (b.status == Some(Status::Deposited) && now >= b.bind_until_ns)
                || (b.status == Some(Status::Bound) && now >= b.refund_after_ns),
            "NOT_EXPIRED"
        );
        b.decision = Some(Decision::NeutralRefund);
        b.outcome = Some(Outcome::REFUND);
        b.settlement_id = Some(format!("timeout:{}", b.escrow_id));
        self.start_refund(b);
    }
    pub fn retry_failed_refund(&mut self, escrow_id: String) {
        let b = self.bond(&escrow_id);
        assert!(b.status == Some(Status::RefundRetryable), "STATE");
        self.start_refund(b);
    }
    #[private]
    pub fn on_refund(&mut self, escrow_id: String, attempt: u64) -> bool {
        let mut b = self.bond(&escrow_id);
        if b.status != Some(Status::RefundPending) || b.attempt != attempt {
            return false;
        }
        assert_eq!(env::promise_results_count(), 1, "RESULT_COUNT");
        self.pending -= b.amount.0;
        let ok = env::promise_result_checked(0, 0).is_ok();
        b.status = Some(if ok {
            Status::Refunded
        } else {
            self.locked += b.amount.0;
            Status::RefundRetryable
        });
        self.bonds.insert(&escrow_id, &b);
        self.event(if ok { "refunded" } else { "refund_retryable" }, &b);
        ok
    }
    pub fn interface_version(&self) -> String {
        "proofee-treasury/0.2.0".into()
    }
    pub fn register_intent(&mut self, intent: Intent) -> String {
        assert!(!self.paused, "PAUSED");
        let goal = env::predecessor_account_id();
        self.authorized(&goal);
        bounded(&intent.proof_id);
        bounded(&intent.case_id);
        bounded(&intent.submission_id);
        let n = 10u128.pow(24);
        let b = (intent.fee_snapshot.0 / 5).max(n).min(5 * n);
        let r = (intent.fee_snapshot.0 / 10).min(b / 2).min(2 * n);
        assert!(
            intent.amount.0 == b && intent.reward_cap.0 == r,
            "ECONOMICS"
        );
        assert!(
            env::block_timestamp() < intent.deposit_until_ns
                && intent.deposit_until_ns < intent.bind_until_ns
                && intent.bind_until_ns < intent.refund_after_ns,
            "DEADLINES"
        );
        let id = key(&[goal.as_str(), &intent.case_id, &intent.submission_id]);
        let pk = key(&[goal.as_str(), &intent.proof_id]);
        if let Some(old) = self.bonds.get(&id) {
            assert!(
                !old.demo
                    && old.proof_id == intent.proof_id
                    && old.payer == intent.payer
                    && old.amount == intent.amount
                    && old.reward_cap == intent.reward_cap
                    && old.deposit_until_ns == intent.deposit_until_ns
                    && old.bind_until_ns == intent.bind_until_ns
                    && old.refund_after_ns == intent.refund_after_ns,
                "INTENT_CONFLICT"
            );
            return id;
        }
        assert!(self.proofs.get(&pk).is_none(), "PROOF_ALREADY_REGISTERED");
        let ck = key(&[goal.as_str(), &intent.case_id]);
        assert!(self.case_ids.get(&ck).is_none(), "CASE_ALREADY_REGISTERED");
        let bond = Bond {
            escrow_id: id.clone(),
            goal,
            proof_id: intent.proof_id,
            case_id: intent.case_id,
            submission_id: intent.submission_id,
            payer: intent.payer,
            amount: intent.amount,
            deposit_until_ns: intent.deposit_until_ns,
            bind_until_ns: intent.bind_until_ns,
            refund_after_ns: intent.refund_after_ns,
            created_at_ns: None,
            status: None,
            settlement_id: None,
            outcome: None,
            attempt: 0,
            reward_cap: intent.reward_cap,
            decision: None,
            demo: false,
        };
        self.proofs.insert(&pk, &id);
        self.case_ids.insert(&ck, &id);
        self.bonds.insert(&id, &bond);
        self.solvent();
        self.event("intent_registered", &bond);
        id
    }
    /// Goal attests qualification or final arbitration. Treasury cannot independently verify Goal state.
    pub fn authorize_decision(&mut self, escrow_id: String, decision: Decision) {
        let mut b = self.bond(&escrow_id);
        self.goal_caller(&b);
        assert!(
            env::block_timestamp() < b.refund_after_ns && b.outcome.is_none(),
            "DECISION_EXPIRED"
        );
        assert!(
            b.status == Some(Status::Bound)
                || (b.status == Some(Status::Deposited)
                    && decision == Decision::QualificationRejected),
            "STATE"
        );
        if let Some(old) = &b.decision {
            assert!(old == &decision, "DECISION_CONFLICT");
            return;
        }
        b.decision = Some(decision);
        self.bonds.insert(&escrow_id, &b);
        self.event("decision_authorized", &b);
    }
    #[payable]
    pub fn fund_reward(&mut self, escrow_id: String, funding_id: String) -> String {
        bounded(&funding_id);
        let b = self.bond(&escrow_id);
        self.goal_caller(&b);
        assert!(
            !b.demo
                && b.decision == Some(Decision::ProofInvalidFinal)
                && b.outcome == Some(Outcome::REFUND),
            "FINAL_FAILED_REQUIRED"
        );
        let amount = env::attached_deposit().as_yoctonear();
        assert!(amount > 0 && amount <= b.reward_cap.0, "REWARD_CAP");
        let fid = key(&[b.goal.as_str(), &funding_id]);
        assert!(
            self.rewards.get(&fid).is_none() && self.funded_escrows.get(&escrow_id).is_none(),
            "FUNDING_REPLAY"
        );
        let reward = Reward {
            funding_id: fid.clone(),
            escrow_id: escrow_id.clone(),
            goal: b.goal,
            beneficiary: b.payer,
            amount: U128(amount),
            status: RewardStatus::Funded,
            settlement_id: None,
            attempt: 0,
        };
        self.reward_locked += amount;
        self.funded_escrows.insert(&escrow_id, &fid);
        self.rewards.insert(&fid, &reward);
        self.solvent();
        self.reward_event("reward_funded", &reward);
        fid
    }
    pub fn pay_reward(&mut self, funding_id: String, settlement_id: String) {
        bounded(&settlement_id);
        let mut r = self.rewards.get(&funding_id).expect("UNKNOWN_FUNDING");
        assert_eq!(env::predecessor_account_id(), r.goal, "GOAL_CALLER");
        self.authorized(&r.goal);
        if let Some(old) = &r.settlement_id {
            assert_eq!(old, &settlement_id, "CONFLICT");
            return;
        }
        let b = self.bond(&r.escrow_id);
        assert!(
            b.status == Some(Status::Refunded) && b.decision == Some(Decision::ProofInvalidFinal),
            "BOND_REFUND_REQUIRED"
        );
        let sk = key(&[r.goal.as_str(), &settlement_id]);
        assert!(self.settlements.get(&sk).is_none(), "SETTLEMENT_REPLAY");
        self.settlements.insert(&sk, &r.escrow_id);
        r.settlement_id = Some(settlement_id);
        self.start_reward(r);
    }
    pub fn retry_failed_reward(&mut self, funding_id: String) {
        let r = self.rewards.get(&funding_id).expect("UNKNOWN_FUNDING");
        assert!(r.status == RewardStatus::PayRetryable, "STATE");
        self.start_reward(r);
    }
    #[private]
    pub fn on_reward(&mut self, funding_id: String, attempt: u64) -> bool {
        let mut r = self.rewards.get(&funding_id).expect("UNKNOWN_FUNDING");
        if r.status != RewardStatus::PayPending || r.attempt != attempt {
            return false;
        }
        assert_eq!(env::promise_results_count(), 1, "RESULT_COUNT");
        self.reward_pending -= r.amount.0;
        let ok = env::promise_result_checked(0, 0).is_ok();
        r.status = if ok {
            RewardStatus::Paid
        } else {
            self.reward_locked += r.amount.0;
            RewardStatus::PayRetryable
        };
        self.rewards.insert(&funding_id, &r);
        self.reward_event(
            if ok {
                "reward_paid"
            } else {
                "reward_retryable"
            },
            &r,
        );
        ok
    }
    pub fn get_reward(&self, funding_id: String) -> Option<Reward> {
        self.rewards.get(&funding_id)
    }
    pub fn get_case_funding(&self, escrow_id: String) -> Option<String> {
        self.funded_escrows.get(&escrow_id)
    }
    pub fn get_bond(&self, escrow_id: String) -> Option<Bond> {
        self.bonds.get(&escrow_id)
    }
    pub fn get_goal(&self, goal: AccountId) -> Option<Goal> {
        self.goals.get(&goal)
    }
    pub fn get_buyback_balance(&self, goal: AccountId) -> U128 {
        U128(self.reserves.get(&goal).unwrap_or(0))
    }
    pub fn get_liabilities(&self) -> Liabilities {
        Liabilities {
            locked: U128(self.locked),
            in_flight: U128(self.pending),
            buyback: U128(self.buyback),
            reward_funding: U128(self.reward_locked),
            reward_in_flight: U128(self.reward_pending),
            storage_required: U128(
                env::storage_usage() as u128 * env::storage_byte_cost().as_yoctonear(),
            ),
            operating_buffer: U128(BUFFER),
            account_balance: U128(env::account_balance().as_yoctonear()),
        }
    }
}
impl Treasury {
    fn admin(&self) {
        assert_eq!(env::predecessor_account_id(), self.admin, "ADMIN");
    }
    fn authorized(&self, g: &AccountId) {
        assert!(
            self.goals.get(g).is_some_and(|v| v.authorized),
            "UNAUTHORIZED_GOAL"
        );
    }
    fn goal_caller(&self, b: &Bond) {
        assert_eq!(env::predecessor_account_id(), b.goal, "GOAL_CALLER");
        self.authorized(&b.goal);
    }
    fn bond(&self, id: &String) -> Bond {
        self.bonds.get(id).expect("UNKNOWN_ESCROW")
    }
    fn solvent(&self) {
        let l = self.get_liabilities();
        assert!(
            l.account_balance.0
                >= self.locked
                    + self.pending
                    + self.buyback
                    + self.reward_locked
                    + self.reward_pending
                    + l.storage_required.0
                    + BUFFER,
            "FUND_STORAGE_SEPARATELY"
        );
    }
    fn start_refund(&mut self, mut b: Bond) {
        assert!(
            env::prepaid_gas().as_gas() - env::used_gas().as_gas() >= Gas::from_tgas(25).as_gas(),
            "REFUND_GAS"
        );
        self.solvent();
        self.locked -= b.amount.0;
        self.pending += b.amount.0;
        b.attempt = b.attempt.checked_add(1).expect("ATTEMPT_OVERFLOW");
        b.status = Some(Status::RefundPending);
        self.bonds.insert(&b.escrow_id, &b);
        self.solvent();
        self.event("refund_pending", &b);
        // State is persisted before scheduling. Only a private, attempt-matched callback can finalize.
        Promise::new(b.payer.clone())
            .transfer(NearToken::from_yoctonear(b.amount.0))
            .then(
                Promise::new(env::current_account_id()).function_call(
                    "on_refund",
                    near_sdk::serde_json::to_vec(
                        &near_sdk::serde_json::json!({"escrow_id":b.escrow_id,"attempt":b.attempt}),
                    )
                    .unwrap(),
                    NearToken::from_yoctonear(0),
                    CALLBACK_GAS,
                ),
            )
            .detach();
    }
    fn start_reward(&mut self, mut r: Reward) {
        assert!(
            env::prepaid_gas().as_gas() - env::used_gas().as_gas() >= Gas::from_tgas(25).as_gas(),
            "REWARD_GAS"
        );
        self.solvent();
        self.reward_locked -= r.amount.0;
        self.reward_pending += r.amount.0;
        r.attempt = r.attempt.checked_add(1).expect("ATTEMPT_OVERFLOW");
        r.status = RewardStatus::PayPending;
        self.rewards.insert(&r.funding_id, &r);
        self.solvent();
        self.reward_event("reward_pending", &r);
        Promise::new(r.beneficiary.clone()).transfer(NearToken::from_yoctonear(r.amount.0)).then(Promise::new(env::current_account_id()).function_call("on_reward",near_sdk::serde_json::to_vec(&near_sdk::serde_json::json!({"funding_id":r.funding_id,"attempt":r.attempt})).unwrap(),NearToken::from_yoctonear(0),CALLBACK_GAS)).detach();
    }
    fn reward_event(&self, name: &str, r: &Reward) {
        env::log_str(&format!(
            "EVENT_JSON:{}",
            near_sdk::serde_json::json!({"standard":"proofee_bond","version":"0.2.0","event":name,"data":[r]})
        ));
    }
    fn event(&self, name: &str, b: &Bond) {
        env::log_str(&format!(
            "EVENT_JSON:{}",
            near_sdk::serde_json::json!({"standard":"proofee_bond","version":"0.2.0","event":name,"data":[b]})
        ));
    }
}
#[cfg(test)]
mod tests;
