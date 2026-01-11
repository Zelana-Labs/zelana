//! Encrypted Transaction for Mempool
//!
//! Implements threshold-encrypted transactions that can only be
//! decrypted after ordering, preventing MEV and front-running.

use chacha20poly1305::{
    ChaCha20Poly1305, Nonce,
    aead::{Aead, KeyInit},
};
use rand::RngCore;
use serde::{Deserialize, Serialize};

use crate::committee::{Committee, EncryptedShare};
use crate::shares::{Share, ThresholdError, combine_shares, random_secret, split_secret};

/// An encrypted transaction for the mempool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedTransaction {
    /// Transaction ID (hash of encrypted content)
    pub tx_id: [u8; 32],
    /// Epoch when this was encrypted (for key rotation)
    pub epoch: u64,
    /// Nonce for symmetric encryption
    pub nonce: [u8; 12],
    /// Encrypted transaction data
    pub ciphertext: Vec<u8>,
    /// Encrypted shares for committee members
    pub encrypted_shares: Vec<EncryptedShare>,
    /// Timestamp when submitted (for ordering)
    pub timestamp: u64,
    /// Sender hint (optional, for fee payment tracking)
    pub sender_hint: Option<[u8; 32]>,
}

impl EncryptedTransaction {
    /// Get the transaction ID
    pub fn id(&self) -> &[u8; 32] {
        &self.tx_id
    }

    /// Compute tx_id from ciphertext
    fn compute_id(ciphertext: &[u8], nonce: &[u8; 12]) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        hasher.update(nonce);
        hasher.update(ciphertext);
        *hasher.finalize().as_bytes()
    }
}

/// Encrypt a transaction for the committee
///
/// # Arguments
/// * `plaintext` - The serialized transaction data
/// * `committee` - The threshold encryption committee
/// * `sender_hint` - Optional sender hint for fee tracking
///
/// # Returns
/// An encrypted transaction that requires K-of-N committee members to decrypt
pub fn encrypt_for_committee(
    plaintext: &[u8],
    committee: &Committee,
    sender_hint: Option<[u8; 32]>,
) -> Result<EncryptedTransaction, ThresholdError> {
    // Generate random symmetric key
    let symmetric_key = random_secret();

    // Split key into shares
    let shares = split_secret(
        &symmetric_key,
        committee.config.threshold,
        committee.config.total_members,
    )?;

    // Encrypt each share for its respective committee member
    let encrypted_shares: Vec<EncryptedShare> = shares
        .iter()
        .zip(committee.members.iter())
        .map(|(share, member)| EncryptedShare::encrypt(share, &member.public_key))
        .collect();

    // Generate nonce
    let mut nonce_bytes = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    // Encrypt transaction with symmetric key
    let cipher = ChaCha20Poly1305::new_from_slice(&symmetric_key)
        .map_err(|_| ThresholdError::DecryptionFailed)?;

    let ciphertext = cipher
        .encrypt(nonce, plaintext)
        .map_err(|_| ThresholdError::DecryptionFailed)?;

    // Compute tx_id
    let tx_id = EncryptedTransaction::compute_id(&ciphertext, &nonce_bytes);

    // Get current timestamp
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);

    Ok(EncryptedTransaction {
        tx_id,
        epoch: committee.config.epoch,
        nonce: nonce_bytes,
        ciphertext,
        encrypted_shares,
        timestamp,
        sender_hint,
    })
}

/// Decrypt a transaction using committee shares
///
/// # Arguments
/// * `encrypted_tx` - The encrypted transaction
/// * `shares` - At least K decrypted shares from committee members
/// * `threshold` - The threshold K
///
/// # Returns
/// The decrypted transaction plaintext
pub fn decrypt_transaction(
    encrypted_tx: &EncryptedTransaction,
    shares: &[Share],
    threshold: usize,
) -> Result<Vec<u8>, ThresholdError> {
    // Reconstruct symmetric key from shares
    let symmetric_key = combine_shares(shares, threshold)?;

    // Decrypt transaction
    let cipher = ChaCha20Poly1305::new_from_slice(&symmetric_key)
        .map_err(|_| ThresholdError::DecryptionFailed)?;

    let nonce = Nonce::from_slice(&encrypted_tx.nonce);

    cipher
        .decrypt(nonce, encrypted_tx.ciphertext.as_slice())
        .map_err(|_| ThresholdError::DecryptionFailed)
}

/// Ordered encrypted transaction (after sequencer ordering)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderedEncryptedTx {
    /// The encrypted transaction
    pub encrypted_tx: EncryptedTransaction,
    /// Assigned sequence number
    pub sequence: u64,
    /// Block/batch this transaction is assigned to
    pub batch_id: u64,
}

/// Encrypted mempool
#[derive(Debug, Default)]
pub struct EncryptedMempool {
    /// Pending encrypted transactions (not yet ordered)
    pending: Vec<EncryptedTransaction>,
    /// Ordered transactions (ready for committee decryption)
    ordered: Vec<OrderedEncryptedTx>,
    /// Next sequence number
    next_sequence: u64,
}

impl EncryptedMempool {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add an encrypted transaction to the pending pool
    pub fn add(&mut self, tx: EncryptedTransaction) {
        self.pending.push(tx);
    }

    /// Order pending transactions (called by sequencer)
    ///
    /// This assigns sequence numbers while transactions are still encrypted,
    /// preventing MEV extraction based on transaction content.
    pub fn order_pending(&mut self, batch_id: u64) -> Vec<OrderedEncryptedTx> {
        // Sort by timestamp (FIFO ordering)
        self.pending.sort_by_key(|tx| tx.timestamp);

        let ordered: Vec<OrderedEncryptedTx> = self
            .pending
            .drain(..)
            .map(|tx| {
                let seq = self.next_sequence;
                self.next_sequence += 1;
                OrderedEncryptedTx {
                    encrypted_tx: tx,
                    sequence: seq,
                    batch_id,
                }
            })
            .collect();

        self.ordered.extend(ordered.clone());
        ordered
    }

    /// Get ordered transactions for a batch
    pub fn get_batch(&self, batch_id: u64) -> Vec<&OrderedEncryptedTx> {
        self.ordered
            .iter()
            .filter(|tx| tx.batch_id == batch_id)
            .collect()
    }

    /// Clear ordered transactions after they've been processed
    pub fn clear_batch(&mut self, batch_id: u64) {
        self.ordered.retain(|tx| tx.batch_id != batch_id);
    }

    /// Get pending count
    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }

    /// Get ordered count
    pub fn ordered_count(&self) -> usize {
        self.ordered.len()
    }
}

/// Decryption coordinator
///
/// Collects shares from committee members and decrypts transactions
/// once threshold is reached.
pub struct DecryptionCoordinator {
    threshold: usize,
    collected_shares: std::collections::HashMap<[u8; 32], Vec<Share>>,
}

impl DecryptionCoordinator {
    pub fn new(threshold: usize) -> Self {
        Self {
            threshold,
            collected_shares: std::collections::HashMap::new(),
        }
    }

    /// Submit a share for a transaction
    pub fn submit_share(&mut self, tx_id: [u8; 32], share: Share) {
        self.collected_shares
            .entry(tx_id)
            .or_insert_with(Vec::new)
            .push(share);
    }

    /// Check if we have enough shares for a transaction
    pub fn can_decrypt(&self, tx_id: &[u8; 32]) -> bool {
        self.collected_shares
            .get(tx_id)
            .map(|shares| shares.len() >= self.threshold)
            .unwrap_or(false)
    }

    /// Decrypt a transaction if we have enough shares
    pub fn try_decrypt(
        &self,
        encrypted_tx: &EncryptedTransaction,
    ) -> Result<Vec<u8>, ThresholdError> {
        let shares = self.collected_shares.get(&encrypted_tx.tx_id).ok_or(
            ThresholdError::InsufficientShares {
                got: 0,
                need: self.threshold,
            },
        )?;

        decrypt_transaction(encrypted_tx, shares, self.threshold)
    }

    /// Get collected shares for a transaction
    pub fn shares_for(&self, tx_id: &[u8; 32]) -> Option<&Vec<Share>> {
        self.collected_shares.get(tx_id)
    }

    /// Clear shares for a transaction
    pub fn clear(&mut self, tx_id: &[u8; 32]) {
        self.collected_shares.remove(tx_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::committee::{CommitteeConfig, LocalCommitteeMember};

    fn setup_test_committee(
        threshold: usize,
        total: usize,
    ) -> (Committee, Vec<LocalCommitteeMember>) {
        let local_members: Vec<LocalCommitteeMember> = (1..=total as u8)
            .map(LocalCommitteeMember::generate)
            .collect();

        let members = local_members.iter().map(|m| m.to_member()).collect();
        let config = CommitteeConfig::new(threshold, total);
        let committee = Committee::new(config, members);

        (committee, local_members)
    }

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let (committee, local_members) = setup_test_committee(3, 5);

        let plaintext = b"Hello, threshold encryption!";
        let encrypted =
            encrypt_for_committee(plaintext, &committee, None).expect("encryption failed");

        // Collect shares from first 3 members (threshold)
        let shares: Vec<Share> = encrypted
            .encrypted_shares
            .iter()
            .take(3)
            .zip(local_members.iter().take(3))
            .map(|(enc_share, member)| {
                member
                    .decrypt_share(enc_share)
                    .expect("share decryption failed")
            })
            .collect();

        // Decrypt
        let decrypted = decrypt_transaction(&encrypted, &shares, 3).expect("decryption failed");

        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_insufficient_shares() {
        let (committee, local_members) = setup_test_committee(3, 5);

        let plaintext = b"Test data";
        let encrypted = encrypt_for_committee(plaintext, &committee, None).unwrap();

        // Only collect 2 shares (below threshold)
        let shares: Vec<Share> = encrypted
            .encrypted_shares
            .iter()
            .take(2)
            .zip(local_members.iter().take(2))
            .map(|(enc_share, member)| member.decrypt_share(enc_share).unwrap())
            .collect();

        let result = decrypt_transaction(&encrypted, &shares, 3);
        assert!(matches!(
            result,
            Err(ThresholdError::InsufficientShares { .. })
        ));
    }

    #[test]
    fn test_mempool_ordering() {
        let mut mempool = EncryptedMempool::new();
        let (committee, _) = setup_test_committee(2, 3);

        // Add some transactions
        for i in 0..5 {
            let plaintext = format!("tx{}", i);
            let tx = encrypt_for_committee(plaintext.as_bytes(), &committee, None).unwrap();
            mempool.add(tx);
        }

        assert_eq!(mempool.pending_count(), 5);

        // Order them
        let ordered = mempool.order_pending(1);
        assert_eq!(ordered.len(), 5);
        assert_eq!(mempool.pending_count(), 0);
        assert_eq!(mempool.ordered_count(), 5);

        // Check sequence numbers
        for (i, tx) in ordered.iter().enumerate() {
            assert_eq!(tx.sequence, i as u64);
            assert_eq!(tx.batch_id, 1);
        }
    }
}
