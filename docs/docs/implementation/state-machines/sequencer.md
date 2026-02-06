# Zelana Sequencer State Machine (Implementation-Accurate)

This document describes the sequencer state machines as they behave **today** in the codebase. It focuses on internal execution and batch lifecycle, not API surface. API docs will live elsewhere.

## 1. Runtime Overview (Process Level)

The sequencer starts three long‑lived subsystems and keeps them running concurrently:

- **Ingress**: HTTP + optional UDP server that submits transactions into the pipeline
- **Pipeline**: batch execution, proving, and settlement loop
- **Deposit Indexer**: L1 log subscriber that turns bridge events into L2 deposits

```
 __________________________
|         STARTUP          |
| - load config            |
| - open RocksDB           |
| - load shielded state    |
| - start pipeline         |
| - start ingress servers  |
| - start deposit indexer  |
|__________________________|
            |
            v
 __________________________
|          RUNNING         |
| - ingress -> pipeline    |
| - pipeline tick loop     |
| - deposit indexer        |
|__________________________|
            |
            | Ctrl+C
            v
 __________________________
|       SHUTTING_DOWN      |
| - seal pending batch     |
| - stop pipeline loop     |
| - exit                   |
|__________________________|
```

## 2. Pipeline Orchestrator State Machine

The pipeline is driven by a timed loop (`tick`) and command channel. It runs in parallel with ingress and the indexer.

```
 __________________________
|         RUNNING          |
| - tick(): timeout check  |
| - try_prove()            |
| - try_settle()           |
|__________________________|
            |
            | settlement failures exceed max_retries
            v
 __________________________
|          PAUSED          |
| - reason recorded        |
| - tick() returns early   |
|__________________________|
            |
            | resume()
            v
 __________________________
|         RUNNING          |
|__________________________|
            |
            | shutdown
            v
 __________________________
|         STOPPING         |
| - seal pending txs       |
| - exit loop              |
|__________________________|
```

Key behaviors:

- `try_prove()` picks the next **sealed** batch and generates a proof in a blocking task.
- `try_settle()` picks the next **proved** batch and submits it to L1 (or mock settler).
- If settlement fails too many times, the pipeline pauses and requires an explicit resume.

## 3. Batch Lifecycle State Machine

Batches are created and executed inside the `BatchManager` and flow through six concrete states.

```
 __________________________
|       ACCUMULATING       |
| - accept transactions    |
| - update pending state   |
|__________________________|
            |
            | should_seal()
            v
 __________________________
|          SEALED          |
| - execute batch          |
| - produce diff + roots   |
|__________________________|
            |
            | prepare_batch_for_proving()
            v
 __________________________
|         PROVING          |
| - build witness          |
| - generate proof         |
|__________________________|
            |
            | proof ok
            v
 __________________________
|          PROVED          |
| - ready for settlement   |
|__________________________|
            |
            | submit to L1
            v
 __________________________
|         SETTLING         |
| - L1 tx submitted        |
|__________________________|
            |
            | confirmed
            v
 __________________________
|         FINALIZED        |
| - commit diff to DB      |
| - update summaries       |
|__________________________|
```

### Seal Triggers (exact logic)

A batch seals when **any** of the following is true:

- `transactions.len() >= max_transactions`
- `shielded_count >= max_shielded`
- `withdrawal_count >= 1` (one withdrawal per batch)
- `shielded_count >= 1` (one shielded tx per batch)
- On submit: `age >= max_batch_age_secs` **and** `transactions.len() >= min_transactions`
- On timeout tick: `age >= max_batch_age_secs` **and** `transactions.len() > 0`

### Dev Mode Note

If `dev_mode` is enabled, the batch **commits immediately on seal** and the witness is built **before** commit to preserve pre‑state Merkle paths.

### Proving Inputs (what the sequencer actually builds)

- `prepare_batch_for_proving()` computes a **withdrawal root** using MiMC (matches the Noir circuit).
- `build_public_inputs()` uses pre/post state roots, shielded roots, withdrawal root, batch hash, and batch ID.
- `build_witness_with_proofs()` builds Merkle paths from the **pre‑batch** account tree.

## 4. Transaction Execution State Machine (TxRouter)

Transactions are routed by type and executed into a unified `BatchDiff`.

```
 __________________________
|       RECEIVED TX        |
| (TransactionType)        |
|__________________________|
            |
            | compute tx_hash
            v
 __________________________
|      ROUTE BY TYPE       |
| Shielded / Transfer /    |
| Deposit / Withdraw       |
|__________________________|
            |
            | validate + execute
            v
 __________________________
|      MUTATE STATE        |
| - account_cache          |
| - shielded_state         |
| - withdrawals queue      |
|__________________________|
            |
            | push TxResult
            v
 __________________________
|       BATCH DIFF         |
| - account_updates        |
| - shielded_diff          |
| - withdrawals            |
| - results                |
|__________________________|
```

Type‑specific rules:

- **Transfer**: verifies signature, checks balance and nonce, debits sender, credits receiver.
- **Withdraw**: verifies signature, checks balance and nonce, debits sender, queues withdrawal.
- **Deposit**: credits receiver (routed from the indexer).
- **Shielded**: checks nullifier, validates proof size (full Groth16 verification is TODO), optionally debits/credits transparent balances during shield/unshield.

Error handling:

- A failure produces `TxResult { success: false }` and the batch continues.
- Failed transactions are stored in `TxSummary` with status `Failed` and never upgraded to `Settled`.

Pending state visibility:

- `pending_states` tracks transactions **added** to the current batch but not executed yet.
- `account_cache` tracks **executed** but not committed changes.
- `get_pending_account()` checks both to provide optimistic balances and nonces.

## 5. Settlement State Machine

Settlement happens after a batch is proved.

```
 __________________________
|        PROVED BATCH      |
| - proof ready            |
|__________________________|
            |
            | try_settle()
            v
 __________________________
|      SUBMIT TO L1        |
| - submit_auto()          |
| - or submit withdrawals  |
|__________________________|
            |
            | success
            v
 __________________________
|     SETTLED + FINALIZE   |
| - batch_settled()        |
| - batch_finalized()      |
| - store BatchSummary     |
| - update TxStatus        |
| - execute withdrawals    |
|__________________________|
            |
            | failure
            v
 __________________________
|      RETRY / PAUSE       |
| - exponential backoff    |
| - pause after max        |
|__________________________|
```

Important behaviors:

- Settlement uses `submit_auto()` or `submit_with_withdrawals()` depending on whether withdrawals are present and the proof format allows it.
- If withdrawals exist **and** the proof format is Noir, the system logs a warning and submits without withdrawal processing.
- After successful settlement, withdrawals are executed on L1 in batches when the real settler is configured.

## 6. Deposit Indexer State Machine

The indexer listens for bridge logs and routes deposits through the pipeline.

```
 __________________________
|      SUBSCRIBE LOGS      |
| - ws_url                 |
| - bridge_program_id      |
|__________________________|
            |
            | log: ZE_DEPOSIT
            v
 __________________________
|      PARSE + DEDUPE      |
| - parse <pubkey:amt:seq> |
| - skip if l1_seq seen    |
|__________________________|
            |
            | submit Deposit
            v
 __________________________
|    ROUTE TO PIPELINE     |
| - pipeline.submit()      |
| - mark processed on OK   |
|__________________________|
```

Notes:

- Historical backfill is **not** implemented yet (placeholder only).
- Deduplication is keyed by `l1_seq` stored in RocksDB.
- The live log subscription currently uses `CommitmentConfig::processed()`.

## 7. Withdrawal Tracking State Machine

Withdrawals are created during execution and persisted on commit. Settlement builds a withdrawal Merkle root and can execute L1 transfers after settlement. The in‑memory `WithdrawalQueue` exists for tracking, but the pipeline currently builds its own `TrackedWithdrawal` set during settlement rather than mutating the queue directly.

```
 __________________________
|      REQUESTED (L2)      |
| - withdraw tx accepted   |
|__________________________|
            |
            | included in batch
            v
 __________________________
|       IN BATCH           |
| - withdrawal in diff     |
|__________________________|
            |
            | batch settled
            v
 __________________________
|       SUBMITTED (L1)     |
| - L1 tx signature        |
|__________________________|
            |
            | execute withdrawals
            v
 __________________________
|        FINALIZED         |
| - L1 transfer complete   |
|__________________________|
```

## 8. Implementation Links (GitHub)

Use these links to jump directly to the implementation files:

- [core/src/main.rs](https://github.com/zelana-Labs/zelana/blob/main/core/src/main.rs)
- [core/src/sequencer/pipeline.rs](https://github.com/zelana-Labs/zelana/blob/main/core/src/sequencer/pipeline.rs)
- [core/src/sequencer/execution/batch.rs](https://github.com/zelana-Labs/zelana/blob/main/core/src/sequencer/execution/batch.rs)
- [core/src/sequencer/execution/tx_router.rs](https://github.com/zelana-Labs/zelana/blob/main/core/src/sequencer/execution/tx_router.rs)
- [core/src/sequencer/bridge/ingest.rs](https://github.com/zelana-Labs/zelana/blob/main/core/src/sequencer/bridge/ingest.rs)
- [core/src/sequencer/bridge/withdrawals.rs](https://github.com/zelana-Labs/zelana/blob/main/core/src/sequencer/bridge/withdrawals.rs)
- [core/src/sequencer/settlement/settler.rs](https://github.com/zelana-Labs/zelana/blob/main/core/src/sequencer/settlement/settler.rs)
- [core/src/sequencer/settlement/prover.rs](https://github.com/zelana-Labs/zelana/blob/main/core/src/sequencer/settlement/prover.rs)
- [core/src/sequencer/settlement/noir_client.rs](https://github.com/zelana-Labs/zelana/blob/main/core/src/sequencer/settlement/noir_client.rs)
- [core/src/sequencer/storage/db.rs](https://github.com/zelana-Labs/zelana/blob/main/core/src/sequencer/storage/db.rs)
- [core/src/sequencer/storage/account_tree.rs](https://github.com/zelana-Labs/zelana/blob/main/core/src/sequencer/storage/account_tree.rs)
- [core/src/sequencer/storage/shielded_state.rs](https://github.com/zelana-Labs/zelana/blob/main/core/src/sequencer/storage/shielded_state.rs)
