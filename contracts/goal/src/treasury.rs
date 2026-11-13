// Challenge with a bond through Bond Treasury v0.2.0 (SPEC 6a).
// The orchestration follows bond-treasury/contracts/mock_goal: the admin is replaced by the
// verifier (qualify/arbitrate) and by the challenger (prepare/confirm); retries are open to anyone.
use near_sdk::json_types::U128;
use near_sdk::serde_json::{Value, json};
use near_sdk::{AccountId, Gas, NearToken, Promise, PromiseOrValue, env, near, require};

use crate::math::{self, bps_share};
use crate::{
    Goal, GoalExt, MAX_REASON_LEN, MilestoneStatus, RESOLVE_TIMEOUT_MS, emit_resolved, events,
    window_end,
};

/// Time to deposit the bond after `prepare_challenge`, and then to bind it (SPEC 6a).
#[cfg(not(feature = "short-timeouts"))]
pub const DEPOSIT_WINDOW_MS: u64 = 600_000;
#[cfg(not(feature = "short-timeouts"))]
pub const BIND_WINDOW_MS: u64 = 600_000;
/// Test-only build for the sandbox: windows short enough for `fast_forward`.
#[cfg(feature = "short-timeouts")]
pub const DEPOSIT_WINDOW_MS: u64 = 60_000;
#[cfg(feature = "short-timeouts")]
pub const BIND_WINDOW_MS: u64 = 60_000;

const ONE_NEAR: u128 = 1_000_000_000_000_000_000_000_000;
const MAX_RESULT_LEN: usize = 16_384;
const NS_PER_MS: u64 = 1_000_000;

// Gas as in Mock Goal; settle_bond and refund_expired get 100 TGas (Treasury ABI).
const REGISTER_GAS: Gas = Gas::from_tgas(35);
const GET_BOND_GAS: Gas = Gas::from_tgas(15);
const BIND_GAS: Gas = Gas::from_tgas(35);
const AUTHORIZE_GAS: Gas = Gas::from_tgas(35);
const SETTLE_GAS: Gas = Gas::from_tgas(100);
const SMALL_CALLBACK_GAS: Gas = Gas::from_tgas(15);
/// on_verified pays for bind_bond + on_bound.
const ON_VERIFIED_GAS: Gas = Gas::from_tgas(65);
/// on_decided pays for settle_bond + on_settled.
const ON_DECIDED_GAS: Gas = Gas::from_tgas(125);

/// Decision attested to the Treasury. Serialized as the plain string the Treasury expects.
#[near(serializers = [json, borsh])]
#[derive(Clone, Debug, PartialEq)]
pub enum Decision {
    QualificationRejected,
    ProofValid,
    ProofInvalidFinal,
}

#[near(serializers = [json, borsh])]
#[derive(Clone, Debug, PartialEq)]
pub enum CaseStage {
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
    Settled,
    TimeoutRequested,
    Orphaned,
}

/// The latest bonded challenge request of a milestone (SPEC 4, 6a).
#[near(serializers = [json, borsh])]
#[derive(Clone)]
pub struct ChallengeCase {
    pub proof_id: String,
    pub case_id: String,
    pub payer: AccountId,
    pub reason: String,
    pub fee_snapshot: U128,
    pub amount: U128,
    pub reward_cap: U128,
    pub deposit_until_ns: u64,
    pub bind_until_ns: u64,
    pub refund_after_ns: u64,
    pub escrow_id: Option<String>,
    pub stage: CaseStage,
    pub decision: Option<Decision>,
    pub nonce: u64,
}

impl ChallengeCase {
    /// The bond is not bound yet (the request may still be waiting for a deposit).
    fn unbound(&self) -> bool {
        matches!(
            self.stage,
            CaseStage::RegisterPending
                | CaseStage::RegisterRetryable
                | CaseStage::Ready
                | CaseStage::VerifyPending
                | CaseStage::BindPending
                | CaseStage::BindRetryable
        )
    }

    /// Blocks a new request on the same milestone.
    pub fn is_active(&self, now_ns: u64) -> bool {
        match self.stage {
            CaseStage::Settled | CaseStage::TimeoutRequested | CaseStage::Orphaned => false,
            _ if self.unbound() => now_ns < self.bind_until_ns,
            // Bound, waiting for a decision or a settlement: until the Treasury timeout.
            _ => now_ns < self.refund_after_ns,
        }
    }

    /// Blocks `finalize`: only a request that may still be bound.
    pub fn blocks_finalize(&self, now_ns: u64) -> bool {
        self.unbound() && now_ns < self.bind_until_ns
    }
}

/// B and R from F exactly as Bond Treasury `register_intent` checks them.
pub fn bond_amounts(fee_snapshot: u128) -> (u128, u128) {
    let bond = (fee_snapshot / 5).clamp(ONE_NEAR, 5 * ONE_NEAR);
    let reward_cap = (fee_snapshot / 10).min(bond / 2).min(2 * ONE_NEAR);
    (bond, reward_cap)
}

fn ms_to_ns(ms: u64) -> u64 {
    ms.checked_mul(NS_PER_MS)
        .unwrap_or_else(|| math::overflow())
}

fn add_ns(a: u64, b: u64) -> u64 {
    a.checked_add(b).unwrap_or_else(|| math::overflow())
}

/// The single result of the Treasury call; None if it failed. Never panics (callbacks must not).
fn promise_result() -> Option<Vec<u8>> {
    if env::promise_results_count() != 1 {
        return None;
    }
    env::promise_result_checked(0, MAX_RESULT_LEN).ok()
}

#[near]
impl Goal {
    /// Bonded challenge request (SPEC 6a). Anyone except the creator and the verifier.
    pub fn prepare_challenge(&mut self, index: u8, reason: String) -> Promise {
        let treasury = self.require_treasury();
        self.check_index(index);
        let challenger = env::predecessor_account_id();
        require!(challenger != self.creator, "Creator cannot challenge");
        require!(challenger != self.verifier, "Verifier cannot challenge");
        let now_ns = env::block_timestamp();
        {
            let m = &self.milestones[usize::from(index)];
            require!(m.status == MilestoneStatus::Submitted, "Wrong status");
            require!(
                env::block_timestamp_ms() < window_end(m, self.challenge_window_ms),
                "Window closed"
            );
            require!(reason.chars().count() <= MAX_REASON_LEN, "Reason too long");
            if let Some(c) = &m.case {
                require!(!c.is_active(now_ns), "Challenge in progress");
            }
        }

        self.sync();
        let locked = self.breakdown().locked;
        let m = &mut self.milestones[usize::from(index)];
        let fee_snapshot = bps_share(locked, u32::from(m.weight_bps));
        let (amount, reward_cap) = bond_amounts(fee_snapshot);
        m.challenge_attempts = m
            .challenge_attempts
            .checked_add(1)
            .unwrap_or_else(|| math::overflow());
        let attempt = m.challenge_attempts;
        let proof_hash = m.proof_hash.clone().unwrap_or_default();
        let deposit_until_ns = add_ns(now_ns, ms_to_ns(DEPOSIT_WINDOW_MS));
        let bind_until_ns = add_ns(deposit_until_ns, ms_to_ns(BIND_WINDOW_MS));
        let refund_after_ns = add_ns(bind_until_ns, ms_to_ns(RESOLVE_TIMEOUT_MS));
        let case = ChallengeCase {
            proof_id: format!("{index}:{proof_hash}:{attempt}"),
            case_id: format!("m{index}-{attempt}"),
            payer: challenger,
            reason,
            fee_snapshot: U128(fee_snapshot),
            amount: U128(amount),
            reward_cap: U128(reward_cap),
            deposit_until_ns,
            bind_until_ns,
            refund_after_ns,
            escrow_id: None,
            stage: CaseStage::RegisterPending,
            decision: None,
            nonce: 1,
        };
        events::emit(
            "challenge_prepared",
            json!({
                "index": index,
                "case_id": case.case_id,
                "payer": case.payer,
                "amount": case.amount,
                "deposit_until_ns": deposit_until_ns,
                "bind_until_ns": bind_until_ns,
                "refund_after_ns": refund_after_ns,
            }),
        );
        let promise = register_promise(&treasury, &case).then(
            Self::ext(env::current_account_id())
                .with_static_gas(SMALL_CALLBACK_GAS)
                .on_registered(index, case.nonce),
        );
        m.case = Some(case);
        promise
    }

    /// Repeats `register_intent` with the same intent (the Treasury returns the same escrow).
    pub fn retry_registration(&mut self, index: u8) -> Promise {
        let treasury = self.require_treasury();
        let c = self.payer_case(index, &[CaseStage::RegisterRetryable]);
        c.nonce += 1;
        c.stage = CaseStage::RegisterPending;
        register_promise(&treasury, c).then(
            Self::ext(env::current_account_id())
                .with_static_gas(SMALL_CALLBACK_GAS)
                .on_registered(index, c.nonce),
        )
    }

    #[private]
    pub fn on_registered(&mut self, index: u8, nonce: u64) -> bool {
        let Some(c) = self.callback_case(index, nonce, CaseStage::RegisterPending) else {
            return false;
        };
        let escrow =
            promise_result().and_then(|v| near_sdk::serde_json::from_slice::<String>(&v).ok());
        let ok = escrow.is_some();
        c.escrow_id = escrow.or(c.escrow_id.take());
        c.stage = if ok {
            CaseStage::Ready
        } else {
            CaseStage::RegisterRetryable
        };
        ok
    }

    /// After `deposit_bond` in the Treasury: check the bond on-chain and bind it.
    pub fn confirm_escrow(&mut self, index: u8) -> Promise {
        let treasury = self.require_treasury();
        self.check_index(index);
        require!(
            self.milestones[usize::from(index)].status == MilestoneStatus::Submitted,
            "Wrong status"
        );
        let c = self.payer_case(index, &[CaseStage::Ready, CaseStage::BindRetryable]);
        c.nonce += 1;
        c.stage = CaseStage::VerifyPending;
        treasury_call(
            &treasury,
            "get_bond",
            json!({ "escrow_id": c.escrow_id }),
            GET_BOND_GAS,
        )
        .then(
            Self::ext(env::current_account_id())
                .with_static_gas(ON_VERIFIED_GAS)
                .on_verified(index, c.nonce),
        )
    }

    #[private]
    pub fn on_verified(&mut self, index: u8, nonce: u64) -> PromiseOrValue<bool> {
        let goal = env::current_account_id();
        let Some(treasury) = self.treasury.clone() else {
            return PromiseOrValue::Value(false);
        };
        let Some(c) = self.callback_case(index, nonce, CaseStage::VerifyPending) else {
            return PromiseOrValue::Value(false);
        };
        let valid = promise_result()
            .and_then(|v| near_sdk::serde_json::from_slice::<Value>(&v).ok())
            .is_some_and(|b| {
                b["goal"] == json!(goal)
                    && b["escrow_id"] == json!(c.escrow_id)
                    && b["proof_id"] == json!(c.proof_id)
                    && b["case_id"] == json!(c.case_id)
                    && b["submission_id"] == json!("0")
                    && b["payer"] == json!(c.payer)
                    && b["amount"] == json!(c.amount)
                    && b["deposit_until_ns"] == json!(c.deposit_until_ns)
                    && b["bind_until_ns"] == json!(c.bind_until_ns)
                    && b["refund_after_ns"] == json!(c.refund_after_ns)
                    && (b["status"] == "Deposited" || b["status"] == "Bound")
            });
        if !valid {
            c.stage = CaseStage::BindRetryable;
            return PromiseOrValue::Value(false);
        }
        c.stage = CaseStage::BindPending;
        PromiseOrValue::Promise(
            treasury_call(
                &treasury,
                "bind_bond",
                json!({ "escrow_id": c.escrow_id }),
                BIND_GAS,
            )
            .then(
                Self::ext(goal)
                    .with_static_gas(SMALL_CALLBACK_GAS)
                    .on_bound(index, nonce),
            ),
        )
    }

    #[private]
    pub fn on_bound(&mut self, index: u8, nonce: u64) -> bool {
        let Some(c) = self.callback_case(index, nonce, CaseStage::BindPending) else {
            return false;
        };
        if promise_result().is_none() {
            c.stage = CaseStage::BindRetryable;
            return false;
        }
        let payer = c.payer.clone();
        let reason = c.reason.clone();
        let case_id = c.case_id.clone();
        let m = &mut self.milestones[usize::from(index)];
        if m.status != MilestoneStatus::Submitted {
            // finalize won the race at bind_until: the bond times out neutrally in the Treasury.
            if let Some(c) = m.case.as_mut() {
                c.stage = CaseStage::Orphaned;
            }
            return false;
        }
        m.status = MilestoneStatus::Challenged;
        m.challenger = Some(payer.clone());
        m.challenged_at_ms = Some(env::block_timestamp_ms());
        if let Some(c) = m.case.as_mut() {
            c.stage = CaseStage::Bound;
        }
        events::emit(
            "proof_challenged",
            json!({ "index": index, "challenger": payer, "reason": reason, "case_id": case_id }),
        );
        true
    }

    /// Qualification by the verifier. Rejected → milestone back to Submitted, full refund.
    pub fn qualify(&mut self, index: u8, qualified: bool) -> Option<Promise> {
        self.require_treasury();
        let c = self.verifier_case(index, CaseStage::Bound);
        events::emit(
            "challenge_qualified",
            json!({ "index": index, "qualified": qualified }),
        );
        if qualified {
            c.stage = CaseStage::Qualified;
            return None;
        }
        let m = &mut self.milestones[usize::from(index)];
        m.status = MilestoneStatus::Submitted;
        m.challenger = None;
        m.challenged_at_ms = None;
        Some(self.decide(index, Decision::QualificationRejected))
    }

    /// Verdict on a qualified challenge. valid → Accepted + slash, invalid → Rejected + refund.
    pub fn arbitrate(&mut self, index: u8, valid: bool) -> Promise {
        self.require_treasury();
        self.verifier_case(index, CaseStage::Qualified);
        let m = &mut self.milestones[usize::from(index)];
        m.status = if valid {
            MilestoneStatus::Accepted
        } else {
            MilestoneStatus::Rejected
        };
        emit_resolved(index, &m.status);
        let decision = if valid {
            Decision::ProofValid
        } else {
            Decision::ProofInvalidFinal
        };
        self.decide(index, decision)
    }

    /// Repeats `authorize_decision` with the stored decision. Anyone.
    pub fn retry_decision(&mut self, index: u8) -> Promise {
        self.require_treasury();
        let c = self.case_in(index, &[CaseStage::DecisionRetryable]);
        let decision = c
            .decision
            .clone()
            .unwrap_or_else(|| env::panic_str("Wrong stage"));
        self.decide(index, decision)
    }

    #[private]
    pub fn on_decided(&mut self, index: u8, nonce: u64) -> PromiseOrValue<bool> {
        if self.treasury.is_none() {
            return PromiseOrValue::Value(false);
        }
        let Some(c) = self.callback_case(index, nonce, CaseStage::DecisionPending) else {
            return PromiseOrValue::Value(false);
        };
        if promise_result().is_none() {
            c.stage = CaseStage::DecisionRetryable;
            let decision = c.decision.clone();
            emit_settlement(index, &decision, &CaseStage::DecisionRetryable);
            return PromiseOrValue::Value(false);
        }
        PromiseOrValue::Promise(self.settle(index))
    }

    /// Repeats `settle_bond` with the same settlement id (a no-op in the Treasury if it went through). Anyone.
    pub fn retry_settlement(&mut self, index: u8) -> Promise {
        self.require_treasury();
        self.case_in(index, &[CaseStage::SettlementRetryable]);
        self.settle(index)
    }

    #[private]
    pub fn on_settled(&mut self, index: u8, nonce: u64) -> bool {
        let Some(c) = self.callback_case(index, nonce, CaseStage::SettlementPending) else {
            return false;
        };
        let ok = promise_result().is_some();
        c.stage = if ok {
            CaseStage::Settled
        } else {
            CaseStage::SettlementRetryable
        };
        let (decision, stage) = (c.decision.clone(), c.stage.clone());
        emit_settlement(index, &decision, &stage);
        ok
    }

    /// Result of Treasury `refund_expired` from `resolve_timeout`: an event only, since the refund
    /// is permissionless in the Treasury.
    #[private]
    pub fn on_timeout_refund(&mut self, index: u8, nonce: u64) -> bool {
        if self
            .callback_case(index, nonce, CaseStage::TimeoutRequested)
            .is_none()
        {
            return false;
        }
        let ok = promise_result().is_some();
        events::emit(
            "bond_settlement",
            json!({ "index": index, "decision": "NeutralRefund", "stage": "TimeoutRequested", "ok": ok }),
        );
        ok
    }
}

fn emit_settlement(index: u8, decision: &Option<Decision>, stage: &CaseStage) {
    events::emit(
        "bond_settlement",
        json!({ "index": index, "decision": decision, "stage": stage }),
    );
}

fn treasury_call(treasury: &AccountId, method: &str, args: Value, gas: Gas) -> Promise {
    let args = near_sdk::serde_json::to_vec(&args).unwrap_or_else(|_| env::panic_str("JSON"));
    // Zero deposit: the goal never sends NEAR to the Treasury (SPEC 6a).
    Promise::new(treasury.clone()).function_call(
        method.to_string(),
        args,
        NearToken::from_yoctonear(0),
        gas,
    )
}

fn register_promise(treasury: &AccountId, c: &ChallengeCase) -> Promise {
    treasury_call(
        treasury,
        "register_intent",
        json!({ "intent": {
            "proof_id": c.proof_id,
            "case_id": c.case_id,
            "submission_id": "0",
            "payer": c.payer,
            "fee_snapshot": c.fee_snapshot,
            "amount": c.amount,
            "reward_cap": c.reward_cap,
            "deposit_until_ns": c.deposit_until_ns,
            "bind_until_ns": c.bind_until_ns,
            "refund_after_ns": c.refund_after_ns,
        }}),
        REGISTER_GAS,
    )
}

impl Goal {
    pub(crate) fn require_treasury(&self) -> AccountId {
        self.treasury
            .clone()
            .unwrap_or_else(|| env::panic_str("No treasury"))
    }

    /// The case of a milestone in one of `stages`, for a public method.
    fn case_in(&mut self, index: u8, stages: &[CaseStage]) -> &mut ChallengeCase {
        self.check_index(index);
        let c = self.milestones[usize::from(index)]
            .case
            .as_mut()
            .unwrap_or_else(|| env::panic_str("Wrong stage"));
        require!(stages.contains(&c.stage), "Wrong stage");
        c
    }

    fn payer_case(&mut self, index: u8, stages: &[CaseStage]) -> &mut ChallengeCase {
        let c = self.case_in(index, stages);
        require!(env::predecessor_account_id() == c.payer, "Only payer");
        c
    }

    /// Verifier, milestone Challenged, case in `stage`, before the Treasury timeout.
    fn verifier_case(&mut self, index: u8, stage: CaseStage) -> &mut ChallengeCase {
        self.check_index(index);
        require!(
            env::predecessor_account_id() == self.verifier,
            "Only verifier"
        );
        require!(
            self.milestones[usize::from(index)].status == MilestoneStatus::Challenged,
            "Wrong status"
        );
        let c = self.case_in(index, &[stage]);
        require!(
            env::block_timestamp() < c.refund_after_ns,
            "Resolve time passed"
        );
        c
    }

    /// The case for a callback, if it is still in `stage` with the same nonce. Never panics.
    fn callback_case(
        &mut self,
        index: u8,
        nonce: u64,
        stage: CaseStage,
    ) -> Option<&mut ChallengeCase> {
        self.milestones
            .get_mut(usize::from(index))?
            .case
            .as_mut()
            .filter(|c| c.stage == stage && c.nonce == nonce)
    }

    /// Stores the decision and attests it to the Treasury; the settlement follows in on_decided.
    fn decide(&mut self, index: u8, decision: Decision) -> Promise {
        let treasury = self.require_treasury();
        let c = self.milestones[usize::from(index)]
            .case
            .as_mut()
            .unwrap_or_else(|| env::panic_str("Wrong stage"));
        c.decision = Some(decision.clone());
        c.nonce += 1;
        c.stage = CaseStage::DecisionPending;
        treasury_call(
            &treasury,
            "authorize_decision",
            json!({ "escrow_id": c.escrow_id, "decision": decision }),
            AUTHORIZE_GAS,
        )
        .then(
            Self::ext(env::current_account_id())
                .with_static_gas(ON_DECIDED_GAS)
                .on_decided(index, c.nonce),
        )
    }

    /// settle_bond: SLASH for ProofValid, REFUND otherwise. Settlement id "bond:{case_id}".
    fn settle(&mut self, index: u8) -> Promise {
        let treasury = self.require_treasury();
        let c = self.milestones[usize::from(index)]
            .case
            .as_mut()
            .unwrap_or_else(|| env::panic_str("Wrong stage"));
        c.nonce += 1;
        c.stage = CaseStage::SettlementPending;
        let outcome = if c.decision == Some(Decision::ProofValid) {
            "SLASH"
        } else {
            "REFUND"
        };
        treasury_call(
            &treasury,
            "settle_bond",
            json!({
                "escrow_id": c.escrow_id,
                "settlement_id": format!("bond:{}", c.case_id),
                "outcome": outcome,
            }),
            SETTLE_GAS,
        )
        .then(
            Self::ext(env::current_account_id())
                .with_static_gas(SMALL_CALLBACK_GAS)
                .on_settled(index, c.nonce),
        )
    }

    /// Treasury-mode part of resolve_timeout: the milestone is already Challenged.
    pub(crate) fn timeout_case(&mut self, index: u8) -> Promise {
        let treasury = self.require_treasury();
        let c = self.case_in(index, &[CaseStage::Bound, CaseStage::Qualified]);
        require!(
            env::block_timestamp() >= c.refund_after_ns,
            "Resolve time not passed"
        );
        c.nonce += 1;
        c.stage = CaseStage::TimeoutRequested;
        let (escrow_id, nonce) = (c.escrow_id.clone(), c.nonce);
        treasury_call(
            &treasury,
            "refund_expired",
            json!({ "escrow_id": escrow_id }),
            SETTLE_GAS,
        )
        .then(
            Self::ext(env::current_account_id())
                .with_static_gas(SMALL_CALLBACK_GAS)
                .on_timeout_refund(index, nonce),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DAY_MS, MilestoneInput, RESOLVE_TIMEOUT_MS};
    use near_sdk::test_utils::{VMContextBuilder, get_logs};
    use near_sdk::{PromiseResult, RuntimeFeesConfig, test_vm_config, testing_env};
    use std::collections::HashMap;

    const NOW_MS: u64 = 1_000_000;
    const WINDOW_MS: u64 = 120_000;
    const HASH: &str = "d4f02eaafd1a9e9de7d10972ca8e47fa7a985825c3c9c1e249c72683cb3e4f19";
    const N: u128 = ONE_NEAR;

    fn a(s: &str) -> AccountId {
        s.parse().unwrap()
    }
    fn goal_id() -> AccountId {
        a("goal.testnet")
    }

    fn ctx(who: &str, now_ms: u64, balance: NearToken) -> VMContextBuilder {
        let mut b = VMContextBuilder::new();
        b.current_account_id(goal_id())
            .predecessor_account_id(a(who))
            .block_timestamp(now_ms * NS_PER_MS)
            .account_balance(balance);
        b
    }
    fn at(who: &str, now_ms: u64) {
        testing_env!(ctx(who, now_ms, NearToken::from_near(5)).build());
    }
    /// A private callback at `now_ms` with one Treasury result.
    fn callback(now_ms: u64, r: PromiseResult) {
        testing_env!(
            ctx("goal.testnet", now_ms, NearToken::from_near(5)).build(),
            test_vm_config(),
            RuntimeFeesConfig::test(),
            HashMap::default(),
            vec![r]
        );
    }
    fn ok(json: &str) -> PromiseResult {
        PromiseResult::Successful(json.as_bytes().to_vec())
    }

    fn new_goal(treasury: Option<AccountId>) -> Goal {
        at("creator.testnet", NOW_MS);
        let milestones = [4000u16, 3000, 3000]
            .iter()
            .map(|&w| MilestoneInput {
                title: "M".into(),
                weight_bps: w,
                deadline_ms: NOW_MS + DAY_MS,
            })
            .collect();
        Goal::new(
            a("creator.testnet"),
            a("verifier.testnet"),
            "Goal".into(),
            String::new(),
            HASH.into(),
            5000,
            WINDOW_MS,
            milestones,
            treasury,
        )
    }

    /// Treasury goal, 10 NEAR of fees synced, proof submitted on milestone 0.
    fn submitted() -> Goal {
        let mut g = new_goal(Some(a("treasury.testnet")));
        testing_env!(ctx("payer.testnet", NOW_MS, NearToken::from_near(15)).build());
        g.sync();
        at("creator.testnet", NOW_MS);
        g.submit_proof(0, "u".into(), HASH.into());
        g
    }

    fn case(g: &Goal) -> ChallengeCase {
        g.milestones[0].case.clone().unwrap()
    }
    fn stage(g: &Goal) -> CaseStage {
        case(g).stage
    }

    /// prepare at NOW_MS + 1; the balance stays 15 NEAR so sync adds nothing.
    fn prepared() -> Goal {
        let mut g = submitted();
        testing_env!(ctx("payer.testnet", NOW_MS + 1, NearToken::from_near(15)).build());
        let _ = g.prepare_challenge(0, "bad proof".into());
        g
    }
    fn bond_json(g: &Goal, status: &str) -> String {
        let c = case(g);
        json!({
            "escrow_id": "e", "goal": goal_id(), "proof_id": c.proof_id, "case_id": c.case_id,
            "submission_id": "0", "payer": c.payer, "amount": c.amount,
            "reward_cap": c.reward_cap, "deposit_until_ns": c.deposit_until_ns,
            "bind_until_ns": c.bind_until_ns, "refund_after_ns": c.refund_after_ns,
            "status": status,
        })
        .to_string()
    }
    /// Registered, deposited and bound: milestone Challenged.
    fn bound() -> Goal {
        let mut g = prepared();
        callback(NOW_MS + 2, ok("\"e\""));
        assert!(g.on_registered(0, 1));
        at("payer.testnet", NOW_MS + 3);
        let _ = g.confirm_escrow(0);
        let json = bond_json(&g, "Deposited");
        callback(NOW_MS + 4, ok(&json));
        assert!(matches!(g.on_verified(0, 2), PromiseOrValue::Promise(_)));
        callback(NOW_MS + 5, ok(""));
        assert!(g.on_bound(0, 2));
        g
    }
    fn qualified() -> Goal {
        let mut g = bound();
        at("verifier.testnet", NOW_MS + 6);
        assert!(g.qualify(0, true).is_none());
        g
    }
    fn refund_after_ms(g: &Goal) -> u64 {
        case(g).refund_after_ns / NS_PER_MS
    }

    #[test]
    #[should_panic(expected = "Treasury cannot be the contract, creator or verifier")]
    fn new_rejects_treasury_equal_to_creator() {
        new_goal(Some(a("creator.testnet")));
    }

    #[test]
    #[should_panic(expected = "Treasury cannot be the contract, creator or verifier")]
    fn new_rejects_treasury_equal_to_contract() {
        new_goal(Some(goal_id()));
    }

    #[test]
    fn new_stores_treasury() {
        assert_eq!(
            new_goal(Some(a("treasury.testnet"))).get_goal().treasury,
            Some(a("treasury.testnet"))
        );
        assert_eq!(new_goal(None).get_goal().treasury, None);
    }

    #[test]
    #[should_panic(expected = "No treasury")]
    fn prepare_requires_treasury() {
        let mut g = new_goal(None);
        at("creator.testnet", NOW_MS);
        g.submit_proof(0, "u".into(), HASH.into());
        at("payer.testnet", NOW_MS + 1);
        let _ = g.prepare_challenge(0, "r".into());
    }

    #[test]
    #[should_panic(expected = "Bond required")]
    fn free_challenge_disabled_with_treasury() {
        let mut g = submitted();
        at("payer.testnet", NOW_MS + 1);
        g.challenge(0, "r".into());
    }

    #[test]
    #[should_panic(expected = "Use arbitrate")]
    fn resolve_disabled_with_treasury() {
        let mut g = bound();
        at("verifier.testnet", NOW_MS + 6);
        g.resolve(0, true);
    }

    #[cfg(not(feature = "short-timeouts"))]
    #[test]
    fn release_timeouts_are_24h_and_10_min() {
        assert_eq!(RESOLVE_TIMEOUT_MS, 86_400_000);
        assert_eq!(DEPOSIT_WINDOW_MS, 600_000);
        assert_eq!(BIND_WINDOW_MS, 600_000);
    }

    #[test]
    fn bond_amounts_match_treasury() {
        assert_eq!(bond_amounts(0), (N, 0));
        assert_eq!(bond_amounts(7 * N), (7 * N / 5, 7 * N / 10));
        assert_eq!(bond_amounts(10 * N), (2 * N, N));
        assert_eq!(bond_amounts(100 * N), (5 * N, 2 * N));
        // F = 1 NEAR: B at the 1 NEAR floor, R = F / 10.
        assert_eq!(bond_amounts(N), (N, N / 10));
    }

    #[test]
    fn prepare_snapshots_f_and_stores_case() {
        let g = prepared();
        let c = case(&g);
        // 10 NEAR received, 50% locked = 5 NEAR, milestone 0 weight 40% → F = 2 NEAR.
        assert_eq!(g.total_received, NearToken::from_near(10));
        assert_eq!(c.fee_snapshot, U128(2 * N));
        assert_eq!(c.amount, U128(N));
        assert_eq!(c.reward_cap, U128(N / 5));
        assert_eq!(c.case_id, "m0-1");
        assert_eq!(c.proof_id, format!("0:{HASH}:1"));
        assert_eq!(c.payer, a("payer.testnet"));
        assert_eq!(c.stage, CaseStage::RegisterPending);
        let t0 = (NOW_MS + 1) * NS_PER_MS;
        assert_eq!(c.deposit_until_ns, t0 + 600_000 * NS_PER_MS);
        assert_eq!(c.bind_until_ns, t0 + 1_200_000 * NS_PER_MS);
        assert_eq!(
            c.refund_after_ns,
            c.bind_until_ns + RESOLVE_TIMEOUT_MS * NS_PER_MS
        );
        // The reason is stored once, in the case; the milestone is still Submitted.
        assert_eq!(c.reason, "bad proof");
        assert_eq!(g.milestones[0].challenge_reason, None);
        assert_eq!(g.milestones[0].status, MilestoneStatus::Submitted);
        assert!(get_logs()[0].contains("challenge_prepared"));
    }

    #[test]
    #[should_panic(expected = "Creator cannot challenge")]
    fn prepare_rejects_creator() {
        let mut g = submitted();
        at("creator.testnet", NOW_MS + 1);
        let _ = g.prepare_challenge(0, "r".into());
    }

    #[test]
    #[should_panic(expected = "Verifier cannot challenge")]
    fn prepare_rejects_verifier() {
        let mut g = submitted();
        at("verifier.testnet", NOW_MS + 1);
        let _ = g.prepare_challenge(0, "r".into());
    }

    #[test]
    #[should_panic(expected = "Window closed")]
    fn prepare_rejects_after_window() {
        let mut g = submitted();
        at("payer.testnet", NOW_MS + WINDOW_MS);
        let _ = g.prepare_challenge(0, "r".into());
    }

    #[test]
    #[should_panic(expected = "Challenge in progress")]
    fn prepare_rejects_second_active_request() {
        let mut g = prepared();
        at("other.testnet", NOW_MS + 2);
        let _ = g.prepare_challenge(0, "r".into());
    }

    #[test]
    fn unbound_request_expires_at_bind_until() {
        let mut g = submitted();
        // Long window so that a new request is still possible after bind_until.
        g.challenge_window_ms = 2 * DAY_MS;
        at("payer.testnet", NOW_MS + 1);
        let _ = g.prepare_challenge(0, "r".into());
        let bind_until_ms = case(&g).bind_until_ns / NS_PER_MS;
        at("other.testnet", bind_until_ms);
        let _ = g.prepare_challenge(0, "r2".into());
        let c = case(&g);
        assert_eq!(c.case_id, "m0-2");
        assert_eq!(c.payer, a("other.testnet"));
        assert_eq!(g.milestones[0].challenge_attempts, 2);
    }

    #[test]
    fn unbound_request_blocks_finalize_only_until_bind_until() {
        let mut g = prepared();
        let bind_until_ms = case(&g).bind_until_ns / NS_PER_MS;
        at("anyone.testnet", bind_until_ms - 1);
        let blocked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| g.finalize(0)));
        assert!(blocked.is_err());
        at("anyone.testnet", bind_until_ms);
        g.finalize(0);
        assert_eq!(g.milestones[0].status, MilestoneStatus::Accepted);
    }

    #[test]
    fn registration_callback_success_failure_and_stale() {
        let mut g = prepared();
        callback(NOW_MS + 2, PromiseResult::Failed);
        assert!(!g.on_registered(0, 1));
        assert_eq!(stage(&g), CaseStage::RegisterRetryable);
        at("payer.testnet", NOW_MS + 3);
        let _ = g.retry_registration(0);
        callback(NOW_MS + 4, ok("\"e\""));
        assert!(!g.on_registered(0, 1), "stale nonce");
        assert!(g.on_registered(0, 2));
        assert_eq!(stage(&g), CaseStage::Ready);
        assert_eq!(case(&g).escrow_id, Some("e".into()));
        assert!(!g.on_registered(0, 2), "replay");
        assert!(!g.on_registered(9, 2), "unknown index never panics");
    }

    #[test]
    #[should_panic(expected = "Only payer")]
    fn retry_registration_only_payer() {
        let mut g = prepared();
        callback(NOW_MS + 2, PromiseResult::Failed);
        g.on_registered(0, 1);
        at("other.testnet", NOW_MS + 3);
        let _ = g.retry_registration(0);
    }

    #[test]
    #[should_panic(expected = "Only payer")]
    fn confirm_escrow_only_payer() {
        let mut g = prepared();
        callback(NOW_MS + 2, ok("\"e\""));
        g.on_registered(0, 1);
        at("other.testnet", NOW_MS + 3);
        let _ = g.confirm_escrow(0);
    }

    #[test]
    fn verification_rejects_wrong_bond() {
        let mut g = prepared();
        callback(NOW_MS + 2, ok("\"e\""));
        g.on_registered(0, 1);
        for bad in [
            bond_json(&g, "Refunded"),
            bond_json(&g, "Deposited").replace("payer.testnet", "other.testnet"),
            bond_json(&g, "Deposited").replace("goal.testnet", "evil.testnet"),
            "null".to_string(),
        ] {
            at("payer.testnet", NOW_MS + 3);
            let _ = g.confirm_escrow(0);
            let nonce = case(&g).nonce;
            callback(NOW_MS + 4, ok(&bad));
            assert!(matches!(
                g.on_verified(0, nonce),
                PromiseOrValue::Value(false)
            ));
            assert_eq!(stage(&g), CaseStage::BindRetryable, "{bad}");
        }
        assert_eq!(g.milestones[0].status, MilestoneStatus::Submitted);
    }

    #[test]
    fn bind_marks_milestone_challenged() {
        let g = bound();
        let m = &g.milestones[0];
        assert_eq!(m.status, MilestoneStatus::Challenged);
        assert_eq!(m.challenger, Some(a("payer.testnet")));
        assert_eq!(m.challenged_at_ms, Some(NOW_MS + 5));
        assert_eq!(m.challenge_reason, None);
        assert_eq!(stage(&g), CaseStage::Bound);
        assert!(get_logs()[0].contains("proof_challenged"));
    }

    #[test]
    fn failed_bind_is_retryable() {
        let mut g = prepared();
        callback(NOW_MS + 2, ok("\"e\""));
        g.on_registered(0, 1);
        at("payer.testnet", NOW_MS + 3);
        let _ = g.confirm_escrow(0);
        let json = bond_json(&g, "Deposited");
        callback(NOW_MS + 4, ok(&json));
        let _ = g.on_verified(0, 2);
        callback(NOW_MS + 5, PromiseResult::Failed);
        assert!(!g.on_bound(0, 2));
        assert_eq!(stage(&g), CaseStage::BindRetryable);
        assert_eq!(g.milestones[0].status, MilestoneStatus::Submitted);
    }

    #[test]
    fn bind_after_finalize_is_orphaned() {
        let mut g = prepared();
        callback(NOW_MS + 2, ok("\"e\""));
        g.on_registered(0, 1);
        at("payer.testnet", NOW_MS + 3);
        let _ = g.confirm_escrow(0);
        let json = bond_json(&g, "Deposited");
        callback(NOW_MS + 4, ok(&json));
        let _ = g.on_verified(0, 2);
        // finalize wins at bind_until before on_bound arrives.
        let bind_until_ms = case(&g).bind_until_ns / NS_PER_MS;
        g.milestones[0].case.as_mut().unwrap().stage = CaseStage::BindRetryable;
        at("anyone.testnet", bind_until_ms);
        g.finalize(0);
        g.milestones[0].case.as_mut().unwrap().stage = CaseStage::BindPending;
        callback(bind_until_ms, ok(""));
        assert!(!g.on_bound(0, 2));
        assert_eq!(stage(&g), CaseStage::Orphaned);
        assert_eq!(g.milestones[0].status, MilestoneStatus::Accepted);
    }

    #[test]
    #[should_panic(expected = "Only verifier")]
    fn stranger_cannot_qualify() {
        let mut g = bound();
        at("stranger.testnet", NOW_MS + 6);
        g.qualify(0, true);
    }

    #[test]
    #[should_panic(expected = "Only verifier")]
    fn stranger_cannot_arbitrate() {
        let mut g = qualified();
        at("stranger.testnet", NOW_MS + 7);
        let _ = g.arbitrate(0, true);
    }

    #[test]
    #[should_panic(expected = "Wrong stage")]
    fn arbitrate_requires_qualification() {
        let mut g = bound();
        at("verifier.testnet", NOW_MS + 6);
        let _ = g.arbitrate(0, true);
    }

    #[test]
    #[should_panic(expected = "Resolve time passed")]
    fn qualify_rejects_at_refund_after() {
        let mut g = bound();
        let t = refund_after_ms(&g);
        at("verifier.testnet", t);
        g.qualify(0, true);
    }

    #[test]
    fn qualification_rejected_returns_to_submitted() {
        let mut g = bound();
        at("verifier.testnet", NOW_MS + 6);
        assert!(g.qualify(0, false).is_some());
        let m = &g.milestones[0];
        assert_eq!(m.status, MilestoneStatus::Submitted);
        assert_eq!(m.challenger, None);
        assert_eq!(m.challenged_at_ms, None);
        assert_eq!(case(&g).decision, Some(Decision::QualificationRejected));
        assert_eq!(stage(&g), CaseStage::DecisionPending);
        // The settlement chain still blocks a new request until it is settled.
        g.challenge_window_ms = DAY_MS;
        at("other.testnet", NOW_MS + 7);
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = g.prepare_challenge(0, "again".into());
        }));
        assert!(r.is_err());
    }

    #[test]
    fn arbitrate_valid_accepts_and_slashes() {
        let mut g = qualified();
        at("verifier.testnet", NOW_MS + 7);
        let _ = g.arbitrate(0, true);
        assert_eq!(g.milestones[0].status, MilestoneStatus::Accepted);
        assert_eq!(case(&g).decision, Some(Decision::ProofValid));
    }

    #[test]
    fn arbitrate_invalid_rejects() {
        let mut g = qualified();
        at("verifier.testnet", NOW_MS + 7);
        let _ = g.arbitrate(0, false);
        assert_eq!(g.milestones[0].status, MilestoneStatus::Rejected);
        assert_eq!(case(&g).decision, Some(Decision::ProofInvalidFinal));
    }

    #[test]
    fn decision_and_settlement_retries() {
        let mut g = qualified();
        at("verifier.testnet", NOW_MS + 7);
        let _ = g.arbitrate(0, true);
        let n = case(&g).nonce;
        callback(NOW_MS + 8, PromiseResult::Failed);
        assert!(matches!(g.on_decided(0, n), PromiseOrValue::Value(false)));
        assert_eq!(stage(&g), CaseStage::DecisionRetryable);
        at("anyone.testnet", NOW_MS + 9);
        let _ = g.retry_decision(0);
        assert_eq!(case(&g).nonce, n + 1);
        callback(NOW_MS + 10, ok(""));
        assert!(
            matches!(g.on_decided(0, n), PromiseOrValue::Value(false)),
            "stale"
        );
        assert!(matches!(g.on_decided(0, n + 1), PromiseOrValue::Promise(_)));
        assert_eq!(stage(&g), CaseStage::SettlementPending);
        callback(NOW_MS + 11, PromiseResult::Failed);
        assert!(!g.on_settled(0, n + 2));
        assert_eq!(stage(&g), CaseStage::SettlementRetryable);
        at("anyone.testnet", NOW_MS + 12);
        let _ = g.retry_settlement(0);
        callback(NOW_MS + 13, ok(""));
        assert!(g.on_settled(0, n + 3));
        assert_eq!(stage(&g), CaseStage::Settled);
        assert!(!g.on_settled(0, n + 3), "replay changes nothing");
        // The decision is unchanged by retries.
        assert_eq!(case(&g).decision, Some(Decision::ProofValid));
        assert_eq!(g.milestones[0].status, MilestoneStatus::Accepted);
    }

    #[test]
    #[should_panic(expected = "Wrong stage")]
    fn settled_case_cannot_be_settled_again() {
        let mut g = qualified();
        at("verifier.testnet", NOW_MS + 7);
        let _ = g.arbitrate(0, false);
        callback(NOW_MS + 8, ok(""));
        let _ = g.on_decided(0, case(&g).nonce);
        callback(NOW_MS + 9, ok(""));
        assert!(g.on_settled(0, case(&g).nonce));
        at("anyone.testnet", NOW_MS + 10);
        let _ = g.retry_settlement(0);
    }

    #[test]
    #[should_panic(expected = "Resolve time not passed")]
    fn timeout_before_refund_after() {
        let mut g = qualified();
        let t = refund_after_ms(&g);
        at("anyone.testnet", t - 1);
        g.resolve_timeout(0);
    }

    #[test]
    fn timeout_accepts_and_requests_neutral_refund() {
        let mut g = qualified();
        let t = refund_after_ms(&g);
        at("anyone.testnet", t);
        assert!(g.resolve_timeout(0).is_some());
        assert_eq!(g.milestones[0].status, MilestoneStatus::Accepted);
        assert_eq!(stage(&g), CaseStage::TimeoutRequested);
        let n = case(&g).nonce;
        callback(t + 1, PromiseResult::Failed);
        assert!(!g.on_timeout_refund(0, n));
        callback(t + 1, ok(""));
        assert!(g.on_timeout_refund(0, n));
        assert!(!g.on_timeout_refund(0, n - 1), "stale");
        assert_eq!(stage(&g), CaseStage::TimeoutRequested);
    }

    #[test]
    fn bond_never_counts_as_income() {
        let mut g = qualified();
        let before = g.total_received;
        // The Treasury holds the bond: the goal balance is unchanged at 15 NEAR.
        testing_env!(ctx("anyone.testnet", NOW_MS + 7, NearToken::from_near(15)).build());
        g.sync();
        assert_eq!(g.total_received, before);
    }
}
