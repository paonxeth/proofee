# Delivery / review

Standalone prototype is intended to live ONLY under bond-treasury/ in paonxeth/proofee branch codex/bond-treasury-v0.2. Existing contracts/goal untouched. Local commits and bundle preserve complete source, pinned lockfile, ABI, WASM, tests and evidence. GitHub API branch creation was denied403 Resource not accessible by integration; git transport authentication also unavailable. No remote push or PR is claimed until verified.

When a Git bundle is supplied, from an existing clone containing base efa87f13478ffbf85fc607430197b146e71b3071:

```sh
git bundle verify /path/bond-treasury-v0.2.bundle
git fetch /path/bond-treasury-v0.2.bundle refs/heads/codex/bond-treasury-v0.2:refs/heads/codex/bond-treasury-v0.2
git switch codex/bond-treasury-v0.2
cd bond-treasury
cargo test --workspace --locked --lib
```

Alternatively extract source archive and inspect README.md. Verify SHA256SUMS for WASM. Source archive and Git bundle contain NO private key/credential files; all ignored work paths, toolchains, caches, node_modules and .git internals excluded. Public transaction signatures and account IDs in evidence are public chain data, not private signing material.

A Drive upload is a separate explicit permission step after automatic review rejected it as an unapproved external destination. Do not assume the archive was uploaded merely because it exists locally. Actual download links are only supplied after successful connector upload/readback. Mainnet not used.
