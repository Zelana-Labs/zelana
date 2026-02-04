---
sidebar_position: 3
---

# Parallel Swarm

The **parallel swarm** is Forge’s execution layer: a coordinator plus multiple prover workers that run circuits in parallel. The swarm design is intended to scale throughput without weakening the threshold privacy guarantees.

## What “Parallel” Means Here

- The coordinator splits a proving job into independent chunks.
- Each worker runs the Noir circuit on its chunk in parallel.
- The coordinator aggregates the returned fragments into a final proof.

## Setup and Run (Developer View)

For full step-by-step commands, see the README in `zelana-forge/deploy/docker/README.md`.

Typical workflow:

1. Build or pull the coordinator and worker images.
2. Bring up a local swarm using the Docker compose file.
3. Verify that workers register and the coordinator reports healthy nodes.
4. Submit a prove request and observe proof aggregation.

## Operational Notes (Protocol Designer View)

- The parallel swarm is designed to support `k-of-n` threshold proving.
- The coordinator can enforce a minimum threshold of participating workers.
- Increasing swarm size improves throughput, but does not change the trust model unless the threshold changes.
