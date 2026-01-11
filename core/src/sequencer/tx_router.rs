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

use crate::sequencer::db::{DbBatch, RocksDbStore};
use crate::sequencer::shielded_state::{ShieldedState, ShieldedStateDiff};
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
    /// - Verifies ZK proof (TODO: actual verification)
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

        // TODO: Verify ZK proof
        // For MVP, we trust the proof bytes
        // In production: groth16_verify(&tx.proof, &public_inputs)?;

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

        // TODO: Verify signature
        // For now, trust the signature bytes

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

        // TODO: Verify signature

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
        for (id, state) in diff.account_updates {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_batch_diff_default() {
        let diff = BatchDiff::default();
        assert!(diff.account_updates.is_empty());
        assert!(diff.shielded_diff.is_empty());
        assert!(diff.withdrawals.is_empty());
        assert!(diff.results.is_empty());
    }
}
