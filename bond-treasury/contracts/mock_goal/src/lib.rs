//! TEST ONLY: centralized simulated qualifications/verdicts, never production Goal.
use near_sdk::collections::LookupMap;
use near_sdk::json_types::U128;
use near_sdk::serde_json::{json, Value};
use near_sdk::{env, near, AccountId, Gas, NearToken, PanicOnDefault, Promise};
#[near(serializers=[borsh,json])]
#[derive(Clone)]
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
#[near(serializers=[borsh,json])]
#[derive(Clone, PartialEq, Debug)]
pub enum Stage {
    RegisterPending,
    RegisterRetryable,
    Ready,
    VerifyPending,
    BindPending,
    BindRetryable,
    Bound,
    Qualified,
    DecisionPending,
    DecisionRetryable,
    SettlementPending,
    SettlementRetryable,
    SettlementAccepted,
    Refunded,
    Slashed,
    FundingPending,
    FundingRetryable,
    Funded,
    PayPending,
    PayRetryable,
    Paid,
    RefreshPending,
}
#[near(serializers=[borsh,json])]
#[derive(Clone)]
pub struct Case {
    pub intent: Intent,
    pub escrow_id: Option<String>,
    pub stage: Stage,
    pub decision: Option<String>,
    pub attempt: u64,
    pub funding_id: Option<String>,
    pub reward_amount: U128,
}
#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct MockGoal {
    admin: AccountId,
    verifier: AccountId,
    treasury: AccountId,
    cases: LookupMap<String, Case>,
}
#[near]
impl MockGoal {
    #[init]
    pub fn new(admin: AccountId, verifier: AccountId, treasury: AccountId) -> Self {
        assert!(
            env::current_account_id().as_str().ends_with(".testnet"),
            "TESTNET_ONLY"
        );
        Self {
            admin,
            verifier,
            treasury,
            cases: LookupMap::new(b"c"),
        }
    }
    /// Admin supplies the simulated fee snapshot; payer never supplies economic truth to Treasury.
    pub fn prepare_challenge(&mut self, intent: Intent) -> Promise {
        self.admin();
        assert!(
            intent.case_id.len() <= 128 && !intent.case_id.is_empty(),
            "CASE_ID"
        );
        assert!(self.cases.get(&intent.case_id).is_none(), "CASE_EXISTS");
        let id = intent.case_id.clone();
        let c = Case {
            intent,
            escrow_id: None,
            stage: Stage::RegisterPending,
            decision: None,
            attempt: 1,
            funding_id: None,
            reward_amount: U128(0),
        };
        self.cases.insert(&id, &c);
        self.request("register_intent", json!({"intent":c.intent}), 0, 35)
            .then(self.cb("on_registered", &id, 1, 15))
    }
    pub fn retry_registration(&mut self, case_id: String) -> Promise {
        self.admin();
        let mut c = self.case(&case_id);
        assert!(c.stage == Stage::RegisterRetryable, "STATE");
        c.attempt += 1;
        c.stage = Stage::RegisterPending;
        self.cases.insert(&case_id, &c);
        self.request("register_intent", json!({"intent":c.intent}), 0, 35)
            .then(self.cb("on_registered", &case_id, c.attempt, 15))
    }
    #[private]
    pub fn on_registered(&mut self, case_id: String, attempt: u64) -> bool {
        let mut c = self.case(&case_id);
        if c.stage != Stage::RegisterPending || c.attempt != attempt {
            return false;
        }
        if let Some(v) = result() {
            if let Ok(id) = near_sdk::serde_json::from_slice::<String>(&v) {
                c.escrow_id = Some(id);
                c.stage = Stage::Ready;
            } else {
                c.stage = Stage::RegisterRetryable;
            }
        } else {
            c.stage = Stage::RegisterRetryable;
        }
        let ok = c.stage == Stage::Ready;
        self.cases.insert(&case_id, &c);
        ok
    }
    pub fn confirm_escrow(&mut self, case_id: String) -> Promise {
        let mut c = self.case(&case_id);
        assert!(
            env::predecessor_account_id() == c.intent.payer
                || env::predecessor_account_id() == self.admin,
            "PAYER"
        );
        assert!(
            c.stage == Stage::Ready || c.stage == Stage::BindRetryable,
            "STATE"
        );
        c.attempt += 1;
        c.stage = Stage::VerifyPending;
        self.cases.insert(&case_id, &c);
        self.request("get_bond", json!({"escrow_id":c.escrow_id}), 0, 15)
            .then(self.cb("on_verified", &case_id, c.attempt, 80))
    }
    #[private]
    pub fn on_verified(&mut self, case_id: String, attempt: u64) -> Promise {
        let mut c = self.case(&case_id);
        assert!(
            c.stage == Stage::VerifyPending && c.attempt == attempt,
            "STALE"
        );
        let valid = result()
            .and_then(|b| near_sdk::serde_json::from_slice::<Value>(&b).ok())
            .is_some_and(|b| {
                b["goal"] == json!(env::current_account_id())
                    && b["payer"] == json!(c.intent.payer)
                    && b["proof_id"] == json!(c.intent.proof_id)
                    && b["case_id"] == json!(c.intent.case_id)
                    && b["submission_id"] == json!(c.intent.submission_id)
                    && b["amount"] == json!(c.intent.amount)
                    && (b["status"] == "Deposited" || b["status"] == "Bound")
            });
        if !valid {
            c.stage = Stage::BindRetryable;
            self.cases.insert(&case_id, &c);
            return Promise::new(env::current_account_id());
        }
        c.stage = Stage::BindPending;
        self.cases.insert(&case_id, &c);
        self.request("bind_bond", json!({"escrow_id":c.escrow_id}), 0, 35)
            .then(self.cb("on_bound", &case_id, attempt, 15))
    }
    #[private]
    pub fn on_bound(&mut self, case_id: String, attempt: u64) -> bool {
        let mut c = self.case(&case_id);
        if c.stage != Stage::BindPending || c.attempt != attempt {
            return false;
        }
        let ok = result().is_some();
        c.stage = if ok {
            Stage::Bound
        } else {
            Stage::BindRetryable
        };
        self.cases.insert(&case_id, &c);
        ok
    }
    pub fn qualify(&mut self, case_id: String, qualified: bool) -> Option<Promise> {
        self.admin();
        let mut c = self.case(&case_id);
        assert!(c.stage == Stage::Bound, "STATE");
        if qualified {
            c.stage = Stage::Qualified;
            self.cases.insert(&case_id, &c);
            None
        } else {
            Some(self.decide(case_id, "QualificationRejected".into()))
        }
    }
    pub fn arbitrate(&mut self, case_id: String, valid: bool) -> Promise {
        assert_eq!(env::predecessor_account_id(), self.verifier, "VERIFIER");
        let c = self.case(&case_id);
        assert!(c.stage == Stage::Qualified, "STATE");
        assert!(c.intent.payer != self.verifier, "SELF_JUDGING");
        self.decide(
            case_id,
            if valid {
                "ProofValid"
            } else {
                "ProofInvalidFinal"
            }
            .into(),
        )
    }
    pub fn retry_decision(&mut self, case_id: String) -> Promise {
        self.admin();
        let c = self.case(&case_id);
        assert!(c.stage == Stage::DecisionRetryable, "STATE");
        self.decide(case_id, c.decision.unwrap())
    }
    #[private]
    pub fn on_decided(&mut self, case_id: String, attempt: u64) -> Option<Promise> {
        let mut c = self.case(&case_id);
        if c.stage != Stage::DecisionPending || c.attempt != attempt {
            return None;
        }
        if result().is_none() {
            c.stage = Stage::DecisionRetryable;
            self.cases.insert(&case_id, &c);
            None
        } else {
            Some(self.settle(case_id, c))
        }
    }
    pub fn retry_settlement(&mut self, case_id: String) -> Promise {
        self.admin();
        let c = self.case(&case_id);
        assert!(c.stage == Stage::SettlementRetryable, "STATE");
        self.settle(case_id, c)
    }
    #[private]
    pub fn on_settled(&mut self, case_id: String, attempt: u64) -> bool {
        let mut c = self.case(&case_id);
        if c.stage != Stage::SettlementPending || c.attempt != attempt {
            return false;
        }
        let ok = result().is_some();
        c.stage = if ok {
            Stage::SettlementAccepted
        } else {
            Stage::SettlementRetryable
        };
        self.cases.insert(&case_id, &c);
        ok
    }
    /// Neutral timeout is enforced by Treasury timestamps. No attestation of truth.
    pub fn resolve_timeout(&mut self, case_id: String) -> Promise {
        let c = self.case(&case_id);
        self.request("refund_expired", json!({"escrow_id":c.escrow_id}), 0, 60)
    }
    #[payable]
    pub fn fund_mock(&mut self) {
        self.admin();
        assert!(env::attached_deposit().as_yoctonear() > 0, "ZERO");
    }
    /// Uses only admin-endowed mock funds to simulate real final-forfeited reserve.
    pub fn fund_case(&mut self, case_id: String, amount: U128) -> Promise {
        self.admin();
        let mut c = self.case(&case_id);
        assert!(
            c.stage == Stage::Refunded || c.stage == Stage::FundingRetryable,
            "STATE"
        );
        assert!(
            c.decision.as_deref() == Some("ProofInvalidFinal"),
            "FINAL_FAILED_REQUIRED"
        );
        assert!(amount.0 > 0 && amount.0 <= c.intent.reward_cap.0, "CAP");
        let storage =
            env::storage_usage() as u128 * env::storage_byte_cost().as_yoctonear() + 10u128.pow(24);
        assert!(
            env::account_balance().as_yoctonear() >= storage + amount.0,
            "MOCK_BALANCE"
        );
        c.attempt += 1;
        c.stage = Stage::FundingPending;
        c.reward_amount = amount;
        self.cases.insert(&case_id, &c);
        self.request(
            "fund_reward",
            json!({"escrow_id":c.escrow_id,"funding_id":format!("fund:{}",case_id)}),
            amount.0,
            40,
        )
        .then(self.cb("on_funded", &case_id, c.attempt, 15))
    }
    #[private]
    pub fn on_funded(&mut self, case_id: String, attempt: u64) -> bool {
        let mut c = self.case(&case_id);
        if c.stage != Stage::FundingPending || c.attempt != attempt {
            return false;
        }
        if let Some(v) = result() {
            if let Ok(id) = near_sdk::serde_json::from_slice::<String>(&v) {
                c.funding_id = Some(id);
                c.stage = Stage::Funded;
            } else {
                c.stage = Stage::FundingRetryable;
            }
        } else {
            c.stage = Stage::FundingRetryable;
        }
        let ok = c.stage == Stage::Funded;
        self.cases.insert(&case_id, &c);
        ok
    }
    /// Reconcile a lost funding callback using authoritative escrow->funding mapping.
    pub fn recover_funding(&self, case_id: String) -> Promise {
        let c = self.case(&case_id);
        assert!(
            matches!(c.stage, Stage::FundingPending | Stage::FundingRetryable),
            "STATE"
        );
        self.request("get_case_funding", json!({"escrow_id":c.escrow_id}), 0, 15)
            .then(self.cb("on_recovered_funding", &case_id, c.attempt, 15))
    }
    #[private]
    pub fn on_recovered_funding(&mut self, case_id: String, attempt: u64) -> bool {
        let mut c = self.case(&case_id);
        if c.attempt != attempt
            || !matches!(c.stage, Stage::FundingPending | Stage::FundingRetryable)
        {
            return false;
        }
        let fid = result()
            .and_then(|b| near_sdk::serde_json::from_slice::<Option<String>>(&b).ok())
            .flatten();
        if let Some(id) = fid {
            c.funding_id = Some(id);
            c.stage = Stage::Funded;
            self.cases.insert(&case_id, &c);
            true
        } else {
            false
        }
    }
    pub fn claim_reward(&mut self, case_id: String) -> Promise {
        let mut c = self.case(&case_id);
        assert!(
            env::predecessor_account_id() == c.intent.payer
                || env::predecessor_account_id() == self.admin,
            "PAYER"
        );
        assert!(
            c.stage == Stage::Funded || c.stage == Stage::PayRetryable,
            "STATE"
        );
        c.attempt += 1;
        c.stage = Stage::PayPending;
        self.cases.insert(&case_id, &c);
        self.request(
            "pay_reward",
            json!({"funding_id":c.funding_id,"settlement_id":format!("reward:{}",case_id)}),
            0,
            60,
        )
        .then(self.cb("on_pay_accepted", &case_id, c.attempt, 15))
    }
    #[private]
    pub fn on_pay_accepted(&mut self, case_id: String, attempt: u64) -> bool {
        let mut c = self.case(&case_id);
        if c.stage != Stage::PayPending || c.attempt != attempt {
            return false;
        }
        let ok = result().is_some();
        c.stage = if ok {
            Stage::Funded
        } else {
            Stage::PayRetryable
        };
        self.cases.insert(&case_id, &c);
        ok
    }
    /// View authoritative bond; does not permit new financial instructions by itself.
    pub fn refresh_bond(&self, case_id: String) -> Promise {
        let c = self.case(&case_id);
        self.request("get_bond", json!({"escrow_id":c.escrow_id}), 0, 15)
            .then(self.cb("on_refresh_bond", &case_id, c.attempt, 15))
    }
    #[private]
    pub fn on_refresh_bond(&mut self, case_id: String, attempt: u64) -> bool {
        let mut c = self.case(&case_id);
        if c.attempt != attempt {
            return false;
        }
        if !matches!(
            c.stage,
            Stage::SettlementAccepted
                | Stage::SettlementRetryable
                | Stage::Qualified
                | Stage::Bound
                | Stage::DecisionRetryable
                | Stage::DecisionPending
                | Stage::Ready
                | Stage::BindRetryable
                | Stage::RegisterPending
        ) {
            return false;
        }
        let b = result()
            .and_then(|b| near_sdk::serde_json::from_slice::<Value>(&b).ok())
            .unwrap_or(Value::Null);
        let st = match b["status"].as_str() {
            Some("Refunded") => Stage::Refunded,
            Some("Slashed") => Stage::Slashed,
            _ => return false,
        };
        if b["decision"] == "NeutralRefund" {
            c.decision = Some("NeutralRefund".into());
        }
        c.stage = st;
        self.cases.insert(&case_id, &c);
        true
    }
    pub fn refresh_reward(&self, case_id: String) -> Promise {
        let c = self.case(&case_id);
        self.request("get_reward", json!({"funding_id":c.funding_id}), 0, 15)
            .then(self.cb("on_refresh_reward", &case_id, c.attempt, 15))
    }
    #[private]
    pub fn on_refresh_reward(&mut self, case_id: String, attempt: u64) -> bool {
        let mut c = self.case(&case_id);
        if c.attempt != attempt || !matches!(c.stage, Stage::Funded | Stage::PayRetryable) {
            return false;
        }
        let r = result()
            .and_then(|b| near_sdk::serde_json::from_slice::<Value>(&b).ok())
            .unwrap_or(Value::Null);
        if r["status"] != "Paid" {
            return false;
        }
        c.stage = Stage::Paid;
        self.cases.insert(&case_id, &c);
        true
    }
    pub fn get_case(&self, case_id: String) -> Option<Case> {
        self.cases.get(&case_id)
    }
}
impl MockGoal {
    fn admin(&self) {
        assert_eq!(env::predecessor_account_id(), self.admin, "ADMIN");
    }
    fn case(&self, id: &String) -> Case {
        self.cases.get(id).expect("UNKNOWN_CASE")
    }
    fn request(&self, method: &str, args: Value, amount: u128, gas: u64) -> Promise {
        Promise::new(self.treasury.clone()).function_call(
            method,
            near_sdk::serde_json::to_vec(&args).unwrap(),
            NearToken::from_yoctonear(amount),
            Gas::from_tgas(gas),
        )
    }
    fn cb(&self, method: &str, id: &str, attempt: u64, gas: u64) -> Promise {
        Promise::new(env::current_account_id()).function_call(
            method,
            near_sdk::serde_json::to_vec(&json!({"case_id":id,"attempt":attempt})).unwrap(),
            NearToken::from_yoctonear(0),
            Gas::from_tgas(gas),
        )
    }
    fn decide(&mut self, id: String, decision: String) -> Promise {
        let mut c = self.case(&id);
        assert!(env::block_timestamp() < c.intent.refund_after_ns, "EXPIRED");
        c.attempt += 1;
        c.stage = Stage::DecisionPending;
        c.decision = Some(decision.clone());
        self.cases.insert(&id, &c);
        self.request(
            "authorize_decision",
            json!({"escrow_id":c.escrow_id,"decision":decision}),
            0,
            35,
        )
        .then(self.cb("on_decided", &id, c.attempt, 85))
    }
    fn settle(&mut self, id: String, mut c: Case) -> Promise {
        c.stage = Stage::SettlementPending;
        let outcome = if c.decision.as_deref() == Some("ProofValid") {
            "SLASH"
        } else {
            "REFUND"
        };
        self.cases.insert(&id, &c);
        self.request("settle_bond",json!({"escrow_id":c.escrow_id,"settlement_id":format!("bond:{}",id),"outcome":outcome}),0,60).then(self.cb("on_settled",&id,c.attempt,15))
    }
}
fn result() -> Option<Vec<u8>> {
    assert_eq!(env::promise_results_count(), 1, "RESULT_COUNT");
    env::promise_result_checked(0, 16_384).ok()
}
#[cfg(test)]
mod tests {
    use super::*;
    use near_sdk::test_utils::VMContextBuilder;
    use near_sdk::{test_vm_config, testing_env, PromiseResult, RuntimeFeesConfig};
    use std::collections::HashMap;
    fn a(s: &str) -> AccountId {
        s.parse().unwrap()
    }
    fn context(who: &str) -> VMContextBuilder {
        let mut b = VMContextBuilder::new();
        b.current_account_id(a("mock.testnet"))
            .predecessor_account_id(a(who))
            .prepaid_gas(Gas::from_tgas(300))
            .account_balance(NearToken::from_near(100));
        b
    }
    fn set(who: &str) {
        testing_env!(context(who).build());
    }
    fn setup() -> MockGoal {
        set("admin.testnet");
        MockGoal::new(
            a("admin.testnet"),
            a("verifier.testnet"),
            a("treasury.testnet"),
        )
    }
    fn intent() -> Intent {
        Intent {
            proof_id: "p".into(),
            case_id: "c".into(),
            submission_id: "0".into(),
            payer: a("payer.testnet"),
            fee_snapshot: U128(0),
            amount: U128(10u128.pow(24)),
            reward_cap: U128(0),
            deposit_until_ns: 10,
            bind_until_ns: 20,
            refund_after_ns: 30,
        }
    }
    fn store(m: &mut MockGoal, stage: Stage) {
        m.cases.insert(
            &"c".into(),
            &Case {
                intent: intent(),
                escrow_id: Some("e".into()),
                stage,
                decision: None,
                attempt: 1,
                funding_id: None,
                reward_amount: U128(0),
            },
        );
    }
    fn callback(r: PromiseResult) {
        testing_env!(
            context("mock.testnet").build(),
            test_vm_config(),
            RuntimeFeesConfig::test(),
            HashMap::default(),
            vec![r]
        );
    }
    #[test]
    fn prepare_callback_retry() {
        let mut m = setup();
        m.prepare_challenge(intent()).detach();
        callback(PromiseResult::Failed);
        assert!(!m.on_registered("c".into(), 1));
        assert_eq!(m.case(&"c".into()).stage, Stage::RegisterRetryable);
        set("admin.testnet");
        m.retry_registration("c".into()).detach();
        callback(PromiseResult::Successful(b"\"escrow\"".to_vec()));
        assert!(!m.on_registered("c".into(), 1));
        assert!(m.on_registered("c".into(), 2));
        assert_eq!(m.case(&"c".into()).stage, Stage::Ready);
    }
    #[test]
    #[should_panic(expected = "ADMIN")]
    fn unauthorized_prepare() {
        let mut m = setup();
        set("evil.testnet");
        m.prepare_challenge(intent()).detach();
    }
    #[test]
    #[should_panic(expected = "CASE_EXISTS")]
    fn duplicate_prepare() {
        let mut m = setup();
        m.prepare_challenge(intent()).detach();
        m.prepare_challenge(intent()).detach();
    }
    #[test]
    fn qualification_and_bound_callback() {
        let mut m = setup();
        store(&mut m, Stage::BindPending);
        callback(PromiseResult::Successful(vec![]));
        assert!(!m.on_bound("c".into(), 0));
        assert!(m.on_bound("c".into(), 1));
        set("admin.testnet");
        assert!(m.qualify("c".into(), true).is_none());
        assert_eq!(m.case(&"c".into()).stage, Stage::Qualified);
    }
    #[test]
    #[should_panic(expected = "ADMIN")]
    fn unauthorized_qualify() {
        let mut m = setup();
        store(&mut m, Stage::Bound);
        set("evil.testnet");
        m.qualify("c".into(), true);
    }
    #[test]
    #[should_panic(expected = "VERIFIER")]
    fn unauthorized_arbitrate() {
        let mut m = setup();
        store(&mut m, Stage::Qualified);
        set("evil.testnet");
        m.arbitrate("c".into(), true).detach();
    }
    #[test]
    #[should_panic(expected = "PAYER")]
    fn unauthorized_bind() {
        let mut m = setup();
        store(&mut m, Stage::Ready);
        set("evil.testnet");
        m.confirm_escrow("c".into()).detach();
    }
    #[test]
    fn failed_verification() {
        let mut m = setup();
        store(&mut m, Stage::VerifyPending);
        callback(PromiseResult::Failed);
        m.on_verified("c".into(), 1).detach();
        assert_eq!(m.case(&"c".into()).stage, Stage::BindRetryable);
    }
    #[test]
    fn decision_and_settlement_callbacks() {
        let mut m = setup();
        store(&mut m, Stage::DecisionPending);
        callback(PromiseResult::Failed);
        assert!(m.on_decided("c".into(), 1).is_none());
        assert_eq!(m.case(&"c".into()).stage, Stage::DecisionRetryable);
        store(&mut m, Stage::SettlementPending);
        callback(PromiseResult::Failed);
        assert!(!m.on_settled("c".into(), 1));
        assert_eq!(m.case(&"c".into()).stage, Stage::SettlementRetryable);
    }
    #[test]
    fn funding_callback_and_replay() {
        let mut m = setup();
        store(&mut m, Stage::FundingPending);
        callback(PromiseResult::Successful(b"\"f\"".to_vec()));
        assert!(m.on_funded("c".into(), 1));
        assert!(!m.on_funded("c".into(), 1));
        assert_eq!(m.case(&"c".into()).funding_id, Some("f".into()));
    }
    #[test]
    #[should_panic(expected = "ADMIN")]
    fn unauthorized_mock_funding() {
        let mut m = setup();
        set("evil.testnet");
        m.fund_mock();
    }
    #[test]
    #[should_panic(expected = "FINAL_FAILED_REQUIRED")]
    fn neutral_not_rewardable() {
        let mut m = setup();
        store(&mut m, Stage::Refunded);
        m.fund_case("c".into(), U128(1)).detach();
    }
    #[test]
    #[should_panic(expected = "PAYER")]
    fn unauthorized_claim() {
        let mut m = setup();
        store(&mut m, Stage::Funded);
        set("evil.testnet");
        m.claim_reward("c".into()).detach();
    }
    #[test]
    fn refresh_timeout() {
        let mut m = setup();
        store(&mut m, Stage::Qualified);
        callback(PromiseResult::Successful(
            br#"{"status":"Refunded","decision":"NeutralRefund"}"#.to_vec(),
        ));
        assert!(m.on_refresh_bond("c".into(), 1));
        assert_eq!(m.case(&"c".into()).decision, Some("NeutralRefund".into()));
    }
    #[test]
    fn refresh_reward_and_stale() {
        let mut m = setup();
        store(&mut m, Stage::Funded);
        callback(PromiseResult::Successful(br#"{"status":"Paid"}"#.to_vec()));
        assert!(!m.on_refresh_reward("c".into(), 0));
        assert!(m.on_refresh_reward("c".into(), 1));
        assert_eq!(m.case(&"c".into()).stage, Stage::Paid);
    }
    #[test]
    fn recover_funding_callback() {
        let mut m = setup();
        store(&mut m, Stage::FundingPending);
        callback(PromiseResult::Successful(b"\"funding-hash\"".to_vec()));
        assert!(m.on_recovered_funding("c".into(), 1));
        assert_eq!(m.case(&"c".into()).stage, Stage::Funded);
        assert!(!m.on_recovered_funding("c".into(), 1));
    }
    #[test]
    fn retry_and_view_routes() {
        let mut m = setup();
        store(&mut m, Stage::DecisionRetryable);
        let mut c = m.case(&"c".into());
        c.decision = Some("ProofValid".into());
        m.cases.insert(&"c".into(), &c);
        m.retry_decision("c".into()).detach();
        store(&mut m, Stage::SettlementRetryable);
        m.retry_settlement("c".into()).detach();
        set("any.testnet");
        m.resolve_timeout("c".into()).detach();
        m.refresh_bond("c".into()).detach();
        assert!(m.get_case("c".into()).is_some());
    }
    #[test]
    fn funding_and_claim_routes() {
        let mut m = setup();
        let mut b = context("admin.testnet");
        b.attached_deposit(NearToken::from_near(1));
        testing_env!(b.build());
        m.fund_mock();
        store(&mut m, Stage::Refunded);
        let mut c = m.case(&"c".into());
        c.decision = Some("ProofInvalidFinal".into());
        c.intent.reward_cap = U128(10u128.pow(24));
        m.cases.insert(&"c".into(), &c);
        set("admin.testnet");
        m.fund_case("c".into(), U128(1)).detach();
        m.recover_funding("c".into()).detach();
        store(&mut m, Stage::Funded);
        let mut c = m.case(&"c".into());
        c.funding_id = Some("f".into());
        m.cases.insert(&"c".into(), &c);
        set("payer.testnet");
        m.claim_reward("c".into()).detach();
        callback(PromiseResult::Failed);
        assert!(!m.on_pay_accepted("c".into(), 2));
        set("any.testnet");
        m.refresh_reward("c".into()).detach();
    }
}
