use serde::{Serialize, Deserialize};
use sha2::{Digest, Sha256};
use zelana_account::AccountId;

#[derive(Clone,Copy,PartialEq,Eq,Hash,Debug,PartialOrd,Ord,Serialize,Deserialize)]
pub struct Pubkey(pub [u8;32]);

/// Helper struct to hold a user's full keypair set.
#[derive(Clone, Debug)]
pub struct PublicKeys {
    pub signer_pk: [u8; 32],  // Ed25519 Public Key
    pub privacy_pk: [u8; 32], // X25519 Public Key (for encryption)
}

impl PublicKeys {
    /// Deterministically derives the L2 Account ID.
    /// Formula: SHA256( signer_pk_bytes || privacy_pk_bytes )
    pub fn derive_id(&self) -> AccountId {
        let mut hasher = Sha256::new();
        hasher.update(&self.signer_pk);
        hasher.update(&self.privacy_pk);
        AccountId(hasher.finalize().into())
    }
}