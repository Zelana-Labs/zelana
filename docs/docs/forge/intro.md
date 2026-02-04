---
sidebar_position: 1
---

# Forge Overview

Forge is Zelana’s distributed proving system. In the repo it lives under `zelana-forge/`, but in documentation we refer to it simply as **Forge**.

Forge lets a client prove statements without revealing sensitive witness data to any single node. It does this by splitting proving work across a coordinator and a swarm of prover workers, then aggregating the result into a single proof.

## Audience

This section is intentionally written for two audiences:

- **Developers** who need to integrate, run, and debug Forge.
- **Protocol designers** who need to understand the trust model, data flow, and cryptographic guarantees.

## System Map

- **Client**: Creates commitments and minimal public inputs.
- **Coordinator**: Orchestrates proving jobs and aggregates responses.
- **Prover Workers (Swarm)**: Execute Noir circuits and return proof fragments.
- **Circuits**: Noir programs that define the statements being proven.

## For Developers

- Start with the parallel swarm setup guide to run a coordinator and workers locally.
- Use the circuits reference to understand required inputs and expected public outputs.
- When you are building APIs, the coordinator is the primary entry point; workers are designed to be stateless compute nodes.

## For Protocol Designers

- Focus on the architecture and privacy model: Forge uses threshold-style distributed proving so no single node learns the full witness.
- The circuit boundary is where security properties are defined. The on-chain or L1 verifiers only see public inputs and final proofs, not witness data.
- The swarm design is intended to scale horizontally while preserving the threshold guarantee.
