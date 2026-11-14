// Evidence verification; no signer access required.
const fs=require('fs'),path=require('path'),assert=require('assert/strict');
const dir=process.env.PROOFEE_EVIDENCE_DIR||path.join(__dirname,'../evidence');
const {read, receipt}=require('./read-evidence.cjs')(dir),checks=[];
function check(name,value){assert(value,name);checks.push({name,passed:true});}
const pre=read('e2e-before'),buy=read('e2e-after-buy'),sell=read('e2e-after-sell'),col=read('e2e-after-collect'),syn=read('e2e-after-sync'),claim=read('e2e-after-claim'),fin=read('e2e-final');
check('fixed 1e9 supply, 18 decimals',pre.launch.supply==='1000000000000000000000000000');
check('all supply assigned to pool or locked rounding dust',BigInt(pre.pool.total_x)+BigInt(pre.launch.dust_x)===BigInt(pre.launch.supply));
check('buy delivered tokens',BigInt(buy.trader_token)>BigInt(pre.trader_token));
check('sell consumed tokens',BigInt(sell.trader_token)<BigInt(buy.trader_token));
check('both fee assets accrued',BigInt(sell.lp.unclaimed_fee_x)>0n&&BigInt(sell.lp.unclaimed_fee_y)>0n);
check('LP principal unchanged across swaps/collections',pre.lp.amount===fin.lp.amount&&fin.lp.owner_id===read('contracts').locker_id);
check('Goal received native',BigInt(col.goal.unsynced)>0n);
check('20/80 immediate',BigInt(syn.goal.immediate)===BigInt(syn.goal.total_received)*2000n/10000n);
check('20/80 locked',BigInt(syn.goal.locked)===BigInt(syn.goal.total_received)-BigInt(syn.goal.immediate));
check('creator claim paid',BigInt(claim.goal.claimed)>0n);
check('fixed Goal binding',pre.launch.goal_id===fin.launch.goal_id);
const submitted=fs.readFileSync(path.join(dir,'transactions.jsonl'),'utf8').trim().split('\n').map(JSON.parse).filter(r=>r.status==='submitted');
const finals=read('final-transactions');
check('every transaction final',finals.length===submitted.length-read('rejected-transactions').length-read('superseded-transactions').length+1&&finals.every(r=>r.final_execution_status==='FINAL'&&!r.error));
const allowedFailure=label=>label.startsWith('negative-')||label==='create-goal';
check('only documented failures',finals.filter(r=>r.receipt_failures?.length||r.status?.Failure).every(r=>allowedFailure(r.label)));
for(const label of ['negative-locker-remove','negative-change-goal','negative-forged-callback','negative-dcl-wrong-owner','negative-parent-upgrade','negative-stranger-claim'])check(label+' fails on-chain',finals.some(r=>r.label===label&&(r.receipt_failures?.length||r.status?.Failure)));
const gas=[];
const results=new Map();
const contracts=read('contracts');
const current=read('onchain-final');
function transaction(label) {
 const match=finals.find(row=>row.label===label && !row.status.Failure && !row.receipt_failures.length);
 assert(match, 'Missing successful transaction '+label);
 return results.get(match.hash);
}
function events(tx, executor, standard, event) {
 return tx.receipts_outcome.filter(receipt=>receipt.outcome.executor_id===executor)
   .flatMap(receipt=>receipt.outcome.logs)
   .filter(log=>log.startsWith('EVENT_JSON:')).map(log=>JSON.parse(log.slice(11)))
   .filter(log=>log.standard===standard && log.event===event).flatMap(log=>log.data);
}
function initializer(tx, account) {
 assert.equal(tx.transaction.receiver_id, account);
 assert(tx.transaction.actions.includes('CreateAccount'));
 assert(tx.transaction.actions.some(action=>action.DeployContract));
 assert(!tx.transaction.actions.some(action=>action.AddKey));
 const call=tx.transaction.actions.find(action=>action.FunctionCall?.method_name==='new');
 return JSON.parse(Buffer.from(call.FunctionCall.args,'base64'));
}
function transfer(tx, from, to, number) {
 return tx.receipts.some(receipt=>receipt.predecessor_id===from && receipt.receiver_id===to
   && receipt.receipt.Action?.actions.some(action=>action.Transfer?.deposit===number)
   && tx.receipts_outcome.some(outcome=>outcome.id===receipt.receipt_id
     && Object.hasOwn(outcome.outcome.status,'SuccessValue')));
}
for(const r of finals){
 const raw=receipt(r);
 results.set(r.hash,raw);
 gas.push({label:r.label,hash:r.hash,gas_burnt:[raw.transaction_outcome,...raw.receipts_outcome].reduce((s,o)=>s+BigInt(o.outcome.gas_burnt),0n).toString(),tokens_burnt:[raw.transaction_outcome,...raw.receipts_outcome].reduce((s,o)=>s+BigInt(o.outcome.tokens_burnt),0n).toString()});
}
// On-chain exact unwrap amounts and outgoing Goal payments are recorded by callbacks
// and independently reconciled from the successful transfer receipts in report tooling.
const security=read('permissionless-security');
check('stranger cannot redirect recipient',security.goal_unchanged&&security.unexpected_receiver_ignored&&BigInt(security.caller_after)<BigInt(security.caller_before));
const dcl='dclv2.ref-dev.testnet';
const tokenTx=transaction('create-token'), lockerTx=transaction('create-locker');
const token=initializer(tokenTx,contracts.token_id), locker=initializer(lockerTx,contracts.locker_id);
check('token deployment and metadata', token.owner_id===contracts.locker_id
  && token.total_supply===pre.launch.supply && token.metadata.decimals===18
  && token.metadata.symbol===current.token.metadata.symbol
  && current.token.supply===token.total_supply
  && events(tokenTx,contracts.token_id,'nep141','ft_mint')
    .some(event=>event.owner_id===contracts.locker_id && event.amount===token.total_supply));
check('Locker initialized with fixed creator, Goal and token', locker.creator_id===contracts.root
  && locker.goal_id===contracts.goal_id && locker.token_id===contracts.token_id
  && locker.decimals===18 && current.contracts.locker_id.keys.length===0
  && current.contracts.token_id.keys.length===0);
const stage=prefix=>finals.find(row=>row.label.startsWith(prefix)).label;
check('Rhea pool created by Locker at the configured point',
  events(transaction(stage('advance-Pool-')),dcl,'dcl.ref','create_pool')
    .some(event=>event.pool_id===contracts.pool_id && event.account_id===contracts.locker_id
      && event.init_point===locker.init_point));
check('Rhea LP seeded and owned by Locker',
  events(transaction(stage('advance-Seed-')),dcl,'dcl.ref','liquidity_added')
    .some(event=>event.owner_id===contracts.locker_id && event.lpt_id===contracts.lpt_id
      && event.cur_amount===pre.lp.amount && event.paid_token_x===pre.pool.total_x
      && event.left_point===locker.left_point && event.right_point===locker.right_point));
check('fee collection retained LP principal',
  events(transaction('e2e-collect'),dcl,'dcl.ref','liquidity_removed')
    .some(event=>event.lpt_id===contracts.lpt_id && event.cur_amount===pre.lp.amount
      && event.claim_fee_token_x===sell.lp.unclaimed_fee_x
      && event.claim_fee_token_y===sell.lp.unclaimed_fee_y));
check('successful native Locker to Goal transfer', transfer(transaction(stage('advance-Pay-')),
  contracts.locker_id,contracts.goal_id,col.launch.native_paid));
check('Goal sync counted the native transfer',
  events(transaction('e2e-goal-sync'),contracts.goal_id,'proofee','fees_synced')
    .some(event=>event.income===col.launch.native_paid && event.total_received===syn.goal.total_received));
const caller=security.caller;
const redirect=transaction('stranger-collect-redirect-args');
const redirectedPayment=transaction(finals.filter(row=>row.label.startsWith('advance-Pay-')).at(-1).label);
const callerPayment=(BigInt(security.native_paid_after)-BigInt(security.native_paid_before)).toString();
check('third-party collection pays the configured Goal', redirect.transaction.signer_id===caller
  && redirect.transaction.receiver_id===contracts.locker_id
  && redirectedPayment.transaction.signer_id===caller
  && transfer(redirectedPayment,contracts.locker_id,contracts.goal_id,callerPayment)
  && !transfer(redirectedPayment,contracts.locker_id,caller,callerPayment));
check('creator received exact immediate share', claim.goal.claimed===syn.goal.immediate
  && transfer(transaction('e2e-creator-claim'),contracts.goal_id,contracts.root,claim.goal.claimed));
function swapEvent(name) {
 const [event]=events(transaction(name),dcl,'dcl.ref','swap').filter(event=>event.pool_id===contracts.pool_id);
 assert(event,'Missing Rhea swap for '+name);
 return event;
}
const buyEvent=swapEvent('e2e-buy'),sellEvent=swapEvent('e2e-sell');
check('buy and sell use the expected assets',
  buyEvent.token_in==='wrap.testnet' && buyEvent.token_out===contracts.token_id
  && sellEvent.token_in===contracts.token_id && sellEvent.token_out==='wrap.testnet');
const sellTx=transaction('e2e-sell'),sellGas=[sellTx.transaction_outcome,...sellTx.receipts_outcome].reduce((sum,o)=>sum+BigInt(o.outcome.tokens_burnt),0n);
check('sell native settlement equals swap output minus actual gas and 1 yocto',BigInt(sell.trader_native)-BigInt(buy.trader_native)===BigInt(sellEvent.amount_out)-sellGas-1n);
const firstConversion=swapEvent(submitted.find(r=>r.label.startsWith('advance-Swap-')).label);
check('first Goal payment is exactly both LP fee legs after conversion',BigInt(col.launch.native_paid)===BigInt(firstConversion.amount_out)+BigInt(sell.lp.unclaimed_fee_y)&&col.goal.unsynced===col.launch.native_paid);
check('conversion input is bounded by collected token LP fees',BigInt(firstConversion.amount_in)>0n&&BigInt(firstConversion.amount_in)<=BigInt(sell.lp.unclaimed_fee_x));
const repeat=read('e2e-repeat-accounting');check('repeat collect consumes newly accrued fee not original fee again',BigInt(repeat.fees_before.unclaimed_fee_x)>0n&&BigInt(repeat.fees_before.unclaimed_fee_x)<BigInt(sell.lp.unclaimed_fee_x)&&repeat.fees_before.unclaimed_fee_y==='0');
check('LP fees exclude protocol allocation, with integer rounding',
 BigInt(sellEvent.total_fee)-BigInt(sellEvent.protocol_fee)-BigInt(sell.lp.unclaimed_fee_x)===1n &&
 BigInt(buyEvent.total_fee)-BigInt(buyEvent.protocol_fee)-BigInt(sell.lp.unclaimed_fee_y)===1n);
const result={checks,gas,totals:{transactions:finals.length,total_tokens_burnt:gas.reduce((s,r)=>s+BigInt(r.tokens_burnt),0n).toString()},
 initial_supply:pre.launch.supply,initial_lp_token_x:pre.pool.total_x,locked_rounding_dust:pre.launch.dust_x,
 first_lp_fee_token:sell.lp.unclaimed_fee_x,first_lp_fee_wnear:sell.lp.unclaimed_fee_y,
 swap_fee_x:sellEvent.total_fee,swap_fee_y:buyEvent.total_fee,protocol_fee_x:sellEvent.protocol_fee,protocol_fee_y:buyEvent.protocol_fee,
 lp_rounding_residual_x:(BigInt(sellEvent.total_fee)-BigInt(sellEvent.protocol_fee)-BigInt(sell.lp.unclaimed_fee_x)).toString(),
 lp_rounding_residual_y:(BigInt(buyEvent.total_fee)-BigInt(buyEvent.protocol_fee)-BigInt(sell.lp.unclaimed_fee_y)).toString(),
 first_converted_token_in:firstConversion.amount_in,first_fee_conversion_rounding_difference:(BigInt(sell.lp.unclaimed_fee_x)-BigInt(firstConversion.amount_in)).toString(),first_cycle_native_paid:col.launch.native_paid,first_goal_accounted:syn.goal.total_received,first_creator_claim:claim.goal.claimed,final_locker:fin.launch};
console.log(JSON.stringify(result,null,2));
