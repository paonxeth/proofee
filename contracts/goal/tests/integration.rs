// Integration test 11 (SPEC 10): the contract in a local NEAR sandbox.
use std::sync::Arc;

use near_api::types::transaction::actions::{
    Action, CreateAccountAction, DeployContractAction, FunctionCallAction, TransferAction,
};
use near_api::types::transaction::result::TransactionResult;
use near_api::types::{AccountId, Data, NearGas, NearToken};
use near_api::{Account, Chain, Contract, NetworkConfig, Signer, Tokens, Transaction};
use near_sandbox::Sandbox;
use near_sandbox::config::{DEFAULT_GENESIS_ACCOUNT, DEFAULT_GENESIS_ACCOUNT_PRIVATE_KEY};
use near_sdk::serde_json::{self, Value, json};
use testresult::TestResult;

const WINDOW_MS: u64 = 600_000; // 10 minutes: without fast_forward, finalize must fail
const DAY_MS: u64 = 86_400_000;
// Measured in the sandbox: about 300 ms per block, 3000 blocks ≈ 15 min > window.
const FAST_FORWARD_BLOCKS: u64 = 3_000;
const PROOF_HASH: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const SPEC_HASH: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

fn near(n: u128) -> u128 {
    NearToken::from_near(n).as_yoctonear()
}

struct User {
    id: AccountId,
    signer: Arc<Signer>,
}

struct Env {
    // The sandbox lives as long as Env: on drop the process stops.
    sandbox: Sandbox,
    wasm: Vec<u8>,
    net: NetworkConfig,
    genesis: User,
    creator: User,
    stranger: User,
    goal: AccountId,
}

async fn create_user(net: &NetworkConfig, genesis: &User, name: &str) -> TestResult<User> {
    let id: AccountId = format!("{name}.{}", genesis.id).parse()?;
    let secret = near_api::signer::generate_secret_key()?;
    Account::create_account(id.clone())
        .fund_myself(genesis.id.clone(), NearToken::from_near(50))
        .with_public_key(secret.public_key())
        .with_signer(genesis.signer.clone())
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

async fn setup() -> TestResult<Env> {
    let wasm_path = cargo_near_build::build_with_cli(Default::default())?;
    let wasm = std::fs::read(wasm_path)?;

    let sandbox = Sandbox::start_sandbox().await?;
    let net = NetworkConfig::from_rpc_url("sandbox", sandbox.rpc_addr.parse()?);
    let genesis = User {
        id: DEFAULT_GENESIS_ACCOUNT.as_str().parse()?,
        signer: Signer::from_secret_key(DEFAULT_GENESIS_ACCOUNT_PRIVATE_KEY.parse()?)?,
    };
    let creator = create_user(&net, &genesis, "creator").await?;
    let verifier = create_user(&net, &genesis, "verifier").await?;
    let stranger = create_user(&net, &genesis, "stranger").await?;

    let now_ms = block_time_ms(&net).await?;
    let args = json!({
        "creator": creator.id,
        "verifier": verifier.id,
        "goal_title": "Integration goal",
        "goal_uri": "",
        "goal_spec_hash": SPEC_HASH,
        "immediate_bps": 5000,
        "challenge_window_ms": WINDOW_MS,
        "milestones": [
            {"title": "M1", "weight_bps": 6000, "deadline_ms": now_ms + 7 * DAY_MS},
            {"title": "M2", "weight_bps": 2000, "deadline_ms": now_ms + 7 * DAY_MS},
            {"title": "M3", "weight_bps": 2000, "deadline_ms": now_ms + 7 * DAY_MS},
        ],
    });

    let goal: AccountId = format!("goal.{}", creator.id).parse()?;
    deploy_goal(&net, &creator, &goal, &wasm, &args, NearToken::from_near(5))
        .await?
        .assert_success();

    Ok(Env {
        sandbox,
        wasm,
        net,
        genesis,
        creator,
        stranger,
        goal,
    })
}

/// Like scripts/deploy-testnet.sh: one transaction from `owner`, the goal has no keys.
async fn deploy_goal(
    net: &NetworkConfig,
    owner: &User,
    goal: &AccountId,
    wasm: &[u8],
    args: &Value,
    deposit: NearToken,
) -> TestResult<TransactionResult> {
    Ok(Transaction::construct(owner.id.clone(), goal.clone())
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

impl Env {
    async fn call(&self, who: &User, method: &str, args: Value) -> TestResult<TransactionResult> {
        Ok(Contract(self.goal.clone())
            .call_function(method, args)
            .transaction()
            .gas(NearGas::from_tgas(100))
            .with_signer(who.id.clone(), who.signer.clone())
            .send_to(&self.net)
            .await?)
    }

    async fn balances(&self) -> TestResult<Value> {
        let data: Data<Value> = Contract(self.goal.clone())
            .call_function("get_balances", ())
            .read_only()
            .fetch_from(&self.net)
            .await?;
        Ok(data.data)
    }

    async fn field(&self, name: &str) -> TestResult<u128> {
        Ok(self.balances().await?[name].as_str().unwrap().parse()?)
    }

    async fn near_balance(&self, id: &AccountId) -> TestResult<u128> {
        let b = Tokens::account(id.clone())
            .near_balance()
            .fetch_from(&self.net)
            .await?;
        Ok(b.total.as_yoctonear())
    }

    async fn send_fees(&self, amount: u128) -> TestResult {
        Tokens::account(self.genesis.id.clone())
            .send_to(self.goal.clone())
            .near(NearToken::from_yoctonear(amount))
            .with_signer(self.genesis.signer.clone())
            .send_to(&self.net)
            .await?
            .assert_success();
        Ok(())
    }
}

/// Error text of a failed transaction. If the transaction succeeded, the test fails.
fn failure(result: TransactionResult) -> String {
    format!("{:?}", result.assert_failure())
}

#[tokio::test]
async fn full_flow_claim() -> TestResult {
    let env = setup().await?;

    // 1. The initial deposit is not income: everything is zero.
    let start = env.balances().await?;
    for (k, v) in start.as_object().unwrap() {
        assert_eq!(v, "0", "{k} after deploy");
    }

    // 2–3. 10 NEAR of fees, sync from a stranger.
    env.send_fees(near(10)).await?;
    env.call(&env.stranger, "sync", json!({})).await?.assert_success();
    assert_eq!(env.field("total_received").await?, near(10));
    assert_eq!(env.field("claimable").await?, near(5));

    // 4. Proof for milestone 0.
    env.call(
        &env.creator,
        "submit_proof",
        json!({"index": 0, "proof_uri": "ipfs://proof", "proof_hash": PROOF_HASH}),
    )
    .await?
    .assert_success();

    // 5. The window is still open.
    let err = failure(env.call(&env.stranger, "finalize", json!({"index": 0})).await?);
    assert!(err.contains("Window still open"), "{err}");

    // 6. Move time past the window.
    let before = block_time_ms(&env.net).await?;
    env.sandbox.fast_forward(FAST_FORWARD_BLOCKS).await?;
    let after = block_time_ms(&env.net).await?;
    println!(
        "fast_forward({FAST_FORWARD_BLOCKS}) moved time by {} ms",
        after - before
    );
    assert!(after - before > WINDOW_MS, "shift {} ms", after - before);

    // 7. Now finalize succeeds: 5 immediate + 60% of the 5 locked.
    env.call(&env.stranger, "finalize", json!({"index": 0}))
        .await?
        .assert_success();
    assert_eq!(env.field("claimable").await?, near(8));

    // 8. claim: the creator receives 8 NEAR minus their gas, the contract pays out 8 NEAR.
    let tolerance = NearToken::from_millinear(10).as_yoctonear();
    let creator_before = env.near_balance(&env.creator.id).await?;
    let goal_before = env.near_balance(&env.goal).await?;
    env.call(&env.creator, "claim", json!({})).await?.assert_success();
    let creator_after = env.near_balance(&env.creator.id).await?;
    let goal_after = env.near_balance(&env.goal).await?;

    assert_eq!(env.field("claimed").await?, near(8));
    assert_eq!(env.field("claimable").await?, 0);
    let creator_gain = creator_after - creator_before;
    assert!(
        creator_gain <= near(8) && creator_gain > near(8) - tolerance,
        "creator received {creator_gain}"
    );
    let goal_loss = goal_before - goal_after;
    assert!(
        goal_loss.abs_diff(near(8)) < tolerance,
        "contract paid out {goal_loss}"
    );

    // 9. The callback ran and cleared the flag: a second claim gets "Nothing to claim".
    let err = failure(env.call(&env.creator, "claim", json!({})).await?);
    assert!(err.contains("Nothing to claim"), "{err}");
    Ok(())
}

#[tokio::test]
async fn on_claim_transfer_is_private() -> TestResult {
    let env = setup().await?;
    env.send_fees(near(10)).await?;
    env.call(&env.stranger, "sync", json!({})).await?.assert_success();
    let before = env.balances().await?;

    let err = failure(
        env.call(
            &env.stranger,
            "on_claim_transfer",
            json!({"amount": near(1).to_string()}),
        )
        .await?,
    );
    assert!(err.contains("is private"), "{err}");
    assert_eq!(env.balances().await?, before);
    Ok(())
}

/// SPEC 3: the deposit must cover storage plus a 2 × 16 000 byte reserve (AUDIT L-1, L-2).
/// 3 NEAR, as in scripts/deploy-testnet.sh, is enough even with maximum-length strings;
/// 1.9 NEAR is not (code ≈ 2.1 NEAR + reserve 0.4 NEAR).
#[tokio::test]
async fn new_requires_storage_deposit() -> TestResult {
    let env = setup().await?;
    let now_ms = block_time_ms(&env.net).await?;
    // 4-byte characters: the most bytes under character-based limits.
    let long = "𝕏".repeat(100);
    let milestones: Vec<Value> = (0..5)
        .map(|_| json!({"title": long, "weight_bps": 2000, "deadline_ms": now_ms + 7 * DAY_MS}))
        .collect();
    let args = json!({
        "creator": env.creator.id,
        "verifier": env.stranger.id,
        "goal_title": long,
        "goal_uri": "𝕏".repeat(256),
        "goal_spec_hash": SPEC_HASH,
        "immediate_bps": 5000,
        "challenge_window_ms": WINDOW_MS,
        "milestones": milestones,
    });

    let small: AccountId = format!("small.{}", env.creator.id).parse()?;
    let deposit = NearToken::from_millinear(1_900);
    let result = deploy_goal(&env.net, &env.creator, &small, &env.wasm, &args, deposit).await?;
    let err = failure(result);
    assert!(err.contains("Deposit too small for storage"), "{err}");

    let max: AccountId = format!("max.{}", env.creator.id).parse()?;
    let deposit = NearToken::from_near(3);
    deploy_goal(&env.net, &env.creator, &max, &env.wasm, &args, deposit)
        .await?
        .assert_success();
    Ok(())
}
