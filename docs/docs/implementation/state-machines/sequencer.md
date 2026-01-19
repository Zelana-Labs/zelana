
# Zelana Sequencer State Machine Analysis

## Overview

The Zelana sequencer is an L2 sequencer for a Solana-based rollup. It receives encrypted transactions via HTTP, executes them against in-memory state, batches them into blocks, and persists state to RocksDB. It also indexes deposit events from L1 (Solana) and supports withdrawals back to L1.

## 1. Complete State Diagram

### 1.1 Sequencer Process States

```
+-------------------+
|   INITIALIZING    |
+-------------------+
         |
         | (DB opened, SessionManager created)
         v
+-------------------+     +-------------------+
|  INGEST_SERVER    |<--->|     INDEXER       |
|    RUNNING        |     |    RUNNING        |
+-------------------+     +-------------------+
         |                         |
         | Ctrl+C                  |
         v                         v
+-------------------+
|   SHUTTING_DOWN   |
+-------------------+
```

**Note:** The sequencer runs two concurrent subsystems:

- **Ingest Server:** HTTP server on port `ZELANA_INGEST_PORT` (default 8080)
- **Indexer:** WebSocket listener for L1 deposit events

### 1.2 Session (Batch) States

```
                     +----------------+
                     |    CREATED     |
                     | (batch_id = N) |
                     +----------------+
                            |
                            | push_execution(ExecutionResult)
                            v
                     +----------------+
                     |   COLLECTING   |
                     | (txs: Vec<>)   |
                     +----------------+
                            |
                            | tx_count >= MAX_TX_PER_BLOCK (2)
                            v
                     +----------------+
                     |    CLOSING     |
                     +----------------+
                            |
                            | close(prev_root, new_root)
                            v
                     +----------------+
                     |  CLOSED        |
                     | (ClosedSession)|
                     +----------------+
                            |
                            | store_block_header()
                            v
                     +----------------+
                     | COMMITTED      |
                     +----------------+
                            |
                            | New Session(batch_id + 1)
                            v
                     +----------------+
                     |    CREATED     |
                     +----------------+
```

**Key Fields:**

- `Session { batch_id: u64, txs: Vec<ExecutionResult> }`
- `ClosedSession { header: BlockHeader, txs: Vec<ExecutionResult> }`

### 1.3 Executor States

```
+------------------+
|     IDLE         |
| (state: empty)   |
+------------------+
         |
         | execute_signed_tx()
         v
+------------------+
|   EXECUTING      |
| (state: cached)  |
+------------------+
         |
         | Successful execution
         v
+------------------+
|  DIRTY_CACHE     |
| (touched: map)   |
+------------------+
         |
         | apply_state_diff()
         v
+------------------+
|   PERSISTED      |
+------------------+
         |
         | (reset() - implicit after batch)
         v
+------------------+
|     IDLE         |
+------------------+
```

**Note:** The executor maintains:

- `accounts: HashMap<AccountId, AccountState>` - In-memory cache
- `touched: HashMap<AccountId, AccountState>` - Dirty write set

### 1.4 Network Session States (SessionManager)

```
+-------------------+
|  NO_SESSION       |
+-------------------+
         |
         | Client connects + X25519 handshake
         | SessionKeys::derive(shared_secret, client_pk, server_pk)
         v
+-------------------+
|  ACTIVE_SESSION   |
| - keys: SessionKeys
| - account_id: Option<AccountId>
| - last_activity: Instant
+-------------------+
         |
         |  (First valid signature received)
         v
+-------------------+
|  AUTHENTICATED    |
| (account_id: Some)|
+-------------------+
         |
         | Timeout or cleanup (retain())
         v
+-------------------+
|  EXPIRED/REMOVED  |
+-------------------+
```

**SessionKeys State:**

- `tx_counter: u64` - Outgoing message counter
- `rx_counter: u64` - Incoming message counter

## 2. State Transitions and Triggers

### 2.1 Transaction Submission Flow

| Current State | Trigger | Next State | Actions |
|---|---|---|---|
| HTTP Idle | POST /submit_tx | Processing | Deserialize blob |
| Processing | Valid blob | Decrypting | `decrypt_signed_tx()` |
| Decrypting | Decryption success + chain_id valid | Executing | Lock executor |
| Executing | `execute_signed_tx()` success | Batching | Push to session |
| Batching | `tx_count < MAX_TX_PER_BLOCK` | HTTP Response | Return ACCEPTED |
| Batching | `tx_count >= MAX_TX_PER_BLOCK` | Closing Batch | Finalize state |

### 2.2 Batch Finalization Triggers

| Trigger | Condition | Actions |
|---|---|---|
| TX Threshold | `session.tx_count() >= MAX_TX_PER_BLOCK (2)` | Close batch |

**Batch Close Actions:**

1. Fetch `prev_root` from DB (`get_latest_state_root()`)
2. Compute `new_root` from executor state (`compute_state_root()`)
3. Apply state diff to DB (`apply_state_diff()`)
4. Create `ClosedSession` with `BlockHeader`
5. Persist block header (`store_block_header()`)
6. Replace session with new `Session(batch_id + 1)`

### 2.3 Deposit Indexing Triggers

| Current State | Trigger | Next State | Actions |
|---|---|---|---|
| Listening | Log matches `Program log: ZE_DEPOSIT:` | Processing | Parse deposit |
| Processing | Valid format `<Pubkey>:<Amount>:<Nonce>` | Applying | Credit account |
| Applying | DB update success | Listening | Log and continue |

### 2.4 Execution Error Transitions

| Error | Trigger | Response |
|---|---|---|
| InsufficientBalance | `balance < amount` | BAD_REQUEST |
| InvalidNonce | `account.nonce != tx.nonce` | BAD_REQUEST |
| AccountNotFound | Load account failure | BAD_REQUEST |
| Decryption failure | Invalid client key or ciphertext | BAD_REQUEST |
| Invalid chain_id | `tx.chain_id != CHAIN_ID (1)` | BAD_REQUEST |

## 3. Transaction Lifecycle States

```
+------------------+
| CLIENT CREATES   |
| SignedTransaction|
+------------------+
         |
         | Encrypt with sequencer pubkey
         v
+------------------+
| ENCRYPTED_BLOB   |
| (EncryptedTxBlobV1)
+------------------+
         |
         | POST /submit_tx
         v
+------------------+
| RECEIVED         |
| (Deserialized)   |
+------------------+
         |
         | tx_blob_hash()
         v
+------------------+
| HASHED           |
| (tx_hash: [u8;32])
+------------------+
         |
         | decrypt_signed_tx()
         v
+------------------+
| DECRYPTED        |
| (SignedTransaction)
+------------------+
         |
         | Validate chain_id
         v
+------------------+
| VALIDATED        |
+------------------+
         |
         | execute_signed_tx()
         v
+------------------+       +------------------+
| EXECUTED         |  or   | REJECTED         |
| (ExecutionResult)|       | (Error returned) |
+------------------+       +------------------+
         |
         | session.push_execution()
         v
+------------------+
| BATCHED          |
| (in Session.txs) |
+------------------+
         |
         | add_encrypted_tx()
         v
+------------------+
| PERSISTED_BLOB   |
| (CF_TX_BLOBS)    |
+------------------+
         |
         | Batch closes
         v
+------------------+
| COMMITTED        |
| (in BlockHeader) |
+------------------+
```

## 4. Batch/Block Lifecycle States

### Key Concept: Batch vs Block

In Zelana, **a Block is the finalized form of a Batch**. They have a 1:1 relationship:

| Aspect | Batch | Block |
|--------|-------|-------|
| **When** | During processing (Accumulating → Settling) | After finalization (Finalized state) |
| **Contains** | Full tx data, diffs, proofs | Compact 96-byte header |
| **Storage** | `CF_BATCHES` (BatchSummary JSON) | `CF_BLOCKS` (BlockHeader binary) |
| **ID** | `batch_id: u64` | Same `batch_id` |

The `batch_id` serves as the block number. When a batch reaches `Finalized` state, a `BlockHeader` is created and stored.

### BlockHeader Structure

```rust
pub struct BlockHeader {
    pub magic: [u8; 4],       // "ZLNA" - identifies Zelana blocks
    pub hdr_version: u16,     // Currently 1  
    pub batch_id: u64,        // Same ID as the batch (block number)
    pub prev_root: [u8; 32],  // State root before batch execution
    pub new_root: [u8; 32],   // State root after batch execution
    pub tx_count: u32,        // Number of transactions in this block
    pub open_at: u64,         // Timestamp when batch was opened
    pub flags: u32,           // Reserved for future use
}
```

### State Diagram

```
+------------------+
| GENESIS          |
| batch_id: 0      |
| prev_root: [0;32]|
| new_root: [0;32] |
+------------------+
         |
         v
+------------------+
| BATCH N OPEN     |
| Session created  |
+------------------+
         |
         | Collect MAX_TX_PER_BLOCK txs
         v
+------------------+
| BATCH N FULL     |
+------------------+
         |
         | Compute roots, apply diff
         v
+------------------+
| BATCH N CLOSING  |
+------------------+
         |
         | Create BlockHeader:
         |   magic: "ZLNA"
         |   hdr_version: 1
         |   batch_id: N
         |   prev_root: previous state root
         |   new_root: current state root
         |   tx_count: number of txs
         |   open_at: timestamp
         |   flags: 0
         v
+------------------+
| BATCH N COMMITTED|
| (Persisted in    |
|  CF_BLOCKS)      |
+------------------+
         |
         v
+------------------+
| BATCH N+1 OPEN   |
+------------------+
```

## 5. Deposits (L1 -> L2)

### Flow

```
L1 (Solana Bridge Program)
         |
         | Emit log: "Program log: ZE_DEPOSIT:<Pubkey>:<Amount>:<Nonce>"
         v
+------------------------+
| INDEXER LISTENING      |
| (PubsubClient.logs_    |
|  subscribe)            |
+------------------------+
         |
         | Log matches prefix
         v
+------------------------+
| PARSE_DEPOSIT_LOG      |
| - Extract pubkey       |
| - Extract amount       |
| - Extract nonce        |
+------------------------+
         |
         | map_l1_to_l2(Pubkey) -> AccountId
         v
+------------------------+
| PROCESS_DEPOSIT        |
| - Load account state   |
| - balance += amount    |
| - Persist to DB        |
+------------------------+
         |
         v
+------------------------+
| DEPOSIT CREDITED       |
+------------------------+
```

### Key Functions

- `start_indexer()`: Subscribes to Solana logs
- `parse_deposit_log()`: Parses `ZE_DEPOSIT:<Pubkey>:<Amount>:<Nonce>`
- `process_deposit()`: Credits account balance
- `map_l1_to_l2()`: Simple byte mapping (L1 Pubkey -> L2 AccountId)

### Deposit Event Structure

```rust
pub struct DepositEvent {
    pub to: AccountId,
    pub amount: u64,
    pub l1_seq: u64,  // L1 nonce
}
```

## 6. Withdrawals (L2 -> L1)

### L2 Side (BatchExecutor)

```
+------------------------+
| USER SUBMITS           |
| WithdrawRequest        |
+------------------------+
         |
         | BatchExecutor.execute_withdraw()
         v
+------------------------+
| VALIDATE NONCE         |
| sender.nonce == req.nonce
+------------------------+
         |
         | Nonce mismatch?
         v (error)
+------------------------+
| VALIDATE BALANCE       |
| sender.balance >= amount
+------------------------+
         |
         | Insufficient?
         v (error)
+------------------------+
| BURN L2 FUNDS          |
| - balance -= amount    |
| - nonce += 1           |
| - Persist state        |
+------------------------+
         |
         v
+------------------------+
| L2 WITHDRAWAL COMPLETE |
| (Funds burned on L2)   |
+------------------------+
```

### L1 Side (Bridge Program)

```
+------------------------+
| SEQUENCER SUBMITS      |
| WithdrawAttestedParams |
| - nullifier            |
| - amount               |
+------------------------+
         |
         | process_withdraw_attested()
         v
+------------------------+
| VERIFY SEQUENCER       |
| signer == config.      |
|   sequencer_authority  |
+------------------------+
         |
         v
+------------------------+
| VERIFY NULLIFIER FRESH |
| nullifier_account empty
+------------------------+
         |
         | Already used?
         v (error: replay)
+------------------------+
| CREATE NULLIFIER PDA   |
| ("nullifier", domain,  |
|  nullifier)            |
+------------------------+
         |
         v
+------------------------+
| TRANSFER SOL           |
| vault -= amount        |
| recipient += amount    |
+------------------------+
         |
         v
+------------------------+
| L1 WITHDRAWAL COMPLETE |
+------------------------+
```

### Withdrawal Request Structure

```rust
pub struct WithdrawRequest {
    pub from: AccountId,
    pub to_l1_address: [u8; 32],
    pub amount: u64,
    pub nonce: u64,
    pub signature: Vec<u8>,
    pub signer_pubkey: [u8; 32],
}
```

## 7. Session Management States

### SessionKeys Lifecycle

```
+------------------------+
| EPHEMERAL KEY GEN      |
| Client generates X25519|
+------------------------+
         |
         | DH: shared_secret = client_sk * server_pk
         v
+------------------------+
| KEY DERIVATION         |
| HKDF(secret, salt)     |
| -> 32-byte key         |
| -> 12-byte IV          |
+------------------------+
         |
         v
+------------------------+
| SESSION ACTIVE         |
| - aead: ChaCha20Poly1305
| - base_iv: [u8; 12]    |
| - tx_counter: 0        |
| - rx_counter: 0        |
+------------------------+
         |
         | encrypt() -> tx_counter++
         | decrypt() -> verify nonce
         v
+------------------------+
| COUNTER INCREMENTED    |
+------------------------+
         |
         | Timeout check via retain()
         v
+------------------------+
| SESSION EXPIRED        |
| (removed from manager) |
+------------------------+
```

### ActiveSession Structure

```rust
pub struct ActiveSession {
    pub keys: SessionKeys,
    pub account_id: Option<AccountId>,  // Set after first valid signature
    pub last_activity: std::time::Instant,
}
```

### SessionManager Operations

| Operation | Description |
|---|---|
| `insert(addr, keys)` | Create new session for socket address |
| `get_mut(addr, f)` | Access session mutably |
| `remove(addr)` | Terminate session |
| `retain(predicate)` | Cleanup expired sessions |

## 8. Gaps and Incomplete Flows

### 8.1 Missing Components

- **No Mempool:** Transactions are executed immediately upon receipt. There is no queuing or prioritization.
- **No Withdrawal Initiation from Core:** The `BatchExecutor` has withdrawal logic, but the main `Executor` in `ingest.rs` only handles transfers. Withdrawals are not triggered from the HTTP endpoint.
- **No Batch Submission to L1:** After closing a batch, the sequencer persists the `BlockHeader` locally but does not submit proofs or state roots to L1.
- **No Prover Integration:** Comment in `main.rs`: "Zelana sequencer started (HTTP mode, no prover)". No ZK proof generation.
- **No Shielded Transaction Execution:** The `TransactionType::Shielded` path exists in `BatchExecutor` but is marked as TODO and not integrated with the main flow.
- **No Deposit Deduplication:** The indexer has `l1_seq` (nonce) in `DepositEvent` but does not check for duplicate deposits before crediting.
- **Incomplete Session Manager Usage:** `SessionManager` is created in `main.rs` but never used. The HTTP server does not use it for session tracking.
- **No State Root Validation on Restart:** After restart, the latest state root is fetched but there's no validation that the DB state matches.

### 8.2 Error Handling Gaps

- **Executor Not Reset After Batch:** The `reset()` method exists but is never called. The state cache persists across batches, which may be intentional for caching but could cause issues.
- **No Rollback on Partial Failure:** If `apply_state_diff()` fails after some state is written, there's no recovery mechanism.
- **Indexer Crash Recovery:** If the WebSocket connection drops, the indexer just returns instead of reconnecting.

### 8.3 State Machine Gaps

- **No "Pending" State for Transactions:** Transactions go directly from RECEIVED to EXECUTED/REJECTED with no intermediate pending state.
- **No Batch Time-Based Closure:** Batches only close when `MAX_TX_PER_BLOCK` (2) is reached. Low-volume periods could leave transactions uncommitted indefinitely.
- **No Explicit Executor State Machine:** The executor doesn't track its own lifecycle state (e.g., LOADING, EXECUTING, COMMITTING).

### Summary Table

| Component | States | Primary Triggers |
|---|---|---|
| Sequencer Process | INITIALIZING, RUNNING, SHUTDOWN | Startup, Ctrl+C |
| Session (Batch) | CREATED, COLLECTING, CLOSING, COMMITTED | TX count threshold |
| Executor | IDLE, EXECUTING, DIRTY_CACHE, PERSISTED | TX submission, batch close |
| Transaction | RECEIVED, DECRYPTED, VALIDATED, EXECUTED/REJECTED, BATCHED, COMMITTED | HTTP POST, validation, execution |
| Block/Batch | GENESIS, OPEN, FULL, COMMITTED | TX accumulation |
| Deposit (L1->L2) | LISTENING, PARSING, PROCESSING, CREDITED | Solana log events |
| Withdrawal (L2->L1) | REQUEST, VALIDATE, BURN, COMPLETE | User request, sequencer attestation |
| Network Session | NO_SESSION, ACTIVE, AUTHENTICATED, EXPIRED | Handshake, signature, timeout |

