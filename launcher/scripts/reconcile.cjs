// Prove non-inclusion after an RPC error only when a different FINAL transaction
// consumed exactly the same signer/public-key nonce. Never blindly retry unknown txs.
const fs=require('fs'),path=require('path'),crypto=require('crypto'),{transactions:t,utils}=require('near-api-js');
const dir=process.env.PROOFEE_EVIDENCE_DIR||path.join(__dirname,'../run/evidence');
const journal=fs.readFileSync(path.join(dir,'transactions.jsonl'),'utf8').trim().split('\n').map(JSON.parse);
const pending=journal.filter(r=>r.status==='rpc_error'&&!r.error.includes('INVALID_TRANSACTION')),info=new Map(),finals=[];
for(const file of fs.readdirSync(dir)){
 if(!file.endsWith('.json'))continue;
 let x;try{x=JSON.parse(fs.readFileSync(path.join(dir,file)));}catch{continue;}
 if(file.startsWith('rpc-')&&x.request?.method==='broadcast_tx_commit'){
  const signed=t.SignedTransaction.decode(Buffer.from(x.request.params[0],'base64')),tx=signed.transaction;
  const hash=utils.serialize.base_encode(crypto.createHash('sha256').update(t.encodeTransaction(tx)).digest());
  info.set(hash,{signer:tx.signerId,public_key:'ed25519:'+utils.serialize.base_encode(Uint8Array.from(tx.publicKey.ed25519Key.data)),nonce:tx.nonce.toString()});
 }
 if(file.startsWith('final-')&&x.response?.result?.final_execution_status==='FINAL'){
  const tx=x.response.result.transaction;finals.push({hash:tx.hash,signer:tx.signer_id,public_key:tx.public_key,nonce:String(tx.nonce),evidence_file:file});
 }
}
const proofs=[];
for(const r of pending){
 const original=info.get(r.hash);if(!original)continue;
 const replacement=finals.find(f=>f.hash!==r.hash&&f.signer===original.signer&&f.public_key===original.public_key&&f.nonce===original.nonce);
 if(replacement)proofs.push({hash:r.hash,label:r.label,status:'not_included_nonce_consumed_by_final_replacement',...original,replacement,original_rpc_error:r.error});
}
fs.writeFileSync(path.join(dir,'superseded-transactions.json'),JSON.stringify(proofs,null,2));
console.log(JSON.stringify({non_inclusion_proofs:proofs.length}));
