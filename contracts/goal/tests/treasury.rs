// SPEC 10, test 12: the goal with the real Bond Treasury v0.2.0 wasm in a local sandbox.
// The goal is built with the test-only feature `short-timeouts` (SPEC 6a): 24 h would need
// ~310 000 blocks of fast_forward, about 45 minutes per test.
use std::sync::{Arc, OnceLock};

use near_api::types::transaction::actions::{
    Action, CreateAccountAction, DeployContractAction, FunctionCallAction, TransferAction,
};
use near_api::types::transaction::result::TransactionResult;
use near_api::types::{AccountId, Data, NearGas, NearToken};
use near_api::{Account, Chain, Contract, NetworkConfig, Signer, Tokens, Transaction};
use near_sandbox::Sandbox;
use near_sandbox::config::{
    DEFAULT_GENESIS_ACCOUNT, DEFAULT_GENESIS_ACCOUNT_PRIVATE_KEY, GenesisAccount, SandboxConfig,
};
use near_sdk::serde_json::{self, Value, json};
use testresult::TestResult;

const TREASURY_WASM: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../bond-treasury/release/bond_treasury.wasm"
);
const WINDOW_MS: u64 = 600_000;
const DAY_MS: u64 = 86_400_000;
// ~300 ms per block: 3000 blocks ≈ 15 min > refund_after (3 + 3 + 5 min with short-timeouts).
const FAST_FORWARD_BLOCKS: u64 = 3_000;
const PROOF_HASH: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const SPEC_HASH: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
/// Must equal STORAGE_RESERVE_BYTES in src/lib.rs (SPEC 3, 5.3).
const STORAGE_RESERVE_BYTES: u64 = 20_000;

fn near(n: u128) -> u128 {
    NearToken::from_near(n).as_yoctonear()
}

/// The goal wasm with `short-timeouts`, built once per test binary into its own target dir,
/// so it never overwrites the release build used by tests/integration.rs.
fn goal_wasm() -> &'static [u8] {
    static WASM: OnceLock<Vec<u8>> = OnceLock::new();
    WASM.get_or_init(|| {
        let opts = cargo_near_build::BuildOpts {
            features: Some("short-timeouts".into()),
            override_cargo_target_dir: Some(
                concat!(env!("CARGO_MANIFEST_DIR"), "/target/short-timeouts").into(),
            ),
            ..Default::default()
        };
        let path = cargo_near_build::build_with_cli(opts).expect("build goal wasm");
        std::fs::read(path).expect("read goal wasm")
    })
}

struct User {
    id: AccountId,
    signer: Arc<Signer>,
}

struct Env {
    sandbox: Sandbox,
    net: NetworkConfig,
    genesis: User,
    creator: User,
    verifier: User,
    challenger: User,
    stranger: User,
    treasury: AccountId,
    goal: AccountId,
}

async fn create_user(net: &NetworkConfig, parent: &User, name: &str) -> TestResult<User> {
    let id: AccountId = format!("{name}.{}", parent.id).parse()?;
    let secret = near_api::signer::generate_secret_key()?;
    Account::create_account(id.clone())
        .fund_myself(parent.id.clone(), NearToken::from_near(200))
        .with_public_key(secret.public_key())
        .with_signer(parent.signer.clone())
        .send_to(net)
        .await?
        .assert_success();
    Ok(User {
        id,
        signer: Signer::from_secret_key(secret)?,
    })
}

async fn block_time_ms(net: &NetworkConfig) -> TestResult<u64> {
    let block = Chain::block().fetch_from(net).await?;
    Ok(block.header.timestamp / 1_000_000)
}

/// One transaction: create the account, fund it, deploy, call `new`. No keys on the account.
async fn deploy(
    net: &NetworkConfig,
    owner: &User,
    id: &AccountId,
    wasm: &[u8],
    args: &Value,
    deposit: NearToken,
) -> TestResult<TransactionResult> {
    Ok(Transaction::construct(owner.id.clone(), id.clone())
        .add_actions(vec![
            Action::CreateAccount(CreateAccountAction {}),
            Action::Transfer(TransferAction { deposit }),
            Action::DeployContract(DeployContractAction {
                code: wasm.to_vec(),
            }),
            Action::FunctionCall(Box::new(FunctionCallAction {
                method_name: "new".to_string(),
                args: serde_json::to_vec(args)?,
                gas: NearGas::from_tgas(100),
                deposit: NearToken::from_near(0),
            })),
        ])
        .with_signer(owner.signer.clone())
        .send_to(net)
        .await?)
}

fn goal_args(
    creator: &AccountId,
    verifier: &AccountId,
    treasury: &AccountId,
    now_ms: u64,
    weights: &[u16],
    long: bool,
) -> Value {
    let title = if long { "𝕏".repeat(100) } else { "M".into() };
    let milestones: Vec<Value> = weights
        .iter()
        .map(|w| json!({"title": title, "weight_bps": w, "deadline_ms": now_ms + 7 * DAY_MS}))
        .collect();
    json!({
        "creator": creator,
        "verifier": verifier,
        "goal_title": title,
        "goal_uri": if long { "𝕏".repeat(256) } else { String::new() },
        "goal_spec_hash": SPEC_HASH,
        "immediate_bps": 5000,
        "challenge_window_ms": WINDOW_MS,
        "milestones": milestones,
        "treasury": treasury,
    })
}

/// Sandbox with a `testnet` genesis account (Treasury `new` requires the `.testnet` suffix),
/// Treasury at treasury.testnet with the goal registered, and a goal with milestone `weights`.
async fn setup(weights: &[u16], long: bool, challenger_name: &str) -> TestResult<Env> {
    let wasm = goal_wasm();
    let config = SandboxConfig {
        additional_accounts: vec![GenesisAccount::default_with_name("testnet".parse()?)],
        ..Default::default()
    };
    let sandbox = Sandbox::start_sandbox_with_config(config).await?;
    let net = NetworkConfig::from_rpc_url("sandbox", sandbox.rpc_addr.parse()?);
    let key: near_api::types::SecretKey = DEFAULT_GENESIS_ACCOUNT_PRIVATE_KEY.parse()?;
    let genesis = User {
        id: DEFAULT_GENESIS_ACCOUNT.as_str().parse()?,
        signer: Signer::from_secret_key(key.clone())?,
    };
    let root = User {
        id: "testnet".parse()?,
        signer: Signer::from_secret_key(key)?,
    };
    let creator = create_user(&net, &genesis, "creator").await?;
    let verifier = create_user(&net, &genesis, "verifier").await?;
    let challenger = create_user(&net, &genesis, challenger_name).await?;
    let stranger = create_user(&net, &genesis, "stranger").await?;

    let treasury: AccountId = "treasury.testnet".parse()?;
    let goal: AccountId = format!("goal.{}", creator.id).parse()?;
    let treasury_wasm = std::fs::read(TREASURY_WASM)?;
    deploy(
        &net,
        &root,
        &treasury,
        &treasury_wasm,
        &json!({"admin": root.id}),
        NearToken::from_near(10),
    )
    .await?
    .assert_success();
    Contract(treasury.clone())
        .call_function(
            "register_goal",
            json!({"goal": goal, "policy_version": "proofee-goal/1.1"}),
        )
        .transaction()
        .with_signer(root.id.clone(), root.signer.clone())
        .send_to(&net)
        .await?
        .assert_success();

    let now_ms = block_time_ms(&net).await?;
    let args = goal_args(&creator.id, &verifier.id, &treasury, now_ms, weights, long);
    deploy(&net, &creator, &goal, wasm, &args, NearToken::from_near(5))
        .await?
        .assert_success();
    Ok(Env {
        sandbox,
        net,
        genesis,
        creator,
        verifier,
        challenger,
        stranger,
        treasury,
        goal,
    })
}

/// Error text of a failed transaction. If the transaction succeeded, the test fails.
fn failure(result: TransactionResult) -> String {
    format!("{:?}", result.assert_failure())
}

/// The release Treasury wasm aborts without a message (only an `unreachable` trap), so for
/// direct Treasury calls we can only require a failure; the tests then check that the bond and
/// the balances did not change.
fn treasury_refused(result: TransactionResult) {
    let err = failure(result);
    assert!(err.contains("unreachable"), "{err}");
}

impl Env {
    async fn call_on(
        &self,
        contract: &AccountId,
        who: &User,
        method: &str,
        args: Value,
        deposit: u128,
    ) -> TestResult<TransactionResult> {
        Ok(Contract(contract.clone())
            .call_function(method, args)
            .transaction()
            .deposit(NearToken::from_yoctonear(deposit))
            .gas(NearGas::from_tgas(300))
            .with_signer(who.id.clone(), who.signer.clone())
            .send_to(&self.net)
            .await?)
    }

    async fn call(&self, who: &User, method: &str, args: Value) -> TestResult<TransactionResult> {
        self.call_on(&self.goal, who, method, args, 0).await
    }

    async fn view(&self, contract: &AccountId, method: &str, args: Value) -> TestResult<Value> {
        let data: Data<Value> = Contract(contract.clone())
            .call_function(method, args)
            .read_only()
            .fetch_from(&self.net)
            .await?;
        Ok(data.data)
    }

    async fn milestone(&self, index: usize) -> TestResult<Value> {
        Ok(self.view(&self.goal, "get_goal", json!({})).await?["milestones"][index].clone())
    }

    async fn bond(&self, escrow_id: &str) -> TestResult<Value> {
        self.view(&self.treasury, "get_bond", json!({"escrow_id": escrow_id}))
            .await
    }

    async fn total_received(&self) -> TestResult<u128> {
        let b = self.view(&self.goal, "get_balances", json!({})).await?;
        Ok(b["total_received"].as_str().unwrap().parse()?)
    }

    async fn near_balance(&self, id: &AccountId) -> TestResult<u128> {
        let b = Tokens::account(id.clone())
            .near_balance()
            .fetch_from(&self.net)
            .await?;
        Ok(b.total.as_yoctonear())
    }

    async fn storage_usage(&self) -> TestResult<u64> {
        Ok(Account(self.goal.clone())
            .view()
            .fetch_from(&self.net)
            .await?
            .data
            .storage_usage)
    }

    async fn send_fees(&self, amount: u128) -> TestResult {
        Tokens::account(self.genesis.id.clone())
            .send_to(self.goal.clone())
            .near(NearToken::from_yoctonear(amount))
            .with_signer(self.genesis.signer.clone())
            .send_to(&self.net)
            .await?
            .assert_success();
        self.call(&self.stranger, "sync", json!({}))
            .await?
            .assert_success();
        Ok(())
    }

    async fn submit(&self, index: usize, uri: &str) -> TestResult {
        self.call(
            &self.creator,
            "submit_proof",
            json!({"index": index, "proof_uri": uri, "proof_hash": PROOF_HASH}),
        )
        .await?
        .assert_success();
        Ok(())
    }

    /// prepare_challenge → deposit_bond (exactly B) → confirm_escrow. Returns (escrow_id, B).
    async fn bind(&self, index: usize, reason: &str) -> TestResult<(String, u128)> {
        self.call(
            &self.challenger,
            "prepare_challenge",
            json!({"index": index, "reason": reason}),
        )
        .await?
        .assert_success();
        let case = self.milestone(index).await?["case"].clone();
        assert_eq!(case["stage"], "Ready", "{case}");
        let escrow = case["escrow_id"].as_str().unwrap().to_string();
        let amount: u128 = case["amount"].as_str().unwrap().parse()?;
        let deposit = self
            .call_on(
                &self.treasury,
                &self.challenger,
                "deposit_bond",
                json!({"escrow_id": escrow}),
                amount,
            )
            .await?;
        if !deposit.is_success() {
            let now_ns = u128::from(block_time_ms(&self.net).await?) * 1_000_000;
            let liabilities = self
                .view(&self.treasury, "get_liabilities", json!({}))
                .await?;
            panic!(
                "deposit_bond failed for milestone {index}: now {now_ns} ns, case {case}, \
                 treasury {liabilities}"
            );
        }
        self.call(&self.challenger, "confirm_escrow", json!({"index": index}))
            .await?
            .assert_success();
        let m = self.milestone(index).await?;
        assert_eq!(m["status"], "Challenged", "{m}");
        assert_eq!(m["case"]["stage"], "Bound", "{m}");
        assert_eq!(self.bond(&escrow).await?["status"], "Bound");
        Ok((escrow, amount))
    }

    async fn verifier_call(
        &self,
        method: &str,
        index: usize,
        flag: &str,
        value: bool,
    ) -> TestResult {
        self.call(&self.verifier, method, json!({"index": index, flag: value}))
            .await?
            .assert_success();
        Ok(())
    }
}

/// Gas for a handful of calls stays far below this; a bond is at least 1 NEAR.
fn tolerance() -> u128 {
    NearToken::from_millinear(50).as_yoctonear()
}

/// 100 NEAR of fees, 50% locked, milestone 0 weighs 40% → F = 20 NEAR, B = 4 NEAR.
async fn funded_goal() -> TestResult<Env> {
    let env = setup(&[4000, 3000, 3000], false, "challenger").await?;
    env.send_fees(near(100)).await?;
    env.submit(0, "ipfs://proof").await?;
    Ok(env)
}

#[tokio::test]
async fn qualification_rejected_refunds_bond() -> TestResult {
    let env = funded_goal().await?;
    let before = env.near_balance(&env.challenger.id).await?;
    let (escrow, amount) = env.bind(0, "fake").await?;
    assert_eq!(amount, near(4));
    let case = env.milestone(0).await?["case"].clone();
    assert_eq!(case["fee_snapshot"], near(20).to_string());
    let deposited = env.near_balance(&env.challenger.id).await?;
    assert!(before - deposited >= amount, "the bond left the challenger");

    // A stranger can neither qualify nor confirm someone else's escrow.
    let err = failure(
        env.call(
            &env.stranger,
            "qualify",
            json!({"index": 0, "qualified": false}),
        )
        .await?,
    );
    assert!(err.contains("Only verifier"), "{err}");

    env.verifier_call("qualify", 0, "qualified", false).await?;
    let m = env.milestone(0).await?;
    assert_eq!(m["status"], "Submitted", "{m}");
    assert_eq!(m["case"]["stage"], "Settled", "{m}");
    let bond = env.bond(&escrow).await?;
    assert_eq!(bond["status"], "Refunded", "{bond}");
    assert_eq!(bond["decision"], "QualificationRejected");

    let after = env.near_balance(&env.challenger.id).await?;
    assert!(
        before - after < tolerance(),
        "refunded in full: lost {}",
        before - after
    );
    // The bond never went through the goal.
    env.call(&env.stranger, "sync", json!({}))
        .await?
        .assert_success();
    assert_eq!(env.total_received().await?, near(100));
    Ok(())
}

#[tokio::test]
async fn proof_valid_slashes_and_accepts() -> TestResult {
    let env = funded_goal().await?;
    let before = env.near_balance(&env.challenger.id).await?;
    let (escrow, amount) = env.bind(0, "fake").await?;
    let err = failure(
        env.call(
            &env.verifier,
            "arbitrate",
            json!({"index": 0, "valid": true}),
        )
        .await?,
    );
    assert!(
        err.contains("Wrong stage"),
        "arbitrate before qualification: {err}"
    );
    env.verifier_call("qualify", 0, "qualified", true).await?;
    let err = failure(
        env.call(
            &env.stranger,
            "arbitrate",
            json!({"index": 0, "valid": false}),
        )
        .await?,
    );
    assert!(err.contains("Only verifier"), "{err}");

    env.verifier_call("arbitrate", 0, "valid", true).await?;
    let m = env.milestone(0).await?;
    assert_eq!(m["status"], "Accepted", "{m}");
    assert_eq!(m["case"]["stage"], "Settled", "{m}");
    assert_eq!(env.bond(&escrow).await?["status"], "Slashed");
    let buyback = env
        .view(
            &env.treasury,
            "get_buyback_balance",
            json!({"goal": env.goal}),
        )
        .await?;
    assert_eq!(buyback, json!(amount.to_string()));
    let after = env.near_balance(&env.challenger.id).await?;
    assert!(before - after >= amount, "the bond is lost");

    // Settling again pays nothing: through the goal, directly, or by timeout.
    let err = failure(
        env.call(&env.stranger, "retry_settlement", json!({"index": 0}))
            .await?,
    );
    assert!(err.contains("Wrong stage"), "{err}");
    treasury_refused(
        env.call_on(
            &env.treasury,
            &env.challenger,
            "settle_bond",
            json!({"escrow_id": escrow, "settlement_id": "bond:m0-1", "outcome": "REFUND"}),
            0,
        )
        .await?,
    );
    env.sandbox.fast_forward(FAST_FORWARD_BLOCKS).await?;
    treasury_refused(
        env.call_on(
            &env.treasury,
            &env.challenger,
            "refund_expired",
            json!({"escrow_id": escrow}),
            0,
        )
        .await?,
    );
    assert_eq!(env.bond(&escrow).await?["status"], "Slashed");
    let later = env.near_balance(&env.challenger.id).await?;
    assert!(later <= after, "no second payout");
    env.call(&env.stranger, "sync", json!({}))
        .await?
        .assert_success();
    assert_eq!(env.total_received().await?, near(100));
    Ok(())
}

#[tokio::test]
async fn proof_invalid_refunds_and_rejects() -> TestResult {
    let env = funded_goal().await?;
    let before = env.near_balance(&env.challenger.id).await?;
    let (escrow, _) = env.bind(0, "fake").await?;
    env.verifier_call("qualify", 0, "qualified", true).await?;
    env.verifier_call("arbitrate", 0, "valid", false).await?;
    let m = env.milestone(0).await?;
    assert_eq!(m["status"], "Rejected", "{m}");
    let bond = env.bond(&escrow).await?;
    assert_eq!(bond["status"], "Refunded", "{bond}");
    assert_eq!(bond["decision"], "ProofInvalidFinal");
    let after = env.near_balance(&env.challenger.id).await?;
    assert!(
        before - after < tolerance(),
        "refunded: lost {}",
        before - after
    );

    // A second refund is impossible: retry through the goal and the Treasury timeout both fail.
    let err = failure(
        env.call(&env.stranger, "retry_settlement", json!({"index": 0}))
            .await?,
    );
    assert!(err.contains("Wrong stage"), "{err}");
    env.sandbox.fast_forward(FAST_FORWARD_BLOCKS).await?;
    treasury_refused(
        env.call_on(
            &env.treasury,
            &env.stranger,
            "refund_expired",
            json!({"escrow_id": escrow}),
            0,
        )
        .await?,
    );
    let later = env.near_balance(&env.challenger.id).await?;
    assert_eq!(later, after, "no second refund");
    // The rejected share is forfeited from the goal's fees; the bond is not income.
    env.call(&env.stranger, "sync", json!({}))
        .await?
        .assert_success();
    assert_eq!(env.total_received().await?, near(100));
    Ok(())
}

#[tokio::test]
async fn timeout_refunds_neutrally_and_accepts() -> TestResult {
    let env = funded_goal().await?;
    let before = env.near_balance(&env.challenger.id).await?;
    let (escrow, _) = env.bind(0, "fake").await?;
    env.verifier_call("qualify", 0, "qualified", true).await?;
    let err = failure(
        env.call(&env.stranger, "resolve_timeout", json!({"index": 0}))
            .await?,
    );
    assert!(err.contains("Resolve time not passed"), "{err}");

    env.sandbox.fast_forward(FAST_FORWARD_BLOCKS).await?;
    let err = failure(
        env.call(
            &env.verifier,
            "arbitrate",
            json!({"index": 0, "valid": true}),
        )
        .await?,
    );
    assert!(err.contains("Resolve time passed"), "{err}");
    env.call(&env.stranger, "resolve_timeout", json!({"index": 0}))
        .await?
        .assert_success();
    let m = env.milestone(0).await?;
    assert_eq!(m["status"], "Accepted", "{m}");
    assert_eq!(m["case"]["stage"], "TimeoutRequested", "{m}");
    let bond = env.bond(&escrow).await?;
    assert_eq!(bond["status"], "Refunded", "{bond}");
    assert_eq!(bond["decision"], "NeutralRefund");
    let after = env.near_balance(&env.challenger.id).await?;
    assert!(
        before - after < tolerance(),
        "refunded: lost {}",
        before - after
    );

    let err = failure(
        env.call(&env.stranger, "resolve_timeout", json!({"index": 0}))
            .await?,
    );
    assert!(err.contains("Wrong status"), "{err}");
    treasury_refused(
        env.call_on(
            &env.treasury,
            &env.stranger,
            "refund_expired",
            json!({"escrow_id": escrow}),
            0,
        )
        .await?,
    );
    assert_eq!(
        env.near_balance(&env.challenger.id).await?,
        after,
        "no second refund"
    );
    env.call(&env.stranger, "sync", json!({}))
        .await?
        .assert_success();
    assert_eq!(env.total_received().await?, near(100));
    Ok(())
}

#[tokio::test]
async fn outsiders_cannot_drive_the_case() -> TestResult {
    let env = funded_goal().await?;
    env.call(
        &env.challenger,
        "prepare_challenge",
        json!({"index": 0, "reason": "r"}),
    )
    .await?
    .assert_success();
    let err = failure(
        env.call(&env.stranger, "confirm_escrow", json!({"index": 0}))
            .await?,
    );
    assert!(err.contains("Only payer"), "{err}");
    let err = failure(
        env.call(&env.stranger, "on_bound", json!({"index": 0, "nonce": 1}))
            .await?,
    );
    assert!(err.contains("is private"), "{err}");
    let err = failure(
        env.call(
            &env.creator,
            "prepare_challenge",
            json!({"index": 0, "reason": "r"}),
        )
        .await?,
    );
    assert!(err.contains("Creator cannot challenge"), "{err}");
    let err = failure(
        env.call(
            &env.stranger,
            "challenge",
            json!({"index": 0, "reason": "r"}),
        )
        .await?,
    );
    assert!(err.contains("Bond required"), "{err}");
    Ok(())
}

/// SPEC 3, 5.3: after a full claim, the worst-case growth of the state (5 milestones, strings at
/// the limit in 4-byte characters, a 64-character challenger, a rejected request and a new one on
/// every milestone) still fits into the storage reserve that claim never pays out.
#[tokio::test]
async fn storage_reserve_covers_worst_case_growth() -> TestResult {
    // The longest account id: 64 characters in total, "ccc….sandbox".
    let env = setup(&[2000; 5], true, &"c".repeat(64 - ".sandbox".len())).await?;
    assert_eq!(env.challenger.id.as_str().len(), 64);
    env.send_fees(near(100)).await?;
    // Claim before any proof: everything after it must fit into the reserve.
    env.call(&env.creator, "claim", json!({}))
        .await?
        .assert_success();
    let start = env.storage_usage().await?;

    let reason = "𝕏".repeat(280);
    for i in 0..5 {
        env.submit(i, &"𝕏".repeat(256)).await?;
    }
    let after_proofs = env.storage_usage().await?;
    for i in 0..5 {
        env.bind(i, &reason).await?;
    }
    let all_bound = env.storage_usage().await?;
    for i in 0..5 {
        env.verifier_call("qualify", i, "qualified", false).await?;
    }
    let mut peak = all_bound.max(env.storage_usage().await?);
    for i in 0..5 {
        env.bind(i, &reason).await?;
        peak = peak.max(env.storage_usage().await?);
    }
    let growth = peak - start;
    println!(
        "storage: start {start}, proofs +{}, 5 bound cases +{}, peak growth {growth} bytes",
        after_proofs - start,
        all_bound - after_proofs
    );
    assert!(
        growth <= STORAGE_RESERVE_BYTES,
        "growth {growth} > reserve {STORAGE_RESERVE_BYTES}"
    );
    Ok(())
}
