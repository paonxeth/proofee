# ProoFee per-launch Locker

Rust/near-sdk contract; see ../../README.md and ../../INTEGRATION_HANDOFF.md.
The only revenue recipient is the Goal fixed in new(). There is no admin,
upgrade, withdrawal of LP principal, or configurable cross-contract call method.

The account must be created without access keys. Verify that property and its
on-chain code hash in addition to inspecting this source.

Run `cargo test --lib`; build with `cargo near build non-reproducible-wasm --locked`.
Use the supplied launcher to deploy and initialize in the account-creation batch.
Do not deploy uninitialized contract accounts.
