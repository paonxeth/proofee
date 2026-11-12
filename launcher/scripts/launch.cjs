// Sequential Launcher, adapted from verified PoC deployment/registration/HotZap scripts.
const fs=require('fs'),path=require('path'),crypto=require('crypto');
const n=require('../lib/near.cjs');
const BASE=path.resolve(__dirname,'..'), N=10n**24n;
function exists(id){try{return n.rpc('query',{request_type:'view_account',finality:'final',account_id:id});}catch(e){if(e.message.includes('UNKNOWN_ACCOUNT'))return null;throw e;}}
async function create(id,wasm,args,fund,label){
 const actual=exists(id);
 if(actual){
  const expected=require('near-api-js').utils.serialize.base_encode(crypto.createHash('sha256').update(fs.readFileSync(wasm)).digest());
  if(actual.code_hash!==expected)throw Error('Existing code hash mismatch: '+id);
  if(n.rpc('query',{request_type:'view_access_key_list',finality:'final',account_id:id}).keys.length)throw Error('Existing contract is not keyless');
  return;
 }
 await n.tx('root',id,[n.t.createAccount(),n.t.transfer(fund),n.t.deployContract(fs.readFileSync(wasm)),
  n.t.functionCall('new',Buffer.from(JSON.stringify(args)),100000000000000n,0n)],label);
 // Accounts are born with no keys. No temporary administrator ever owns the LP.
 if(n.rpc('query',{request_type:'view_access_key_list',finality:'final',account_id:id}).keys.length)throw Error('Unexpected access key');
}
async function register(contract,id,deposit,label){
 if(n.view(contract,'storage_balance_of',{account_id:id}))return;
 await n.call('root',contract,'storage_deposit',{account_id:id,registration_only:false},deposit,'30000000000000',label);
}
async function advance(locker,max=24){
 for(let i=0;i<max;i++){
  const state=n.view(locker,'get_launch');n.save('launch-state',state);n.save('state-'+Date.now()+'-'+state.phase,state);console.log('phase',state.phase);
  if(state.phase==='Active')return state;
  if(state.phase==='Busy'){if(state.last_error)throw Error('Ambiguous execution: '+state.last_error);await new Promise(resolve=>setTimeout(resolve,1000));continue;}
  await n.call('root',locker,'advance',{},'0','300000000000000','advance-'+state.phase+'-'+Date.now());
 }
 throw Error('Advance limit');
}
async function launch(config){
 const root=n.key('root').account_id;
 if(config.network!=='testnet')throw Error('Testnet only');
 if(!/^[a-z][a-z0-9-]{0,15}$/.test(config.launch_id))throw Error('Invalid immutable launch_id');
 if(!Number.isInteger(config.decimals)||config.decimals<0||config.decimals>29)throw Error('Invalid decimals');
 const token=`token-${config.launch_id}.${root}`,locker=`lock-${config.launch_id}.${root}`,goal=config.goal_id||`goal-${config.launch_id}.${root}`;
 if([token,locker,goal].some(id=>id.length>64||!id.endsWith('.testnet')))throw Error('Invalid Testnet IDs');
 if(goal==='goal-4.proofee-anton.testnet')throw Error('Existing team goal forbidden');
 const canonical=JSON.stringify(config),digest=crypto.createHash('sha256').update(canonical).digest('hex');
 const registry=path.join(n.dir,'launch-'+config.launch_id+'.json');
 if(fs.existsSync(registry)&&JSON.parse(fs.readFileSync(registry)).config_sha256!==digest)throw Error('Launch ID already reserved for other parameters');
 fs.writeFileSync(registry,JSON.stringify({config,config_sha256:digest,root,token_id:token,locker_id:locker,goal_id:goal},null,2));
 if(!config.goal_id){
  const spec=JSON.stringify({title:'Isolated Launcher E2E',network:'testnet',launch_id:config.launch_id});
  n.save('goal-spec-'+config.launch_id,JSON.parse(spec));
  const deadline=Date.now()+7*86400000;
  await create(goal,path.join(BASE,'artifacts/goal.wasm'),{creator:root,verifier:`verifier.${root}`,
   goal_title:'Isolated Launcher E2E',goal_uri:'data:application/json,'+encodeURIComponent(spec),
   goal_spec_hash:crypto.createHash('sha256').update(spec).digest('hex'),immediate_bps:2000,
   challenge_window_ms:120000,milestones:[{title:'Launch',weight_bps:1875,deadline_ms:deadline},
    {title:'Trade',weight_bps:3125,deadline_ms:deadline+86400000},{title:'Fees',weight_bps:5000,deadline_ms:deadline+172800000}]},21n*N/10n,'create-goal');
 }
 const g=n.view(goal,'get_goal');
 if(g.immediate_bps!==2000||g.creator!==(config.creator_id||root))throw Error('Goal creator or 20/80 mismatch');
 const supply=(1000000000n*10n**BigInt(config.decimals)).toString();
 await create(token,path.join(BASE,'artifacts/fungible_token.wasm'),{owner_id:locker,total_supply:supply,
  metadata:{spec:'ft-1.0.0',name:config.name,symbol:config.symbol,decimals:config.decimals,...(config.image_uri?{icon:config.image_uri}:{}),
   ...(config.reference?{reference:config.reference,reference_hash:config.reference_hash}:{} )}},32n*N/10n,'create-token');
 if(n.view(token,'ft_total_supply')!==supply)throw Error('Unexpected supply');
 await create(locker,path.join(BASE,'artifacts/locker.wasm'),{creator_id:config.creator_id||root,goal_id:goal,token_id:token,
  decimals:config.decimals,init_point:config.init_point,left_point:config.left_point,right_point:config.right_point},25n*N/10n,'create-locker');
 const s=n.view(locker,'get_launch');
 if(s.goal_id!==goal||s.token_id!==token||s.supply!==supply)throw Error('Locker binding mismatch');
 const dcl='dclv2.ref-dev.testnet',wrap='wrap.testnet';
 for(const id of [dcl,root])await register(token,id,n.view(token,'storage_balance_bounds').min,'register-token-'+id);
 for(const id of [locker,root])await register(wrap,id,n.view(wrap,'storage_balance_bounds').min,'register-wrap-'+id);
 await register(dcl,locker,'600000000000000000000000','register-dcl-locker');
 const state=await advance(locker);
 n.save('contracts',{network:'testnet',root,token_id:token,locker_id:locker,goal_id:goal,pool_id:state.pool_id,lpt_id:state.lpt_id});
 return state;
}
if(require.main===module)launch(JSON.parse(fs.readFileSync(process.argv[2]||path.join(BASE,'launch.example.json')))).catch(e=>{console.error(e.message);process.exitCode=1});
module.exports={launch,advance,exists};
