#![allow(dead_code)] // Some methods reserved for L1 settlement phase
//! Withdrawal Queue Management
//!
//! Manages the queue of pending withdrawals waiting for L1 settlement.
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                   Withdrawal Lifecycle                          │
//! │                                                                  │
//! │  ┌──────────┐    ┌─────-────┐    ┌──────────┐    ┌───────────┐  │
//! │  │ Pending  │───>│ Included │───>│ Submitted│───>│ Finalized │  │
//! │  │          │    │ in Batch │    │  to L1   │    │           │  │
//! │  └──────────┘    └──────────┘    └──────────┘    └───────────┘  │
//! │                                                                  │
//! │  MVP: Standard withdrawals (7-day challenge period on L1)       │
//! │  Phase 2: Fast withdrawals (liquidity provider fronts funds)    │
//! └─────────────────────────────────────────────────────────────────┘
//! ```

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::sequencer::execution::tx_router::PendingWithdrawal;
use crate::sequencer::storage::db::RocksDbStore;
use zelana_account::AccountId;

// ============================================================================
// Withdrawal States
// ============================================================================

/// State of a withdrawal in the queue
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum WithdrawalState {
    /// Deducted from L2 account, waiting for batch
    Pending,
    /// Included in a batch, proving in progress
    InBatch { batch_id: u64 },
    /// Batch settled on L1, in challenge period
    Submitted { l1_tx_sig: String },
    /// Challenge period passed, funds released
    Finalized,
    /// Something went wrong (for debugging)
    Failed { reason: String },
}

/// A tracked withdrawal with full state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackedWithdrawal {
    /// Transaction hash (unique identifier)
    pub tx_hash: [u8; 32],
    /// Source account on L2
    pub from: AccountId,
    /// Destination address on L1 (Solana pubkey)
    pub to_l1_address: [u8; 32],
    /// Amount in lamports
    pub amount: u64,
    /// L2 nonce at time of withdrawal
    pub l2_nonce: u64,
    /// Current state
    pub state: WithdrawalState,
    /// Timestamp when created (unix seconds)
    pub created_at: u64,
    /// Batch ID if included
    pub batch_id: Option<u64>,
}

impl From<PendingWithdrawal> for TrackedWithdrawal {
    fn from(pw: PendingWithdrawal) -> Self {
        Self {
            tx_hash: pw.tx_hash,
            from: pw.from,
            to_l1_address: pw.to_l1_address,
            amount: pw.amount,
            l2_nonce: pw.l2_nonce,
            state: WithdrawalState::Pending,
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            batch_id: None,
        }
    }
}

// ============================================================================
// Withdrawal Queue
// ============================================================================

/// Manages the withdrawal queue
pub struct WithdrawalQueue {
    db: Arc<RocksDbStore>,
    /// In-memory index of withdrawals by tx_hash
    withdrawals: HashMap<[u8; 32], TrackedWithdrawal>,
    /// Index by L1 destination for quick lookup
    by_destination: HashMap<[u8; 32], Vec<[u8; 32]>>,
    /// Index by L2 source account
    by_source: HashMap<AccountId, Vec<[u8; 32]>>,
}

impl WithdrawalQueue {
    /// Create a new withdrawal queue
    pub fn new(db: Arc<RocksDbStore>) -> Self {
        Self {
            db,
            withdrawals: HashMap::new(),
            by_destination: HashMap::new(),
            by_source: HashMap::new(),
        }
    }

    /// Load withdrawals from database
    pub fn load(db: Arc<RocksDbStore>) -> Result<Self> {
        let mut queue = Self::new(db.clone());

        // Load all withdrawals from database
        for (tx_hash, data) in db.get_all_withdrawals()? {
            if let Ok(tracked) = serde_json::from_slice::<TrackedWithdrawal>(&data) {
                // Rebuild indexes
                queue
                    .by_destination
                    .entry(tracked.to_l1_address)
                    .or_default()
                    .push(tx_hash);
                queue
                    .by_source
                    .entry(tracked.from)
                    .or_default()
                    .push(tx_hash);
                queue.withdrawals.insert(tx_hash, tracked);
            }
        }

        log::info!(
            "Loaded {} withdrawals from database",
            queue.withdrawals.len()
        );
        Ok(queue)
    }

    /// Add a new withdrawal to the queue
    pub fn add(&mut self, withdrawal: PendingWithdrawal) -> Result<()> {
        let tracked = TrackedWithdrawal::from(withdrawal);
        let tx_hash = tracked.tx_hash;

        // Index by destination
        self.by_destination
            .entry(tracked.to_l1_address)
            .or_default()
            .push(tx_hash);

        // Index by source
        self.by_source
            .entry(tracked.from)
            .or_default()
            .push(tx_hash);

        // Store in DB
        let data = serde_json::to_vec(&tracked)?;
        self.db.store_withdrawal(tx_hash, &data)?;

        // Store in memory
        self.withdrawals.insert(tx_hash, tracked);

        Ok(())
    }

    /// Add multiple withdrawals from a batch
    pub fn add_batch(&mut self, withdrawals: Vec<PendingWithdrawal>) -> Result<()> {
        for w in withdrawals {
            self.add(w)?;
        }
        Ok(())
    }

    /// Mark withdrawals as included in a batch
    pub fn mark_in_batch(&mut self, tx_hashes: &[[u8; 32]], batch_id: u64) -> Result<()> {
        for hash in tx_hashes {
            if let Some(w) = self.withdrawals.get_mut(hash) {
                w.state = WithdrawalState::InBatch { batch_id };
                w.batch_id = Some(batch_id);

                // Update DB
                let data = serde_json::to_vec(&w)?;
                self.db.store_withdrawal(*hash, &data)?;
            }
        }
        Ok(())
    }

    /// Mark batch withdrawals as submitted to L1
    pub fn mark_submitted(&mut self, batch_id: u64, l1_tx_sig: String) -> Result<()> {
        for w in self.withdrawals.values_mut() {
            if w.batch_id == Some(batch_id) && matches!(w.state, WithdrawalState::InBatch { .. }) {
                w.state = WithdrawalState::Submitted {
                    l1_tx_sig: l1_tx_sig.clone(),
                };

                // Update DB
                let data = serde_json::to_vec(&w)?;
                self.db.store_withdrawal(w.tx_hash, &data)?;
            }
        }
        Ok(())
    }

    /// Finalize a withdrawal (challenge period passed)
    pub fn finalize(&mut self, tx_hash: &[u8; 32]) -> Result<Option<TrackedWithdrawal>> {
        if let Some(w) = self.withdrawals.get_mut(tx_hash) {
            w.state = WithdrawalState::Finalized;

            // Remove from DB (finalized withdrawals are complete)
            self.db.delete_withdrawal(tx_hash)?;

            // Keep in memory for queries but could be pruned
            return Ok(Some(w.clone()));
        }
        Ok(None)
    }

    /// Finalize all withdrawals in a batch
    pub fn finalize_batch(&mut self, batch_id: u64) -> Result<Vec<TrackedWithdrawal>> {
        let mut finalized = Vec::new();

        let hashes: Vec<[u8; 32]> = self
            .withdrawals
            .iter()
            .filter(|(_, w)| w.batch_id == Some(batch_id))
            .map(|(h, _)| *h)
            .collect();

        for hash in hashes {
            if let Some(w) = self.finalize(&hash)? {
                finalized.push(w);
            }
        }

        Ok(finalized)
    }

    /// Get withdrawal by tx hash
    pub fn get(&self, tx_hash: &[u8; 32]) -> Option<&TrackedWithdrawal> {
        self.withdrawals.get(tx_hash)
    }

    /// Get all withdrawals for an L2 account
    pub fn get_by_source(&self, account: &AccountId) -> Vec<&TrackedWithdrawal> {
        self.by_source
            .get(account)
            .map(|hashes| {
                hashes
                    .iter()
                    .filter_map(|h| self.withdrawals.get(h))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get all withdrawals to an L1 address
    pub fn get_by_destination(&self, l1_address: &[u8; 32]) -> Vec<&TrackedWithdrawal> {
        self.by_destination
            .get(l1_address)
            .map(|hashes| {
                hashes
                    .iter()
                    .filter_map(|h| self.withdrawals.get(h))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get all pending withdrawals (not yet in a batch)
    pub fn get_pending(&self) -> Vec<&TrackedWithdrawal> {
        self.withdrawals
            .values()
            .filter(|w| matches!(w.state, WithdrawalState::Pending))
            .collect()
    }

    /// Get pending withdrawal count
    pub fn pending_count(&self) -> usize {
        self.withdrawals
            .values()
            .filter(|w| matches!(w.state, WithdrawalState::Pending))
            .count()
    }

    /// Get total queued amount
    pub fn total_pending_amount(&self) -> u64 {
        self.withdrawals
            .values()
            .filter(|w| {
                !matches!(
                    w.state,
                    WithdrawalState::Finalized | WithdrawalState::Failed { .. }
                )
            })
            .map(|w| w.amount)
            .sum()
    }

    /// Get statistics
    pub fn stats(&self) -> WithdrawalStats {
        let mut pending = 0;
        let mut in_batch = 0;
        let mut submitted = 0;
        let mut finalized = 0;
        let mut failed = 0;

        for w in self.withdrawals.values() {
            match w.state {
                WithdrawalState::Pending => pending += 1,
                WithdrawalState::InBatch { .. } => in_batch += 1,
                WithdrawalState::Submitted { .. } => submitted += 1,
                WithdrawalState::Finalized => finalized += 1,
                WithdrawalState::Failed { .. } => failed += 1,
            }
        }

        WithdrawalStats {
            pending,
            in_batch,
            submitted,
            finalized,
            failed,
            total_pending_amount: self.total_pending_amount(),
        }
    }

    /// Prune finalized withdrawals (memory cleanup)
    pub fn prune_finalized(&mut self) {
        let to_remove: Vec<[u8; 32]> = self
            .withdrawals
            .iter()
            .filter(|(_, w)| matches!(w.state, WithdrawalState::Finalized))
            .map(|(h, _)| *h)
            .collect();

        for hash in to_remove {
            if let Some(w) = self.withdrawals.remove(&hash) {
                // Remove from indexes
                if let Some(list) = self.by_destination.get_mut(&w.to_l1_address) {
                    list.retain(|h| h != &hash);
                }
                if let Some(list) = self.by_source.get_mut(&w.from) {
                    list.retain(|h| h != &hash);
                }
            }
        }
    }
}

/// Withdrawal statistics
#[derive(Debug, Clone)]
pub struct WithdrawalStats {
    pub pending: usize,
    pub in_batch: usize,
    pub submitted: usize,
    pub finalized: usize,
    pub failed: usize,
    pub total_pending_amount: u64,
}

// ============================================================================
// Withdrawal Merkle Tree (for L1 verification)
// ============================================================================

/// Build a merkle tree of withdrawals for L1 verification
pub fn build_withdrawal_merkle_root(withdrawals: &[TrackedWithdrawal]) -> [u8; 32] {
    if withdrawals.is_empty() {
        return [0u8; 32];
    }

    // Compute leaf hashes
    let mut leaves: Vec<[u8; 32]> = withdrawals
        .iter()
        .map(|w| {
            let mut hasher = blake3::Hasher::new();
            hasher.update(&w.tx_hash);
            hasher.update(&w.to_l1_address);
            hasher.update(&w.amount.to_le_bytes());
            *hasher.finalize().as_bytes()
        })
        .collect();

    // Pad to power of 2
    let target_len = leaves.len().next_power_of_two();
    while leaves.len() < target_len {
        leaves.push([0u8; 32]);
    }

    // Build tree
    while leaves.len() > 1 {
        let mut next_level = Vec::with_capacity(leaves.len() / 2);
        for chunk in leaves.chunks(2) {
            let mut hasher = blake3::Hasher::new();
            hasher.update(&chunk[0]);
            hasher.update(&chunk[1]);
            next_level.push(*hasher.finalize().as_bytes());
        }
        leaves = next_level;
    }

    leaves[0]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_pending(amount: u64) -> PendingWithdrawal {
        PendingWithdrawal {
            tx_hash: [amount as u8; 32],
            from: AccountId([1u8; 32]),
            to_l1_address: [2u8; 32],
            amount,
            l2_nonce: 0,
        }
    }

    #[test]
    fn test_withdrawal_state_transitions() {
        // Test state serialization
        let state = WithdrawalState::InBatch { batch_id: 42 };
        let json = serde_json::to_string(&state).unwrap();
        let parsed: WithdrawalState = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, state);
    }

    #[test]
    fn test_tracked_withdrawal_from_pending() {
        let pending = make_pending(1000);
        let tracked = TrackedWithdrawal::from(pending);
        assert_eq!(tracked.amount, 1000);
        assert!(matches!(tracked.state, WithdrawalState::Pending));
    }

    #[test]
    fn test_withdrawal_merkle_root_empty() {
        let root = build_withdrawal_merkle_root(&[]);
        assert_eq!(root, [0u8; 32]);
    }

    #[test]
    fn test_withdrawal_merkle_root_single() {
        let withdrawals = vec![TrackedWithdrawal::from(make_pending(1000))];
        let root = build_withdrawal_merkle_root(&withdrawals);
        assert_ne!(root, [0u8; 32]);
    }
}
