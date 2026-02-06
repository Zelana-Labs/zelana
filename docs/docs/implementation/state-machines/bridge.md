# Bridge Program State Machine Analysis

## Program Overview

**Program ID:** `8SE6gCijcFQixvDQqWu29mCm9AydN8hcwWh2e2Q6RQgE`

The bridge program is a Solana-to-L2 bridge that supports:

- SOL deposits from users to an L2 domain
- Sequencer-attested withdrawals from L2 back to Solana
- Batch state root submissions with Groth16 proof verification via the verifier program

## 1. Account States

### 1.1 Config Account

**PDA Seeds:** `["config", domain]`

| Field | Type | Description |
|-------|------|-------------|
| `sequencer_authority` | `Pubkey` | The authorized sequencer that can submit batches and process withdrawals |
| `domain` | `[u8; 32]` | Unique identifier for this bridge domain |
| `state_root` | `[u8; 32]` | Current Merkle root of L2 state (starts at 0 or genesis) |
| `batch_index` | `u64` | Index of the last processed batch |
| `bump` | `u8` | PDA bump seed |
| `is_initialized` | `u8` | Initialization flag (1 = initialized) |
| `_padding` | `[u8; 6]` | Alignment padding |

**State Diagram:**
```
 __________________________
| NonExistent               |
|__________________________|
            |
            | Initialize
            v
 __________________________
| Initialized               |
| state_root = [0; 32]      |
| batch_index = 0           |
|__________________________|
            |
            | SubmitBatch (verified)
            v
 __________________________
| Updated                   |
| state_root, batch_index   |
|__________________________|
```

**States:**
- **NonExistent:** Account does not exist (lamports = 0, data empty)
- **Initialized:** `is_initialized == 1`, `sequencer_authority != 0`, `domain != 0`

### 1.2 Vault Account

**PDA Seeds:** `["vault", domain]`

| Field | Type | Description |
|-------|------|-------------|
| `domain` | `[u8; 32]` | Domain this vault belongs to |
| `bump` | `u8` | PDA bump seed |
| `_padding` | `[u8; 7]` | Alignment padding |

**State Diagram:**
```
 __________________________
| NonExistent               |
|__________________________|
            |
            | Initialize
            v
 __________________________
| Initialized               |
| (Vault PDA)               |
|__________________________|
            |
            | Deposit: +lamports
            v
 __________________________
| Active                    |
| (lamports increase)       |
|__________________________|
            |
            | WithdrawAttested: -lamports
            v
 __________________________
| Active                    |
| (lamports decrease)       |
|__________________________|
```

**States:**
- **NonExistent:** Account does not exist
- **Initialized:** `bump != 0` (PDA derivation guarantees non-zero bump)

> **Note:** The vault holds SOL (lamports). It is a lamport sink/source, not a complex state machine.

### 1.3 DepositReceipt Account

**PDA Seeds:** `["receipt", domain, depositor, nonce_le_bytes]`

| Field | Type | Description |
|-------|------|-------------|
| `depositor` | `Pubkey` | The user who made the deposit |
| `domain` | `Pubkey` | The bridge domain |
| `amount` | `u64` | Amount of SOL deposited (in lamports) |
| `nonce` | `u64` | Unique nonce for this deposit |
| `ts` | `i64` | Unix timestamp of the deposit |
| `bump` | `u8` | PDA bump seed |
| `is_initialized` | `u8` | Initialization flag |
| `_padding` | `[u8; 6]` | Alignment padding |

**State Diagram:**
```
 __________________________
| NonExistent               |
|__________________________|
            |
            | Deposit
            v
 __________________________
| Initialized/Finalized     |
| (write-once)              |
|__________________________|
```

**States:**
- **NonExistent:** Account does not exist (data empty)
- **Initialized:** `is_initialized == 1`, receipt is permanent proof of deposit

> **Note:** DepositReceipts are write-once, read-many. Once created, they cannot be modified or deleted by the program.

### 1.4 UsedNullifier Account

**PDA Seeds:** `["nullifier", domain, nullifier]`

| Field | Type | Description |
|-------|------|-------------|
| `domain` | `[u8; 32]` | The bridge domain |
| `nullifier` | `[u8; 32]` | Unique nullifier hash (prevents replay) |
| `recipient` | `Pubkey` | The recipient of the withdrawal |
| `amount` | `u64` | Amount withdrawn |
| `used` | `u8` | Used flag (1 = used) |
| `bump` | `u8` | PDA bump seed |
| `_padding` | `[u8; 6]` | Alignment padding |

**State Diagram:**
```
 __________________________
| NonExistent               |
|__________________________|
            |
            | WithdrawAttested
            v
 __________________________
| Used                      |
| (replay protected)        |
|__________________________|
```

**States:**
- **NonExistent:** Account does not exist - withdrawal can proceed
- **Used:** `used == 1` - withdrawal has been processed, replay prevented

## 2. Instructions and State Transitions

### 2.1 Initialize (Discriminator: 0)

**Accounts:**

| Index | Name | Writable | Signer | Description |
|-------|------|----------|--------|-------------|
| 0 | `payer` | Yes | Yes | Fee payer |
| 1 | `config` | Yes | No | Config PDA to create |
| 2 | `vault` | Yes | No | Vault PDA to create |
| 3 | `system_program` | No | No | System program |

**Params:**
```rust
pub struct InitParams {
    pub sequencer_authority: Pubkey,
    pub domain: [u8; 32],
}
```

**Pre-conditions (Guards):**
- `payer` must be a signer
- `domain != [0u8; 32]`
- `sequencer_authority != Pubkey::default()`
- `config_account.key() == derive_config_pda(program_id, domain)`
- `vault_account.key() == derive_vault_pda(program_id, domain)`
- `config_account.lamports() == 0` (not already funded)
- `config_account.data_is_empty()` (not already initialized)
- `vault_account.data_is_empty()` (not already initialized)

**State Transitions:**

**Config:** `NonExistent -> Initialized`
- `sequencer_authority = params.sequencer_authority`
- `domain = params.domain`
- `state_root = [0u8; 32]`
- `batch_index = 0`
- `is_initialized = 1`

**Vault:** `NonExistent -> Initialized`
- `domain = params.domain`
- `bump = vault_bump`

**Post-conditions:**
- Config and Vault accounts are created and owned by the bridge program
- `Config.is_initialized == 1`
- `Vault.bump != 0`

**Error Conditions:**

| Error | Condition |
|-------|-----------|
| `NotEnoughAccountKeys` | Less than 4 accounts provided |
| `MissingRequiredSignature` | Payer is not a signer |
| `InvalidInstructionData` | Domain is all zeros |
| `InvalidArgument` | Sequencer authority is default pubkey |
| `InvalidSeeds` | Config or Vault PDA doesn't match expected |
| `AccountAlreadyInitialized` | Config/Vault already has lamports or data |

### 2.2 Deposit (Discriminator: 1)

**Accounts:**

| Index | Name | Writable | Signer | Description |
|-------|------|----------|--------|-------------|
| 0 | `depositor` | Yes | Yes | User depositing SOL |
| 1 | `config` | No | No | Bridge config (read-only for domain) |
| 2 | `vault` | Yes | No | Bridge vault receiving SOL |
| 3 | `deposit_receipt` | Yes | No | Receipt PDA to create |
| 4 | `system_program` | No | No | System program |

**Params:**
```rust
pub struct DepositParams {
    pub amount: u64,
    pub nonce: u64,
}
```

**Pre-conditions (Guards):**
- `depositor` must be a signer
- `amount > 0`
- `config.is_initialized == 1`
- `vault_account.key() == derive_vault_pda(program_id, config.domain)`
- `receipt_account.key() == derive_deposit_receipt_pda(program_id, domain, depositor, nonce)`
- `receipt_account.data_is_empty()` (nonce not already used)

**State Transitions:**

**Vault:** `lamports += amount`

**DepositReceipt:** `NonExistent -> Initialized`
- `depositor = depositor.key()`
- `domain = config.domain`
- `amount = params.amount`
- `nonce = params.nonce`
- `ts = clock.unix_timestamp`
- `is_initialized = 1`

**Post-conditions:**
- SOL transferred from depositor to vault
- DepositReceipt created with deposit details
- Log emitted: `ZE_DEPOSIT:{depositor}:{amount}:{nonce}`

**Error Conditions:**

| Error | Condition |
|-------|-----------|
| `NotEnoughAccountKeys` | Less than 5 accounts provided |
| `MissingRequiredSignature` | Depositor is not a signer |
| `InvalidInstructionData` | Amount is 0 |
| `UninitializedAccount` | Config not initialized |
| `InvalidSeeds` | Vault or Receipt PDA mismatch |
| `AccountAlreadyInitialized` | Receipt already exists (nonce reuse) |
| `InvalidArgument` | Depositor is default pubkey, domain is zeros |

### 2.3 WithdrawAttested (Discriminator: 2)

**Accounts:**

| Index | Name | Writable | Signer | Description |
|-------|------|----------|--------|-------------|
| 0 | `sequencer` | Yes | Yes | Authorized sequencer |
| 1 | `config` | No | No | Bridge config |
| 2 | `vault` | Yes | No | Bridge vault (source of funds) |
| 3 | `recipient` | Yes | No | Account receiving withdrawn SOL |
| 4 | `used_nullifier` | Yes | No | Nullifier PDA to create |
| 5 | `system_program` | No | No | System program |

**Params:**
```rust
pub struct WithdrawAttestedParams {
    pub recipient: Pubkey,
    pub amount: u64,
    pub nullifier: [u8; 32],
}
```

**Pre-conditions (Guards):**
- `sequencer` must be a signer
- `config.is_initialized == 1`
- `sequencer.key() == config.sequencer_authority` (authorization)
- `amount > 0`
- `vault_account.key() == derive_vault_pda(program_id, config.domain)`
- `nullifier_account.key() == derive_nullifier_pda(program_id, domain, nullifier)`
- `nullifier_account.data_is_empty()` (not already used - replay protection)

> **Implementation detail:** The recipient account (account index 3) is used
> directly for the SOL transfer. The `recipient` field inside
> `WithdrawAttestedParams` is not validated or used.

**State Transitions:**

**Vault:** `lamports -= amount`

**Recipient:** `lamports += amount`

**UsedNullifier:** `NonExistent -> Used`
- `domain = config.domain`
- `nullifier = params.nullifier`
- `recipient = recipient.key()`
- `amount = params.amount`
- `used = 1`

**Post-conditions:**
- SOL transferred from vault to recipient
- Nullifier account created (prevents replay)
- Logs emitted: `withdraw:{amount}`, `ts:{timestamp}`

**Error Conditions:**

| Error | Condition |
|-------|-----------|
| `NotEnoughAccountKeys` | Less than 6 accounts provided |
| `MissingRequiredSignature` | Sequencer is not a signer |
| `UninitializedAccount` | Config not initialized |
| `IncorrectAuthority` | Sequencer is not authorized |
| `InvalidInstructionData` | Amount is 0, or nullifier already used (replay) |
| `InvalidSeeds` | Vault or Nullifier PDA mismatch |
| `InvalidArgument` | Domain or nullifier is all zeros |

### 2.4 SubmitBatch (Discriminator: 3)

**Accounts:**

| Index | Name | Writable | Signer | Description |
|-------|------|----------|--------|-------------|
| 0 | `sequencer` | No | Yes | Authorized sequencer |
| 1 | `config` | Yes | No | Bridge config to update |
| 2 | `verifier_program` | No | No | Verifier program for Groth16 proof checks |
| 3 | `vk_account` | No | No | Verifying key account used by the verifier |
| 4+ | `recipients` | No | No | Recipient accounts for withdrawal intents |

**Params (Header):**
```rust
pub struct SubmitBatchHeader {
    pub prev_batch_index: u64,
    pub new_batch_index: u64,
    pub new_state_root: [u8; 32],
    pub proof_len: u32,
    pub withdrawal_count: u32,
}
```

> **Implementation detail:** The current on-chain handler skips one extra byte
> before parsing the header. Clients must include a one-byte padding value after
> the discriminator so the header parses correctly.

**Variable-length data after header:**
- `proof: [u8; 256]` - Groth16 proof bytes (`proof_len` must equal 256)
- `public_inputs: [BatchPublicInputs]` - 200-byte public input struct
- `withdrawals: [WithdrawalRequest; withdrawal_count]` - Withdrawal intents

```rust
pub struct BatchPublicInputs {
    pub pre_state_root: [u8; 32],
    pub post_state_root: [u8; 32],
    pub pre_shielded_root: [u8; 32],
    pub post_shielded_root: [u8; 32],
    pub withdrawal_root: [u8; 32],
    pub batch_hash: [u8; 32],
    pub batch_id: u64,
}
```

```rust
pub struct WithdrawalRequest {
    pub recipient: Pubkey,
    pub amount: u64,
}
```

**Pre-conditions (Guards):**
- At least 4 accounts provided
- `sequencer` must be a signer
- `config.is_initialized == 1`
- `sequencer.key() == config.sequencer_authority`
- `header.prev_batch_index == config.batch_index` (sequential)
- `header.new_batch_index == config.batch_index + 1` (increment by 1)
- `header.proof_len == 256`
- Public inputs `post_state_root == header.new_state_root`
- Public inputs `batch_id == header.new_batch_index`
- `accounts[4..].len() == header.withdrawal_count` (account count matches)
- For each withdrawal: `recipient_account.key() == withdrawal.recipient`
- Instruction data is properly formatted

**State Transitions:**

**Verifier CPI:**
- Calls verifier program with Groth16 proof + public inputs
- Fails the instruction if the proof is invalid

**Config (after successful verification):**
- `state_root = header.new_state_root`
- `batch_index = header.new_batch_index`

> **Note:** No withdrawals are executed - only logged as intents

**Post-conditions:**
- `Config.state_root` updated to new merkle root
- `Config.batch_index` incremented
- For each withdrawal: Log `ZE_WITHDRAW_INTENT:{recipient}:{amount}`
- Final log: `ZE_BATCH_FINALIZED:{domain}:{batch_index}`
- Groth16 proof verified via verifier CPI

**Error Conditions:**

| Error | Condition |
|-------|-----------|
| `NotEnoughAccountKeys` | Less than 4 accounts |
| `MissingRequiredSignature` | Sequencer not a signer |
| `UninitializedAccount` | Config not initialized |
| `IncorrectAuthority` | Sequencer not authorized |
| `InvalidInstructionData` | Data too short, bad prev/new batch index, invalid proof length, or public input mismatch |
| `InvalidAccountData` | Recipient count mismatch or recipient key mismatch |
| *(Verifier CPI error)* | Verifier program rejects the proof or inputs |

## 3. Complete Flow Diagrams

### 3.1 Deposit Flow

```
 __________________________
| USER SUBMITS DEPOSIT      |
| Deposit(amount, nonce)    |
|__________________________|
            |
            v
 __________________________
| BRIDGE VALIDATES          |
| - User is signer          |
| - amount > 0              |
| - Config initialized      |
| - Vault PDA valid         |
| - Nonce not used          |
|__________________________|
            |
            v
 __________________________
| BRIDGE TRANSITION         |
| - Transfer SOL to Vault   |
| - Create DepositReceipt   |
| - Log ZE_DEPOSIT           |
|__________________________|
            |
            v
 __________________________
| SEQUENCER CREDITS USER    |
| (Indexer consumes log)    |
|__________________________|
```

### 3.2 Withdrawal Flow (Attested)

```
 __________________________
| SEQUENCER SUBMITS         |
| WithdrawAttested(...)     |
|__________________________|
            |
            v
 __________________________
| BRIDGE VALIDATES          |
| - Sequencer is signer     |
| - Sequencer == authority  |
| - amount > 0              |
| - Config initialized      |
| - Vault PDA valid         |
| - Nullifier not used      |
|__________________________|
            |
            v
 __________________________
| BRIDGE TRANSITION         |
| - Create UsedNullifier    |
| - Transfer SOL to user    |
|__________________________|
            |
            v
 __________________________
| USER RECEIVES SOL         |
|__________________________|
```

### 3.3 Batch Submission Flow

```
 __________________________
| SEQUENCER SUBMITS BATCH   |
| SubmitBatch(...)          |
|__________________________|
            |
            v
 __________________________
| BRIDGE VALIDATES          |
| - Sequencer is signer     |
| - Sequencer == authority  |
| - prev_batch == current   |
| - new_batch == current+1  |
| - Account count matches   |
| - Recipients match        |
|__________________________|
            |
            v
 __________________________
| VERIFY PROOF (CPI)        |
| - Groth16 proof + inputs  |
| - Verifier program        |
|__________________________|
            |
            v
 __________________________
| BRIDGE TRANSITION         |
| - Update state_root       |
| - Increment batch_index   |
| - Log withdrawal intents  |
| - Emit ZE_BATCH_FINALIZED |
|__________________________|
            |
            v
 __________________________
| LATER: WithdrawAttested   |
| for each logged intent    |
|__________________________|
```

## 4. State Transition Summary Table

| Account | From State | Instruction | To State | Reversible |
|---------|-----------|-------------|----------|-----------|
| Config | NonExistent | Initialize | Initialized | No |
| Config | Initialized | SubmitBatch | Initialized (updated) | No (append-only) |
| Vault | NonExistent | Initialize | Initialized | No |
| Vault | Initialized | Deposit | Initialized (+lamports) | No |
| Vault | Initialized | WithdrawAttested | Initialized (-lamports) | No |
| DepositReceipt | NonExistent | Deposit | Initialized | No |
| UsedNullifier | NonExistent | WithdrawAttested | Used | No |

## 5. Error States and Recovery

### 5.1 Error Categories

**Instruction-Level Errors:**

| Error | Code | Recovery |
|-------|------|----------|
| `NotEnoughAccountKeys` | - | Retry with correct accounts |
| `MissingRequiredSignature` | - | Retry with proper signer |
| `InvalidInstructionData` | - | Fix instruction data format |
| `InvalidSeeds` | - | Derive correct PDA addresses |
| `InvalidArgument` | - | Fix parameters (non-zero values) |

**State-Level Errors:**

| Error | Code | Recovery |
|-------|------|----------|
| `AccountAlreadyInitialized` | - | Cannot recover - account exists |
| `UninitializedAccount` | - | Initialize account first |
| `IncorrectAuthority` | - | Use authorized sequencer |

**Replay Protection:**

| Scenario | Prevention | Recovery |
|----------|-----------|----------|
| Double deposit (same nonce) | DepositReceipt PDA exists | Use different nonce |
| Double withdrawal | UsedNullifier PDA exists | Cannot recover - intended behavior |
| Batch replay | batch_index sequential check | Cannot skip or replay batches |

### 5.2 Invariants

- **Config Singleton:** Only one Config per domain can exist
- **Vault Singleton:** Only one Vault per domain can exist
- **Nonce Uniqueness:** Each `(domain, depositor, nonce)` tuple creates a unique DepositReceipt
- **Nullifier Uniqueness:** Each nullifier can only be used once per domain
- **Sequential Batches:** Batch index must increment by exactly 1
- **Authority Check:** Only the configured sequencer can submit batches or process withdrawals

### 5.3 Failure Scenarios

| Scenario | Symptom | Mitigation |
|----------|---------|-----------|
| Insufficient vault balance | `WithdrawAttested` fails | Ensure deposits > withdrawals |
| Sequencer key compromise | Unauthorized withdrawals | Migrate to new domain (no current upgrade path) |
| Missed batch | Cannot submit batch N+2 before N+1 | Submit batches in order |
| Orphaned deposit | Deposit made but L2 doesn't credit | Off-chain reconciliation needed |

## 6. Key Design Observations

- **Append-Only State:** All state accounts (Config, DepositReceipt, UsedNullifier) are append-only or immutable after creation. This provides strong auditability.

- **No Close/Reclaim:** There is no mechanism to close accounts or reclaim rent. DepositReceipts and UsedNullifiers are permanent.

- **Two-Phase Withdrawal:** SubmitBatch logs withdrawal intents (`ZE_WITHDRAW_INTENT`) but does not execute them. `WithdrawAttested` must be called separately with nullifiers.

- **ZK Verification Active:** SubmitBatch performs a CPI to the verifier program and fails if the Groth16 proof is invalid.

- **Single Sequencer:** The system has a single point of trust - the `sequencer_authority`. There is no multi-sig or upgrade mechanism visible in the code.

- **Domain Isolation:** Each domain has its own Config, Vault, and derived PDAs. Multiple independent bridges can coexist.

## Implementation Links (GitHub)

Use these links to jump directly to the implementation files:

- [onchain-programs/bridge/src/lib.rs](https://github.com/zelana-Labs/zelana/blob/main/onchain-programs/bridge/src/lib.rs)
- [onchain-programs/bridge/src/entrypoint.rs](https://github.com/zelana-Labs/zelana/blob/main/onchain-programs/bridge/src/entrypoint.rs)
- [onchain-programs/bridge/src/instruction/init.rs](https://github.com/zelana-Labs/zelana/blob/main/onchain-programs/bridge/src/instruction/init.rs)
- [onchain-programs/bridge/src/instruction/deposit.rs](https://github.com/zelana-Labs/zelana/blob/main/onchain-programs/bridge/src/instruction/deposit.rs)
- [onchain-programs/bridge/src/instruction/submit_batch.rs](https://github.com/zelana-Labs/zelana/blob/main/onchain-programs/bridge/src/instruction/submit_batch.rs)
- [onchain-programs/bridge/src/instruction/withdraw.rs](https://github.com/zelana-Labs/zelana/blob/main/onchain-programs/bridge/src/instruction/withdraw.rs)
- [onchain-programs/bridge/src/state/config.rs](https://github.com/zelana-Labs/zelana/blob/main/onchain-programs/bridge/src/state/config.rs)
- [onchain-programs/bridge/src/state/vault.rs](https://github.com/zelana-Labs/zelana/blob/main/onchain-programs/bridge/src/state/vault.rs)
- [onchain-programs/bridge/src/state/depositreceipt.rs](https://github.com/zelana-Labs/zelana/blob/main/onchain-programs/bridge/src/state/depositreceipt.rs)
- [onchain-programs/bridge/src/state/usernullifier.rs](https://github.com/zelana-Labs/zelana/blob/main/onchain-programs/bridge/src/state/usernullifier.rs)
- [onchain-programs/verifier/programs/onchain_verifier/src/lib.rs](https://github.com/zelana-Labs/zelana/blob/main/onchain-programs/verifier/programs/onchain_verifier/src/lib.rs)
- [core/src/sequencer/settlement/settler.rs](https://github.com/zelana-Labs/zelana/blob/main/core/src/sequencer/settlement/settler.rs)
- [core/src/sequencer/bridge/ingest.rs](https://github.com/zelana-Labs/zelana/blob/main/core/src/sequencer/bridge/ingest.rs)
