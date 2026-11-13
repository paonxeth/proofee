//! One immutable launch per keyless account. No arbitrary calls, upgrades, LP transfers,
//! principal removal, recipient changes, or native-balance sweep.
use near_sdk::{env, near, require, AccountId, Gas, NearToken, PanicOnDefault, Promise, PromiseResult};
use near_sdk::json_types::U128;
use near_sdk::serde_json::{self, json, Value};
use std::collections::BTreeMap;

const DCL: &str = "dclv2.ref-dev.testnet";
const WRAP: &str = "wrap.testnet";
const ONE: NearToken = NearToken::from_yoctonear(1);

#[near(serializers=[json,borsh])]
#[derive(Clone, Debug, PartialEq)]
pub enum Phase {
    Pool, Seed, Verify, Baseline, Active, ReadFees, WithdrawX, CheckX,
    SwapBaseline, Quote, Swap, CheckSwap, WithdrawY, CheckY, Unwrap, Pay,
    Busy,
}

#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct Locker {
    creator_id: AccountId,
    goal_id: AccountId,
    token_id: AccountId,
    pool_id: String,
    supply: u128,
    init_point: i32,
    left_point: i32,
    right_point: i32,
    phase: Phase,
    pending: Option<Phase>,
    lpt_id: Option<String>,
    principal_liquidity: String,
    dust_x: u128,
    dust_y: u128,
    fee_x: u128,
    fee_y: u128,
    native_due: u128,
    paid: u128,
    native_before: u128,
    min_output: u128,
    wrap_before: u128,
    swapped_wnear: u128,
    last_error: Option<String>,
}

fn account(id: &str) -> AccountId { id.parse().unwrap() }
fn parse<T: near_sdk::serde::de::DeserializeOwned>() -> Option<T> {
    match env::promise_result(0) {
        PromiseResult::Successful(bytes) => serde_json::from_slice(&bytes).ok(),
        _ => None,
    }
}
fn succeeded() -> bool { matches!(env::promise_result(0), PromiseResult::Successful(_)) }

#[near]
impl Locker {
    #[init]
    pub fn new(creator_id: AccountId, goal_id: AccountId, token_id: AccountId,
        decimals: u8, init_point: i32, left_point: i32, right_point: i32) -> Self {
        for id in [&creator_id, &goal_id, &token_id, &env::current_account_id()] {
            require!(id.as_str().ends_with(".testnet"), "Testnet only");
        }
        require!(goal_id != env::current_account_id(), "Goal cannot be locker");
        require!(token_id.as_str() < WRAP, "Token must be DCL token_x");
        require!(decimals <= 29, "Supply overflows u128");
        require!(init_point >= -800000 && right_point <= 800000, "Point out of bounds");
        require!(init_point < left_point && left_point < right_point, "One-sided range required");
        require!(init_point % 200 == 0 && left_point % 200 == 0 && right_point % 200 == 0, "Point delta 200");
        let supply = 1_000_000_000u128.checked_mul(10u128.pow(decimals as u32)).expect("Supply overflow");
        Self { creator_id, goal_id, pool_id: format!("{}|{}|10000", token_id, WRAP), token_id,
            supply, init_point, left_point, right_point, phase: Phase::Pool, pending: None,
            lpt_id: None, principal_liquidity: "0".into(), dust_x: 0, dust_y: 0,
            fee_x: 0, fee_y: 0, native_due: 0, paid: 0, native_before: 0, min_output: 0, wrap_before: 0, swapped_wnear: 0, last_error: None }
    }

    pub fn get_launch(&self) -> Value {
        json!({"creator_id":self.creator_id,"goal_id":self.goal_id,"token_id":self.token_id,
            "pool_id":self.pool_id,"lpt_id":self.lpt_id,"supply":self.supply.to_string(),
            "phase":self.phase,"pending":self.pending,"principal_liquidity":self.principal_liquidity,
            "dust_x":self.dust_x.to_string(),"dust_y":self.dust_y.to_string(),
            "fee_x":self.fee_x.to_string(),"fee_y":self.fee_y.to_string(),
            "swapped_wnear":self.swapped_wnear.to_string(),"native_due":self.native_due.to_string(),"native_paid":self.paid.to_string(),
            "last_error":self.last_error})
    }

    /// Permissionless; every destination and amount comes from immutable configuration/state.
    /// One external operation per invocation keeps callback gas bounded.
    pub fn advance(&mut self) -> Promise {
        require!(self.phase != Phase::Busy && self.phase != Phase::Active, "No advance available");
        let phase = self.phase.clone();
        self.native_before = env::account_balance().as_yoctonear().saturating_sub(
            if matches!(phase, Phase::Swap | Phase::Unwrap) { 1 } else { 0 });
        let (id, method, args, deposit, gas) = match phase {
            Phase::Pool => (account(DCL), "create_pool", json!({"token_a":self.token_id,"token_b":WRAP,
                "fee":10000,"init_point":self.init_point}), NearToken::from_millinear(100), 180),
            Phase::Seed => (self.token_id.clone(), "ft_transfer_call", json!({"receiver_id":DCL,
                "amount":self.supply.to_string(),"msg":json!({"HotZap":{
                    "swap_infos":[{"pool_ids":[self.pool_id],"input_token":self.token_id,
                        "output_token":WRAP,"amount_in":"0","min_output_amount":"0"}],
                    "add_liquidity_infos":[{"pool_id":self.pool_id,"left_point":self.left_point,
                        "right_point":self.right_point,"amount_x":self.supply.to_string(),"amount_y":"0",
                        "min_amount_x":(self.supply / 100 * 99).to_string(),"min_amount_y":"0"}]}}).to_string()}), ONE, 210),
            Phase::Verify => (account(DCL), "list_liquidities", json!({"account_id":env::current_account_id()}), NearToken::from_yoctonear(0), 15),
            Phase::Baseline | Phase::ReadFees => (account(DCL), "list_user_assets", json!({"account_id":env::current_account_id()}), NearToken::from_yoctonear(0), 15),
            Phase::WithdrawX => (account(DCL), "withdraw_asset", json!({"token_id":self.token_id,
                "amount":self.fee_x.to_string(),"skip_unwrap_near":true}), NearToken::from_yoctonear(0), 100),
            Phase::CheckX => (self.token_id.clone(), "ft_balance_of", json!({"account_id":env::current_account_id()}), NearToken::from_yoctonear(0), 15),
            Phase::SwapBaseline | Phase::CheckSwap => (account(WRAP), "ft_balance_of", json!({"account_id":env::current_account_id()}), NearToken::from_yoctonear(0), 15),
            Phase::Quote => (account(DCL), "quote", json!({"pool_ids":[self.pool_id],"input_token":self.token_id,
                "output_token":WRAP,"input_amount":self.fee_x.to_string()}), NearToken::from_yoctonear(0), 30),
            Phase::Swap => (self.token_id.clone(), "ft_transfer_call", json!({"receiver_id":DCL,
                "amount":self.fee_x.to_string(),"msg":json!({"Swap":{"pool_ids":[self.pool_id],
                    "output_token":WRAP,"skip_unwrap_near":true,"min_output_amount":self.min_output.to_string()}}).to_string()}), ONE, 210),
            Phase::WithdrawY => (account(DCL), "withdraw_asset", json!({"token_id":WRAP,
                "amount":self.fee_y.to_string(),"skip_unwrap_near":true}), NearToken::from_yoctonear(0), 100),
            Phase::CheckY => (account(WRAP), "ft_balance_of", json!({"account_id":env::current_account_id()}), NearToken::from_yoctonear(0), 15),
            Phase::Unwrap => (account(WRAP), "near_withdraw", json!({"amount":(self.fee_y+self.swapped_wnear).to_string()}), ONE, 50),
            Phase::Pay => {
                require!(self.native_due > 0, "No revenue");
                require!(env::account_balance().as_yoctonear() >= self.native_due +
                    env::storage_byte_cost().as_yoctonear() * (env::storage_usage() as u128 + 128), "Storage reserve");
                self.begin(phase);
                return Promise::new(self.goal_id.clone()).transfer(NearToken::from_yoctonear(self.native_due))
                    .then(self.callback());
            },
            _ => env::panic_str("Invalid phase"),
        };
        self.begin(phase);
        Promise::new(id).function_call(method, serde_json::to_vec(&args).unwrap(), deposit, Gas::from_tgas(gas))
            .then(self.callback())
    }

    /// Only zero-liquidity removal exists. There is no public amount or receiver argument.
    pub fn collect(&mut self) -> Promise {
        require!(self.phase == Phase::Active, "Collection already running or launch incomplete");
        self.begin(Phase::Active);
        Promise::new(account(DCL)).function_call("remove_liquidity", serde_json::to_vec(&json!({
            "lpt_id":self.lpt_id,"amount":"0","min_amount_x":"0","min_amount_y":"0",
            "skip_refund_transfer":true})).unwrap(), NearToken::from_yoctonear(0), Gas::from_tgas(150))
            .then(self.callback())
    }

    #[private]
    pub fn on_complete(&mut self) {
        require!(self.phase == Phase::Busy, "No operation pending");
        let phase = self.pending.take().expect("Pending operation");
        self.phase = phase.clone();
        if !succeeded() {
            // Retrying transfers/seed after an ambiguous receipt is unsafe. Read-only
            // reconciliation phases remain available; irreversible ambiguity stays frozen.
            self.last_error = Some(format!("{:?} failed; inspect receipts", phase));
            if matches!(phase, Phase::Seed | Phase::WithdrawX | Phase::WithdrawY | Phase::Swap | Phase::Unwrap) {
                self.phase = Phase::Busy;
                self.pending = Some(phase);
            }
            return;
        }
        self.last_error = None;
        match phase {
            Phase::Pool => self.phase = Phase::Seed,
            Phase::Seed => {
                match parse::<U128>() {
                    Some(used) if used.0 == self.supply => self.phase = Phase::Verify,
                    Some(U128(0)) => self.phase = Phase::Seed,
                    _ => self.freeze("Partial seed; inspect receipts", Phase::Seed),
                }
            },
            Phase::Verify => {
                if let Some(v) = parse::<Vec<Value>>() {
                    if v.len() == 1 && v[0]["owner_id"] == env::current_account_id().as_str()
                        && v[0]["pool_id"] == self.pool_id && v[0]["left_point"] == self.left_point
                        && v[0]["right_point"] == self.right_point {
                        if let (Some(id),Some(amount))=(v[0]["lpt_id"].as_str(),v[0]["amount"].as_str()) {
                            if amount.parse::<u128>().unwrap_or(0) > 0 {
                                self.lpt_id=Some(id.into()); self.principal_liquidity=amount.into(); self.phase=Phase::Baseline;
                            }
                        }
                    }
                }
            },
            Phase::Baseline | Phase::ReadFees => {
                if let Some(assets) = parse::<BTreeMap<String,U128>>() {
                    let x=assets.get(self.token_id.as_str()).map(|x|x.0).unwrap_or(0);
                    let y=assets.get(WRAP).map(|x|x.0).unwrap_or(0);
                    if phase == Phase::Baseline {
                        self.dust_x=x; self.dust_y=y; self.phase=Phase::Active;
                    } else if x >= self.dust_x && y >= self.dust_y {
                        self.fee_x=x-self.dust_x; self.fee_y=y-self.dust_y;
                        self.phase=self.after_read();
                    }
                }
            },
            Phase::Active => self.phase=Phase::ReadFees,
            Phase::WithdrawX => self.phase=Phase::CheckX,
            Phase::CheckX => {
                if parse::<U128>().map(|x|x.0>=self.fee_x).unwrap_or(false) { self.phase=Phase::SwapBaseline; }
                else { self.last_error=Some("Token withdrawal not delivered; reconcile DCL receipts".into()); }
            },
            Phase::Quote => {
                if let Some(q)=parse::<Value>() {
                    if let Some(amount)=q["amount"].as_str().and_then(|s|s.parse::<u128>().ok()) {
                        self.min_output=amount/100*99;
                        if self.min_output>0 { self.phase=Phase::Swap; }
                    }
                }
            },
            Phase::SwapBaseline => {
                if let Some(v)=parse::<U128>() { self.wrap_before=v.0; self.phase=Phase::Quote; }
            },
            Phase::Swap => {
                if let Some(used)=parse::<U128>() {
                    if used.0 == self.fee_x { self.phase=Phase::CheckSwap; }
                    else if used.0 == 0 { self.phase=Phase::Quote; }
                    else { self.freeze("Partial swap", Phase::Swap); }
                } else { self.freeze("Unknown swap result", Phase::Swap); }
            },
            Phase::CheckSwap => {
                if let Some(v)=parse::<U128>() {
                    if v.0>self.wrap_before {
                        self.swapped_wnear=v.0-self.wrap_before; self.fee_x=0; self.phase=self.after_read();
                    } else { self.last_error=Some("Swap did not deliver wNEAR; inspect receipts".into()); }
                }
            },
            Phase::WithdrawY => self.phase=Phase::CheckY,
            Phase::CheckY => {
                if parse::<U128>().map(|x|x.0>=self.fee_y+self.swapped_wnear).unwrap_or(false) { self.phase=Phase::Unwrap; }
                else { self.last_error=Some("wNEAR withdrawal not delivered; reconcile DCL receipts".into()); }
            },
            Phase::Unwrap => {
                let delta=env::account_balance().as_yoctonear().saturating_sub(self.native_before);
                if delta>=self.fee_y+self.swapped_wnear {
                    self.native_due=self.native_due.checked_add(self.fee_y+self.swapped_wnear).expect("Overflow");
                    self.fee_y=0; self.swapped_wnear=0; self.phase=self.after_read();
                } else { self.freeze("Unwrap missing native settlement", Phase::Unwrap); }
            },
            Phase::Pay => {
                self.paid=self.paid.checked_add(self.native_due).expect("Overflow");
                self.native_due=0; self.phase=Phase::Active;
            },
            _ => (),
        }
    }
}
impl Locker {
    fn begin(&mut self, phase: Phase) {
        require!(env::prepaid_gas() >= Gas::from_tgas(270), "Attach at least 270 TGas");
        self.pending=Some(phase); self.phase=Phase::Busy;
    }
    fn callback(&self) -> Promise {
        Promise::new(env::current_account_id()).function_call("on_complete", b"{}".to_vec(),
            NearToken::from_yoctonear(0), Gas::from_tgas(30))
    }
    fn after_read(&self) -> Phase {
        if self.fee_x>0 { Phase::WithdrawX }
        else if self.fee_y>0 { Phase::WithdrawY }
        else if self.swapped_wnear>0 { Phase::Unwrap }
        else if self.native_due>0 { Phase::Pay } else { Phase::Active }
    }
    fn freeze(&mut self, error: &str, phase: Phase) {
        self.last_error=Some(error.into()); self.phase=Phase::Busy; self.pending=Some(phase);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use near_sdk::{testing_env, test_utils::{VMContextBuilder, get_created_receipts}};
    fn context(result: Option<PromiseResult>, balance: u128) {
        let mut b=VMContextBuilder::new();
        b.current_account_id(account("locker.demo.testnet")).predecessor_account_id(account("caller.testnet"))
            .account_balance(NearToken::from_yoctonear(balance)).prepaid_gas(Gas::from_tgas(300));
        testing_env!(b.build(), near_sdk::test_vm_config(), near_sdk::RuntimeFeesConfig::test(),
            std::collections::HashMap::new(), result.into_iter().collect());
    }
    fn setup() -> Locker {
        context(None, 10u128.pow(25));
        Locker::new(account("creator.testnet"), account("goal.testnet"), account("token.demo.testnet"),18,-69400,-69200,-59400)
    }
    fn callback(c: &mut Locker, phase: Phase, result: PromiseResult) {
        c.phase=Phase::Busy; c.pending=Some(phase); context(Some(result),10u128.pow(25)); c.on_complete();
    }
    fn ok(value: Value) -> PromiseResult { PromiseResult::Successful(serde_json::to_vec(&value).unwrap()) }
    #[test] fn fixed_supply_and_recipient() {
        let c=setup(); assert_eq!(c.supply,1_000_000_000*10u128.pow(18));
        assert_eq!(c.goal_id,account("goal.testnet"));
    }
    #[test] #[should_panic(expected="Testnet only")] fn rejects_mainnet() {
        context(None,10u128.pow(25)); Locker::new(account("creator.near"),account("goal.testnet"),account("token.testnet"),18,0,200,10000);
    }
    #[test] #[should_panic(expected="One-sided range required")] fn rejects_overlapping_range() {
        context(None,10u128.pow(25)); Locker::new(account("creator.testnet"),account("goal.testnet"),account("token.testnet"),18,0,0,10000);
    }
    #[test] fn failed_seed_can_only_retry_after_full_refund() {
        let mut c=setup(); callback(&mut c,Phase::Seed,ok(json!("0"))); assert_eq!(c.phase,Phase::Seed);
        callback(&mut c,Phase::Seed,ok(json!("123"))); assert_eq!(c.phase,Phase::Busy);
    }
    #[test] fn ambiguous_withdraw_freezes() {
        let mut c=setup(); callback(&mut c,Phase::WithdrawX,PromiseResult::Failed);
        assert_eq!(c.phase,Phase::Busy); assert_eq!(c.pending,Some(Phase::WithdrawX));
    }
    #[test] fn principal_dust_is_never_counted_as_fees() {
        let mut c=setup(); c.dust_x=100; c.dust_y=7;
        callback(&mut c,Phase::ReadFees,ok(json!({"token.demo.testnet":"150","wrap.testnet":"17"})));
        assert_eq!((c.fee_x,c.fee_y),(50,10)); assert_eq!(c.phase,Phase::WithdrawX);
    }
    #[test] fn zero_collect_is_no_income() {
        let mut c=setup(); c.dust_x=100;
        callback(&mut c,Phase::ReadFees,ok(json!({"token.demo.testnet":"100"})));
        assert_eq!(c.native_due,0); assert_eq!(c.phase,Phase::Active);
    }
    #[test] fn failed_payment_keeps_exact_debt() {
        let mut c=setup(); c.native_due=99;
        callback(&mut c,Phase::Pay,PromiseResult::Failed);
        assert_eq!(c.native_due,99); assert_eq!(c.phase,Phase::Pay); assert_eq!(c.paid,0);
        callback(&mut c,Phase::Pay,ok(json!(null)));
        assert_eq!((c.native_due,c.paid),(0,99)); assert_eq!(c.phase,Phase::Active);
    }
    #[test] fn collect_receipt_cannot_remove_principal() {
        let mut c=setup(); c.phase=Phase::Active; c.lpt_id=Some("pool#1".into());
        let p=c.collect(); drop(p);
        let receipts=get_created_receipts();
        let action=receipts.iter().flat_map(|r|r.actions.iter()).find_map(|a| match a {
            near_sdk::mock::MockAction::FunctionCallWeight { method_name,args,.. } if method_name == b"remove_liquidity" => Some(args), _=>None
        }).expect("Remove call");
        let args:Value=serde_json::from_slice(action).unwrap();
        assert_eq!(args["amount"],"0"); assert_eq!(args["skip_refund_transfer"],true);
        assert_eq!(c.phase,Phase::Busy);
    }
    #[test] #[should_panic(expected="Collection already running")] fn concurrent_collect_rejected() {
        let mut c=setup(); c.phase=Phase::Busy; c.collect();
    }
    #[test] fn position_owner_and_pool_checked() {
        let mut c=setup(); let pool=c.pool_id.clone(); callback(&mut c,Phase::Verify,ok(json!([{"owner_id":"attacker.testnet","pool_id":pool,"left_point":-69200,"right_point":-59400,"lpt_id":"x","amount":"1"}])));
        assert_eq!(c.phase,Phase::Verify); assert!(c.lpt_id.is_none());
    }
    #[test] fn unwrap_accounts_exact_wnear_amount_not_native_reward_or_reserve() {
        let mut c=setup(); c.fee_y=30; c.swapped_wnear=40; c.native_due=5;
        c.native_before=0; callback(&mut c,Phase::Unwrap,ok(json!(null)));
        assert_eq!(c.native_due,75); assert_eq!(c.fee_y,0); assert_eq!(c.swapped_wnear,0);
        assert_eq!(c.phase,Phase::Pay);
    }
    #[test] fn conversion_uses_actual_wnear_balance_delta() {
        let mut c=setup(); c.fee_x=99; c.wrap_before=10;
        callback(&mut c,Phase::CheckSwap,ok(json!("52")));
        assert_eq!(c.swapped_wnear,42); assert_eq!(c.fee_x,0); assert_eq!(c.phase,Phase::Unwrap);
    }
    #[test] fn native_only_swap_result_does_not_get_mislabelled_as_wnear() {
        let mut c=setup(); c.wrap_before=0; c.fee_x=99;
        callback(&mut c,Phase::CheckSwap,ok(json!("0")));
        assert_eq!(c.phase,Phase::CheckSwap); assert_eq!(c.native_due,0); assert_eq!(c.fee_x,99);
    }

}
