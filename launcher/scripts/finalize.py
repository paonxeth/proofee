import json,pathlib,urllib.request,concurrent.futures,datetime,base64,subprocess
import os
p=pathlib.Path(os.environ.get('PROOFEE_EVIDENCE_DIR', pathlib.Path(__file__).parent.parent/'run/evidence'))
records={}
journal=[json.loads(line) for line in (p/'transactions.jsonl').read_text().splitlines()]
rejected=[]
for r in journal:
 if r['status']=='submitted':records[r['hash']]=r
for r in journal:
 if r['status'] in ('rpc_error','rejected') and 'INVALID_TRANSACTION' in r.get('error',''):
  item=records.pop(r['hash'],None)
  if item:rejected.append({**item,'status':'rejected_before_inclusion','error':r['error']})
(p/'rejected-transactions.json').write_text(json.dumps(rejected,indent=2))
proof_file=p/'superseded-transactions.json'
if proof_file.exists():
 for proof in json.loads(proof_file.read_text()):records.pop(proof['hash'],None)
f=json.loads((p/'faucet.json').read_text());body=f.get('response',{}).get('body',{})
if body.get('transaction'):records[body['transaction']['hash']]={'label':'faucet-create-root','hash':body['transaction']['hash'],'signer':'testnet'}

def get(item):
 h,r=item;req={'jsonrpc':'2.0','id':r['label'],'method':'tx','params':{'tx_hash':h,'sender_account_id':r['signer'],'wait_until':'FINAL'}}
 try:
  target=p/('final-'+r['label']+'-'+h+'.json')
  raw=json.loads(target.read_text()).get('response',{}) if target.exists() else {}
  if raw.get('result',{}).get('final_execution_status')!='FINAL':
   raw=json.load(urllib.request.urlopen(urllib.request.Request('https://rpc.testnet.fastnear.com',data=json.dumps(req).encode(),headers={'Content-Type':'application/json'}),timeout=45))
   target.write_text(json.dumps({'time':datetime.datetime.now(datetime.timezone.utc).isoformat(),'endpoint':'https://rpc.testnet.fastnear.com','request':req,'response':raw},indent=2))
  x=raw.get('result',{});fail=[v['outcome']['status'] for v in x.get('receipts_outcome',[]) if 'Failure' in v['outcome']['status']]
  return {'label':r['label'],'hash':h,'signer':r['signer'],'final_execution_status':x.get('final_execution_status'),'status':x.get('status'),'receipt_failures':fail,'error':raw.get('error')}
 except Exception as e:return {'label':r['label'],'hash':h,'error':str(e)}
with concurrent.futures.ThreadPoolExecutor(max_workers=4) as e:res=list(e.map(get,records.items()))
subprocess.run(['node',str(pathlib.Path(__file__).parent/'reconcile.cjs')],check=True)
proofs=json.loads((p/'superseded-transactions.json').read_text())
res=[r for r in res if r['hash'] not in {x['hash'] for x in proofs}]
(p/'final-transactions.json').write_text(json.dumps(res,indent=2));print(json.dumps({'transactions':len(res),'final':sum(x.get('final_execution_status')=='FINAL' for x in res),'errors':[x for x in res if x.get('error')],'failed_receipts':[{'label':x['label'],'hash':x['hash']} for x in res if x.get('receipt_failures')]},indent=2))
