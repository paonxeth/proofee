#!/usr/bin/env bash
# Reproducible LOCAL chain, never public testnet/mainnet. Linux x86_64 + C compiler.
set -euo pipefail
TASK_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$TASK_ROOT/tests/sandbox"
export npm_config_cache="${npm_config_cache:-$TASK_ROOT/work/npm-cache}"
npm ci --ignore-scripts
TASK_CACHE="$TASK_ROOT/work/sandbox-runtime"
mkdir -p "$TASK_CACHE"
# Official matching modern protocol; near-workspaces' default 2.6.2 rejects Rust 1.99 WASM.
SANDBOX_VERSION=2.14.1
curl -fLsS "https://s3-us-west-1.amazonaws.com/build.nearprotocol.com/nearcore/Linux-x86_64/$SANDBOX_VERSION/near-sandbox.tar.gz" -o "$TASK_CACHE/sandbox.tar.gz"
tar xzf "$TASK_CACHE/sandbox.tar.gz" -C "$TASK_CACHE"
cc -shared -fPIC -o "$TASK_CACHE/nofile_adapter.so" nofile_adapter.c -ldl
python3 - "$TASK_CACHE" "$PWD" <<'PY'
import pathlib,sys
cache=pathlib.Path(sys.argv[1]);here=pathlib.Path(sys.argv[2]);dest=here/'node_modules/near-sandbox/bin/near-sandbox';dest.parent.mkdir(parents=True,exist_ok=True)
s=(here/'near-sandbox-wrapper.sh').read_text().replace('@ADAPTER@',str(cache/'nofile_adapter.so')).replace('@BINARY@',str(cache/'Linux-x86_64/near-sandbox'))
dest.write_text(s);dest.chmod(0o700)
PY
NEAR_WORKSPACES_DEBUG=1 node test.cjs
