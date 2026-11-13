// Derived from PoC bootstrap.cjs. Signing material never enters source/evidence.
const fs=require('fs'),path=require('path'),{KeyPair}=require('near-api-js');
const n=require('../lib/near.cjs');
const file=process.env.NEAR_SIGNER_FILE;
if(!file || !path.isAbsolute(file)) throw Error('Set absolute NEAR_SIGNER_FILE outside repository');
if(fs.existsSync(file)) throw Error('Signer exists; reuse it, never overwrite');
const kp=KeyPair.fromRandom('ed25519'),id='pf-launch-'+Date.now().toString(36)+'.testnet';
fs.mkdirSync(path.dirname(file),{recursive:true,mode:0o700});
fs.writeFileSync(file,JSON.stringify({account_id:id,public_key:kp.getPublicKey().toString(),private_key:kp.toString()}),{mode:0o600,flag:'wx'});
const response=n.http('https://helper.testnet.near.org/account',{newAccountId:id,newAccountPublicKey:kp.getPublicKey().toString()});
n.save('faucet',{account_id:id,public_key:kp.getPublicKey().toString(),response});
console.log(JSON.stringify({account_id:id,response}));
