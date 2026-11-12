const fs=require('fs'),path=require('path'),{spawnSync}=require('child_process'),crypto=require('crypto');
const {KeyPair,transactions:t,utils}=require('near-api-js');
const RPC_URL=process.env.NEAR_TESTNET_RPC||'https://rpc.testnet.fastnear.com';
if(!['https://rpc.testnet.fastnear.com','https://rpc.testnet.near.org','https://near-testnet.drpc.org'].includes(RPC_URL))throw Error('RPC must be an allowlisted Testnet endpoint');
let chainChecked=false;
const dir=process.env.PROOFEE_EVIDENCE_DIR || path.join(__dirname,'../run/evidence');fs.mkdirSync(dir,{recursive:true});let seq=fs.readdirSync(dir).filter(x=>/^rpc-/.test(x)).length;
function http(url,body){const r=spawnSync('python',[path.join(__dirname,'transport.py')],{input:JSON.stringify({url,body}),encoding:'utf8',maxBuffer:20*1024*1024});if(r.status!==0)throw Error(r.stderr);return JSON.parse(r.stdout);}
function rpc(method,params){const request={jsonrpc:'2.0',id:++seq,method,params},response=http(RPC_URL,request);fs.writeFileSync(path.join(dir,`rpc-${String(seq).padStart(4,'0')}.json`),JSON.stringify({time:new Date().toISOString(),endpoint:RPC_URL,request,response},null,2));if(response.error||response.body?.error)throw Error(JSON.stringify(response));return response.body.result;}
function view(account_id,method_name,args={}){const r=rpc('query',{request_type:'call_function',finality:'final',account_id,method_name,args_base64:Buffer.from(JSON.stringify(args)).toString('base64')});if(r.error)throw Error(r.error);return JSON.parse(Buffer.from(r.result).toString());}
function key(role){return JSON.parse(fs.readFileSync(process.env.NEAR_SIGNER_FILE || (()=>{throw Error('NEAR_SIGNER_FILE required')})()));}
function assertId(id){if(!id.endsWith('.testnet'))throw Error('Only .testnet accounts permitted: '+id);}
async function tx(role,receiver,actions,label){if(!chainChecked){if(rpc('status',[]).chain_id!=='testnet')throw Error('RPC is not Testnet');chainChecked=true;}assertId(receiver);const k=key(role);assertId(k.account_id);const kp=KeyPair.fromString(k.private_key),pk=kp.getPublicKey();const a=rpc('query',{request_type:'view_access_key',finality:'final',account_id:k.account_id,public_key:pk.toString()});
 const transaction=t.createTransaction(k.account_id,pk,receiver,BigInt(a.nonce)+1n,actions,utils.serialize.base_decode(a.block_hash));const bytes=t.encodeTransaction(transaction),digest=crypto.createHash('sha256').update(bytes).digest(),sig=kp.sign(digest);
 const describe=a=>a.functionCall?{type:'FunctionCall',method:a.functionCall.methodName,args:JSON.parse(Buffer.from(a.functionCall.args)),deposit:a.functionCall.deposit.toString(),gas:a.functionCall.gas.toString()}:a.deployContract?{type:'DeployContract',bytes:a.deployContract.code.length,sha256:crypto.createHash('sha256').update(a.deployContract.code).digest('hex')}:a.transfer?{type:'Transfer',deposit:a.transfer.deposit.toString()}:{type:a.enum};
 fs.appendFileSync(path.join(dir,'calls.jsonl'),JSON.stringify({label,signer:k.account_id,receiver,actions:actions.map(describe)})+'\n');
 const signed=new t.SignedTransaction({transaction,signature:new t.Signature({keyType:pk.keyType,data:sig.signature})});const encoded=signed.encode(),hash=utils.serialize.base_encode(digest);
 fs.appendFileSync(path.join(dir,'transactions.jsonl'),JSON.stringify({time:new Date().toISOString(),label,hash,signer:k.account_id,receiver,status:'submitted'})+'\n');
 let r;try{r=rpc('broadcast_tx_commit',[Buffer.from(encoded).toString('base64')]);}catch(e){fs.appendFileSync(path.join(dir,'transactions.jsonl'),JSON.stringify({label,hash,status:e.message.includes('INVALID_TRANSACTION')?'rejected':'rpc_error',error:e.message})+'\n');throw e;}
 if(r.final_execution_status!=='FINAL')r=rpc('tx',{tx_hash:hash,sender_account_id:k.account_id,wait_until:'FINAL'});
 const failures=[r.status,...(r.receipts_outcome||[]).map(x=>x.outcome.status)].filter(x=>x?.Failure);
 fs.writeFileSync(path.join(dir,label+'.json'),JSON.stringify({label,hash,response:r,failures},null,2));fs.appendFileSync(path.join(dir,'transactions.jsonl'),JSON.stringify({label,hash,status:failures.length?'failure':'success'})+'\n');console.log(label,hash,failures.length?'FAILED':'OK');if(failures.length)throw Error(JSON.stringify(failures));return r;
}
function call(role,id,method,args,deposit='0',gas='150000000000000',label=method){return tx(role,id,[t.functionCall(method,Buffer.from(JSON.stringify(args)),BigInt(gas),BigInt(deposit))],label);}
function save(name,data){fs.writeFileSync(path.join(dir,name+'.json'),JSON.stringify(data,null,2));}
module.exports={rpc,view,tx,call,key,save,t,http,dir};
