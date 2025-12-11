use chacha20poly1305::aead::OsRng;
use ed25519_dalek::{Signer, SigningKey};
use x25519_dalek::{PublicKey as X25519PublicKey, StaticSecret};
use zelana_core::{
    identity::{AccountId, IdentityKeys},
    transaction::{SignedTransaction, TransactionData},
};
use zelana_transaction::{SignedTransaction, TransactionData}

/// A user's wallet containing private keys.
/// NEVER expose this struct's internals.
pub struct Keypair {
    signing_key: SigningKey,
    privacy_key: StaticSecret,
}

impl Keypair {
    /// Generates a fresh random wallet.
    pub fn new_random() -> Self {
        let signing_key = SigningKey::generate(&mut OsRng);
        let privacy_key = StaticSecret::random_from_rng(OsRng);

        Self {
            signing_key,
            privacy_key,
        }
    }

    /// Reconstructs a wallet from raw seed bytes (e.g., from a mnemonic).
    /// seed must be 64 bytes: 32 for signer + 32 for privacy.
    pub fn from_seed(seed: &[u8; 64]) -> Self {
        let sign_seed: [u8; 32] = seed[0..32].try_into().unwrap();
        let priv_seed: [u8; 32] = seed[32..64].try_into().unwrap();

        Self {
            signing_key: SigningKey::from_bytes(&sign_seed),
            privacy_key: StaticSecret::from(priv_seed),
        }
    }

    /// Returns the public Account ID (The "Address").
    pub fn account_id(&self) -> AccountId {
        self.public_keys().derive_id()
    }

    /// Returns the public key set (safe to share).
    pub fn public_keys(&self) -> IdentityKeys {
        IdentityKeys {
            signer_pk: self.signing_key.verifying_key().to_bytes(),
            privacy_pk: X25519PublicKey::from(&self.privacy_key).to_bytes(),
        }
    }

    /// Signs a transaction payload.
    /// This automatically attaches the signer's public key for the ZK Circuit.
    pub fn sign_transaction(&self, data: TransactionData) -> SignedTransaction {
        let msg = wincode::serialize(&data).expect("Serialization failed");

        // Sign the serialized bytes
        let signature = self.signing_key.sign(&msg).to_bytes().to_vec();

        SignedTransaction {
            data,
            signature,
            signer_pubkey: self.signing_key.verifying_key().to_bytes(),
        }
    }
}
