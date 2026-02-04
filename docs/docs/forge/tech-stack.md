---
sidebar_position: 5
---

# Tech Stack

This page summarizes the main technologies and cryptographic primitives used by Forge.

## Languages and Runtimes

- **Rust**: Coordinator, workers, and core cryptography (`zelana-forge/crates/*`).
- **Noir**: Circuits are authored in Noir (`zelana-forge/circuits/*`).
- **nargo + sunspot**: Used by the worker to execute Noir circuits (see `zelana-forge/crates/prover-worker`).

## Cryptography and Primitives

- **Threshold proving**: Coordinator and workers collaborate using a `k-of-n` threshold model.
- **Shamir-style secret sharing**: Used to split secrets across nodes.
- **Schnorr-style proofs**: Used in the core prover flow and some circuit components.
- **Poseidon hash**: SNARK-friendly hash used in circuits and Merkle trees.
- **Sparse Merkle trees**: Account and note commitments use a 32-level tree.
- **Nullifiers**: Prevent double-spends while preserving privacy.

## Deployment and Ops

- **Docker**: Local swarm and containerized deployment (`zelana-forge/deploy/docker`).
- **Kubernetes**: Clustered deployment (`zelana-forge/deploy/k8s`).

## For Developers

- Keep the Noir toolchain version in sync with circuits and worker binaries.
- Any change to hash or Merkle logic must be reflected in both Noir and Rust implementations.

## For Protocol Designers

- Threshold parameters (`k`, `n`) directly affect the adversary model and liveness.
- Hash domain separation is part of the protocol boundary and must be stable.
