const fs=require('fs'),path=require('path'),assert=require('assert/strict'),{KeyPair}=require('near-api-js'),n=require('../lib/near.cjs');
const c=JSON.parse(fs.readFileSync(n.dir+'/contracts.json')),root=n.key().account_id;
(async()=>{
 const file=process.env.NEAR_CALLER_FILE;if(!file||!path.isAbsolute(file))throw Error('NEAR_CALLER_FILE required outside repository');
 const id='caller.'+root;
 if(!fs.existsSync(file)){
  const kp=KeyPair.fromRandom('ed25519');fs.writeFileSync(file,JSON.stringify({account_id:id,public_key:kp.getPublicKey().toString(),private_key:kp.toString()}),{mode:0o600,flag:'wx'});
 }
 const kp=KeyPair.fromString(JSON.parse(fs.readFileSync(file)).private_key);
 const {exists,advance}=require('./launch.cjs');
 if(!exists(id))await n.tx('root',id,[n.t.createAccount(),n.t.transfer(6n*10n**23n),n.t.addKey(kp.getPublicKey(),n.t.fullAccessKey())],'create-stranger');
 if(BigInt(n.rpc('query',{request_type:'view_account',finality:'final',account_id:id}).amount)<5n*10n**23n)await n.tx('root',id,[n.t.transfer(4n*10n**23n)],'topup-stranger-gas');
 async function fail(label,fn){try{await fn();}catch(e){n.save(label+'-expected',{expected_failure:true,error:e.message});return;}throw Error('Unexpected success '+label);}
 await fail('negative-parent-upgrade',()=>n.tx('root',c.locker_id,[n.t.deployContract(fs.readFileSync(path.join(__dirname,'../artifacts/locker.wasm')))],'negative-parent-upgrade'));
 process.env.NEAR_SIGNER_FILE=file;
 const before=n.view(c.locker_id,'get_launch'),caller_before=n.rpc('query',{request_type:'view_account',finality:'final',account_id:id}).amount;
 await n.call('stranger',c.locker_id,'collect',{receiver_id:id,goal_id:id},'0','300000000000000','stranger-collect-redirect-args');
 await advance(c.locker_id);
 const after=n.view(c.locker_id,'get_launch');assert.equal(after.goal_id,c.goal_id);assert(BigInt(after.native_paid)>=BigInt(before.native_paid));const caller_after=n.rpc('query',{request_type:'view_account',finality:'final',account_id:id}).amount;assert(BigInt(caller_after)<BigInt(caller_before));
 await fail('negative-stranger-claim',()=>n.call('stranger',c.goal_id,'claim',{},'0','100000000000000','negative-stranger-claim'));
 n.save('permissionless-security',{caller:id,creator:root,goal:c.goal_id,goal_unchanged:true,unexpected_receiver_ignored:true,caller_before,caller_after,native_paid_before:before.native_paid,native_paid_after:after.native_paid});
})().catch(e=>{console.error(e.message);process.exitCode=1});
