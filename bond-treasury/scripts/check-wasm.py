#!/usr/bin/env python3
"""Read actual compiled WASM exports; do not infer ABI exclusion from failed bad arguments."""
import json,pathlib,sys

def exports(path):
 b=path.read_bytes();assert b[:8]==b'\x00asm\x01\x00\x00\x00';p=8
 def leb():
  nonlocal p
  x=0;shift=0
  while True:
   v=b[p];p+=1;x|=(v&127)<<shift
   if v<128:return x
   shift+=7
 while p<len(b):
  sec=b[p];p+=1;size=leb();end=p+size
  if sec==7:
   names=[]
   for _ in range(leb()):
    size=leb();name=b[p:p+size].decode();p+=size;kind=b[p];p+=1;leb()
    if kind==0:names.append(name)
   return sorted(names)
  p=end
 raise AssertionError('Missing export section')
root=pathlib.Path(__file__).resolve().parent.parent
artifact_dir=pathlib.Path(sys.argv[1]) if len(sys.argv)>1 else root/'release'
result={name:exports(artifact_dir/f'{name}.wasm') for name in ['bond_treasury','mock_goal']}
assert 'register_demo_case' not in result['bond_treasury'], 'TEST FEATURE EXPORTED'
for method in ['new','register_intent','bind_bond','authorize_decision','settle_bond','refund_expired','fund_reward','pay_reward','retry_failed_reward','on_refund','on_reward']:
 assert method in result['bond_treasury'],method
for method in ['prepare_challenge','confirm_escrow','arbitrate','fund_case','claim_reward','recover_funding']:
 assert method in result['mock_goal'],method
print(json.dumps({'pass':True,'exports':result},indent=2))
