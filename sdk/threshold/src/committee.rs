//! Committee Management
//!
//! Manages the threshold encryption committee for the encrypted mempool.

use rand::RngCore;
use serde::{Deserialize, Serialize};
use x25519_dalek::{PublicKey, StaticSecret};

use crate::shares::{Share, ShareId};

/// Committee configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitteeConfig {
    /// Threshold K: minimum members needed to decrypt
    pub threshold: usize,
    /// Total members N
    pub total_members: usize,
    /// Epoch number (for key rotation)
    pub epoch: u64,
}

impl CommitteeConfig {
    pub fn new(threshold: usize, total_members: usize) -> Self {
        Self {
            threshold,
            total_members,
            epoch: 0,
        }
    }

    /// Check if config is valid
    pub fn is_valid(&self) -> bool {
        self.threshold > 0 && self.threshold <= self.total_members && self.total_members <= 255
    }
}

/// A committee member
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitteeMember {
    /// Member ID (1-indexed)
    pub id: ShareId,
    /// Member's X25519 public key (for encrypted share delivery)
    pub public_key: [u8; 32],
    /// Optional endpoint URL for decryption requests
    pub endpoint: Option<String>,
}

impl CommitteeMember {
    pub fn new(id: ShareId, public_key: [u8; 32]) -> Self {
        Self {
            id,
            public_key,
            endpoint: None,
        }
    }

    pub fn with_endpoint(mut self, endpoint: String) -> Self {
        self.endpoint = Some(endpoint);
        self
    }
}

/// The threshold encryption committee
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Committee {
    /// Committee configuration
    pub config: CommitteeConfig,
    /// Committee members
    pub members: Vec<CommitteeMember>,
    /// Combined public key for encryption (optional, for DKG schemes)
    pub combined_pk: Option<[u8; 32]>,
}

impl Committee {
    /// Create a new committee from members
    pub fn new(config: CommitteeConfig, members: Vec<CommitteeMember>) -> Self {
        assert_eq!(members.len(), config.total_members);
        Self {
            config,
            members,
            combined_pk: None,
        }
    }

    /// Get member by ID
    pub fn member(&self, id: ShareId) -> Option<&CommitteeMember> {
        self.members.iter().find(|m| m.id == id)
    }

    /// Get all member public keys
    pub fn public_keys(&self) -> Vec<[u8; 32]> {
        self.members.iter().map(|m| m.public_key).collect()
    }

    /// Check if we have enough shares for decryption
    pub fn can_decrypt(&self, shares: &[Share]) -> bool {
        shares.len() >= self.config.threshold
    }
}

/// Committee member with secret key (for local member)
pub struct LocalCommitteeMember {
    pub id: ShareId,
    secret_key: StaticSecret,
    pub public_key: PublicKey,
}

impl Clone for LocalCommitteeMember {
    fn clone(&self) -> Self {
        // StaticSecret doesn't implement Clone, so we need to reconstruct from bytes
        // We can access the secret by doing ECDH with a known public key and deriving
        // For simplicity, we store and reconstruct
        let secret_bytes = self.secret_bytes();
        Self::from_secret(self.id, secret_bytes)
    }
}

impl LocalCommitteeMember {
    /// Generate a new random member
    pub fn generate(id: ShareId) -> Self {
        let mut rng = rand::thread_rng();
        let secret_key = StaticSecret::random_from_rng(&mut rng);
        let public_key = PublicKey::from(&secret_key);
        Self {
            id,
            secret_key,
            public_key,
        }
    }

    /// Create from existing secret key
    pub fn from_secret(id: ShareId, secret_bytes: [u8; 32]) -> Self {
        let secret_key = StaticSecret::from(secret_bytes);
        let public_key = PublicKey::from(&secret_key);
        Self {
            id,
            secret_key,
            public_key,
        }
    }

    /// Get the secret key bytes (for cloning/serialization)
    pub fn secret_bytes(&self) -> [u8; 32] {
        self.secret_key.to_bytes()
    }

    /// Get the public member info
    pub fn to_member(&self) -> CommitteeMember {
        CommitteeMember::new(self.id, *self.public_key.as_bytes())
    }

    /// Decrypt a share encrypted to this member
    pub fn decrypt_share(&self, encrypted_share: &EncryptedShare) -> Option<Share> {
        use chacha20poly1305::{
            ChaCha20Poly1305, Nonce,
            aead::{Aead, KeyInit},
        };

        // ECDH
        let sender_pk = PublicKey::from(encrypted_share.ephemeral_pk);
        let shared_secret = self.secret_key.diffie_hellman(&sender_pk);

        // Derive key
        let key = derive_share_key(shared_secret.as_bytes(), &encrypted_share.ephemeral_pk);

        // Decrypt
        let cipher = ChaCha20Poly1305::new_from_slice(&key).ok()?;
        let nonce = Nonce::from_slice(&encrypted_share.nonce);
        let plaintext = cipher
            .decrypt(nonce, encrypted_share.ciphertext.as_slice())
            .ok()?;

        if plaintext.len() != 32 {
            return None;
        }

        let mut value = [0u8; 32];
        value.copy_from_slice(&plaintext);

        Some(Share::new(self.id, value))
    }
}

/// An encrypted share for a committee member
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedShare {
    /// Target member ID
    pub member_id: ShareId,
    /// Ephemeral public key
    pub ephemeral_pk: [u8; 32],
    /// Nonce
    pub nonce: [u8; 12],
    /// Encrypted share value + tag
    pub ciphertext: Vec<u8>,
}

impl EncryptedShare {
    /// Encrypt a share for a specific member
    pub fn encrypt(share: &Share, member_pk: &[u8; 32]) -> Self {
        use chacha20poly1305::{
            ChaCha20Poly1305, Nonce,
            aead::{Aead, KeyInit},
        };
        use x25519_dalek::EphemeralSecret;

        // Generate ephemeral keypair
        let mut rng = rand::thread_rng();
        let ephemeral_secret = EphemeralSecret::random_from_rng(&mut rng);
        let ephemeral_pk = PublicKey::from(&ephemeral_secret);

        // ECDH
        let recipient_pk = PublicKey::from(*member_pk);
        let shared_secret = ephemeral_secret.diffie_hellman(&recipient_pk);

        // Derive encryption key
        let key = derive_share_key(shared_secret.as_bytes(), ephemeral_pk.as_bytes());

        // Generate nonce
        let mut nonce_bytes = [0u8; 12];
        rng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        // Encrypt
        let cipher = ChaCha20Poly1305::new_from_slice(&key).expect("valid key");
        let ciphertext = cipher
            .encrypt(nonce, share.value.as_slice())
            .expect("encryption should not fail");

        Self {
            member_id: share.id,
            ephemeral_pk: *ephemeral_pk.as_bytes(),
            nonce: nonce_bytes,
            ciphertext,
        }
    }
}

/// Derive encryption key for share transport
fn derive_share_key(shared_secret: &[u8], ephemeral_pk: &[u8]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new_derive_key("zelana-threshold-share-v1");
    hasher.update(shared_secret);
    hasher.update(ephemeral_pk);
    *hasher.finalize().as_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypted_share_roundtrip() {
        let member = LocalCommitteeMember::generate(1);
        let share = Share::new(1, [42u8; 32]);

        let encrypted = EncryptedShare::encrypt(&share, member.public_key.as_bytes());
        let decrypted = member.decrypt_share(&encrypted).expect("decryption failed");

        assert_eq!(decrypted.id, share.id);
        assert_eq!(decrypted.value, share.value);
    }

    #[test]
    fn test_committee_creation() {
        let members: Vec<CommitteeMember> = (1..=5)
            .map(|i| {
                let m = LocalCommitteeMember::generate(i);
                m.to_member()
            })
            .collect();

        let config = CommitteeConfig::new(3, 5);
        let committee = Committee::new(config, members);

        assert!(committee.config.is_valid());
        assert_eq!(committee.members.len(), 5);
    }
}
