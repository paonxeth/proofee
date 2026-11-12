const fs=require('fs'),assert=require('assert/strict'),crypto=require('crypto'),path=require('path'),n=require('../lib/near.cjs'),{utils}=require('near-api-js');
const c=JSON.parse(fs.readFileSync(n.dir+'/contracts.json'));
(async()=>{
 if(process.argv.includes('--settle')){
  await n.call('root',c.goal_id,'sync',{},'0','50000000000000','final-goal-sync');
  if(BigInt(n.view(c.goal_id,'get_balances').claimable)>0n)await n.call('root',c.goal_id,'claim',{},'0','100000000000000','final-creator-claim');
 }
 const contracts={};
 for(const [field,artifact] of [['token_id','fungible_token'],['locker_id','locker'],['goal_id','goal']]){
  const id=c[field],account=n.rpc('query',{request_type:'view_account',finality:'final',account_id:id}),keys=n.rpc('query',{request_type:'view_access_key_list',finality:'final',account_id:id}).keys;
  const expected=utils.serialize.base_encode(crypto.createHash('sha256').update(fs.readFileSync(path.join(__dirname,'../artifacts/'+artifact+'.wasm'))).digest());
  assert.equal(account.code_hash,expected);assert.equal(keys.length,0);contracts[field]={account_id:id,account,keys,code_hash_verified:true};
 }
 const token={metadata:n.view(c.token_id,'ft_metadata'),supply:n.view(c.token_id,'ft_total_supply'),locker_wallet:n.view(c.token_id,'ft_balance_of',{account_id:c.locker_id}),internal_assets:n.view('dclv2.ref-dev.testnet','list_user_assets',{account_id:c.locker_id})};
 const launch=n.view(c.locker_id,'get_launch'),lp=n.view('dclv2.ref-dev.testnet','get_liquidity',{lpt_id:c.lpt_id}),goal=n.view(c.goal_id,'get_balances');
 assert.equal(lp.owner_id,c.locker_id);assert.equal(lp.amount,launch.principal_liquidity);assert.equal(launch.phase,'Active');assert.equal(token.supply,launch.supply);
 n.save('onchain-final',{network:'testnet',contracts,token,launch,lp,goal});console.log('Final snapshot verified');
})().catch(e=>{console.error(e.message);process.exitCode=1});
