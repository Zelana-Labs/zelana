---
title: "Forge Prover State Machines"
description: "Distributed proving in Zelana Forge: coordinator, workers, and sequencer integration."
---

## Overview

Zelana no longer relies on the centralized prover in `prover/`. Proof generation
is handled by the **Forge network** in `forge/`, which distributes proving across
coordinator + worker nodes. The sequencer integrates through the Forge **Core
API** (`/v2/batch/*`) using `core/src/sequencer/settlement/noir_client.rs`.

## Implementation Sources

- **Coordinator**: `forge/crates/prover-coordinator`
- **Parallel workers**: `forge/crates/prover-worker`
- **Core API (sequencer)**: `forge/crates/prover-coordinator/src/core_api.rs`
- **Batch dispatcher**: `forge/crates/prover-coordinator/src/dispatcher.rs`
- **Settlement**: `forge/crates/prover-coordinator/src/settler.rs`
- **Blind proving nodes (legacy/optional)**: `forge/crates/prover-node`

## Core API Proof Job State (Sequencer Integration)

Used by the sequencer through the Core API endpoints:
- `POST /v2/batch/prove`
- `GET /v2/batch/:job_id/status` (SSE)
- `GET /v2/batch/:job_id/proof`
- `DELETE /v2/batch/:job_id`

### States

| State | Description |
| --- | --- |
| `pending` | Job accepted, waiting to start |
| `preparing` | Inputs validated, witness prepared |
| `proving` | Proof generation in progress |
| `completed` | Proof ready for retrieval |
| `failed` | Job failed, error stored |
| `cancelled` | Job cancelled by client |

### State Flow

```
 __________________________
| pending                  |
|__________________________|
            |
            v
 __________________________
| preparing                |
|__________________________|
            |
            v
 __________________________
| proving                  |
|__________________________|
            |
            v
 __________________________
| completed                |
|__________________________|
            |
            v
 __________________________
| proof available          |
| GET /v2/batch/:job_id    |
|__________________________|

(error paths)
  pending/preparing/proving -> failed
  pending/preparing -> cancelled
```

## Parallel Swarm Batch State (Coordinator)

Used by the coordinator for chunked proving via `/batch/submit` and worker
dispatch. States are defined in `forge/crates/prover-coordinator/src/main.rs`.

### States

| State | Description |
| --- | --- |
| `pending` | Batch accepted, waiting to slice |
| `slicing` | Chunking transactions + computing roots |
| `proving` | Dispatching chunks to workers |
| `settling` | Optional Solana verification (mock or real) |
| `completed` | Proofs collected (and settled if enabled) |
| `failed` | Any error in worker, proof, or settlement |

### State Flow

```
 __________________________
| pending                  |
|__________________________|
            |
            v
 __________________________
| slicing                  |
|__________________________|
            |
            v
 __________________________
| proving                  |
|__________________________|
            |
            v
 __________________________
| settling (optional)      |
|__________________________|
            |
            v
 __________________________
| completed                |
|__________________________|

(error path)
  pending/slicing/proving/settling -> failed
```

## Prover Worker Job State (Parallel Swarm)

Workers run Noir/Sunspot to prove a chunk. Endpoints:
- `GET /health`
- `POST /prove`
- `GET /status/:job_id`

### States

| State | Description |
| --- | --- |
| `pending` | Job accepted by worker |
| `proving` | Circuit execution + proof generation |
| `completed` | Proof ready for pickup |
| `failed` | Worker error |

### State Flow

```
 __________________________
| pending                  |
|__________________________|
            |
            v
 __________________________
| proving                  |
|__________________________|
            |
            v
 __________________________
| completed                |
|__________________________|

(error path)
  pending/proving -> failed
```

## Blind Proving Node State (Legacy/Optional)

The blind proving node (`forge/crates/prover-node`) supports privacy-preserving
threshold Schnorr flows. It is **not** used by the sequencer batch pipeline.

Endpoints:
- `POST /share`
- `POST /commitment`
- `POST /fragment`

### State Flow

```
 __________________________
| idle                     |
|__________________________|
            |
            v
 __________________________
| share assigned           |
|__________________________|
            |
            v
 __________________________
| commitment generated     |
|__________________________|
            |
            v
 __________________________
| fragment generated       |
|__________________________|
            |
            v
 __________________________
| idle                     |
|__________________________|
```

## Sequencer Integration Summary

```
 __________________________
| Core Sequencer            |
| (NoirProverClient)        |
|__________________________|
            |
            v
 __________________________
| Forge Coordinator         |
| /v2/batch/prove           |
|__________________________|
            |
            v
 __________________________
| Prover Workers            |
| /prove (chunk proofs)     |
|__________________________|
            |
            v
 __________________________
| Proof Result              |
| /v2/batch/:job_id/proof   |
|__________________________|
```

## Implementation Notes

- **Mock modes**: Coordinator supports mock settlement and mock proving for
  development (`MOCK_SETTLEMENT`, `MOCK_PROVER`).
- **Chunking**: Batch slicing and intermediate roots are computed in
  `dispatcher.rs` using deterministic hashing (placeholder for real state
  execution). Replace with real state transition logic before production.
- **Proof outputs**: Core API returns `proof_bytes` and `public_witness_bytes`
  as hex strings along with `batch_hash` and `withdrawal_root`.

## Implementation Links (GitHub)

Use these links to jump directly to the implementation files:

- [forge/crates/prover-coordinator/src/main.rs](https://github.com/zelana-Labs/zelana/blob/main/forge/crates/prover-coordinator/src/main.rs)
- [forge/crates/prover-coordinator/src/core_api.rs](https://github.com/zelana-Labs/zelana/blob/main/forge/crates/prover-coordinator/src/core_api.rs)
- [forge/crates/prover-coordinator/src/dispatcher.rs](https://github.com/zelana-Labs/zelana/blob/main/forge/crates/prover-coordinator/src/dispatcher.rs)
- [forge/crates/prover-coordinator/src/settler.rs](https://github.com/zelana-Labs/zelana/blob/main/forge/crates/prover-coordinator/src/settler.rs)
- [forge/crates/prover-worker/src/main.rs](https://github.com/zelana-Labs/zelana/blob/main/forge/crates/prover-worker/src/main.rs)
- [forge/crates/prover-node/src/main.rs](https://github.com/zelana-Labs/zelana/blob/main/forge/crates/prover-node/src/main.rs)
- [core/src/sequencer/settlement/noir_client.rs](https://github.com/zelana-Labs/zelana/blob/main/core/src/sequencer/settlement/noir_client.rs)
