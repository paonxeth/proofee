// Evidence verification; no signer access required.
const fs=require('fs'),path=require('path'),assert=require('assert/strict');
const dir=process.env.PROOFEE_EVIDENCE_DIR||path.join(__dirname,'../run/evidence');
const read=name=>JSON.parse(fs.readFileSync(path.join(dir,name+'.json'))),checks=[];
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
for(const r of finals){
 const raw=read('final-'+r.label+'-'+r.hash).response.result;
 gas.push({label:r.label,hash:r.hash,gas_burnt:[raw.transaction_outcome,...raw.receipts_outcome].reduce((s,o)=>s+BigInt(o.outcome.gas_burnt),0n).toString(),tokens_burnt:[raw.transaction_outcome,...raw.receipts_outcome].reduce((s,o)=>s+BigInt(o.outcome.tokens_burnt),0n).toString()});
}
// On-chain exact unwrap amounts and outgoing Goal payments are recorded by callbacks
// and independently reconciled from the successful transfer receipts in report tooling.
const security=read('permissionless-security');
check('stranger cannot redirect recipient',security.goal_unchanged&&security.unexpected_receiver_ignored&&BigInt(security.caller_after)<BigInt(security.caller_before));
function swapEvent(name){return read(name).response.receipts_outcome.flatMap(o=>o.outcome.logs||[]).filter(l=>l.startsWith('EVENT_JSON:')).map(l=>JSON.parse(l.slice(11))).find(e=>e.event==='swap'&&e.standard==='dcl.ref').data[0];}
const buyEvent=swapEvent('e2e-buy'),sellEvent=swapEvent('e2e-sell');
const sellTx=read('e2e-sell').response,sellGas=[sellTx.transaction_outcome,...sellTx.receipts_outcome].reduce((sum,o)=>sum+BigInt(o.outcome.tokens_burnt),0n);
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
fs.writeFileSync(path.join(dir,'verification.json'),JSON.stringify(result,null,2));
console.log(JSON.stringify(result,null,2));
