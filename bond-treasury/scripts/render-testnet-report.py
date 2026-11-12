#!/usr/bin/env python3
import json,base64,pathlib,sys
x=json.load(open(sys.argv[1]));a=x['accounts'];done=x.get('complete',False)
lines=['# Public NEAR Testnet results — 12 November 2026','',f"Status: {'COMPLETE: refund, slash and separately funded reward confirmed' if done else 'INCOMPLETE; see recorded transaction/view evidence'}. No Mainnet operations.",'','## Dedicated demo accounts','']
for k,v in a.items():lines.append(f'- {k}: [{v}](https://testnet.nearblocks.io/address/{v})')
lines+=['','Treasury and Mock are atomic no-key deployments. Parent/payer/verifier TEST-only keys were generated locally and remain protected in ignored work directory; never included in this report/archive. New user-owned deployment should use locally controlled signer/OS keychain. Parent and support accounts were each funded10 test NEAR by official Testnet helper; support transferred9.8 to parent for code/storage sponsorship. This is centralized Mock demonstration, not actual Goal fees. Existing Goal remains untouched.','','## Confirmed signed transactions','','| Operation | Case | Transaction / explorer |','|---|---|---|']
for t in x['transactions']:
 case='—'
 for action in t['result'].get('transaction',{}).get('actions',[]):
  if isinstance(action,dict) and 'FunctionCall' in action:
   try:
    args=json.loads(base64.b64decode(action['FunctionCall']['args']));case=args.get('case_id',args.get('intent',{}).get('case_id','—'))
   except Exception:pass
 lines.append(f"| {t['label']} | {case} | [{t['hash']}]({t['explorer']}) |")
lines+=['','Full receipt results and final ledger views: evidence/testnet-transactions.json. Independent code hash / key / balance readback: evidence/testnet-readback.json when completed. Local sandbox tx hashes must not be confused with these public hashes.','','## Final recorded views','','```json',json.dumps(x['views'],indent=2),'```','','## Scope and limitations','','Public demo covers deposit, bind, rejection refund, winning refund, actual Goal reward funding/payment, losing slash and quiescent balance coverage. Runtime failure/retry, timeout and replay tests are demonstrated in local sandbox, not claimed as public Testnet transactions unless separately recorded. RPC InvalidNonce and429 interrupted tooling; authoritative views and known confirmed receipts were checked before continuing. Initial verifier0.2NEAR could not reserve300Tgas; topped up1NEAR; fresh deployment script now provisions1NEAR upfront. No previously confirmed payment was resent.','','Mainnet not supported or claimed ready. Real Goal adapter, production timeout policy and real fee/forfeiture accounting remain another implementation/review gate. No independent security audit.']
print('\n'.join(lines))
