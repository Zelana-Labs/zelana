# Implementation Documentation

This section provides detailed technical documentation for the Zelana codebase.

## Overview

Zelana is a **privacy-focused Layer 2 rollup prototype** on Solana. The codebase
currently includes:

- Shielded transaction primitives (notes, nullifiers, encrypted outputs)
- Threshold encryption building blocks for sequencer-blind ordering
- Low-latency UDP transport (Zephyr)
- ZK rollup settlement with Groth16 proofs (BN254)

## Documentation Structure

### [Architecture Overview](./architecture.md)

High-level system architecture, component relationships, and design decisions.

### State Machines

Detailed state machine analysis for each major component:

- **[Sequencer](./state-machines/sequencer.md)** - Transaction processing, batch lifecycle, session management
- **[Bridge](./state-machines/bridge.md)** - L1↔L2 deposits and withdrawals
- **[Prover](./state-machines/prover.md)** - ZK proof generation and L1 settlement
- **[Transaction Types](./state-machines/types.md)** - All transaction structures and flows

### [Zephyr Protocol](./zephyr.md)

Low-latency UDP transport protocol for fast transaction submission.

> Note: Pages under **Drafts** are design notes and may not match the current
> implementation.

## Quick Reference

### Key Concepts

| Concept | Description |
|---------|-------------|
| **Batch** | Collection of transactions being processed together |
| **Block** | Finalized batch with compact 96-byte header |
| **Nullifier** | Unique identifier preventing double-spends |
| **Commitment** | Hash of a shielded note |
| **State Root** | Merkle root of all L2 account states |

### Transaction Types

| Type | Privacy | Flow |
|------|---------|------|
| Transfer | Transparent | L2 → L2 |
| Deposit | Transparent | L1 → L2 |
| Withdraw | Transparent | L2 → L1 |
| Shielded | Private | L2 → L2 (ZK) |

### Component Locations

| Component | Path |
|-----------|------|
| Sequencer Core | `core/src/sequencer/` |
| API Layer | `core/src/api/` |
| Bridge Program | `onchain-programs/bridge/` |
| Verifier Program | `onchain-programs/verifier/` |
| Prover | `prover/src/` |
| SDK Crates | `sdk/` |

## Implementation Links (GitHub)

Use these links to jump directly to key implementation entry points:

- [core/src/main.rs](https://github.com/zelana-Labs/zelana/blob/main/core/src/main.rs)
- [core/src/sequencer/](https://github.com/zelana-Labs/zelana/tree/main/core/src/sequencer)
- [onchain-programs/bridge/](https://github.com/zelana-Labs/zelana/tree/main/onchain-programs/bridge)
- [onchain-programs/verifier/programs/onchain_verifier/src/lib.rs](https://github.com/zelana-Labs/zelana/blob/main/onchain-programs/verifier/programs/onchain_verifier/src/lib.rs)
- [forge/crates/prover-coordinator/](https://github.com/zelana-Labs/zelana/tree/main/forge/crates/prover-coordinator)
- [sdk/transaction/src/lib.rs](https://github.com/zelana-Labs/zelana/blob/main/sdk/transaction/src/lib.rs)
- [sdk/keypair/src/lib.rs](https://github.com/zelana-Labs/zelana/blob/main/sdk/keypair/src/lib.rs)
- [sdk/txblob/src/types.rs](https://github.com/zelana-Labs/zelana/blob/main/sdk/txblob/src/types.rs)
