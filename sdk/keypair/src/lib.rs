use chacha20poly1305::aead::OsRng;
use chacha20poly1305::aead::rand_core::RngCore;
use ed25519_dalek::{Signer, SigningKey};
use solana_sdk::signature::Keypair as SolanaKeypair;
use x25519_dalek::{PublicKey as X25519PublicKey, StaticSecret};
use zelana_account::AccountId;
use zelana_pubkey::PublicKeys;
use zelana_transaction::{SignedTransaction, TransactionData};

/// A user's wallet containing private keys.
/// NEVER expose this struct's internals.
pub struct Keypair {
    signing_key: SigningKey,
    privacy_key: StaticSecret,
}

impl Keypair {
    /// Generates a fresh random wallet.
    pub fn new_random() -> Self {
        let mut rng = OsRng;

        // Ed25519 signing key (via raw bytes)
        let mut seed = [0u8; 32];
        rng.fill_bytes(&mut seed);
        let signing_key = SigningKey::from_bytes(&seed);
        let privacy_key = StaticSecret::random_from_rng(OsRng);

        Self {
            signing_key,
            privacy_key,
        }
    }

    pub fn solana_keypair(&self) -> SolanaKeypair {
        // SolanaKeypair::new_from_array expects just the 32-byte private key
        // It will automatically derive the public key
        let private_key_bytes = self.signing_key.to_bytes();
        SolanaKeypair::new_from_array(private_key_bytes)
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
    /// IMPORTANT: This must match the bridge's map_l1_to_l2 function!
    pub fn account_id(&self) -> AccountId {
        // Use ONLY the signing key to match L1 compatibility
        AccountId(self.signing_key.verifying_key().to_bytes())
    }
    /// Returns the public key set (safe to share).
    pub fn public_keys(&self) -> PublicKeys {
        PublicKeys {
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

    /// Signs a withdrawal request.
    /// The message format is: from || to_l1_address || amount (le) || nonce (le)
    pub fn sign_withdrawal(
        &self,
        to_l1_address: [u8; 32],
        amount: u64,
        nonce: u64,
    ) -> zelana_transaction::WithdrawRequest {
        // Build canonical message
        let mut msg = Vec::with_capacity(32 + 32 + 8 + 8);
        let from = self.account_id();
        msg.extend_from_slice(&from.0);
        msg.extend_from_slice(&to_l1_address);
        msg.extend_from_slice(&amount.to_le_bytes());
        msg.extend_from_slice(&nonce.to_le_bytes());

        // Sign
        let signature = self.signing_key.sign(&msg).to_bytes().to_vec();

        zelana_transaction::WithdrawRequest {
            from,
            to_l1_address,
            amount,
            nonce,
            signature,
            signer_pubkey: self.signing_key.verifying_key().to_bytes(),
        }
    }

    /// Exports the keypair as a 64-byte seed (for saving to file).
    /// ⚠️ SENSITIVE: Only use this for encrypted storage!
    pub fn to_seed(&self) -> [u8; 64] {
        let mut seed = [0u8; 64];
        seed[0..32].copy_from_slice(&self.signing_key.to_bytes());
        seed[32..64].copy_from_slice(&self.privacy_key.to_bytes());
        seed
    }

    /// Loads a keypair from a JSON file (Solana CLI format).
    pub fn from_file(path: &str) -> std::io::Result<Self> {
        let contents = std::fs::read_to_string(path)?;
        let bytes: Vec<u8> = serde_json::from_str(&contents)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        if bytes.len() != 64 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Invalid keypair file: expected 64 bytes",
            ));
        }

        let seed: [u8; 64] = bytes.try_into().unwrap();
        Ok(Self::from_seed(&seed))
    }
}
