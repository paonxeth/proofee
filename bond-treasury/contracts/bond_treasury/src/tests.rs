use super::*;
use near_sdk::test_utils::{get_created_receipts, VMContextBuilder};
use near_sdk::{test_vm_config, testing_env, PromiseResult, RuntimeFeesConfig};
use std::collections::HashMap;
const N: u128 = 10u128.pow(24);
fn a(s: &str) -> AccountId {
    s.parse().unwrap()
}
fn ctx(who: &str, amount: u128, time: u64) -> VMContextBuilder {
    let mut c = VMContextBuilder::new();
    c.current_account_id(a("treasury.testnet"))
        .predecessor_account_id(a(who))
        .signer_account_id(a(who))
        .attached_deposit(NearToken::from_yoctonear(amount))
        .account_balance(NearToken::from_near(100))
        .block_timestamp(time)
        .prepaid_gas(Gas::from_tgas(100));
    c
}
fn set(who: &str, amount: u128, time: u64) {
    testing_env!(ctx(who, amount, time).build());
}
fn setup() -> (Treasury, String) {
    set("admin.testnet", 0, 1);
    let mut t = Treasury::new(a("admin.testnet"));
    t.register_goal(a("goal.testnet"), "demo".into());
    let id = t.register_demo_case(
        a("goal.testnet"),
        "proof".into(),
        "1".into(),
        "0".into(),
        a("payer.testnet"),
        U128(2 * N),
        10,
        20,
        30,
    );
    (t, id)
}
fn deposited() -> (Treasury, String) {
    let (mut t, id) = setup();
    set("payer.testnet", 2 * N, 2);
    t.deposit_bond(id.clone());
    (t, id)
}
fn bound() -> (Treasury, String) {
    let (mut t, id) = deposited();
    set("goal.testnet", 0, 3);
    t.bind_bond(id.clone());
    (t, id)
}
#[test]
fn refund_success_and_replay() {
    let (mut t, id) = bound();
    t.settle_bond(id.clone(), "s".into(), Outcome::REFUND);
    assert_eq!(t.pending, 2 * N);
    assert_eq!(t.locked, 0);
    assert!(!get_created_receipts().is_empty());
    t.settle_bond(id.clone(), "s".into(), Outcome::REFUND);
    assert_eq!(t.bond(&id).attempt, 1);
    testing_env!(
        ctx("treasury.testnet", 0, 4).build(),
        test_vm_config(),
        RuntimeFeesConfig::test(),
        HashMap::default(),
        vec![PromiseResult::Successful(vec![])]
    );
    assert!(t.on_refund(id.clone(), 1));
    assert!(!t.on_refund(id.clone(), 1));
    assert_eq!(t.bond(&id).status, Some(Status::Refunded));
    assert_eq!(t.pending + t.locked + t.buyback, 0);
}
#[test]
fn failure_retry_stale_callback() {
    let (mut t, id) = bound();
    t.settle_bond(id.clone(), "s".into(), Outcome::REFUND);
    testing_env!(
        ctx("treasury.testnet", 0, 4).build(),
        test_vm_config(),
        RuntimeFeesConfig::test(),
        HashMap::default(),
        vec![PromiseResult::Failed]
    );
    assert!(!t.on_refund(id.clone(), 1));
    assert_eq!(t.locked, 2 * N);
    assert_eq!(t.pending, 0);
    set("any.testnet", 0, 5);
    t.retry_failed_refund(id.clone());
    assert_eq!(t.bond(&id).attempt, 2);
    assert!(!t.on_refund(id.clone(), 1));
    assert_eq!(t.pending, 2 * N);
    testing_env!(
        ctx("treasury.testnet", 0, 6).build(),
        test_vm_config(),
        RuntimeFeesConfig::test(),
        HashMap::default(),
        vec![PromiseResult::Successful(vec![])]
    );
    assert!(t.on_refund(id, 2));
    assert_eq!(t.pending, 0);
}
#[test]
fn slash_isolation() {
    let (mut t, id) = bound();
    t.settle_bond(id.clone(), "s".into(), Outcome::SLASH);
    t.settle_bond(id, "s".into(), Outcome::SLASH);
    assert_eq!(t.get_buyback_balance(a("goal.testnet")).0, 2 * N);
    assert_eq!(t.locked + t.pending + t.buyback, 2 * N);
    set("admin.testnet", 0, 4);
    t.register_goal(a("other.testnet"), "demo".into());
    let other = t.register_demo_case(
        a("other.testnet"),
        "proof".into(),
        "1".into(),
        "0".into(),
        a("payer.testnet"),
        U128(N),
        10,
        20,
        30,
    );
    assert_eq!(t.get_buyback_balance(a("other.testnet")).0, 0);
    assert_eq!(t.bond(&other).amount.0, N);
}
#[test]
fn orphan_timeout() {
    let (mut t, id) = deposited();
    set("any.testnet", 0, 20);
    t.refund_expired(id.clone());
    assert_eq!(t.bond(&id).status, Some(Status::RefundPending));
}
#[test]
fn bound_timeout_even_if_goal_disabled() {
    let (mut t, id) = bound();
    set("admin.testnet", 0, 4);
    t.set_goal_authorized(a("goal.testnet"), false);
    set("any.testnet", 0, 30);
    t.refund_expired(id);
    assert_eq!(t.pending, 2 * N);
}
#[test]
fn qualification_rejected_before_bind() {
    let (mut t, id) = deposited();
    set("goal.testnet", 0, 3);
    t.settle_bond(id, "rejected".into(), Outcome::REFUND);
    assert_eq!(t.pending, 2 * N);
}
#[test]
#[should_panic(expected = "ADMIN")]
fn outsider_register() {
    let (mut t, _) = setup();
    set("evil.testnet", 0, 2);
    t.register_goal(a("evil.testnet"), "x".into());
}
#[test]
#[should_panic(expected = "GOAL_CALLER")]
fn signer_is_not_authority() {
    let (mut t, id) = bound();
    let mut c = ctx("evil.testnet", 0, 4);
    c.signer_account_id(a("goal.testnet"));
    testing_env!(c.build());
    t.settle_bond(id, "x".into(), Outcome::SLASH);
}
#[test]
#[should_panic(expected = "AMOUNT")]
fn wrong_amount() {
    let (mut t, id) = setup();
    set("payer.testnet", N, 2);
    t.deposit_bond(id);
}
#[test]
#[should_panic(expected = "PAYER")]
fn wrong_payer() {
    let (mut t, id) = setup();
    set("evil.testnet", 2 * N, 2);
    t.deposit_bond(id);
}
#[test]
#[should_panic(expected = "ALREADY_DEPOSITED")]
fn duplicate_deposit() {
    let (mut t, id) = deposited();
    t.deposit_bond(id);
}
#[test]
#[should_panic(expected = "CONFLICT")]
fn conflict() {
    let (mut t, id) = bound();
    t.settle_bond(id.clone(), "x".into(), Outcome::SLASH);
    t.settle_bond(id, "x".into(), Outcome::REFUND);
}
#[test]
#[should_panic(expected = "STATE")]
fn pending_retry() {
    let (mut t, id) = bound();
    t.settle_bond(id.clone(), "x".into(), Outcome::REFUND);
    t.retry_failed_refund(id);
}
#[test]
#[should_panic(expected = "BIND_EXPIRED")]
fn no_late_bind() {
    let (mut t, id) = deposited();
    set("goal.testnet", 0, 20);
    t.bind_bond(id);
}
#[test]
#[should_panic(expected = "FUND_STORAGE_SEPARATELY")]
fn underfunded_deposit() {
    let (mut t, id) = setup();
    let mut c = ctx("payer.testnet", 2 * N, 2);
    c.account_balance(NearToken::from_yoctonear(2 * N));
    testing_env!(c.build());
    t.deposit_bond(id);
}
#[test]
#[should_panic(expected = "REFUND_GAS")]
fn low_gas() {
    let (mut t, id) = bound();
    let mut c = ctx("goal.testnet", 0, 3);
    c.prepaid_gas(Gas::from_tgas(20));
    testing_env!(c.build());
    t.settle_bond(id, "x".into(), Outcome::REFUND);
}
#[test]
#[should_panic(expected = "STATE")]
fn slash_unbound_rejected() {
    let (mut t, id) = deposited();
    set("goal.testnet", 0, 3);
    t.settle_bond(id, "x".into(), Outcome::SLASH);
}
#[test]
#[should_panic(expected = "PAUSED")]
fn pause_deposit() {
    let (mut t, id) = setup();
    t.set_paused(true);
    set("payer.testnet", 2 * N, 2);
    t.deposit_bond(id);
}
#[test]
fn economic_boundary_reference_only() {
    // Proposed Goal formula only: Treasury does not derive B or F from its balance.
    fn calc(f: u128) -> (u128, u128) {
        let b = (f / 5).max(N).min(5 * N);
        (b, (f / 10).min(b / 2).min(2 * N))
    }
    assert_eq!(calc(0), (N, 0));
    assert_eq!(calc(10 * N), (2 * N, N));
    assert_eq!(calc(5 * N - 1), (N, (5 * N - 1) / 10));
    assert_eq!(calc(25 * N + 9), (5 * N, 2 * N));
    assert_eq!(calc(u128::MAX), (5 * N, 2 * N));
}
#[test]
#[should_panic(expected = "SETTLEMENT_REPLAY")]
fn settlement_id_cannot_cross_escrows() {
    let (mut t, id) = bound();
    t.settle_bond(id, "s".into(), Outcome::SLASH);
    set("admin.testnet", 0, 4);
    let id2 = t.register_demo_case(
        a("goal.testnet"),
        "p".into(),
        "2".into(),
        "0".into(),
        a("payer.testnet"),
        U128(2 * N),
        10,
        20,
        30,
    );
    set("payer.testnet", 2 * N, 5);
    t.deposit_bond(id2.clone());
    set("goal.testnet", 0, 6);
    t.bind_bond(id2.clone());
    t.settle_bond(id2, "s".into(), Outcome::SLASH);
}
#[test]
#[should_panic(expected = "SETTLEMENT_EXPIRED")]
fn late_arbitration_slash_rejected_at_boundary() {
    let (mut t, id) = bound();
    set("goal.testnet", 0, 30);
    t.settle_bond(id, "late".into(), Outcome::SLASH);
}
#[test]
#[should_panic(expected = "CONFLICT")]
fn arbitration_cannot_override_timeout_refund() {
    let (mut t, id) = bound();
    set("any.testnet", 0, 30);
    t.refund_expired(id.clone());
    testing_env!(
        ctx("treasury.testnet", 0, 31).build(),
        test_vm_config(),
        RuntimeFeesConfig::test(),
        HashMap::default(),
        vec![PromiseResult::Successful(vec![])]
    );
    assert!(t.on_refund(id.clone(), 1));
    assert_eq!(t.bond(&id).status, Some(Status::Refunded));
    assert_eq!(t.locked + t.pending + t.buyback, 0);
    set("goal.testnet", 0, 32);
    t.settle_bond(id, "late".into(), Outcome::SLASH);
}
#[test]
#[should_panic(expected = "NOT_EXPIRED")]
fn timeout_cannot_override_prior_slash() {
    let (mut t, id) = bound();
    set("goal.testnet", 0, 29);
    t.settle_bond(id.clone(), "valid".into(), Outcome::SLASH);
    set("any.testnet", 0, 30);
    t.refund_expired(id);
}
#[test]
#[should_panic(expected = "NOT_EXPIRED")]
fn timeout_cannot_repeat_pending_refund() {
    let (mut t, id) = bound();
    set("any.testnet", 0, 30);
    t.refund_expired(id.clone());
    t.refund_expired(id);
}
fn intent(proof: &str, case: &str) -> Intent {
    Intent {
        proof_id: proof.into(),
        case_id: case.into(),
        submission_id: "0".into(),
        payer: a("payer.testnet"),
        fee_snapshot: U128(10 * N),
        amount: U128(2 * N),
        reward_cap: U128(N),
        deposit_until_ns: 10,
        bind_until_ns: 20,
        refund_after_ns: 30,
    }
}
fn production() -> (Treasury, String) {
    let (mut t, _) = setup();
    set("goal.testnet", 0, 2);
    let id = t.register_intent(intent("p2", "c2"));
    set("payer.testnet", 2 * N, 3);
    t.deposit_bond(id.clone());
    set("goal.testnet", 0, 4);
    t.bind_bond(id.clone());
    (t, id)
}
fn won() -> (Treasury, String) {
    let (mut t, id) = production();
    t.authorize_decision(id.clone(), Decision::ProofInvalidFinal);
    t.settle_bond(id.clone(), "refund-win".into(), Outcome::REFUND);
    testing_env!(
        ctx("treasury.testnet", 0, 5).build(),
        test_vm_config(),
        RuntimeFeesConfig::test(),
        HashMap::default(),
        vec![PromiseResult::Successful(vec![])]
    );
    assert!(t.on_refund(id.clone(), 1));
    (t, id)
}
#[test]
fn funded_reward_success_replay() {
    let (mut t, id) = won();
    set("goal.testnet", N, 6);
    let fid = t.fund_reward(id, "f".into());
    assert_eq!(t.reward_locked, N);
    set("goal.testnet", 0, 7);
    t.pay_reward(fid.clone(), "reward-win".into());
    assert_eq!(t.reward_pending, N);
    assert_eq!(t.reward_locked, 0);
    t.pay_reward(fid.clone(), "reward-win".into());
    assert_eq!(t.rewards.get(&fid).unwrap().attempt, 1);
    testing_env!(
        ctx("treasury.testnet", 0, 8).build(),
        test_vm_config(),
        RuntimeFeesConfig::test(),
        HashMap::default(),
        vec![PromiseResult::Successful(vec![])]
    );
    assert!(t.on_reward(fid.clone(), 1));
    assert!(!t.on_reward(fid.clone(), 1));
    assert_eq!(t.reward_pending + t.reward_locked, 0);
    assert_eq!(t.rewards.get(&fid).unwrap().status, RewardStatus::Paid);
}
#[test]
fn reward_failure_retry() {
    let (mut t, id) = won();
    set("goal.testnet", N, 6);
    let fid = t.fund_reward(id, "f".into());
    set("goal.testnet", 0, 7);
    t.pay_reward(fid.clone(), "r".into());
    testing_env!(
        ctx("treasury.testnet", 0, 8).build(),
        test_vm_config(),
        RuntimeFeesConfig::test(),
        HashMap::default(),
        vec![PromiseResult::Failed]
    );
    assert!(!t.on_reward(fid.clone(), 1));
    assert_eq!(t.reward_locked, N);
    set("any.testnet", 0, 9);
    t.retry_failed_reward(fid.clone());
    assert!(!t.on_reward(fid.clone(), 1));
    testing_env!(
        ctx("treasury.testnet", 0, 10).build(),
        test_vm_config(),
        RuntimeFeesConfig::test(),
        HashMap::default(),
        vec![PromiseResult::Successful(vec![])]
    );
    assert!(t.on_reward(fid, 2));
    assert_eq!(t.reward_locked + t.reward_pending, 0);
}
#[test]
#[should_panic(expected = "DECISION_REQUIRED")]
fn production_slash_needs_verdict() {
    let (mut t, id) = production();
    t.settle_bond(id, "x".into(), Outcome::SLASH);
}
#[test]
fn production_valid_slash() {
    let (mut t, id) = production();
    t.authorize_decision(id.clone(), Decision::ProofValid);
    t.settle_bond(id, "x".into(), Outcome::SLASH);
    assert_eq!(t.buyback, 2 * N);
}
#[test]
#[should_panic(expected = "FINAL_FAILED_REQUIRED")]
fn timeout_has_no_reward() {
    let (mut t, id) = production();
    set("any.testnet", 0, 30);
    t.refund_expired(id.clone());
    set("goal.testnet", N, 31);
    t.fund_reward(id, "f".into());
}
#[test]
#[should_panic(expected = "FINAL_FAILED_REQUIRED")]
fn valid_proof_has_no_reward() {
    let (mut t, id) = production();
    t.authorize_decision(id.clone(), Decision::ProofValid);
    t.settle_bond(id.clone(), "x".into(), Outcome::SLASH);
    set("goal.testnet", N, 6);
    t.fund_reward(id, "f".into());
}
#[test]
#[should_panic(expected = "REWARD_CAP")]
fn excessive_reward() {
    let (mut t, id) = won();
    set("goal.testnet", N + 1, 6);
    t.fund_reward(id, "f".into());
}
#[test]
#[should_panic(expected = "FUNDING_REPLAY")]
fn duplicate_funding() {
    let (mut t, id) = won();
    set("goal.testnet", N, 6);
    t.fund_reward(id.clone(), "f".into());
    t.fund_reward(id, "f2".into());
}
#[test]
#[should_panic(expected = "GOAL_CALLER")]
fn other_goal_cannot_fund() {
    let (mut t, id) = won();
    set("other.testnet", N, 6);
    t.fund_reward(id, "f".into());
}
#[test]
#[should_panic(expected = "PROOF_ALREADY_REGISTERED")]
fn one_challenger_per_proof() {
    let (mut t, _) = production();
    set("goal.testnet", 0, 5);
    t.register_intent(intent("p2", "other-case"));
}
#[test]
fn intent_replay() {
    let (mut t, id) = production();
    set("goal.testnet", 0, 5);
    assert_eq!(t.register_intent(intent("p2", "c2")), id);
}
#[test]
#[should_panic(expected = "ECONOMICS")]
fn bad_economics() {
    let (mut t, _) = setup();
    set("goal.testnet", 0, 2);
    let mut i = intent("p2", "c2");
    i.reward_cap = U128(N + 1);
    t.register_intent(i);
}
#[test]
#[should_panic(expected = "UNAUTHORIZED_GOAL")]
fn outsider_intent() {
    let (mut t, _) = setup();
    set("evil.testnet", 0, 2);
    t.register_intent(intent("p", "c"));
}
#[test]
#[should_panic(expected = "DECISION_CONFLICT")]
fn decision_conflict() {
    let (mut t, id) = production();
    t.authorize_decision(id.clone(), Decision::ProofValid);
    t.authorize_decision(id, Decision::ProofInvalidFinal);
}
#[test]
#[should_panic(expected = "DECISION_EXPIRED")]
fn late_decision() {
    let (mut t, id) = production();
    set("goal.testnet", 0, 30);
    t.authorize_decision(id, Decision::ProofValid);
}
#[test]
#[should_panic(expected = "UNKNOWN_FUNDING")]
fn unfunded_reward() {
    let (mut t, _) = production();
    t.pay_reward("nonexistent".into(), "r".into());
}
#[test]
#[should_panic(expected = "FUND_STORAGE_SEPARATELY")]
fn reward_insolvency() {
    let (mut t, id) = won();
    set("goal.testnet", N, 6);
    let fid = t.fund_reward(id, "f".into());
    let mut c = ctx("goal.testnet", 0, 7);
    c.account_balance(NearToken::from_yoctonear(0));
    testing_env!(c.build());
    t.pay_reward(fid, "r".into());
}
#[test]
#[should_panic(expected = "CASE_ALREADY_REGISTERED")]
fn one_escrow_per_case() {
    let (mut t, _) = production();
    set("goal.testnet", 0, 5);
    let mut i = intent("different-proof", "c2");
    i.submission_id = "1".into();
    t.register_intent(i);
}
#[test]
#[should_panic(expected = "INTENT_CONFLICT")]
fn immutable_intent_payer() {
    let (mut t, _) = production();
    set("goal.testnet", 0, 5);
    let mut i = intent("p2", "c2");
    i.payer = a("evil.testnet");
    t.register_intent(i);
}
#[test]
#[should_panic(expected = "STATE")]
fn reward_pending_retry_rejected() {
    let (mut t, id) = won();
    set("goal.testnet", N, 6);
    let fid = t.fund_reward(id, "f".into());
    set("goal.testnet", 0, 7);
    t.pay_reward(fid.clone(), "r".into());
    t.retry_failed_reward(fid);
}
#[test]
#[should_panic(expected = "CONFLICT")]
fn reward_conflict_rejected() {
    let (mut t, id) = won();
    set("goal.testnet", N, 6);
    let fid = t.fund_reward(id, "f".into());
    set("goal.testnet", 0, 7);
    t.pay_reward(fid.clone(), "r".into());
    t.pay_reward(fid, "different".into());
}
#[test]
#[should_panic(expected = "GOAL_CALLER")]
fn other_goal_cannot_pay() {
    let (mut t, id) = won();
    set("goal.testnet", N, 6);
    let fid = t.fund_reward(id, "f".into());
    set("evil.testnet", 0, 7);
    t.pay_reward(fid, "r".into());
}
#[test]
fn production_neutral_refund() {
    let (mut t, id) = production();
    t.authorize_decision(id.clone(), Decision::NeutralRefund);
    t.settle_bond(id, "neutral".into(), Outcome::REFUND);
    assert_eq!(t.pending, 2 * N);
}
#[test]
fn production_rejected_no_reward() {
    let (mut t, id) = production();
    t.authorize_decision(id.clone(), Decision::QualificationRejected);
    t.settle_bond(id, "rejected".into(), Outcome::REFUND);
    assert_eq!(t.pending, 2 * N);
    assert_eq!(t.reward_locked, 0);
}
#[test]
#[should_panic(expected = "BOND_REFUND_REQUIRED")]
fn reward_requires_confirmed_refund() {
    let (mut t, id) = production();
    t.authorize_decision(id.clone(), Decision::ProofInvalidFinal);
    t.settle_bond(id.clone(), "s".into(), Outcome::REFUND);
    set("goal.testnet", N, 6);
    let fid = t.fund_reward(id, "f".into());
    set("goal.testnet", 0, 7);
    t.pay_reward(fid, "r".into());
}
#[test]
fn view_contract_and_donation() {
    let (mut t, id) = setup();
    assert_eq!(t.interface_version(), "proofee-treasury/0.2.0");
    assert!(t.get_goal(a("goal.testnet")).unwrap().authorized);
    assert!(t.get_bond(id).is_some());
    assert!(t.get_reward("none".into()).is_none());
    assert!(t.get_case_funding("none".into()).is_none());
    assert_eq!(t.get_liabilities().reward_in_flight.0, 0);
    set("donor.testnet", N, 2);
    t.fund_storage();
    set("admin.testnet", 0, 3);
    t.set_goal_authorized(a("goal.testnet"), false);
    assert!(!t.get_goal(a("goal.testnet")).unwrap().authorized);
}
#[test]
#[should_panic(expected = "ZERO")]
fn zero_storage_funding() {
    let (mut t, _) = setup();
    t.fund_storage();
}
#[test]
#[should_panic(expected = "GOAL_EXISTS")]
fn duplicate_goal() {
    let (mut t, _) = setup();
    t.register_goal(a("goal.testnet"), "demo".into());
}
#[test]
#[should_panic(expected = "ADMIN")]
fn outsider_pause() {
    let (mut t, _) = setup();
    set("evil.testnet", 0, 2);
    t.set_paused(true);
}
#[test]
#[should_panic(expected = "ADMIN")]
fn outsider_revoke() {
    let (mut t, _) = setup();
    set("evil.testnet", 0, 2);
    t.set_goal_authorized(a("goal.testnet"), false);
}
#[test]
#[should_panic(expected = "GOAL_CALLER")]
fn outsider_decision() {
    let (mut t, id) = production();
    set("evil.testnet", 0, 5);
    t.authorize_decision(id, Decision::ProofValid);
}
#[test]
#[should_panic(expected = "GOAL_CALLER")]
fn outsider_bind() {
    let (mut t, id) = deposited();
    set("evil.testnet", 0, 5);
    t.bind_bond(id);
}
#[test]
#[should_panic(expected = "REWARD_CAP")]
fn zero_reward_funding() {
    let (mut t, id) = won();
    set("goal.testnet", 0, 6);
    t.fund_reward(id, "f".into());
}
