use chacha20poly1305::{
    ChaCha20Poly1305, Key, Nonce,
    aead::{Aead, KeyInit, rand_core::OsRng},
};
use hkdf::Hkdf;
use sha2::{Digest, Sha256};
use x25519_dalek::{EphemeralSecret, PublicKey};

/// A temporary keypair generated for every new connection session.
pub struct EphemeralKeyPair {
    pub sk: EphemeralSecret,
    pub pk: PublicKey,
}

impl EphemeralKeyPair {
    pub fn generate() -> Self {
        let sk = EphemeralSecret::random_from_rng(OsRng);
        let pk = PublicKey::from(&sk);
        Self { sk, pk }
    }
}

/// The established session state after a successful handshake.
pub struct SessionKeys {
    aead: ChaCha20Poly1305,
    base_iv: [u8; 12],
    /// We track the sequence number to prevent replay attacks
    tx_counter: u64,
    #[allow(dead_code)] // Reserved for future RX validation
    rx_counter: u64,
}

impl SessionKeys {
    /// Derives session keys from a Diffie-Hellman shared secret.
    /// salt = H(client_pk || server_pk)
    pub fn derive(shared_secret: [u8; 32], client_pk: &[u8; 32], server_pk: &[u8; 32]) -> Self {
        // 1. Compute Salt
        let mut hasher = Sha256::new();
        hasher.update(client_pk);
        hasher.update(server_pk);
        let salt = hasher.finalize();

        // 2. HKDF Expand
        let hk = Hkdf::<Sha256>::new(Some(&salt), &shared_secret);
        let mut okm = [0u8; 44]; // 32 bytes Key + 12 bytes IV
        hk.expand(b"zelana-v2-session", &mut okm)
            .expect("HKDF expansion failed");

        let key = Key::from_slice(&okm[0..32]);
        let iv: [u8; 12] = okm[32..44].try_into().unwrap();

        Self {
            aead: ChaCha20Poly1305::new(key),
            base_iv: iv,
            tx_counter: 0,
            rx_counter: 0,
        }
    }

    /// Encrypts a payload and increments the TX counter.
    /// Returns: [Nonce (12B) || Ciphertext]
    pub fn encrypt(&mut self, plaintext: &[u8]) -> anyhow::Result<Vec<u8>> {
        self.tx_counter += 1;
        let nonce = compute_nonce(&self.base_iv, self.tx_counter);

        let ciphertext = self
            .aead
            .encrypt(&nonce, plaintext)
            .map_err(|_| anyhow::anyhow!("Encryption failure"))?;

        // Prepend nonce for the receiver
        let mut output = Vec::with_capacity(12 + ciphertext.len());
        output.extend_from_slice(nonce.as_slice());
        output.extend_from_slice(&ciphertext);

        Ok(output)
    }

    /// Decrypts a payload given the nonce provided in the packet.
    /// Note:  verify the nonce > rx_counter.
    pub fn decrypt(&mut self, nonce_bytes: &[u8], ciphertext: &[u8]) -> anyhow::Result<Vec<u8>> {
        if nonce_bytes.len() != 12 {
            return Err(anyhow::anyhow!("Invalid nonce length"));
        }
        let nonce = Nonce::from_slice(nonce_bytes);

        let plaintext = self
            .aead
            .decrypt(nonce, ciphertext)
            .map_err(|_| anyhow::anyhow!("Decryption failure (Bad Key or Mac)"))?;

        Ok(plaintext)
    }
}

/// XOR-based counter nonce generation (WireGuard style).
fn compute_nonce(base_iv: &[u8; 12], counter: u64) -> Nonce {
    let mut n = *base_iv;
    let c = counter.to_be_bytes();
    // XOR the counter into the last 8 bytes of the IV
    for i in 0..8 {
        n[11 - i] ^= c[7 - i];
    }
    *Nonce::from_slice(&n)
}
