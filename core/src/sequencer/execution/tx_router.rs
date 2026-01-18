#![allow(dead_code)] // Some methods reserved for future features
//! Transaction Router
//!
//! Unified transaction execution layer that routes all transaction types
//! through appropriate handlers and produces state diffs.
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                     Transaction Router                          │
//! │                                                                  │
//! │  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌────────────────┐  │
//! │  │ Shielded │  │ Transfer │  │ Deposit  │  │   Withdraw     │  │
//! │  │ (ZK)     │  │ (Signed) │  │ (L1→L2)  │  │   (L2→L1)      │  │
//! │  └────┬─────┘  └────┬─────┘  └────┬─────┘  └───────┬────────┘  │
//! │       │             │             │                │           │
//! │       ▼             ▼             ▼                ▼           │
//! │  ┌─────────────────────────────────────────────────────────┐   │
//! │  │                   Unified StateDiff                      │   │
//! │  │  • Account updates                                       │   │
//! │  │  • Nullifiers spent                                      │   │
//! │  │  • Commitments added                                     │   │
//! │  │  • Encrypted notes stored                                │   │
//! │  │  • Withdrawals queued                                    │   │
//! │  └─────────────────────────────────────────────────────────┘   │
//! └─────────────────────────────────────────────────────────────────┘
//! ```

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};

use crate::sequencer::storage::db::{DbBatch, RocksDbStore};
use crate::sequencer::storage::shielded_state::{ShieldedState, ShieldedStateDiff};
use crate::storage::StateStore;
use zelana_account::{AccountId, AccountState};
use zelana_privacy::{Commitment, EncryptedNote, Nullifier};
use zelana_transaction::{
    DepositEvent, PrivateTransaction, SignedTransaction, TransactionType, WithdrawRequest,
};

// ============================================================================
// Execution Results
// ============================================================================

/// Result of executing a single transaction
#[derive(Debug, Clone)]
pub struct TxResult {
    /// Transaction hash (for tracking)
    pub tx_hash: [u8; 32],
    /// Type of transaction executed
    pub tx_type: TxResultType,
    /// Success or failure
    pub success: bool,
    /// Error message if failed
    pub error: Option<String>,
}

/// The type-specific result data
#[derive(Debug, Clone)]
pub enum TxResultType {
    /// Shielded transaction result
    Shielded {
        nullifier: Nullifier,
        commitment: Commitment,
        position: u32,
    },
    /// Transfer result
    Transfer {
        from: AccountId,
        to: AccountId,
        amount: u64,
    },
    /// Deposit result
    Deposit { to: AccountId, amount: u64 },
    /// Withdrawal result (queued)
    Withdrawal {
        from: AccountId,
        to_l1: [u8; 32],
        amount: u64,
    },
}

/// Aggregated state changes from a batch of transactions
#[derive(Debug, Default, Clone)]
pub struct BatchDiff {
    /// Account balance/nonce updates
    pub account_updates: HashMap<AccountId, AccountState>,
    /// Shielded state changes
    pub shielded_diff: ShieldedStateDiff,
    /// Pending withdrawals to queue
    pub withdrawals: Vec<PendingWithdrawal>,
    /// Transaction results
    pub results: Vec<TxResult>,
}

/// A withdrawal waiting to be settled on L1
#[derive(Debug, Clone)]
pub struct PendingWithdrawal {
    pub tx_hash: [u8; 32],
    pub from: AccountId,
    pub to_l1_address: [u8; 32],
    pub amount: u64,
    pub l2_nonce: u64,
}

// ============================================================================
// Transaction Router
// ============================================================================

/// The main transaction execution router
pub struct TxRouter {
    db: Arc<RocksDbStore>,
    /// In-memory account cache for the current batch
    account_cache: HashMap<AccountId, AccountState>,
    /// Shielded pool state
    shielded_state: ShieldedState,
}

impl TxRouter {
    /// Create a new router with database and shielded state
    pub fn new(db: Arc<RocksDbStore>, shielded_state: ShieldedState) -> Self {
        Self {
            db,
            account_cache: HashMap::new(),
            shielded_state,
        }
    }

    /// Load shielded state from database
    pub fn load(db: Arc<RocksDbStore>) -> Result<Self> {
        let shielded_state = ShieldedState::load(&db)?;
        Ok(Self::new(db, shielded_state))
    }

    /// Execute a batch of transactions
    ///
    /// Returns a BatchDiff that can be atomically committed
    pub fn execute_batch(&mut self, transactions: Vec<TransactionType>) -> BatchDiff {
        let mut diff = BatchDiff::default();

        for tx in transactions {
            let tx_hash = self.compute_tx_hash(&tx);
            let result = self.execute_single(tx, tx_hash, &mut diff);

            match result {
                Ok(tx_result) => {
                    diff.results.push(tx_result);
                }
                Err(e) => {
                    diff.results.push(TxResult {
                        tx_hash,
                        tx_type: TxResultType::Transfer {
                            from: AccountId([0; 32]),
                            to: AccountId([0; 32]),
                            amount: 0,
                        },
                        success: false,
                        error: Some(e.to_string()),
                    });
                }
            }
        }

        // Collect account updates from cache
        diff.account_updates = self.account_cache.clone();

        diff
    }

    /// Execute a single transaction
    fn execute_single(
        &mut self,
        tx: TransactionType,
        tx_hash: [u8; 32],
        diff: &mut BatchDiff,
    ) -> Result<TxResult> {
        match tx {
            TransactionType::Shielded(private_tx) => {
                self.execute_shielded(private_tx, tx_hash, diff)
            }
            TransactionType::Transfer(signed_tx) => self.execute_transfer(signed_tx, tx_hash),
            TransactionType::Deposit(deposit) => self.execute_deposit(deposit, tx_hash),
            TransactionType::Withdraw(withdraw) => self.execute_withdraw(withdraw, tx_hash, diff),
        }
    }

    /// Execute a shielded (private) transaction
    ///
    /// - Verifies nullifier hasn't been spent
    /// - Verifies ZK proof (currently mocked for MVP - will be enabled later)
    /// - Adds commitment to tree
    /// - Marks nullifier as spent
    fn execute_shielded(
        &mut self,
        tx: PrivateTransaction,
        tx_hash: [u8; 32],
        diff: &mut BatchDiff,
    ) -> Result<TxResult> {
        let nullifier = Nullifier(tx.nullifier);
        let commitment = Commitment(tx.commitment);

        // Check nullifier not already spent (in persistent state)
        if self.shielded_state.nullifier_exists(&nullifier) {
            bail!("nullifier already spent");
        }

        // Check nullifier not spent in this batch
        if diff.shielded_diff.spent_nullifiers.contains(&nullifier) {
            bail!("nullifier double-spent in batch");
        }

        // ZK Proof Verification (currently mocked for MVP)
        // In production, this will call: groth16_verify(&tx.proof, &public_inputs)?;
        //
        // The proof verifies:
        // 1. The nullifier is correctly derived from a valid note
        // 2. The commitment is correctly computed for the new note
        // 3. The sender knows the spending key for the input note
        // 4. Value is conserved (input value == output value)
        //
        // For now, we only validate that a proof blob exists (non-empty)
        if tx.proof.is_empty() {
            bail!("missing ZK proof");
        }

        // Add commitment to shielded state
        let position = self.shielded_state.insert_commitment(commitment);

        // Mark nullifier as spent (in memory - will persist on commit)
        self.shielded_state
            .spend_nullifier(nullifier)
            .context("failed to mark nullifier")?;

        // Create encrypted note for storage
        let encrypted_note = EncryptedNote {
            ephemeral_pk: tx.ephemeral_key,
            nonce: [0u8; 12], // Extract from ciphertext if needed
            ciphertext: tx.ciphertext,
        };

        // Record in batch diff
        diff.shielded_diff
            .add_commitment(position, commitment, encrypted_note);
        diff.shielded_diff.add_nullifier(nullifier);

        Ok(TxResult {
            tx_hash,
            tx_type: TxResultType::Shielded {
                nullifier,
                commitment,
                position,
            },
            success: true,
            error: None,
        })
    }

    /// Execute a standard transfer
    fn execute_transfer(&mut self, tx: SignedTransaction, tx_hash: [u8; 32]) -> Result<TxResult> {
        let from = AccountId(tx.signer_pubkey);
        let to = tx.data.to;
        let amount = tx.data.amount;
        let nonce = tx.data.nonce;

        // Verify Ed25519 signature over the transaction data
        Self::verify_transfer_signature(&tx)?;

        // Load sender state
        let mut from_state = self.load_account(&from)?;

        // Validate
        if from_state.balance < amount {
            bail!(
                "insufficient balance: has {}, needs {}",
                from_state.balance,
                amount
            );
        }
        if from_state.nonce != nonce {
            bail!(
                "invalid nonce: expected {}, got {}",
                from_state.nonce,
                nonce
            );
        }

        // Apply transfer
        if from == to {
            // Self-transfer: only nonce changes
            from_state.nonce += 1;
            self.account_cache.insert(from, from_state);
        } else {
            let mut to_state = self.load_account(&to)?;

            from_state.balance -= amount;
            from_state.nonce += 1;
            to_state.balance += amount;

            self.account_cache.insert(from, from_state);
            self.account_cache.insert(to, to_state);
        }

        Ok(TxResult {
            tx_hash,
            tx_type: TxResultType::Transfer { from, to, amount },
            success: true,
            error: None,
        })
    }

    /// Execute a deposit (L1 → L2)
    fn execute_deposit(&mut self, deposit: DepositEvent, tx_hash: [u8; 32]) -> Result<TxResult> {
        let to = deposit.to;
        let amount = deposit.amount;

        // Load recipient state and credit
        let mut to_state = self.load_account(&to)?;
        to_state.balance += amount;
        self.account_cache.insert(to, to_state);

        Ok(TxResult {
            tx_hash,
            tx_type: TxResultType::Deposit { to, amount },
            success: true,
            error: None,
        })
    }

    /// Execute a withdrawal (L2 → L1)
    fn execute_withdraw(
        &mut self,
        withdraw: WithdrawRequest,
        tx_hash: [u8; 32],
        diff: &mut BatchDiff,
    ) -> Result<TxResult> {
        let from = withdraw.from;
        let to_l1 = withdraw.to_l1_address;
        let amount = withdraw.amount;
        let nonce = withdraw.nonce;

        // Verify Ed25519 signature over the withdrawal data
        Self::verify_withdraw_signature(&withdraw)?;

        // Load and validate
        let mut from_state = self.load_account(&from)?;

        if from_state.balance < amount {
            bail!("insufficient balance for withdrawal");
        }
        if from_state.nonce != nonce {
            bail!("invalid nonce for withdrawal");
        }

        // Debit immediately (funds locked until L1 settlement)
        from_state.balance -= amount;
        from_state.nonce += 1;
        self.account_cache.insert(from, from_state);

        // Queue withdrawal for L1 settlement
        diff.withdrawals.push(PendingWithdrawal {
            tx_hash,
            from,
            to_l1_address: to_l1,
            amount,
            l2_nonce: nonce,
        });

        Ok(TxResult {
            tx_hash,
            tx_type: TxResultType::Withdrawal {
                from,
                to_l1,
                amount,
            },
            success: true,
            error: None,
        })
    }

    /// Load account from cache or database
    fn load_account(&mut self, id: &AccountId) -> Result<AccountState> {
        if let Some(state) = self.account_cache.get(id) {
            return Ok(state.clone());
        }

        let state = self.db.get_account_state(id).unwrap_or_default();
        // Don't cache on load - only cache on write
        Ok(state)
    }

    /// Compute transaction hash
    fn compute_tx_hash(&self, tx: &TransactionType) -> [u8; 32] {
        // Simple hash of serialized transaction
        let bytes = match tx {
            TransactionType::Shielded(p) => {
                let mut data = p.nullifier.to_vec();
                data.extend_from_slice(&p.commitment);
                data
            }
            TransactionType::Transfer(t) => {
                let mut data = t.signer_pubkey.to_vec();
                data.extend_from_slice(&t.data.nonce.to_le_bytes());
                data
            }
            TransactionType::Deposit(d) => {
                let mut data = d.to.0.to_vec();
                data.extend_from_slice(&d.l1_seq.to_le_bytes());
                data
            }
            TransactionType::Withdraw(w) => {
                let mut data = w.from.0.to_vec();
                data.extend_from_slice(&w.nonce.to_le_bytes());
                data
            }
        };
        *blake3::hash(&bytes).as_bytes()
    }

    /// Commit a batch diff to persistent storage
    pub fn commit(&mut self, diff: BatchDiff) -> Result<()> {
        let mut db_batch = DbBatch::default();

        // Account updates
        log::debug!(
            "[COMMIT] Committing {} account updates",
            diff.account_updates.len()
        );
        for (id, state) in diff.account_updates {
            log::debug!(
                "[COMMIT] Account {}: balance={}, nonce={}",
                hex::encode(&id.0[..8]),
                state.balance,
                state.nonce
            );
            db_batch.account_updates.push((id, state));
        }

        // Shielded state updates
        for nullifier in diff.shielded_diff.spent_nullifiers {
            db_batch.nullifiers.push(nullifier);
        }

        for (position, commitment, note) in diff.shielded_diff.new_commitments {
            db_batch.commitments.push((position, commitment.0));
            db_batch.encrypted_notes.push((commitment.0, note));
        }

        // Persist withdrawals
        for withdrawal in &diff.withdrawals {
            let data = serde_json::to_vec(&withdrawal)?;
            self.db.store_withdrawal(withdrawal.tx_hash, &data)?;
            // Track withdrawal amount for stats
            if let Err(e) = self.db.add_withdrawal(withdrawal.amount) {
                log::warn!("Failed to track withdrawal amount: {}", e);
            }
        }

        // Atomic batch write
        self.db.apply_batch(db_batch)?;

        // Clear cache for next batch
        self.account_cache.clear();

        Ok(())
    }

    /// Get current shielded state root
    pub fn shielded_root(&self) -> [u8; 32] {
        self.shielded_state.root()
    }

    /// Get transparent state root (from account merkle tree)
    pub fn transparent_root(&self) -> [u8; 32] {
        // For now, simple hash of cached accounts
        // In production: proper sparse merkle tree
        let mut hasher = blake3::Hasher::new();
        let mut accounts: Vec<_> = self.account_cache.iter().collect();
        accounts.sort_by_key(|(id, _)| id.0);
        for (id, state) in accounts {
            hasher.update(&id.0);
            hasher.update(&state.balance.to_le_bytes());
            hasher.update(&state.nonce.to_le_bytes());
        }
        *hasher.finalize().as_bytes()
    }

    /// Get shielded state reference
    pub fn shielded_state(&self) -> &ShieldedState {
        &self.shielded_state
    }

    /// Get shielded state mutable reference
    pub fn shielded_state_mut(&mut self) -> &mut ShieldedState {
        &mut self.shielded_state
    }

    /// Get successful transaction count from results
    pub fn successful_count(results: &[TxResult]) -> usize {
        results.iter().filter(|r| r.success).count()
    }

    // ========================================================================
    // Signature Verification
    // ========================================================================

    /// Verify Ed25519 signature on a transfer transaction.
    ///
    /// The message signed is the wincode-serialized TransactionData.
    fn verify_transfer_signature(tx: &SignedTransaction) -> Result<()> {
        // Reconstruct the message that was signed (serialized TransactionData)
        let msg = wincode::serialize(&tx.data).context("failed to serialize tx data")?;

        // Parse the public key
        let verifying_key = VerifyingKey::from_bytes(&tx.signer_pubkey)
            .map_err(|e| anyhow::anyhow!("invalid signer public key: {}", e))?;

        // Parse the signature
        if tx.signature.len() != 64 {
            bail!(
                "invalid signature length: expected 64, got {}",
                tx.signature.len()
            );
        }
        let sig_bytes: [u8; 64] = tx.signature.as_slice().try_into().unwrap();
        let signature = Signature::from_bytes(&sig_bytes);

        // Verify
        verifying_key
            .verify(&msg, &signature)
            .map_err(|e| anyhow::anyhow!("signature verification failed: {}", e))?;

        // Verify that from field matches signer_pubkey
        if tx.data.from.0 != tx.signer_pubkey {
            bail!(
                "from address mismatch: tx.data.from={} but signer_pubkey={}",
                hex::encode(tx.data.from.0),
                hex::encode(tx.signer_pubkey)
            );
        }

        Ok(())
    }

    /// Verify Ed25519 signature on a withdrawal request.
    ///
    /// The message signed is the canonical encoding of the withdrawal data:
    /// from || to_l1_address || amount || nonce
    fn verify_withdraw_signature(withdraw: &WithdrawRequest) -> Result<()> {
        // Build the message that should have been signed
        // We use a deterministic encoding: from || to_l1 || amount (le) || nonce (le)
        let mut msg = Vec::with_capacity(32 + 32 + 8 + 8);
        msg.extend_from_slice(&withdraw.from.0);
        msg.extend_from_slice(&withdraw.to_l1_address);
        msg.extend_from_slice(&withdraw.amount.to_le_bytes());
        msg.extend_from_slice(&withdraw.nonce.to_le_bytes());

        // Parse the public key
        let verifying_key = VerifyingKey::from_bytes(&withdraw.signer_pubkey)
            .map_err(|e| anyhow::anyhow!("invalid signer public key: {}", e))?;

        // Parse the signature
        if withdraw.signature.len() != 64 {
            bail!(
                "invalid signature length: expected 64, got {}",
                withdraw.signature.len()
            );
        }
        let sig_bytes: [u8; 64] = withdraw.signature.as_slice().try_into().unwrap();
        let signature = Signature::from_bytes(&sig_bytes);

        // Verify
        verifying_key
            .verify(&msg, &signature)
            .map_err(|e| anyhow::anyhow!("signature verification failed: {}", e))?;

        // Verify that from field matches signer_pubkey
        if withdraw.from.0 != withdraw.signer_pubkey {
            bail!(
                "from address mismatch: withdraw.from={} but signer_pubkey={}",
                hex::encode(withdraw.from.0),
                hex::encode(withdraw.signer_pubkey)
            );
        }

        Ok(())
    }
}

// ============================================================================
// Serialization for PendingWithdrawal
// ============================================================================

impl serde::Serialize for PendingWithdrawal {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("PendingWithdrawal", 5)?;
        state.serialize_field("tx_hash", &hex::encode(self.tx_hash))?;
        state.serialize_field("from", &hex::encode(self.from.0))?;
        state.serialize_field("to_l1_address", &hex::encode(self.to_l1_address))?;
        state.serialize_field("amount", &self.amount)?;
        state.serialize_field("l2_nonce", &self.l2_nonce)?;
        state.end()
    }
}

impl<'de> serde::Deserialize<'de> for PendingWithdrawal {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::{self, MapAccess, Visitor};
        use std::fmt;

        struct PendingWithdrawalVisitor;

        impl<'de> Visitor<'de> for PendingWithdrawalVisitor {
            type Value = PendingWithdrawal;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("struct PendingWithdrawal")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<PendingWithdrawal, V::Error>
            where
                V: MapAccess<'de>,
            {
                let mut tx_hash: Option<String> = None;
                let mut from: Option<String> = None;
                let mut to_l1_address: Option<String> = None;
                let mut amount: Option<u64> = None;
                let mut l2_nonce: Option<u64> = None;

                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "tx_hash" => tx_hash = Some(map.next_value()?),
                        "from" => from = Some(map.next_value()?),
                        "to_l1_address" => to_l1_address = Some(map.next_value()?),
                        "amount" => amount = Some(map.next_value()?),
                        "l2_nonce" => l2_nonce = Some(map.next_value()?),
                        _ => {
                            let _: serde_json::Value = map.next_value()?;
                        }
                    }
                }

                let tx_hash_str = tx_hash.ok_or_else(|| de::Error::missing_field("tx_hash"))?;
                let from_str = from.ok_or_else(|| de::Error::missing_field("from"))?;
                let to_l1_str =
                    to_l1_address.ok_or_else(|| de::Error::missing_field("to_l1_address"))?;
                let amount = amount.ok_or_else(|| de::Error::missing_field("amount"))?;
                let l2_nonce = l2_nonce.ok_or_else(|| de::Error::missing_field("l2_nonce"))?;

                let tx_hash_bytes = hex::decode(&tx_hash_str)
                    .map_err(|_| de::Error::custom("invalid hex for tx_hash"))?;
                let from_bytes = hex::decode(&from_str)
                    .map_err(|_| de::Error::custom("invalid hex for from"))?;
                let to_l1_bytes = hex::decode(&to_l1_str)
                    .map_err(|_| de::Error::custom("invalid hex for to_l1_address"))?;

                if tx_hash_bytes.len() != 32 {
                    return Err(de::Error::custom("tx_hash must be 32 bytes"));
                }
                if from_bytes.len() != 32 {
                    return Err(de::Error::custom("from must be 32 bytes"));
                }
                if to_l1_bytes.len() != 32 {
                    return Err(de::Error::custom("to_l1_address must be 32 bytes"));
                }

                let mut tx_hash = [0u8; 32];
                let mut from_arr = [0u8; 32];
                let mut to_l1_arr = [0u8; 32];
                tx_hash.copy_from_slice(&tx_hash_bytes);
                from_arr.copy_from_slice(&from_bytes);
                to_l1_arr.copy_from_slice(&to_l1_bytes);

                Ok(PendingWithdrawal {
                    tx_hash,
                    from: AccountId(from_arr),
                    to_l1_address: to_l1_arr,
                    amount,
                    l2_nonce,
                })
            }
        }

        deserializer.deserialize_map(PendingWithdrawalVisitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use zelana_transaction::TransactionData;

    /// Create a test RocksDB store
    fn create_test_db() -> (Arc<RocksDbStore>, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let db = RocksDbStore::open(temp_dir.path()).unwrap();
        (Arc::new(db), temp_dir)
    }

    /// Create a test router with a fresh database
    fn create_test_router() -> (TxRouter, Arc<RocksDbStore>, TempDir) {
        let (db, temp_dir) = create_test_db();
        let router = TxRouter::load(db.clone()).unwrap();
        (router, db, temp_dir)
    }

    #[test]
    fn test_batch_diff_default() {
        let diff = BatchDiff::default();
        assert!(diff.account_updates.is_empty());
        assert!(diff.shielded_diff.is_empty());
        assert!(diff.withdrawals.is_empty());
        assert!(diff.results.is_empty());
    }

    // ========================================================================
    // Transfer Tests
    // ========================================================================

    #[test]
    fn test_transfer_with_valid_signature() {
        let (mut router, db, _temp) = create_test_router();

        // Create sender keypair
        let sender = zelana_keypair::Keypair::new_random();
        let sender_id = sender.account_id();

        // Create recipient keypair
        let recipient = zelana_keypair::Keypair::new_random();
        let recipient_id = recipient.account_id();

        // Fund sender account
        let initial_balance = 1_000_000u64;
        db.set_account_state(
            sender_id,
            AccountState {
                balance: initial_balance,
                nonce: 0,
            },
        )
        .unwrap();

        // Create and sign a transfer
        let amount = 100_000u64;
        let tx_data = TransactionData {
            from: sender_id,
            to: recipient_id,
            amount,
            nonce: 0,
            chain_id: 1,
        };
        let signed_tx = sender.sign_transaction(tx_data);

        // Execute the transfer
        let transactions = vec![TransactionType::Transfer(signed_tx)];
        let diff = router.execute_batch(transactions);

        // Verify success
        assert_eq!(diff.results.len(), 1);
        assert!(
            diff.results[0].success,
            "Transfer should succeed: {:?}",
            diff.results[0].error
        );

        // Verify account updates
        let sender_state = diff.account_updates.get(&sender_id).unwrap();
        let recipient_state = diff.account_updates.get(&recipient_id).unwrap();

        assert_eq!(
            sender_state.balance,
            initial_balance - amount,
            "Sender balance should be debited"
        );
        assert_eq!(sender_state.nonce, 1, "Sender nonce should be incremented");
        assert_eq!(
            recipient_state.balance, amount,
            "Recipient should receive funds"
        );
    }

    #[test]
    fn test_transfer_with_invalid_signature() {
        let (mut router, db, _temp) = create_test_router();

        // Create sender keypair
        let sender = zelana_keypair::Keypair::new_random();
        let sender_id = sender.account_id();

        // Create different keypair to sign with (attacker)
        let attacker = zelana_keypair::Keypair::new_random();

        // Create recipient keypair
        let recipient = zelana_keypair::Keypair::new_random();
        let recipient_id = recipient.account_id();

        // Fund sender account
        db.set_account_state(
            sender_id,
            AccountState {
                balance: 1_000_000,
                nonce: 0,
            },
        )
        .unwrap();

        // Create tx data claiming to be from sender
        let tx_data = TransactionData {
            from: sender_id,
            to: recipient_id,
            amount: 100_000,
            nonce: 0,
            chain_id: 1,
        };

        // But sign with attacker's key
        let mut signed_tx = attacker.sign_transaction(tx_data.clone());
        // Override signer_pubkey to claim it's from sender
        signed_tx.signer_pubkey = sender_id.0;

        // Execute - should fail signature verification
        let transactions = vec![TransactionType::Transfer(signed_tx)];
        let diff = router.execute_batch(transactions);

        assert_eq!(diff.results.len(), 1);
        assert!(!diff.results[0].success, "Transfer should fail");
        assert!(
            diff.results[0]
                .error
                .as_ref()
                .unwrap()
                .contains("signature verification failed"),
            "Should fail signature verification: {:?}",
            diff.results[0].error
        );
    }

    #[test]
    fn test_transfer_insufficient_balance() {
        let (mut router, db, _temp) = create_test_router();

        // Create sender with low balance
        let sender = zelana_keypair::Keypair::new_random();
        let sender_id = sender.account_id();
        let recipient = zelana_keypair::Keypair::new_random();

        db.set_account_state(
            sender_id,
            AccountState {
                balance: 100,
                nonce: 0,
            },
        )
        .unwrap();

        // Try to transfer more than balance
        let tx_data = TransactionData {
            from: sender_id,
            to: recipient.account_id(),
            amount: 1_000_000,
            nonce: 0,
            chain_id: 1,
        };
        let signed_tx = sender.sign_transaction(tx_data);

        let transactions = vec![TransactionType::Transfer(signed_tx)];
        let diff = router.execute_batch(transactions);

        assert!(!diff.results[0].success);
        assert!(
            diff.results[0]
                .error
                .as_ref()
                .unwrap()
                .contains("insufficient balance")
        );
    }

    #[test]
    fn test_transfer_invalid_nonce() {
        let (mut router, db, _temp) = create_test_router();

        let sender = zelana_keypair::Keypair::new_random();
        let sender_id = sender.account_id();
        let recipient = zelana_keypair::Keypair::new_random();

        db.set_account_state(
            sender_id,
            AccountState {
                balance: 1_000_000,
                nonce: 5, // Account has nonce 5
            },
        )
        .unwrap();

        // Try with wrong nonce (0 instead of 5)
        let tx_data = TransactionData {
            from: sender_id,
            to: recipient.account_id(),
            amount: 1_000,
            nonce: 0, // Wrong nonce
            chain_id: 1,
        };
        let signed_tx = sender.sign_transaction(tx_data);

        let transactions = vec![TransactionType::Transfer(signed_tx)];
        let diff = router.execute_batch(transactions);

        assert!(!diff.results[0].success);
        assert!(
            diff.results[0]
                .error
                .as_ref()
                .unwrap()
                .contains("invalid nonce")
        );
    }

    #[test]
    fn test_self_transfer() {
        let (mut router, db, _temp) = create_test_router();

        let sender = zelana_keypair::Keypair::new_random();
        let sender_id = sender.account_id();

        let initial_balance = 1_000_000u64;
        db.set_account_state(
            sender_id,
            AccountState {
                balance: initial_balance,
                nonce: 0,
            },
        )
        .unwrap();

        // Self transfer
        let tx_data = TransactionData {
            from: sender_id,
            to: sender_id, // Same as sender
            amount: 100,
            nonce: 0,
            chain_id: 1,
        };
        let signed_tx = sender.sign_transaction(tx_data);

        let transactions = vec![TransactionType::Transfer(signed_tx)];
        let diff = router.execute_batch(transactions);

        assert!(diff.results[0].success);
        let state = diff.account_updates.get(&sender_id).unwrap();
        assert_eq!(
            state.balance, initial_balance,
            "Balance unchanged for self-transfer"
        );
        assert_eq!(state.nonce, 1, "Nonce should still increment");
    }

    // ========================================================================
    // Withdrawal Tests
    // ========================================================================

    #[test]
    fn test_withdrawal_with_valid_signature() {
        let (mut router, db, _temp) = create_test_router();

        let sender = zelana_keypair::Keypair::new_random();
        let sender_id = sender.account_id();

        let initial_balance = 1_000_000u64;
        db.set_account_state(
            sender_id,
            AccountState {
                balance: initial_balance,
                nonce: 0,
            },
        )
        .unwrap();

        // Create withdrawal to some L1 address
        let l1_address = [42u8; 32];
        let amount = 500_000u64;
        let withdraw_req = sender.sign_withdrawal(l1_address, amount, 0);

        let transactions = vec![TransactionType::Withdraw(withdraw_req)];
        let diff = router.execute_batch(transactions);

        assert_eq!(diff.results.len(), 1);
        assert!(
            diff.results[0].success,
            "Withdrawal should succeed: {:?}",
            diff.results[0].error
        );

        // Verify balance debited
        let sender_state = diff.account_updates.get(&sender_id).unwrap();
        assert_eq!(sender_state.balance, initial_balance - amount);
        assert_eq!(sender_state.nonce, 1);

        // Verify withdrawal queued
        assert_eq!(diff.withdrawals.len(), 1);
        assert_eq!(diff.withdrawals[0].amount, amount);
        assert_eq!(diff.withdrawals[0].to_l1_address, l1_address);
    }

    #[test]
    fn test_withdrawal_with_invalid_signature() {
        let (mut router, db, _temp) = create_test_router();

        let sender = zelana_keypair::Keypair::new_random();
        let sender_id = sender.account_id();
        let attacker = zelana_keypair::Keypair::new_random();

        db.set_account_state(
            sender_id,
            AccountState {
                balance: 1_000_000,
                nonce: 0,
            },
        )
        .unwrap();

        // Attacker signs withdrawal claiming to be sender
        let mut withdraw_req = attacker.sign_withdrawal([42u8; 32], 500_000, 0);
        // Override to claim it's from sender's account
        withdraw_req.from = sender_id;
        withdraw_req.signer_pubkey = sender_id.0;

        let transactions = vec![TransactionType::Withdraw(withdraw_req)];
        let diff = router.execute_batch(transactions);

        assert!(!diff.results[0].success);
        assert!(
            diff.results[0]
                .error
                .as_ref()
                .unwrap()
                .contains("signature verification failed")
        );
    }

    #[test]
    fn test_withdrawal_insufficient_balance() {
        let (mut router, db, _temp) = create_test_router();

        let sender = zelana_keypair::Keypair::new_random();
        let sender_id = sender.account_id();

        db.set_account_state(
            sender_id,
            AccountState {
                balance: 100,
                nonce: 0,
            },
        )
        .unwrap();

        let withdraw_req = sender.sign_withdrawal([42u8; 32], 1_000_000, 0);

        let transactions = vec![TransactionType::Withdraw(withdraw_req)];
        let diff = router.execute_batch(transactions);

        assert!(!diff.results[0].success);
        assert!(
            diff.results[0]
                .error
                .as_ref()
                .unwrap()
                .contains("insufficient balance")
        );
    }

    // ========================================================================
    // Shielded Transaction Tests
    // ========================================================================

    #[test]
    fn test_shielded_tx_success() {
        let (mut router, _, _temp) = create_test_router();

        let nullifier = [1u8; 32];
        let commitment = [2u8; 32];

        let private_tx = PrivateTransaction {
            proof: vec![1, 2, 3, 4], // Non-empty mock proof
            nullifier,
            commitment,
            ciphertext: vec![5, 6, 7, 8],
            ephemeral_key: [9u8; 32],
        };

        let transactions = vec![TransactionType::Shielded(private_tx)];
        let diff = router.execute_batch(transactions);

        assert_eq!(diff.results.len(), 1);
        assert!(
            diff.results[0].success,
            "Shielded tx should succeed: {:?}",
            diff.results[0].error
        );

        // Verify shielded state updates
        assert_eq!(diff.shielded_diff.spent_nullifiers.len(), 1);
        assert_eq!(diff.shielded_diff.new_commitments.len(), 1);
    }

    #[test]
    fn test_shielded_tx_missing_proof() {
        let (mut router, _, _temp) = create_test_router();

        let private_tx = PrivateTransaction {
            proof: vec![], // Empty proof should fail
            nullifier: [1u8; 32],
            commitment: [2u8; 32],
            ciphertext: vec![5, 6, 7, 8],
            ephemeral_key: [9u8; 32],
        };

        let transactions = vec![TransactionType::Shielded(private_tx)];
        let diff = router.execute_batch(transactions);

        assert!(!diff.results[0].success);
        assert!(
            diff.results[0]
                .error
                .as_ref()
                .unwrap()
                .contains("missing ZK proof")
        );
    }

    #[test]
    fn test_shielded_tx_double_spend() {
        let (mut router, _, _temp) = create_test_router();

        let nullifier = [1u8; 32];

        // First shielded tx
        let tx1 = PrivateTransaction {
            proof: vec![1, 2, 3, 4],
            nullifier, // Same nullifier
            commitment: [2u8; 32],
            ciphertext: vec![5, 6, 7, 8],
            ephemeral_key: [9u8; 32],
        };

        // Second shielded tx with same nullifier
        let tx2 = PrivateTransaction {
            proof: vec![1, 2, 3, 4],
            nullifier, // Same nullifier - double spend attempt
            commitment: [3u8; 32],
            ciphertext: vec![5, 6, 7, 8],
            ephemeral_key: [10u8; 32],
        };

        let transactions = vec![
            TransactionType::Shielded(tx1),
            TransactionType::Shielded(tx2),
        ];
        let diff = router.execute_batch(transactions);

        // First should succeed
        assert!(diff.results[0].success, "First tx should succeed");
        // Second should fail - double spend
        assert!(!diff.results[1].success, "Second tx should fail");
        let error_msg = diff.results[1].error.as_ref().unwrap();
        assert!(
            error_msg.contains("double-spent") || error_msg.contains("already spent"),
            "Should detect double spend: {:?}",
            diff.results[1].error
        );
    }

    // ========================================================================
    // Deposit Tests
    // ========================================================================

    #[test]
    fn test_deposit_creates_account() {
        let (mut router, _, _temp) = create_test_router();

        let recipient = AccountId([42u8; 32]);
        let amount = 5_000_000u64;

        let deposit = DepositEvent {
            to: recipient,
            amount,
            l1_seq: 1,
        };

        let transactions = vec![TransactionType::Deposit(deposit)];
        let diff = router.execute_batch(transactions);

        assert!(diff.results[0].success);

        let state = diff.account_updates.get(&recipient).unwrap();
        assert_eq!(state.balance, amount);
        assert_eq!(state.nonce, 0);
    }

    #[test]
    fn test_deposit_adds_to_existing_balance() {
        let (mut router, db, _temp) = create_test_router();

        let recipient = AccountId([42u8; 32]);
        let initial_balance = 1_000_000u64;

        db.set_account_state(
            recipient,
            AccountState {
                balance: initial_balance,
                nonce: 5,
            },
        )
        .unwrap();

        let deposit_amount = 500_000u64;
        let deposit = DepositEvent {
            to: recipient,
            amount: deposit_amount,
            l1_seq: 1,
        };

        let transactions = vec![TransactionType::Deposit(deposit)];
        let diff = router.execute_batch(transactions);

        assert!(diff.results[0].success);

        let state = diff.account_updates.get(&recipient).unwrap();
        assert_eq!(state.balance, initial_balance + deposit_amount);
        assert_eq!(state.nonce, 5, "Nonce unchanged for deposit");
    }

    // ========================================================================
    // Multi-Transaction Batch Tests
    // ========================================================================

    #[test]
    fn test_mixed_batch() {
        let (mut router, db, _temp) = create_test_router();

        // Setup accounts
        let alice = zelana_keypair::Keypair::new_random();
        let alice_id = alice.account_id();
        let bob = zelana_keypair::Keypair::new_random();
        let bob_id = bob.account_id();

        db.set_account_state(
            alice_id,
            AccountState {
                balance: 10_000_000,
                nonce: 0,
            },
        )
        .unwrap();

        // Batch: deposit + transfer + shielded + withdrawal
        let deposit = DepositEvent {
            to: bob_id,
            amount: 5_000_000,
            l1_seq: 1,
        };

        let transfer_data = TransactionData {
            from: alice_id,
            to: bob_id,
            amount: 1_000_000,
            nonce: 0,
            chain_id: 1,
        };
        let transfer = alice.sign_transaction(transfer_data);

        let shielded = PrivateTransaction {
            proof: vec![1, 2, 3],
            nullifier: [11u8; 32],
            commitment: [22u8; 32],
            ciphertext: vec![4, 5, 6],
            ephemeral_key: [33u8; 32],
        };

        let transactions = vec![
            TransactionType::Deposit(deposit),
            TransactionType::Transfer(transfer),
            TransactionType::Shielded(shielded),
        ];

        let diff = router.execute_batch(transactions);

        // All 3 should succeed
        assert_eq!(diff.results.len(), 3);
        for (i, result) in diff.results.iter().enumerate() {
            assert!(
                result.success,
                "Tx {} should succeed: {:?}",
                i, result.error
            );
        }

        // Verify final states
        let alice_state = diff.account_updates.get(&alice_id).unwrap();
        let bob_state = diff.account_updates.get(&bob_id).unwrap();

        assert_eq!(alice_state.balance, 10_000_000 - 1_000_000);
        assert_eq!(bob_state.balance, 5_000_000 + 1_000_000);
    }

    #[test]
    fn test_sequential_transfers_same_sender() {
        let (mut router, db, _temp) = create_test_router();

        let sender = zelana_keypair::Keypair::new_random();
        let sender_id = sender.account_id();
        let recipient = zelana_keypair::Keypair::new_random();
        let recipient_id = recipient.account_id();

        db.set_account_state(
            sender_id,
            AccountState {
                balance: 10_000_000,
                nonce: 0,
            },
        )
        .unwrap();

        // Multiple transfers in sequence
        let tx1 = sender.sign_transaction(TransactionData {
            from: sender_id,
            to: recipient_id,
            amount: 1_000_000,
            nonce: 0,
            chain_id: 1,
        });

        let tx2 = sender.sign_transaction(TransactionData {
            from: sender_id,
            to: recipient_id,
            amount: 2_000_000,
            nonce: 1, // Incremented nonce
            chain_id: 1,
        });

        let tx3 = sender.sign_transaction(TransactionData {
            from: sender_id,
            to: recipient_id,
            amount: 500_000,
            nonce: 2,
            chain_id: 1,
        });

        let transactions = vec![
            TransactionType::Transfer(tx1),
            TransactionType::Transfer(tx2),
            TransactionType::Transfer(tx3),
        ];

        let diff = router.execute_batch(transactions);

        // All should succeed
        for result in &diff.results {
            assert!(
                result.success,
                "All transfers should succeed: {:?}",
                result.error
            );
        }

        let sender_state = diff.account_updates.get(&sender_id).unwrap();
        let recipient_state = diff.account_updates.get(&recipient_id).unwrap();

        assert_eq!(
            sender_state.balance,
            10_000_000 - 1_000_000 - 2_000_000 - 500_000
        );
        assert_eq!(sender_state.nonce, 3);
        assert_eq!(recipient_state.balance, 1_000_000 + 2_000_000 + 500_000);
    }
}
