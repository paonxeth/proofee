# ProoFee

Memecoin goals: token fees go to a goal contract, and the creator receives them as milestones are accepted.
NEAR hackathon, track 11. Deadline: 14 November, 23:59 UTC. Demo: testnet only.

## Where things are
- docs/SPEC.md — contract specification. The source of truth. Do not change behavior without editing SPEC.
- docs/DECISIONS.md — decisions made.
- contracts/goal — the contract in Rust, near-sdk 5.
- contracts/goal/tests — sandbox tests (integration.rs; treasury.rs against the real Bond Treasury wasm).
- bond-treasury — Bond Treasury v0.2 (Rust): challenge bonds, refund or slash to a buyback reserve.
- launcher — testnet launcher (Node.js CLI) and the LP Locker contract (Rust). Not a web app.
- client — framework-free TypeScript client (src/index.ts → dist/) and the read-only demo page client/demo.html,
  served by GitHub Pages from main. There is no Next.js app in the repository.
- release/goal-treasury — release package of the goal wasm for the frontend (ABI, errors, permissions, examples).
- scripts — deploy-testnet.sh and goal parameter files; specs — goal spec JSON files.
- docs — also AUDIT.md (internal review), IMPLEMENTATION_STATUS.md, DEMO_RUN*.md (testnet run logs).

## Contract rules
- No admin methods and no parameter changes after new.
- State first, then the transfer; every transfer gets a callback with rollback.
- Division rounds down, checked arithmetic.
- Every change comes with a test. cargo test must be green.
- Testnet only. Do nothing on mainnet.

## Environment
- macOS, the regular Terminal (zsh).

## Commands
- cd contracts/goal && cargo test
- cargo near build non-reproducible-wasm
- deploy — per SPEC section 11 (check near-cli-rs syntax against near --help)

## Documentation
- https://docs.near.org/llms.txt
- skills near-smart-contracts, near-contract-audit, near-cli-rs

## How to work with me
- Talk to me in Russian.
- I am learning Rust. Before code, a short plan; after it, what changed and why.
- One step at a time. Do not touch what is outside the task.
- Explain Rust and near-sdk constructs that are new to me in two or three sentences.
