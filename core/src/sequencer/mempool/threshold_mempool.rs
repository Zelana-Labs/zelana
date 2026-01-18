#![allow(dead_code)] // Future feature: MEV-resistant ordering
//! Threshold Encrypted Mempool Integration
//!
//! Integrates the threshold encryption SDK with the sequencer for MEV-resistant
//! transaction ordering.
//!
//! ```text
//! Transaction Flow:
//! ┌─────────────────────────────────────────────────────────────────┐
//! │ 1. Client encrypts tx to committee                              │
//! │ 2. Sequencer orders encrypted txs (blind ordering)              │
//! │ 3. At batch seal, committee decrypts in order                   │
//! │ 4. Decrypted txs execute in committed order                     │
//! └─────────────────────────────────────────────────────────────────┘
//! ```

use std::sync::Arc;

use anyhow::{Context, Result, bail};
use log::{debug, info};
use tokio::sync::Mutex;

use zelana_threshold::{
    Committee, CommitteeConfig, CommitteeMember, DecryptionCoordinator, EncryptedMempool,
    EncryptedTransaction, LocalCommitteeMember, OrderedEncryptedTx, Share,
};

/// Configuration for the encrypted mempool
#[derive(Debug, Clone)]
pub struct EncryptedMempoolConfig {
    /// Enable threshold encryption (if false, txs are processed normally)
    pub enabled: bool,
    /// Threshold K for decryption
    pub threshold: usize,
    /// Total committee members N
    pub total_members: usize,
    /// Maximum pending encrypted transactions
    pub max_pending: usize,
}

impl Default for EncryptedMempoolConfig {
    fn default() -> Self {
        Self {
            enabled: false, // Disabled by default for MVP
            threshold: 2,
            total_members: 3,
            max_pending: 1000,
        }
    }
}

/// State for managing the encrypted mempool
pub struct ThresholdMempoolState {
    config: EncryptedMempoolConfig,
    /// The encryption committee
    committee: Option<Committee>,
    /// Encrypted mempool
    mempool: EncryptedMempool,
    /// Decryption coordinator
    coordinator: DecryptionCoordinator,
    /// Local committee member (if this node is a committee member)
    local_member: Option<LocalCommitteeMember>,
}

impl ThresholdMempoolState {
    /// Create a new threshold mempool state
    pub fn new(config: EncryptedMempoolConfig) -> Self {
        let threshold = config.threshold;
        Self {
            config,
            committee: None,
            mempool: EncryptedMempool::new(),
            coordinator: DecryptionCoordinator::new(threshold),
            local_member: None,
        }
    }

    /// Initialize with a committee
    pub fn with_committee(mut self, committee: Committee) -> Self {
        self.committee = Some(committee);
        self
    }

    /// Set this node as a committee member
    pub fn with_local_member(mut self, member: LocalCommitteeMember) -> Self {
        self.local_member = Some(member);
        self
    }

    /// Check if threshold encryption is enabled and configured
    pub fn is_active(&self) -> bool {
        self.config.enabled && self.committee.is_some()
    }

    /// Get the committee (if configured)
    pub fn committee(&self) -> Option<&Committee> {
        self.committee.as_ref()
    }

    /// Add an encrypted transaction
    pub fn add_encrypted_tx(&mut self, tx: EncryptedTransaction) -> Result<()> {
        if self.mempool.pending_count() >= self.config.max_pending {
            bail!("Encrypted mempool full");
        }

        // Validate epoch
        if let Some(committee) = &self.committee {
            if tx.epoch != committee.config.epoch {
                bail!(
                    "Invalid epoch: expected {}, got {}",
                    committee.config.epoch,
                    tx.epoch
                );
            }
        }

        debug!("Added encrypted tx: {}", hex::encode(tx.tx_id));
        self.mempool.add(tx);
        Ok(())
    }

    /// Order pending transactions for a batch
    ///
    /// Called when sealing a batch. Returns ordered encrypted txs.
    pub fn order_for_batch(&mut self, batch_id: u64) -> Vec<OrderedEncryptedTx> {
        let ordered = self.mempool.order_pending(batch_id);
        info!(
            "Ordered {} encrypted txs for batch {}",
            ordered.len(),
            batch_id
        );
        ordered
    }

    /// Submit a decryption share from a committee member
    pub fn submit_share(&mut self, tx_id: [u8; 32], share: Share) {
        let share_id = share.id;
        self.coordinator.submit_share(tx_id, share);
        debug!("Share {} submitted for tx {}", share_id, hex::encode(tx_id));
    }

    /// Try to decrypt a transaction
    pub fn try_decrypt(&self, encrypted_tx: &EncryptedTransaction) -> Result<Vec<u8>> {
        self.coordinator
            .try_decrypt(encrypted_tx)
            .context("Decryption failed")
    }

    /// Check if we can decrypt a transaction
    pub fn can_decrypt(&self, tx_id: &[u8; 32]) -> bool {
        self.coordinator.can_decrypt(tx_id)
    }

    /// Decrypt our share if we're a committee member
    pub fn decrypt_local_share(&self, encrypted_tx: &EncryptedTransaction) -> Option<Share> {
        let local_member = self.local_member.as_ref()?;

        // Find our encrypted share
        let our_share = encrypted_tx
            .encrypted_shares
            .iter()
            .find(|s| s.member_id == local_member.id)?;

        local_member.decrypt_share(our_share)
    }

    /// Get pending count
    pub fn pending_count(&self) -> usize {
        self.mempool.pending_count()
    }

    /// Get ordered count
    pub fn ordered_count(&self) -> usize {
        self.mempool.ordered_count()
    }

    /// Clear processed batch
    pub fn clear_batch(&mut self, batch_id: u64) {
        self.mempool.clear_batch(batch_id);
    }
}

/// Thread-safe threshold mempool manager
pub struct ThresholdMempoolManager {
    inner: Arc<Mutex<ThresholdMempoolState>>,
}

impl ThresholdMempoolManager {
    /// Create a new manager
    pub fn new(config: EncryptedMempoolConfig) -> Self {
        Self {
            inner: Arc::new(Mutex::new(ThresholdMempoolState::new(config))),
        }
    }

    /// Initialize with committee
    pub async fn initialize_committee(&self, committee: Committee) {
        let mut state = self.inner.lock().await;
        state.committee = Some(committee);
        info!("Threshold mempool committee initialized");
    }

    /// Set local committee member
    pub async fn set_local_member(&self, member: LocalCommitteeMember) {
        let mut state = self.inner.lock().await;
        state.local_member = Some(member);
        info!("Local committee member configured");
    }

    /// Check if active
    pub async fn is_active(&self) -> bool {
        self.inner.lock().await.is_active()
    }

    /// Add encrypted transaction
    pub async fn add_encrypted_tx(&self, tx: EncryptedTransaction) -> Result<()> {
        self.inner.lock().await.add_encrypted_tx(tx)
    }

    /// Order for batch
    pub async fn order_for_batch(&self, batch_id: u64) -> Vec<OrderedEncryptedTx> {
        self.inner.lock().await.order_for_batch(batch_id)
    }

    /// Submit share
    pub async fn submit_share(&self, tx_id: [u8; 32], share: Share) {
        self.inner.lock().await.submit_share(tx_id, share);
    }

    /// Try decrypt
    pub async fn try_decrypt(&self, encrypted_tx: &EncryptedTransaction) -> Result<Vec<u8>> {
        self.inner.lock().await.try_decrypt(encrypted_tx)
    }

    /// Get our share if we're a committee member
    pub async fn decrypt_local_share(&self, encrypted_tx: &EncryptedTransaction) -> Option<Share> {
        self.inner.lock().await.decrypt_local_share(encrypted_tx)
    }

    /// Get pending count
    pub async fn pending_count(&self) -> usize {
        self.inner.lock().await.pending_count()
    }

    /// Get committee
    pub async fn committee(&self) -> Option<Committee> {
        self.inner.lock().await.committee.clone()
    }
}

/// Helper to create a test committee
pub fn create_test_committee(
    threshold: usize,
    total: usize,
) -> (Committee, Vec<LocalCommitteeMember>) {
    let local_members: Vec<LocalCommitteeMember> = (1..=total as u8)
        .map(LocalCommitteeMember::generate)
        .collect();

    let members: Vec<CommitteeMember> = local_members.iter().map(|m| m.to_member()).collect();
    let config = CommitteeConfig::new(threshold, total);
    let committee = Committee::new(config, members);

    (committee, local_members)
}

#[cfg(test)]
mod tests {
    use zelana_threshold::encrypt_for_committee;

    use super::*;

    #[tokio::test]
    async fn test_threshold_mempool_basic() {
        let config = EncryptedMempoolConfig {
            enabled: true,
            threshold: 2,
            total_members: 3,
            max_pending: 100,
        };

        let manager = ThresholdMempoolManager::new(config);
        let (committee, local_members) = create_test_committee(2, 3);

        manager.initialize_committee(committee.clone()).await;
        manager.set_local_member(local_members[0].clone()).await;

        assert!(manager.is_active().await);

        // Encrypt a transaction
        let plaintext = b"test transaction";
        let encrypted =
            encrypt_for_committee(plaintext, &committee, None).expect("encryption failed");

        // Add to mempool
        manager
            .add_encrypted_tx(encrypted.clone())
            .await
            .expect("should add");
        assert_eq!(manager.pending_count().await, 1);

        // Order for batch
        let ordered = manager.order_for_batch(1).await;
        assert_eq!(ordered.len(), 1);
        assert_eq!(manager.pending_count().await, 0);

        // Submit shares for decryption
        for (i, local_member) in local_members.iter().enumerate().take(2) {
            let share = local_member
                .decrypt_share(&encrypted.encrypted_shares[i])
                .expect("share decrypt failed");
            manager.submit_share(encrypted.tx_id, share).await;
        }

        // Decrypt
        let decrypted = manager
            .try_decrypt(&encrypted)
            .await
            .expect("decryption failed");
        assert_eq!(decrypted, plaintext);
    }
}
