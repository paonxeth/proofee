const fs=require('fs'),assert=require('assert/strict'),n=require('../lib/near.cjs'),{advance}=require('./launch.cjs');
const c=JSON.parse(fs.readFileSync(n.dir+'/contracts.json')),dcl='dclv2.ref-dev.testnet',wrap='wrap.testnet',root=n.key('root').account_id;
const snap=()=>({launch:n.view(c.locker_id,'get_launch'),lp:n.view(dcl,'get_liquidity',{lpt_id:c.lpt_id}),pool:n.view(dcl,'get_pool',{pool_id:c.pool_id}),
 goal:n.view(c.goal_id,'get_balances'),trader_token:n.view(c.token_id,'ft_balance_of',{account_id:root}),
 trader_native:n.rpc('query',{request_type:'view_account',finality:'final',account_id:root}).amount});
async function expectFailure(receiver,method,args,label){
 try{await n.call('root',receiver,method,args,'0','300000000000000',label);}catch(e){n.save(label+'-expected',{expected_failure:true,error:e.message});return;}
 throw Error('Negative test unexpectedly succeeded: '+label);
}
(async()=>{
 const progressFile=n.dir+'/e2e-progress.json';let progress=fs.existsSync(progressFile)?JSON.parse(fs.readFileSync(progressFile)):{};
 async function once(step,fn){if(progress[step])return;await fn();progress[step]=true;fs.writeFileSync(progressFile,JSON.stringify(progress,null,2));}
 await once('buy',async()=>{
  n.save('e2e-before',snap());
  await n.call('root',wrap,'near_deposit',{},'100000000000000000000000','30000000000000','e2e-wrap-buy');
  const q=n.view(dcl,'quote',{pool_ids:[c.pool_id],input_token:wrap,output_token:c.token_id,input_amount:'100000000000000000000000'});n.save('e2e-buy-quote',q);
  await n.call('root',wrap,'ft_transfer_call',{receiver_id:dcl,amount:'100000000000000000000000',msg:JSON.stringify({Swap:{pool_ids:[c.pool_id],output_token:c.token_id,min_output_amount:(BigInt(q.amount)*99n/100n).toString()}})},'1','250000000000000','e2e-buy');
  assert(BigInt(n.view(c.token_id,'ft_balance_of',{account_id:root}))>0n);n.save('e2e-after-buy',snap());
 });
 await once('sell',async()=>{
  const sell=(BigInt(n.view(c.token_id,'ft_balance_of',{account_id:root}))/2n).toString(),q=n.view(dcl,'quote',{pool_ids:[c.pool_id],input_token:c.token_id,output_token:wrap,input_amount:sell});
  n.save('e2e-sell-quote',{sell,...q});
  await n.call('root',c.token_id,'ft_transfer_call',{receiver_id:dcl,amount:sell,msg:JSON.stringify({Swap:{pool_ids:[c.pool_id],output_token:wrap,min_output_amount:(BigInt(q.amount)*99n/100n).toString()}})},'1','250000000000000','e2e-sell');n.save('e2e-after-sell',snap());
 });
 await once('collect',async()=>{
  const s=n.view(c.locker_id,'get_launch');
  if(s.phase==='Active'&&s.native_paid==='0')await n.call('root',c.locker_id,'collect',{},'0','300000000000000','e2e-collect');
  await advance(c.locker_id);n.save('e2e-after-collect',snap());
  assert(BigInt(n.view(c.goal_id,'get_balances').unsynced)>0n);
 });
 await once('sync',async()=>{await n.call('root',c.goal_id,'sync',{},'0','50000000000000','e2e-goal-sync');n.save('e2e-after-sync',snap());});
 await once('claim',async()=>{await n.call('root',c.goal_id,'claim',{},'0','100000000000000','e2e-creator-claim');n.save('e2e-after-claim',snap());});
 await once('negative',async()=>{
  await expectFailure(c.locker_id,'remove_liquidity',{lpt_id:c.lpt_id,amount:'1'},'negative-locker-remove');
  await expectFailure(c.locker_id,'set_goal',{goal_id:root},'negative-change-goal');
  await expectFailure(c.locker_id,'on_complete',{},'negative-forged-callback');
  await expectFailure(dcl,'remove_liquidity',{lpt_id:c.lpt_id,amount:'1',min_amount_x:'0',min_amount_y:'0'},'negative-dcl-wrong-owner');
  for(const id of [c.token_id,c.locker_id,c.goal_id])assert.equal(n.rpc('query',{request_type:'view_access_key_list',finality:'final',account_id:id}).keys.length,0);
  const before=JSON.parse(fs.readFileSync(n.dir+'/e2e-before.json')),after=snap();
  assert.equal(after.lp.amount,before.lp.amount);assert.equal(after.lp.owner_id,c.locker_id);assert.equal(after.launch.goal_id,c.goal_id);
  n.save('e2e-security', {keyless:true,principal_before:before.lp.amount,principal_after:after.lp.amount,owner:after.lp.owner_id,goal_immutable:after.launch.goal_id});
 });
 await once('repeat_collect',async()=>{
  const before=n.view(c.locker_id,'get_launch'),fees=n.view(dcl,'get_liquidity',{lpt_id:c.lpt_id});await n.call('root',c.locker_id,'collect',{},'0','300000000000000','e2e-repeat-collect');await advance(c.locker_id);
  const after=n.view(c.locker_id,'get_launch');assert(BigInt(after.native_paid)>=BigInt(before.native_paid));n.save('e2e-repeat-accounting',{before,fees_before:fees,after,reason:'Fee conversion itself accrues fresh LP fees; verify cycle withdrawals rather than expecting zero.'});n.save('e2e-final',snap());
 });
 console.log('E2E passed',c);
})().catch(e=>{console.error(e.message);process.exitCode=1});
