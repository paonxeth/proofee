#!/bin/sh
set -eu
# Required vars are substituted into the generated wrapper by run.sh.
export LD_PRELOAD="@ADAPTER@"
if [ "${3:-}" = init ]; then
 "@BINARY@" "$@"
 python3 - "$2" <<'PY'
import json, pathlib,sys
home=pathlib.Path(sys.argv[1])
# Dedicated local testnet namespace, NOT a connection to public Testnet.
for name in ['genesis.json','validator_key.json','node_key.json']:
 p=home/name
 p.write_text(p.read_text().replace('test.near','testnet'))
p=home/'config.json';x=json.loads(p.read_text());x['store']['max_open_files']=1000;p.write_text(json.dumps(x))
PY
else
 exec "@BINARY@" "$@"
fi
