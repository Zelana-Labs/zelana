# State Machines

This section documents the state machines for all major Zelana components. Understanding these state transitions is crucial for debugging, extending, and reasoning about the system.

## Overview

Zelana is built around several interconnected state machines that govern transaction processing, batch lifecycle, and cross-chain operations.

## Core State Machines

### [Sequencer](./sequencer.md)

The sequencer is the heart of Zelana's L2. It manages:

- **Process States**: Initializing → Running → Shutdown
- **Session/Batch States**: Created → Collecting → Closing → Committed
- **Executor States**: Idle → Executing → Dirty Cache → Persisted
- **Network Session States**: No Session → Active → Authenticated → Expired

Key transitions are triggered by transaction submissions, batch thresholds, and timeouts.

### [Bridge](./bridge.md)

The bridge handles L1↔L2 asset transfers:

- **Config Account**: NonExistent → Initialized
- **Vault Account**: Tracks SOL balance (+deposits, -withdrawals)
- **Deposit Receipt**: NonExistent → Initialized (permanent proof)
- **Used Nullifier**: NonExistent → Used (replay protection)

All bridge state is append-only for auditability.

### [Prover](./prover.md)

The prover service generates ZK proofs for batches:

- **Batch Proof States**: Pending → Settled-OnChain (or SettlementFailed)
- **Proof Generation Flow**: Fetch → Witness Build → Circuit → Groth16 → Export

Uses BN254 curve with Groth16 proving system.

### [Transaction Types](./types.md)

Documents all transaction structures and their lifecycle:

- **Transfer**: Standard L2 balance transfer
- **Deposit**: L1 → L2 deposit with receipt
- **Withdraw**: L2 → L1 withdrawal with nullifier
- **Shielded**: Private transaction with ZK proof

## Unified Transaction Lifecycle

```
                          ┌─────────────────────────────────────────┐
                          │            TRANSACTION FLOW              │
                          └─────────────────────────────────────────┘
                                           │
     ┌─────────────────────────────────────┼─────────────────────────────────────┐
     │                                     │                                     │
     ▼                                     ▼                                     ▼
┌─────────┐                          ┌─────────┐                          ┌─────────┐
│ Deposit │                          │Transfer │                          │Withdraw │
│ (L1→L2) │                          │Shielded │                          │ (L2→L1) │
└────┬────┘                          └────┬────┘                          └────┬────┘
     │                                    │                                    │
     ▼                                    ▼                                    ▼
┌─────────────┐                    ┌─────────────┐                    ┌─────────────┐
│ Indexed by  │                    │ Encrypted   │                    │ Signature   │
│ DepositIdx  │                    │ Submission  │                    │ Verified    │
└──────┬──────┘                    └──────┬──────┘                    └──────┬──────┘
       │                                  │                                  │
       └──────────────────┬───────────────┴──────────────────┬───────────────┘
                          │                                  │
                          ▼                                  ▼
                   ┌─────────────┐                    ┌─────────────┐
                   │   PENDING   │                    │  INCLUDED   │
                   │ (in mempool)│─────────────────▶  │ (in batch)  │
                   └─────────────┘                    └──────┬──────┘
                                                             │
                                                             ▼
                                                      ┌─────────────┐
                                                      │  EXECUTED   │
                                                      │(state diff) │
                                                      └──────┬──────┘
                                                             │
                          ┌──────────────────────────────────┼──────────────────────────────────┐
                          │                                  │                                  │
                          ▼                                  ▼                                  ▼
                   ┌─────────────┐                    ┌─────────────┐                    ┌─────────────┐
                   │   PROVED    │                    │  SETTLING   │                    │ FINALIZED   │
                   │ (ZK proof)  │─────────────────▶  │  (on L1)    │─────────────────▶  │ (confirmed) │
                   └─────────────┘                    └─────────────┘                    └─────────────┘
```

## Batch Lifecycle

```
┌────────────┐     ┌────────┐     ┌─────────┐     ┌────────┐     ┌──────────┐     ┌───────────┐
│Accumulating│────▶│ Sealed │────▶│ Proving │────▶│ Proved │────▶│ Settling │────▶│ Finalized │
└────────────┘     └────────┘     └─────────┘     └────────┘     └──────────┘     └───────────┘
      │                 │              │               │              │                 │
      │                 │              │               │              │                 │
   Collect          Freeze          Generate        Proof          Submit           L1
   transactions     batch           ZK proof        ready          to L1           confirmed
```

## Key Invariants

1. **Sequential Batches**: batch_id always increments by 1
2. **Nullifier Uniqueness**: Each nullifier can only be used once
3. **Nonce Ordering**: Account nonces must match transaction nonces
4. **State Root Chain**: Each block's prev_root == previous block's new_root
5. **Single Sequencer**: Only configured authority can submit batches
