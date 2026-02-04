---
sidebar_position: 2
---

# Architecture

Forge is a distributed proving pipeline composed of a coordinator, a swarm of prover workers, and a set of Noir circuits. The coordinator breaks work into chunks, distributes it across workers, and aggregates the results into a single proof.

## Core Components

- **Coordinator** (`zelana-forge/crates/prover-coordinator`): Accepts prove requests, normalizes inputs, and orchestrates workers.
- **Workers** (`zelana-forge/crates/prover-worker`): Execute Noir circuits and return proof fragments.
- **Core Crypto** (`zelana-forge/crates/prover-core`): Threshold logic, Schnorr proofs, and Shamir-style secret sharing.
- **Network Layer** (`zelana-forge/crates/prover-network`): Message formats and transport serialization.
- **Circuits** (`zelana-forge/circuits/*`): Noir programs defining the statements being proven.

## Data Flow (High Level)

1. **Client** creates commitments and public inputs, then submits a prove request.
2. **Coordinator** validates inputs and selects circuit + worker set.
3. **Workers** execute the circuit with provided witnesses and return proof artifacts.
4. **Coordinator** aggregates worker responses into a final proof.
5. **Verifier** checks the final proof using public inputs only.

## For Developers

- The coordinator is the main integration surface; workers are meant to be horizontally scalable and stateless.
- When debugging, trace: client input serialization → coordinator normalization → worker circuit execution → aggregation.
- Keep circuit versions and worker binaries in sync. A circuit change without a worker update will break proving.

## For Protocol Designers

- The trust model assumes fewer than the threshold of workers collude. No single worker should learn the full witness.
- The coordinator is a logical orchestrator. It does not need to be trusted with secrets, but it is critical for liveness.
- Circuit definitions are the source of truth for security properties. Changes in circuits must be reviewed as protocol changes.
