# Zelana Architecture Overview

## What is Zelana?

Zelana is a **privacy-focused Layer 2 rollup prototype** built on Solana. It combines:

- **Shielded transaction primitives** for privacy (nullifiers, commitments, ZK proofs)
- **Threshold encryption building blocks** for MEV resistance (transactions encrypted until ordering is finalized)
- **Low-latency UDP transport** (Zephyr) for fast transaction submission
- **ZK rollup settlement** on Solana using Groth16 proofs (BN254), with hooks for other proof systems

## High-Level Architecture

```
 _______________________________
|        ZELANA L2 ROLLUP       |
|_______________________________|

 _______________________________
|         CLIENT LAYER          |
| TypeScript SDK | React UI     |
| Rust CLI      | Zephyr UDP    |
|_______________________________|

            HTTP/REST + UDP/Zephyr
                     |
                     v

 _______________________________
|       SEQUENCER (core/)       |
| API -> Pipeline -> Settlement |
| Threshold Mempool -> Batch    |
| TxRouter <-> Storage Layer    |
| Bridge (deposits/withdrawals) |
|_______________________________|

 WebSocket (deposits) / Transactions (settlement)
                     |
                     v

 _______________________________
| SOLANA L1 (onchain-programs/) |
| Bridge Program | Verifier     |
|_______________________________|
```

## Core Components

### 1. Sequencer (`core/`)

The sequencer is the heart of Zelana. It:
- Receives transactions (HTTP or UDP)
- Executes them against L2 state
- Batches transactions into blocks
- Generates ZK proofs
- Settles batches on Solana L1

#### Key Modules:

| Module | Path | Purpose |
|--------|------|---------|
| API | `core/src/api/` | HTTP endpoints (Axum-based) |
| Pipeline | `core/src/sequencer/pipeline.rs` | Orchestrates batch→prove→settle flow |
| BatchManager | `core/src/sequencer/execution/batch.rs` | Manages batch lifecycle |
| TxRouter | `core/src/sequencer/execution/tx_router.rs` | Routes and executes transactions |
| Storage | `core/src/sequencer/storage/` | RocksDB persistence layer |
| Bridge | `core/src/sequencer/bridge/` | L1↔L2 deposit/withdrawal handling |
| Prover | `core/src/sequencer/settlement/prover.rs` | ZK proof generation |
| Settler | `core/src/sequencer/settlement/settler.rs` | L1 batch submission |

### 2. On-Chain Programs (`onchain-programs/`)

Two Solana programs secure the L1 side:

| Program | Purpose |
|---------|---------|
| **Bridge** | Handles deposits, withdrawals, batch submissions |
| **Verifier** | Verifies Groth16 proofs on-chain |

### 3. SDK Crates (`sdk/`)

| Crate | Purpose |
|-------|---------|
| `zelana-transaction` | Transaction types and signing |
| `zelana-privacy` | Shielded transactions, notes, nullifiers |
| `zelana-threshold` | Threshold encryption for MEV resistance |
| `zelana-block` | Block header structure |
| `zelana-account` | Account ID and state types |
| `txblob` | Encrypted transaction blob format + crypto |
| `zephyr` | Low-latency UDP transport protocol |

### 4. Prover (`prover/`)

The ZK prover generates Groth16 proofs (BN254 curve) that:
- Prove correct state transitions
- Verify all transactions were valid
- Enable trustless L1 verification

## Batches vs Blocks

**Key Insight: A Block is the finalized form of a Batch**

```
 ________________________________
| Batch (during processing)      |
| batch_id: u64                  |
| transactions: Vec<Tx>          |
| state: BatchState              |
| pre_state_root                 |
| post_state_root                |
| diff: BatchDiff                |
| proof: Option<BatchProof>      |
|_______________________________|

 ________________________________
| Block (after finalization)     |
| batch_id: u64 (same!)          |
| prev_root: [u8; 32]            |
| new_root: [u8; 32]             |
| tx_count: u32                  |
| open_at: u64 (timestamp)       |
| flags: u32                     |
| magic: "ZLNA"                  |
| hdr_version: u16               |
|_______________________________|
```

### Batch Lifecycle

```
Accumulating → Sealed → Proving → Proved → Settling → Finalized
                                                          ↓
                                                   BlockHeader created
                                                          ↓
                                                   Stored in CF_BLOCKS
```

### Why Two Concepts?

| Concept | When Used | Purpose |
|---------|-----------|---------|
| **Batch** | During processing | Internal unit with full transaction data, proofs, diffs |
| **Block** | After settlement | Compact 96-byte header for chain state |

### BlockHeader Structure

```rust
pub struct BlockHeader {
    pub magic: [u8; 4],       // "ZLNA" - identifies Zelana blocks
    pub hdr_version: u16,     // Currently 1
    pub batch_id: u64,        // Same ID as the batch (1:1 mapping)
    pub prev_root: [u8; 32],  // State root before batch execution
    pub new_root: [u8; 32],   // State root after batch execution
    pub tx_count: u32,        // Number of transactions in this block
    pub open_at: u64,         // Timestamp when batch was opened
    pub flags: u32,           // Reserved for future use
}
```

## Pipeline Parallelism

The sequencer runs three operations in parallel:

```
Time ________________________________________________▶

       ________________________
      |  Batch N              | -> Accumulating transactions
      |  ACCUMULATING         |
      |_______________________|

                         ________________________
                        |  Batch N-1            | -> Generating ZK proof
                        |  PROVING              |
                        |_______________________|

                                           ________________________
                                          |  Batch N-2            | -> Submitting to L1
                                          |  SETTLING             |
                                          |_______________________|
```

This maximizes throughput by overlapping:
1. **Accumulation** - Collecting new transactions
2. **Proving** - CPU-intensive ZK proof generation
3. **Settlement** - Network-bound L1 submission

## Transaction Types

Zelana supports four transaction types:

| Type | Privacy | Description |
|------|---------|-------------|
| `Transfer` | Transparent | Standard L2 balance transfer |
| `Deposit` | Transparent | L1 → L2 deposit (indexed from Solana) |
| `Withdraw` | Transparent | L2 → L1 withdrawal |
| `Shielded` | Private | ZK-shielded transfer with nullifier/commitment |

## State Machine Summary

| Component | States | Primary Trigger |
|-----------|--------|-----------------|
| **Transaction** | pending → included → executed → settled | Submission, execution, settlement |
| **Batch** | Accumulating → Sealed → Proving → Proved → Settling → Finalized | TX count, time, proof, L1 confirm |
| **Withdrawal** | Pending → InBatch → Submitted → Finalized | Batch progression |
| **Shielded Note** | Created → Inserted → Spent | Commitment, nullifier reveal |
| **Deposit** | Indexed → InBatch → Credited | L1 event, batch execution |

## Storage Architecture

### RocksDB Column Families

| CF Name | Key Format | Value Format | Purpose |
|---------|------------|--------------|---------|
| `accounts` | `[u8; 32]` (AccountId) | `wincode(AccountState)` | L2 balances/nonces |
| `blocks` | `u64` (batch_id, BE) | `wincode(BlockHeader)` | Finalized block headers |
| `batches` | `u64` (batch_id, BE) | `JSON(BatchSummary)` | Batch metadata |
| `tx_index` | `[u8; 32]` (tx_hash) | `JSON(TxSummary)` | Transaction lookups |
| `tx_blobs` | `[u8; 32]` (tx_hash) | `Vec<u8>` (encrypted) | Encrypted tx blobs |
| `nullifiers` | `[u8; 32]` | `[]` (empty) | Double-spend prevention |
| `commitments` | `u32` (position, BE) | `[u8; 32]` | Merkle tree notes |
| `encrypted_notes` | `[u8; 32]` (commitment) | `JSON(EncryptedNote)` | For viewing key scanning |
| `tree_meta` | `string` (key) | varies | Merkle tree frontier |
| `withdrawals` | `[u8; 32]` (tx_hash) | `Vec<u8>` | Pending withdrawals |
| `processed_deposits` | `u64` (L1 seq, BE) | `u64` (slot, BE) | Deposit deduplication |
| `indexer_meta` | `string` (key) | `u64` (slot) | Indexer checkpoint |

## Security Model

### Trust Assumptions

1. **Sequencer**: Currently centralized (single sequencer authority)
2. **ZK Proofs**: State transitions are verified on L1 via Groth16
3. **Bridge**: Deposits/withdrawals are secured by L1 program logic
4. **Privacy**: Shielded transactions use Zcash-style nullifiers

### MEV Resistance

When enabled, threshold encryption protects against MEV:
1. Transactions are encrypted with threshold key
2. Sequencer orders encrypted transactions
3. After ordering is finalized, threshold is reached
4. Transactions are decrypted and executed in the fixed order

## Configuration

Configuration loads in this order:

1. `ZL_CONFIG` (explicit config path)
2. `./config.toml` (repository root)
3. `~/.zelana/config.toml`

Environment variables override file values. Common overrides:

| Variable | Purpose |
|----------|---------|
| `ZL_DB_PATH` | RocksDB storage path |
| `ZL_API_HOST` | Sequencer host (HTTP) |
| `ZL_UDP_PORT` | UDP ingest port |
| `SOLANA_RPC_URL` | Solana RPC endpoint |
| `SOLANA_WS_URL` | Solana WebSocket endpoint |
| `ZL_BRIDGE_PROGRAM` | Bridge program ID |
| `ZL_VERIFIER_PROGRAM` | Verifier program ID |
| `ZL_PROVER_MODE` | `mock`, `groth16`, or `noir` |
| `ZL_SETTLEMENT_ENABLED` | Toggle settlement pipeline |

## Related Documentation

- [State Machines: Sequencer](./state-machines/sequencer.md)
- [State Machines: Bridge](./state-machines/bridge.md)
- [State Machines: Prover](./state-machines/prover.md)
- [Transaction Types](./state-machines/types.md)
- [Zephyr Protocol](./zephyr.md)

## Implementation Links (GitHub)

Use these links to jump directly to key architecture files:

- [core/src/main.rs](https://github.com/zelana-Labs/zelana/blob/main/core/src/main.rs)
- [core/src/sequencer/pipeline.rs](https://github.com/zelana-Labs/zelana/blob/main/core/src/sequencer/pipeline.rs)
- [core/src/sequencer/execution/batch.rs](https://github.com/zelana-Labs/zelana/blob/main/core/src/sequencer/execution/batch.rs)
- [core/src/sequencer/execution/tx_router.rs](https://github.com/zelana-Labs/zelana/blob/main/core/src/sequencer/execution/tx_router.rs)
- [core/src/sequencer/settlement/settler.rs](https://github.com/zelana-Labs/zelana/blob/main/core/src/sequencer/settlement/settler.rs)
- [core/src/sequencer/settlement/noir_client.rs](https://github.com/zelana-Labs/zelana/blob/main/core/src/sequencer/settlement/noir_client.rs)
- [core/src/sequencer/bridge/ingest.rs](https://github.com/zelana-Labs/zelana/blob/main/core/src/sequencer/bridge/ingest.rs)
- [onchain-programs/bridge/src/lib.rs](https://github.com/zelana-Labs/zelana/blob/main/onchain-programs/bridge/src/lib.rs)
- [onchain-programs/verifier/programs/onchain_verifier/src/lib.rs](https://github.com/zelana-Labs/zelana/blob/main/onchain-programs/verifier/programs/onchain_verifier/src/lib.rs)
- [forge/crates/prover-coordinator/](https://github.com/zelana-Labs/zelana/tree/main/forge/crates/prover-coordinator)
- [sdk/zephyr/src/lib.rs](https://github.com/zelana-Labs/zelana/blob/main/sdk/zephyr/src/lib.rs)
