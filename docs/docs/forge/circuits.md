---
sidebar_position: 4
---

# Circuits

Forge uses Noir circuits to define the statements being proven. These circuits live in `zelana-forge/circuits/` and are executed by prover workers.

## Circuit Index

- `ownership/`: Client-side ownership proof.
- `zelana_batch/`: Batch rollup circuit for transfers, withdrawals, and shielded txs.
- `batch_processor/`: Simplified batch processor circuit (prototype / hackathon scope).
- `zelana_lib/`: Shared Noir library functions used across circuits.

## ownership

**Purpose:** Prove a user owns a note without revealing the spending key.

- Private inputs: spending key, note value, blinding, position.
- Public outputs: commitment, nullifier, blinded proxy.
- The circuit re-derives the public key from the spending key, checks the commitment, derives a nullifier, and computes a blinded proxy used by the swarm to fetch a Merkle path.

## zelana_batch

**Purpose:** Prove correctness of a batch of L2 transactions.

Public inputs (7):

- `pre_state_root`, `post_state_root`
- `pre_shielded_root`, `post_shielded_root`
- `withdrawal_root`
- `batch_hash`
- `batch_id`

Private witness arrays:

- Transfers (sender/receiver membership + balance updates)
- Withdrawals (sender membership + L1 recipient)
- Shielded txs (commitments + nullifiers)

The circuit validates membership in Merkle trees, verifies balances/nonces, updates roots, and accumulates batch hashes.

## batch_processor

**Purpose:** A simplified batch circuit used for early validation flows.

- Verifies basic signatures and Merkle membership for a fixed-size batch.
- Serves as a lightweight reference implementation of the batch flow.

## zelana_lib

Shared Noir primitives used by Forge circuits:

- Poseidon hash utilities (SNARK-friendly hashing).
- Sparse Merkle tree operations (32 levels).
- Nullifier computation.
- Account leaf derivation.
- Simplified Schnorr-style signature verification.

## For Developers

- Keep the Rust-side worker input struct definitions aligned with the Noir circuit inputs.
- When changing a circuit, update the worker and coordinator input normalization logic.
- The ownership circuit is intended for client-side proving (WASM/mobile), while batch circuits run in the swarm.

## For Protocol Designers

- Circuit public inputs define what the verifier sees on-chain.
- Changes to nullifier rules, Merkle structure, or batch hash logic are protocol changes.
- Consider circuit constraints and hash domains as part of the security boundary.
