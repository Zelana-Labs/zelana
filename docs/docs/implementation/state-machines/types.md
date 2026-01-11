# Zelana Transaction SDK Analysis

## Overview

The Zelana transaction SDK implements a hybrid L2 (Layer 2) system on Solana that supports both transparent and shielded (private) transactions. The architecture includes:

- Transaction creation and signing (client-side)
- Encrypted transmission (client to sequencer)
- Execution and state management (sequencer-side)
- Bridge operations (L1 deposits/withdrawals with nullifier protection)

## 1. All Transaction Types and Structures

### Transaction Type Enum

```rust
pub enum TransactionType {
    /// Shielded transaction with ZK proof (sender/receiver hidden)
    Shielded(PrivateTransaction),
    
    /// Standard transparent transfer
    Transfer(SignedTransaction),
    
    /// Deposit from L1 (Solana) to L2
    Deposit(DepositEvent),
    
    /// Withdrawal from L2 back to L1
    Withdraw(WithdrawRequest),
}
```

### Core Transaction Structures

| Structure | Purpose |
|-----------|---------|
| TransactionData | Payload that gets signed |
| SignedTransaction | Authenticated wrapper with Ed25519 signature |
| PrivateTransaction | ZK-shielded transaction blob |
| Transaction | Top-level wrapper with sender + signature |
| DepositEvent | L1 deposit bridged to L2 |
| WithdrawRequest | L2 withdrawal to L1 |

### TransactionData Structure

```rust
pub struct TransactionData {
    pub from: AccountId,      // 32-byte sender ID
    pub to: AccountId,        // 32-byte recipient ID
    pub amount: u64,          // Transfer amount
    pub nonce: u64,           // Replay protection
    pub chain_id: u64,        // Network identifier (1=Mainnet, 2=Devnet)
}
```

### SignedTransaction Structure

```rust
pub struct SignedTransaction {
    pub data: TransactionData,
    pub signature: Vec<u8>,        // Ed25519 signature (64 bytes)
    pub signer_pubkey: [u8; 32],   // Raw public key
}
```

### PrivateTransaction (Shielded) Structure

```rust
pub struct PrivateTransaction {
    pub proof: Vec<u8>,            // Groth16 ZK proof bytes
    pub nullifier: [u8; 32],       // Unique tag preventing double-spends
    pub commitment: [u8; 32],      // New note (encrypted hash)
    pub ciphertext: Vec<u8>,       // Encrypted data for recipient
    pub ephemeral_key: [u8; 32],   // ECDH key for shared secret
}
```

## 2. Transaction Serialization

### Serialization Format: Wincode

The SDK uses wincode for binary serialization (compact binary format with schema support).

Key serialization points:

| Operation | Description |
|-----------|-------------|
| Transaction signing | `wincode::serialize(&data)` |
| Encrypted blob | `wincode::serialize(signed_tx)` |
| Blob hashing | serialize(blob) then SHA256 |
| Block headers | Custom BigEndian binary format |

### EncryptedTxBlobV1 Structure

```rust
pub struct EncryptedTxBlobV1 {
    pub version: u8,           // Always 1 for V1
    pub flags: u8,             // Metadata flags
    pub sender_hint: [u8; 32], // H(signer_pubkey) - for recipient lookup
    pub nonce: [u8; 12],       // Random AEAD nonce
    pub ciphertext: Vec<u8>,   // Encrypted SignedTransaction
    pub tag: [u8; 16],         // Poly1305 authentication tag
}
```

## 3. Transaction Signing

### Signing Flow

1. Serialize TransactionData using wincode
2. Sign serialized bytes with Ed25519 (ed25519_dalek)
3. Return SignedTransaction with:
   - Original TransactionData
   - 64-byte Ed25519 signature
   - 32-byte signer public key

### Key Types

| Type | Algorithm | Purpose |
|------|-----------|---------|
| signing_key | Ed25519 | Transaction authentication |
| privacy_key | X25519 | Encryption (ECDH key exchange) |

### Keypair Structure

```rust
pub struct Keypair {
    signing_key: SigningKey,    // Ed25519 for signatures
    privacy_key: StaticSecret,  // X25519 for encryption
}
```

### AccountId Derivation

```rust
// AccountId is derived from the Ed25519 public key only
pub fn account_id(&self) -> AccountId {
    AccountId(self.signing_key.verifying_key().to_bytes())
}
```

## 4. Bridge Parameters for Deposits/Withdrawals

### Deposit Flow

```rust
// SDK Side
pub struct DepositParams {
    pub amount: u64,
    pub nonce: u64,
}

// Event emitted on L1, indexed by L2
pub struct DepositEvent {
    pub to: AccountId,
    pub amount: u64,
    pub l1_seq: u64,
}
```

### Withdrawal Flow

```rust
// L2 SDK
pub struct WithdrawRequest {
    pub from: AccountId,
    pub to_l1_address: [u8; 32],
    pub amount: u64,
    pub nonce: u64,
    pub signature: Vec<u8>,
    pub signer_pubkey: [u8; 32],
}

// On-chain instruction params
pub struct WithdrawAttestedParams {
    pub recipient: Pubkey,
    pub amount: u64,
    pub nullifier: [u8; 32],
}
```

### Bridge Initialization

```rust
pub struct InitParams {
    pub sequencer_authority: [u8; 32],
    pub domain: [u8; 32],
}
```

## 5. Private Transaction Structure (Nullifiers & Commitments)

### PrivateTransaction Implementation

The shielded transaction model uses a UTXO-like approach with:

| Component | Size | Purpose |
|-----------|------|---------|
| proof | Variable | Groth16 ZK proof for validity |
| nullifier | 32 bytes | Unique identifier to prevent double-spending |
| commitment | 32 bytes | Hash of the new "note" (hidden output) |
| ciphertext | Variable | Encrypted data only recipient can decrypt |
| ephemeral_key | 32 bytes | For ECDH shared secret derivation |

### Nullifier Tracking

**L2 Storage (Sequencer):**

```rust
// Check if nullifier was already used
pub fn nullifier_exists(&self, nullifier: &[u8]) -> Result<bool>

// Mark nullifier as spent
pub fn mark_nullifier(&self, nullifier: &[u8]) -> Result<()>
```

**L1 On-chain (Bridge Withdrawals):**

```rust
pub struct UsedNullifier {
    pub domain: [u8; 32],
    pub nullifier: [u8; 32],
    pub recipient: Pubkey,
    pub amount: u64,
    pub used: u8,            // 1 = used
    pub bump: u8,
    pub _padding: [u8; 6],
}
```

### Storage Effects

```rust
impl TransactionType {
    pub fn apply_storage_effects(&self, batch: &mut WriteBatch, cf_nullifiers: &ColumnFamily) {
        match self {
            TransactionType::Shielded(blob) => {
                batch.put_cf(cf_nullifiers, &blob.nullifier, b"1");
            }
            _ => {}
        }
    }
}
```

## 6. Encryption Flow

### Client-Side Encryption

1. Serialize SignedTransaction with wincode
2. Compute sender_hint = SHA256(signer_pubkey)
3. Generate random 12-byte nonce
4. Derive AEAD key via ECDH:
   - Shared secret = client_secret * sequencer_pub
   - Key = HKDF-SHA256(shared_secret, "zelana-tx-v1")
5. Create AAD = [version, flags, sender_hint]
6. Encrypt with ChaCha20-Poly1305
7. Return EncryptedTxBlobV1

### Sequencer-Side Decryption

1. Derive AEAD key (sequencer_secret * client_pub)
2. Reconstruct AAD from blob fields
3. Decrypt with ChaCha20-Poly1305
4. Deserialize to SignedTransaction

### Key Derivation

```rust
fn derive_aead_key(my_secret: &StaticSecret, their_pub: &PublicKey) -> [u8; 32] {
    let shared = my_secret.diffie_hellman(their_pub);
    let hk = Hkdf::<Sha256>::new(None, shared.as_bytes());
    let mut key = [0u8; 32];
    hk.expand(b"zelana-tx-v1", &mut key).unwrap();
    key
}
```

## 7. Transaction Lifecycle

### Creation to Finalization Flow

```
┌─────────────────────────────────────────────────────────────────────────┐
│                         CLIENT SIDE                                      │
├─────────────────────────────────────────────────────────────────────────┤
│ 1. Create TransactionData (from, to, amount, nonce, chain_id)           │
│ 2. Sign with Ed25519 → SignedTransaction                                │
│ 3. Encrypt with ChaCha20-Poly1305 → EncryptedTxBlobV1                   │
│ 4. POST to /submit_tx                                                    │
└─────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                       SEQUENCER INGEST                                   │
├─────────────────────────────────────────────────────────────────────────┤
│ 5. Deserialize EncryptedTxBlobV1                                        │
│ 6. Compute tx_hash = SHA256(blob)                                       │
│ 7. Decrypt → SignedTransaction                                          │
│ 8. Validate chain_id                                                     │
└─────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                       EXECUTOR                                           │
├─────────────────────────────────────────────────────────────────────────┤
│ 9. Load sender/receiver state from DB (or cache)                        │
│ 10. Validate: balance >= amount, nonce matches                          │
│ 11. Update in-memory state                                               │
│ 12. Return ExecutionResult with StateDiff                               │
└─────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                       SESSION                                            │
├─────────────────────────────────────────────────────────────────────────┤
│ 13. Push ExecutionResult to session                                      │
│ 14. Persist encrypted blob to DB                                         │
│ 15. When tx_count >= MAX_TX_PER_BLOCK (2):                              │
│     - Compute new_root from state                                        │
│     - Apply state diff to DB                                             │
│     - Close session → ClosedSession                                      │
│     - Store BlockHeader                                                  │
└─────────────────────────────────────────────────────────────────────────┘
```

### Block Header Structure

```rust
pub struct BlockHeader {
    pub magic: [u8; 4],      // "ZLNA"
    pub hdr_version: u16,    // 1
    pub batch_id: u64,       // Incrementing batch number
    pub prev_root: [u8; 32], // Previous state root
    pub new_root: [u8; 32],  // New state root after batch
    pub tx_count: u32,       // Transactions in batch
    pub open_at: u64,        // Timestamp
    pub flags: u32,          // Reserved flags
}
```
