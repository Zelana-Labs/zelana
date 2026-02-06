---
sidebar_position: 1
title: Introduction
---

# Zelana Documentation

Zelana is a privacy-focused Layer 2 rollup prototype built on Solana. This site documents the
architecture, state machines, and protocol details that power the stack.

## What Exists Today

- A Rust sequencer pipeline in `core/` that batches transactions and submits them to Solana
- Groth16 proving support in `prover/` and the sequencer
- Solana programs in `onchain-programs/` for the bridge and on-chain verification
- SDKs in `sdk/` covering privacy primitives, transactions, and the Zephyr UDP transport

## Repository Map

- `core/`: Sequencer, batching, settlement, and storage logic
- `prover/`: Groth16 proof generation tooling
- `onchain-programs/`: Solana bridge + verifier programs
- `sdk/`: Rust and TypeScript SDKs (privacy, transactions, transport)
- `rpc/`, `cli/`, `udp-client/`: Supporting tooling and clients

## Quickstart (Local Sequencer)

1. Start a local Solana test validator:

```bash
surfpool start
```

2. Point the Solana CLI at localnet:

```bash
solana config set --url http://127.0.0.1:8899
```

3. Build and deploy the bridge program:

```bash
cd onchain-programs/bridge
cargo build-sbf
solana program deploy target/deploy/bridge.so
```

4. Update the program ID in `onchain-programs/bridge/src/lib.rs` and re-deploy if needed.

5. Run the sequencer:

```bash
RUST_LOG=info cargo run -p core --release
```

## Where To Go Next

- Start with the [Architecture Overview](./implementation/architecture.md)
- Dive into the [State Machines](./implementation/state-machines/index.md)
- Explore protocol details in the [Zephyr page](./implementation/zephyr.md)
- Design notes live under [Drafts](./drafts/full-privacy.md)
