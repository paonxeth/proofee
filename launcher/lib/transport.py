import sys,json,urllib.request,urllib.error
x=json.load(sys.stdin)
try:
 req=urllib.request.Request(x['url'],data=json.dumps(x['body']).encode() if 'body' in x else None,headers={'Content-Type':'application/json','User-Agent':'ProoFee-Testnet-PoC/1.0'})
 with urllib.request.urlopen(req,timeout=45) as r:
  body=r.read().decode();print(json.dumps({'http_status':r.status,'body':json.loads(body)}))
except urllib.error.HTTPError as e:
 print(json.dumps({'http_status':e.code,'error':e.read().decode()[:3000]}))
except Exception as e:print(json.dumps({'error':str(e)}))
