# Prover Implementation Analysis

## Files Analyzed

- Main prover service with lifecycle, polling, proof generation, and L1 settlement
- Module exports
- Main L2 block circuit implementation (AccountsFoldHashV1)
- Witness data structures
- Witness construction helpers
- Public/private input structures
- Constants (MAX_TXS=64, MERKLE_DEPTH=32)
- Circuit module organization
- Merkle-based L2 block circuit (alternative)
- Merkle tree verification/update gadgets
- Poseidon configuration for BLS12-381
- Hash2 gadget using Poseidon
- SnarkJS-compatible export utilities

## Prover Lifecycle States

The prover operates as a polling service with the following batch states tracked in SQLite:

| State | Description |
|-------|-------------|
| Pending | Batch awaiting proof generation |
| Settled-OnChain | Proof verified and settled on L1 (Solana) |
| SettlementFailed | L1 settlement transaction failed (with error message stored) |
| Settled-Mock | (Commented out) Simulated settlement for testing |

### State Machine Flow

```
Pending → [Proof Generation] → [L1 Settlement] → Settled-OnChain
                            ↓ (error)
                      SettlementFailed
```

## Polling for Pending Batches

**Polling Query:**

```sql
SELECT id FROM batches WHERE proof_status = 'Pending' ORDER BY id ASC LIMIT 1
```

**Constants:**
- `POLL_INTERVAL_SECONDS = 10` - Polling interval between batch checks

## Proof Generation Flow

### Pipeline

1. **Fetch Pending Batch** (SQLite)
   - Query for oldest pending batch ID

2. **Load Block Header** (RocksDB)
   - Read from batches column family using batch_id
   - Extract prev_root and new_root from BlockHeader

3. **Fetch Transactions**
   - Use tx_by_batch index CF to find tx signatures
   - Deserialize transactions from txs CF

4. **Build Witness Data**
   - Fetch current accounts from RocksDB (accounts CF)
   - Calculate pre-state by reversing transactions from post-state
   - Convert transactions to TransactionWitness (sender_pk, recipient_pk, amount)

5. **Calculate Expected Root**
   - Use off-chain Poseidon hash (AccountsFoldHashV1 scheme)
   - Verify calculated root matches header's new_root

6. **Generate Groth16 Proof**

```rust
let circuit = L2BlockCircuit {
   prev_root: Some(header.prev_root),
   new_root: Some(calculated_new_root_bytes),
   transactions: Some(transactions_witness),
   initial_accounts: Some(initial_accounts_witness),
   batch_id: Some(batch_id),
   poseidon_config: poseidon_config.clone(),
};
let proof = Groth16::<Bn254>::prove(&pk, circuit, &mut rng)?;
```

7. **Export Proof** - Saves as `batch_{id}_proof.json` (Base64 compressed)

## Circuit Structure

### Two Circuit Implementations Exist

#### A. Main Circuit: L2BlockCircuit

Uses AccountsFoldHashV1 - a sequential fold-based commitment scheme.

**Public Inputs:**
- `prev_root: [u8; 32]` - Previous state root (as Fr element)
- `new_root: [u8; 32]` - Expected new state root (as Fr element)

**Private Witness:**
- `transactions: Vec<TransactionWitness>` - List of transfers
- `initial_accounts: BTreeMap<PubkeyBytes, u64>` - Pre-batch account balances
- `batch_id: u64` - Batch identifier
- `poseidon_config: PoseidonConfig<Fr>` - Poseidon parameters

**Constraints Generated:**

- **Account Allocation** - Each account's balance as witness
- **Transaction Application** - For each transfer:
  - Enforce sender_balance >= amount
  - Update sender_balance -= amount
  - Update recipient_balance += amount
- **State Root Computation** (Poseidon-based fold):
  ```
  S0 = Poseidon(domain_separator, batch_id)
  for each (pk, balance) in sorted_accounts:
     leaf = Poseidon(pk, balance)
     S_{i+1} = Poseidon(S_i, leaf)
  computed_root = Poseidon(S_last, account_count)
  ```
- **Final Equality** - Enforce computed_root == expected_new_root

#### B. Alternative Circuit

Uses Merkle Tree structure (BLS12-381 curve).

**Private Witness:**

- WitnessTx with:
  - `enabled: bool` - Real vs padding transaction
  - `tx_type: u8` - Transaction type
  - `sender: AccountWitness` (pubkey, balance, nonce, merkle_path)
  - `receiver: Option<AccountWitness>`
  - amount, nonce, tx_hash

**Constraints:**

- **Merkle Inclusion** - Verify sender in current root
- **Nonce Check** - Sender nonce matches transaction nonce
- **Transaction Hash** - tx_hash == hash2(pubkey, nonce + amount + tx_type)
- **Balance Update** - new_balance = balance - amount
- **Merkle Update** - Compute new root after updating sender leaf

## L1 Settlement (Solana)

### Settlement Flow

1. **Setup Client**
   - Connect to Solana Devnet
   - Load payer keypair from SETTLER_KEYPAIR_PATH

2. **Convert Proof to Solana Format**
   - pi_a (G1) - 64 bytes LE, negated
   - pi_b (G2) - 128 bytes LE
   - pi_c (G1) - 64 bytes LE

3. **Prepare Public Inputs**
   - Convert prev_root and new_root to Fr elements (LE bytes)

4. **Build Instruction Data**
   ```
   [discriminator (1)] + [pi_a (64)] + [pi_b (128)] + [pi_c (64)] + 
   [num_inputs (1)] + [pub_input_0 (32)] + [pub_input_1 (32)]
   ```

5. **Derive PDA**
   ```
   seeds = ["groth16_proof", payer.pubkey(), batch_id.to_string()]
   ```

6. **Send Transaction**
   - Target program: `6qPEb6x1oGhd2pf1UP3bgMWa7NspSNryzrA6ZCdsbFwT`
   - Accounts: payer (signer), proof_account_pda, system_program

## Error Handling and Retry Logic

### Simple Retry on Error

On any error during batch processing, log and wait `POLL_INTERVAL_SECONDS` (10s). Retry indefinitely (no backoff, no max retries).

### Settlement Failure Handling

```rust
Err(e) => {
   sqlx::query("UPDATE batches SET proof_status = 'SettlementFailed', error_message = ? WHERE id = ?")
      .bind(format!("L1 settlement error: {:?}", e))
      .bind(batch_id as i64)
      .execute(sqlite_pool)
      .await?;
}
```

On L1 settlement failure: mark batch as SettlementFailed with error message. This prevents infinite retry loops for permanently failing batches.

### Fatal Mismatch Handling

If off-chain calculated root differs from header root, bail immediately with detailed error.

## Current Limitations and TODOs

| Limitation | Description |
|-----------|-------------|
| DEMO ONLY pre-state calculation | calculate_pre_state reverses transactions from post-state - "NOT cryptographically sound for a real prover" |
| Incomplete merkle module | References MerklePathWitness which doesn't exist in the prover crate |
| Incomplete executor module | References ExecutionTrace which doesn't exist |
| No signature verification in circuit | Transactions are trusted without signature checks |
| Single-threaded polling | Sequential batch processing, no parallel proving |
| No exponential backoff | Fixed 10s retry interval on errors |
| Hardcoded Solana Devnet | Settlement only targets devnet |
| Dual curve confusion | Main circuit uses BN254, circuit module uses BLS12-381 |
| No nonce tracking in main circuit | Comment: "NonceVar would be added here later" |
| Key path typo | SETTLER_KEYPAIR_PATH = "home/..." missing leading / |
| Unused simulation code | Commented out mock settlement code |

## Summary Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                        Prover Service                          │
│                                                                 │
│  ┌─────────┐    ┌─────────────┐    ┌──────────────┐            │
│  │ SQLite  │◄───│   Polling   │───►│   RocksDB    │            │
│  │ batches │    │   Loop      │    │ txs/accounts │            │
│  └────┬────┘    └──────┬──────┘    └──────────────┘            │
│       │                │                                        │
│       │         ┌──────▼──────┐                                │
│       │         │  Witness    │                                │
│       │         │  Builder    │                                │
│       │         └──────┬──────┘                                │
│       │                │                                        │
│       │         ┌──────▼──────┐                                │
│       │         │ L2Block     │   Public: prev_root, new_root  │
│       │         │ Circuit     │   Private: txs, accounts,      │
│       │         │ (Groth16)   │            batch_id            │
│       │         └──────┬──────┘                                │
│       │                │                                        │
│       │         ┌──────▼──────┐                                │
│       │         │   Proof     │                                │
│       │         │ Generation  │                                │
│       │         └──────┬──────┘                                │
│       │                │                                        │
│       │         ┌──────▼──────┐                                │
│       │         │    L1       │                                │
│       │         │ Settlement  │───► Solana Devnet              │
│       │         └──────┬──────┘                                │
│       │                │                                        │
│       ▼                ▼                                        │
│   Settled-OnChain / SettlementFailed                           │
└─────────────────────────────────────────────────────────────────┘
```
