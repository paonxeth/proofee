const {Worker}=require('near-workspaces');
const assert=require('node:assert/strict');
const path=require('node:path');
const N=10n**24n, GAS='300000000000000';
const passes=[];
(async()=>{const w=await Worker.init({rootAccountId:"testnet"});try{
 const root=w.rootAccount;
 const t=await root.createAccount('proofee-treasury-integrated-v02.testnet',{initialBalance:(50n*N).toString()});
 const g=await root.createAccount('proofee-mock-goal-integrated-v02.testnet',{initialBalance:(30n*N).toString()});
 const other=await root.createAccount('proofee-mock-goal-isolation-v02.testnet',{initialBalance:(30n*N).toString()});
 const payer=await root.createSubAccount('payer',{initialBalance:(50n*N).toString()});
 const verifier=await root.createSubAccount('verifier');const evil=await root.createSubAccount('evil');
 await t.deploy(path.resolve(__dirname,'../../target/wasm32-unknown-unknown/release/bond_treasury.wasm'));
 await root.call(t,'new',{admin:root.accountId},{gas:GAS});
 for(const goal of [g,other]){await goal.deploy(path.resolve(__dirname,'../../target/wasm32-unknown-unknown/release/mock_goal.wasm'));await root.call(goal,'new',{admin:root.accountId,verifier:verifier.accountId,treasury:t.accountId},{gas:GAS});}
 const tx=(who,to,m,args={},amount=0n)=>who.call(to,m,args,{gas:GAS,attachedDeposit:amount.toString()});
 const fail=async(fn)=>{let ok=false;try{await fn()}catch(e){ok=true;}assert(ok,'Expected rejection');};
 for(const goal of [g,other])await tx(root,t,'register_goal',{goal:goal.accountId,policy_version:'proofee-treasury/0.2.0'});
 const view=(c)=>t.view('get_bond',{escrow_id:c.escrow_id});
 async function prepare(id,goal=g,who=payer,seconds=3000){
 const block=await w.provider.block({finality:'final'});let now=BigInt(block.header.timestamp_nanosec);
 const intent={proof_id:`proof:${id}`,case_id:id,submission_id:'0',payer:who.accountId,fee_snapshot:(10n*N).toString(),amount:(2n*N).toString(),reward_cap:N.toString(),deposit_until_ns:'DEPOSIT_NS',bind_until_ns:'BIND_NS',refund_after_ns:'REFUND_NS'};
 const args=JSON.stringify({intent}).replace('"DEPOSIT_NS"',String(now+BigInt(seconds)*1000000000n)).replace('"BIND_NS"',String(now+BigInt(seconds+100)*1000000000n)).replace('"REFUND_NS"',String(now+BigInt(seconds+200)*1000000000n));
 await tx(root,goal,'prepare_challenge',Buffer.from(args));let c=await goal.view('get_case',{case_id:id});assert.equal(c.stage,'Ready');return c;
 }
 async function deposit(c,who=payer){await tx(who,t,'deposit_bond',{escrow_id:c.escrow_id},2n*N);}
 async function bind(c,goal=g,who=payer){await tx(who,goal,'confirm_escrow',{case_id:c.intent.case_id});assert.equal((await goal.view('get_case',{case_id:c.intent.case_id})).stage,'Bound');}
 async function refresh(id,goal=g){await tx(root,goal,'refresh_bond',{case_id:id});return goal.view('get_case',{case_id:id});}
 async function settle(id,valid,goal=g){await tx(root,goal,'qualify',{case_id:id,qualified:true});await tx(verifier,goal,'arbitrate',{case_id:id,valid});}
 await fail(()=>tx(evil,t,'register_goal',{goal:evil.accountId,policy_version:'x'}));
 await fail(()=>tx(evil,t,'register_intent',{intent:{}}));
 await fail(()=>tx(root,t,'register_demo_case',{}));
 let c=await prepare('reject');await fail(()=>tx(evil,t,'deposit_bond',{escrow_id:c.escrow_id},2n*N));await fail(()=>tx(payer,t,'deposit_bond',{escrow_id:c.escrow_id},N));await deposit(c);await fail(()=>deposit(c));await bind(c);
 await fail(()=>tx(evil,g,'qualify',{case_id:'reject',qualified:false}));await fail(()=>tx(evil,t,'on_refund',{escrow_id:c.escrow_id,attempt:1}));await fail(()=>tx(evil,g,'on_bound',{case_id:'reject',attempt:1}));
 const before=BigInt((await payer.balance()).total.toString());await tx(root,g,'qualify',{case_id:'reject',qualified:false});assert.equal((await view(c)).status,'Refunded');assert.equal(BigInt((await payer.balance()).total.toString())-before,2n*N);assert.equal((await refresh('reject')).stage,'Refunded');passes.push('qualification rejection -> full 2 NEAR refund; production ABI/no demo method; unauthorized payer/decision/callback rejected');
 c=await prepare('slash');await deposit(c);await bind(c);await fail(()=>tx(evil,g,'arbitrate',{case_id:'slash',valid:true}));await settle('slash',true);assert.equal((await view(c)).status,'Slashed');assert.equal(await t.view('get_buyback_balance',{goal:g.accountId}),(2n*N).toString());await fail(()=>tx(root,t,'refund_expired',{escrow_id:c.escrow_id}));passes.push('qualified losing challenge -> slash; timeout cannot undo slash');
 c=await prepare('win');await deposit(c);await bind(c);await settle('win',false);assert.equal((await view(c)).status,'Refunded');await refresh('win');
 assert.equal(await t.view('get_case_funding',{escrow_id:c.escrow_id}),null);
 await tx(root,g,'fund_case',{case_id:'win',amount:N.toString()});let wc=await g.view('get_case',{case_id:'win'});assert.equal(wc.stage,'Funded');let r=await t.view('get_reward',{funding_id:wc.funding_id});assert.equal(r.amount,N.toString());assert.equal(r.status,'Funded');
 const rewardBefore=BigInt((await payer.balance()).total.toString());await tx(root,g,'claim_reward',{case_id:'win'});assert.equal((await t.view('get_reward',{funding_id:wc.funding_id})).status,'Paid');assert.equal(BigInt((await payer.balance()).total.toString())-rewardBefore,N);await tx(root,g,'claim_reward',{case_id:'win'});assert.equal(BigInt((await payer.balance()).total.toString())-rewardBefore,N);await tx(root,g,'refresh_reward',{case_id:'win'});assert.equal((await g.view('get_case',{case_id:'win'})).stage,'Paid');passes.push('winning challenge -> principal refund -> actual separate Goal funding -> 1 NEAR reward; replay no double pay');
 let oc=await prepare('win',other);await deposit(oc);await bind(oc,other);await settle('win',true,other);assert.equal((await view(oc)).status,'Slashed');assert.notEqual(oc.escrow_id,c.escrow_id);assert.equal(await t.view('get_buyback_balance',{goal:other.accountId}),(2n*N).toString());passes.push('two Mock Goals same case/proof IDs: isolated principal, rewards and Buyback');
 c=await prepare('timeout',g,payer,30);await deposit(c);await bind(c);await tx(root,g,'qualify',{case_id:'timeout',qualified:true});await w.provider.fastForward(2000);await tx(evil,g,'resolve_timeout',{case_id:'timeout'});assert.equal((await view(c)).status,'Refunded');await refresh('timeout');await fail(()=>tx(verifier,g,'arbitrate',{case_id:'timeout',valid:true}));await fail(()=>tx(root,g,'fund_case',{case_id:'timeout',amount:N.toString()}));passes.push('active arbitration timeout -> neutral refund; late verdict/reward rejected');
 const doomed=await root.createSubAccount('doomed',{initialBalance:(10n*N).toString()});c=await prepare('failed-transfer',g,doomed);await deposit(c,doomed);await bind(c,g,doomed);await doomed.delete(root.accountId);await tx(root,g,'qualify',{case_id:'failed-transfer',qualified:false});assert.equal((await view(c)).status,'RefundRetryable');await root.createSubAccount('doomed');await tx(evil,t,'retry_failed_refund',{escrow_id:c.escrow_id});assert.equal((await view(c)).status,'Refunded');await fail(()=>tx(evil,t,'retry_failed_refund',{escrow_id:c.escrow_id}));passes.push('actual failed wallet transfer (deleted payer) -> Retryable -> recreated same payer -> successful refund; duplicate retry rejected');
 // Reward failure also uses actual failed NEAR receipt, never a mocked callback.
 const doomed2=await root.createSubAccount('doomedreward',{initialBalance:(10n*N).toString()});c=await prepare('failed-reward',g,doomed2);await deposit(c,doomed2);await bind(c,g,doomed2);await settle('failed-reward',false);await refresh('failed-reward');await tx(root,g,'fund_case',{case_id:'failed-reward',amount:N.toString()});let fr=await g.view('get_case',{case_id:'failed-reward'});await doomed2.delete(root.accountId);await tx(root,g,'claim_reward',{case_id:'failed-reward'});assert.equal((await t.view('get_reward',{funding_id:fr.funding_id})).status,'PayRetryable');await root.createSubAccount('doomedreward');await tx(evil,t,'retry_failed_reward',{funding_id:fr.funding_id});assert.equal((await t.view('get_reward',{funding_id:fr.funding_id})).status,'Paid');await fail(()=>tx(evil,t,'retry_failed_reward',{funding_id:fr.funding_id}));passes.push('actual failed reward transfer -> permissionless retry -> Paid, no duplicate');
 // Two financial cases exist at once; operations are serialized by receipts, not shared case state.
 const ca=await prepare('parallel-a'),cb=await prepare('parallel-b');await deposit(ca);await deposit(cb);await bind(ca);await bind(cb);await settle('parallel-a',true);await settle('parallel-b',false);assert.equal((await view(ca)).status,'Slashed');assert.equal((await view(cb)).status,'Refunded');passes.push('concurrent open cases settle independently without pooled principal');
 const liabilities=await t.view('get_liabilities',{});assert.equal(liabilities.locked,'0');assert.equal(liabilities.in_flight,'0');assert.equal(liabilities.reward_funding,'0');assert.equal(liabilities.reward_in_flight,'0');assert.equal(liabilities.buyback,(6n*N).toString());
 assert(BigInt(liabilities.account_balance)>=BigInt(liabilities.buyback)+BigInt(liabilities.storage_required)+BigInt(liabilities.operating_buffer));passes.push('final conservation: no outstanding bonds/rewards; 6 NEAR Buyback covered plus storage/buffer');
 console.log(JSON.stringify({network:'local sandbox',passes,liabilities},null,2));
}finally{await w.tearDown()}})().catch(e=>{console.error(e);process.exitCode=1});
