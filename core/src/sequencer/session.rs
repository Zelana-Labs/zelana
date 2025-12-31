use chacha20poly1305::{
    ChaCha20Poly1305, Key, Nonce,
    aead::{Aead, KeyInit},
};
use dashmap::DashMap;
use hkdf::Hkdf;
use sha2::{Digest, Sha256};
use std::net::SocketAddr;
use zelana_account::AccountId;

/// The established session state after a successful handshake.

pub struct SessionKeys {
    aead: ChaCha20Poly1305,
    base_iv: [u8; 12],
    /// We track the sequence number to prevent replay attacks
    pub tx_counter: u64,
    pub rx_counter: u64,
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

impl std::fmt::Debug for SessionKeys {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionKeys")
            .field("base_iv", &self.base_iv)
            .field("tx_counter", &self.tx_counter)
            .field("rx_counter", &self.rx_counter)
            .finish_non_exhaustive() // Hides the aead field
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

/// Manages active secure sessions for connected clients.
pub struct SessionManager {
    /// Maps IP:Port -> Encryption Keys
    sessions: DashMap<SocketAddr, ActiveSession>,
}

#[derive(Debug)]
pub struct ActiveSession {
    pub keys: SessionKeys,
    pub account_id: Option<AccountId>, // Known after first valid signature
    pub last_activity: std::time::Instant,
}

impl SessionManager {
    pub fn new() -> Self {
        Self {
            sessions: DashMap::new(),
        }
    }

    pub fn sessions(&self) -> &DashMap<SocketAddr, ActiveSession> {
        &self.sessions
    }

    pub fn insert(&self, addr: SocketAddr, keys: SessionKeys) {
        self.sessions.insert(
            addr,
            ActiveSession {
                keys,
                account_id: None,
                last_activity: std::time::Instant::now(),
            },
        );
    }

    pub fn get_mut<F, R>(&self, addr: &SocketAddr, f: F) -> Option<R>
    where
        F: FnOnce(&mut ActiveSession) -> R,
    {
        self.sessions.get_mut(addr).map(|mut entry| f(&mut entry))
    }

    pub fn remove(&self, addr: &SocketAddr) {
        self.sessions.remove(addr);
    }

    /// Remove sessions that do not satisfy the predicate
    pub fn retain<F>(&self, mut f: F)
    where
        F: FnMut(&SocketAddr, &ActiveSession) -> bool,
    {
        // Collect keys to remove (avoid deadlock from removing during iteration)
        let keys_to_remove: Vec<SocketAddr> = self
            .sessions
            .iter()
            .filter_map(|entry| {
                let key = *entry.key();
                let session = entry.value();
                if !f(&key, &session) { Some(key) } else { None }
            })
            .collect();

        // Remove collected keys
        for key in keys_to_remove {
            self.sessions.remove(&key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chacha20poly1305::{
        ChaCha20Poly1305, Key, Nonce,
        aead::{Aead, KeyInit},
    };
    use dashmap::DashMap;
    use hkdf::Hkdf;
    use sha2::{Digest, Sha256};
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
    use zelana_account::AccountId;

    #[test]
    fn test_key_derivation_deterministic() {
        let secret = [42u8; 32];
        let client_pk = [1u8; 32];
        let server_pk = [2u8; 32];

        let keys1 = SessionKeys::derive(secret, &client_pk, &server_pk);
        let keys2 = SessionKeys::derive(secret, &client_pk, &server_pk);

        // Same inputs should produce same base_iv
        assert_eq!(keys1.base_iv, keys2.base_iv);
    }

    #[test]
    fn test_key_derivation_different_inputs() {
        let secret = [42u8; 32];
        let client_pk1 = [1u8; 32];
        let client_pk2 = [3u8; 32];
        let server_pk = [2u8; 32];

        let keys1 = SessionKeys::derive(secret, &client_pk1, &server_pk);
        let keys2 = SessionKeys::derive(secret, &client_pk2, &server_pk);

        // Different client keys should produce different IVs
        assert_ne!(keys1.base_iv, keys2.base_iv);
    }

    #[test]
    fn test_encrypt_decrypt_round_trip() {
        let secret = [42u8; 32];
        let client_pk = [1u8; 32];
        let server_pk = [2u8; 32];

        let mut keys = SessionKeys::derive(secret, &client_pk, &server_pk);
        let plaintext = b"Hello, Zelana!";

        let encrypted = keys.encrypt(plaintext).expect("Encryption failed");

        // Extract nonce and ciphertext
        let (nonce, ciphertext) = encrypted.split_at(12);

        let decrypted = keys.decrypt(nonce, ciphertext).expect("Decryption failed");

        assert_eq!(plaintext, &decrypted[..]);
    }

    #[test]
    fn test_encrypt_increments_counter() {
        let secret = [42u8; 32];
        let client_pk = [1u8; 32];
        let server_pk = [2u8; 32];

        let mut keys = SessionKeys::derive(secret, &client_pk, &server_pk);

        assert_eq!(keys.tx_counter, 0);

        keys.encrypt(b"message 1").expect("Encryption failed");
        assert_eq!(keys.tx_counter, 1);

        keys.encrypt(b"message 2").expect("Encryption failed");
        assert_eq!(keys.tx_counter, 2);
    }

    #[test]
    fn test_decrypt_with_wrong_key_fails() {
        let secret1 = [42u8; 32];
        let secret2 = [43u8; 32];
        let client_pk = [1u8; 32];
        let server_pk = [2u8; 32];

        let mut keys1 = SessionKeys::derive(secret1, &client_pk, &server_pk);
        let mut keys2 = SessionKeys::derive(secret2, &client_pk, &server_pk);

        let plaintext = b"Secret message";
        let encrypted = keys1.encrypt(plaintext).expect("Encryption failed");

        let (nonce, ciphertext) = encrypted.split_at(12);

        // Attempting to decrypt with wrong key should fail
        let result = keys2.decrypt(nonce, ciphertext);
        assert!(result.is_err());
    }

    #[test]
    fn test_tampered_ciphertext_fails() {
        let secret = [42u8; 32];
        let client_pk = [1u8; 32];
        let server_pk = [2u8; 32];

        let mut keys = SessionKeys::derive(secret, &client_pk, &server_pk);
        let plaintext = b"Original message";

        let encrypted = keys.encrypt(plaintext).expect("Encryption failed");
        let (nonce, mut ciphertext) = encrypted.split_at(12);

        // Tamper with the ciphertext
        let mut tampered = ciphertext.to_vec();
        if !tampered.is_empty() {
            tampered[0] ^= 0xFF;
        }

        let result = keys.decrypt(nonce, &tampered);
        assert!(result.is_err());
    }

    #[test]
    fn test_multiple_messages_different_nonces() {
        let secret = [42u8; 32];
        let client_pk = [1u8; 32];
        let server_pk = [2u8; 32];

        let mut keys = SessionKeys::derive(secret, &client_pk, &server_pk);

        let msg1 = keys.encrypt(b"message 1").expect("Encryption failed");
        let msg2 = keys.encrypt(b"message 2").expect("Encryption failed");
        let msg3 = keys.encrypt(b"message 3").expect("Encryption failed");

        // First 12 bytes are nonces - they should all be different
        assert_ne!(&msg1[..12], &msg2[..12]);
        assert_ne!(&msg2[..12], &msg3[..12]);
        assert_ne!(&msg1[..12], &msg3[..12]);
    }

    #[test]
    fn test_session_manager_insert_and_get() {
        let manager = SessionManager::new();
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);

        let secret = [42u8; 32];
        let client_pk = [1u8; 32];
        let server_pk = [2u8; 32];
        let keys = SessionKeys::derive(secret, &client_pk, &server_pk);

        manager.insert(addr, keys);

        let result = manager.get_mut(&addr, |session| session.keys.tx_counter);

        assert_eq!(result, Some(0));
    }

    #[test]
    fn test_session_manager_remove() {
        let manager = SessionManager::new();
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);

        let secret = [42u8; 32];
        let client_pk = [1u8; 32];
        let server_pk = [2u8; 32];
        let keys = SessionKeys::derive(secret, &client_pk, &server_pk);

        manager.insert(addr, keys);
        assert!(manager.sessions.contains_key(&addr));

        manager.remove(&addr);
        assert!(!manager.sessions.contains_key(&addr));
    }

    #[test]
    fn test_session_manager_multiple_sessions() {
        let manager = SessionManager::new();
        let addr1 = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
        let addr2 = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8081);

        let secret = [42u8; 32];
        let client_pk = [1u8; 32];
        let server_pk = [2u8; 32];

        let keys1 = SessionKeys::derive(secret, &client_pk, &server_pk);
        let keys2 = SessionKeys::derive(secret, &client_pk, &server_pk);

        manager.insert(addr1, keys1);
        manager.insert(addr2, keys2);

        assert!(manager.sessions.contains_key(&addr1));
        assert!(manager.sessions.contains_key(&addr2));
        assert_eq!(manager.sessions.len(), 2);
    }

    #[test]
    fn test_session_manager_encrypt_through_manager() {
        let manager = SessionManager::new();
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);

        let secret = [42u8; 32];
        let client_pk = [1u8; 32];
        let server_pk = [2u8; 32];
        let keys = SessionKeys::derive(secret, &client_pk, &server_pk);

        manager.insert(addr, keys);

        let plaintext = b"Test message";
        let encrypted = manager.get_mut(&addr, |session| session.keys.encrypt(plaintext));

        assert!(encrypted.is_some());
        assert!(encrypted.unwrap().is_ok());
    }

    #[test]
    fn test_session_manager_retain_keeps_recent_sessions() {
        use std::time::Duration;

        let manager = SessionManager::new();
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);

        let secret = [42u8; 32];
        let client_pk = [1u8; 32];
        let server_pk = [2u8; 32];

        // Create a fresh session (just created, so it's recent)
        manager.insert(addr, SessionKeys::derive(secret, &client_pk, &server_pk));
        assert!(manager.sessions.contains_key(&addr));

        // Run cleanup: remove sessions older than 10 seconds
        // Our session is fresh (< 1ms old), so it should be KEPT
        let timeout = Duration::from_secs(10);
        let now = std::time::Instant::now();
        manager.retain(|_, session| now.duration_since(session.last_activity) < timeout);

        // Session should still exist because it's recent
        assert!(manager.sessions.contains_key(&addr));
    }

    #[test]
    fn test_session_manager_retain_removes_old_sessions() {
        use std::time::Duration;

        let manager = SessionManager::new();
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);

        let secret = [42u8; 32];
        let client_pk = [1u8; 32];
        let server_pk = [2u8; 32];

        // Create a session
        manager.insert(addr, SessionKeys::derive(secret, &client_pk, &server_pk));
        assert!(manager.sessions.contains_key(&addr));

        // Sleep to make the session "old"
        std::thread::sleep(Duration::from_millis(100));

        // Run cleanup: remove sessions older than 50ms
        // Our session is 100ms old, so it should be REMOVED
        let timeout = Duration::from_millis(50);
        let now = std::time::Instant::now();
        manager.retain(|_, session| now.duration_since(session.last_activity) < timeout);

        // Session should be removed because it's too old
        assert!(!manager.sessions.contains_key(&addr));
    }

    #[test]
    fn test_large_payload_encryption() {
        let secret = [42u8; 32];
        let client_pk = [1u8; 32];
        let server_pk = [2u8; 32];

        let mut keys = SessionKeys::derive(secret, &client_pk, &server_pk);

        // Test with a large payload (10KB)
        let plaintext = vec![0x55u8; 10240];

        let encrypted = keys.encrypt(&plaintext).expect("Encryption failed");
        let (nonce, ciphertext) = encrypted.split_at(12);
        let decrypted = keys.decrypt(nonce, ciphertext).expect("Decryption failed");

        assert_eq!(plaintext, decrypted);
    }

    #[test]
    fn test_empty_payload() {
        let secret = [42u8; 32];
        let client_pk = [1u8; 32];
        let server_pk = [2u8; 32];

        let mut keys = SessionKeys::derive(secret, &client_pk, &server_pk);
        let plaintext = b"";

        let encrypted = keys.encrypt(plaintext).expect("Encryption failed");
        let (nonce, ciphertext) = encrypted.split_at(12);
        let decrypted = keys.decrypt(nonce, ciphertext).expect("Decryption failed");

        assert_eq!(plaintext, &decrypted[..]);
    }

    #[test]
    fn test_invalid_nonce_length() {
        let secret = [42u8; 32];
        let client_pk = [1u8; 32];
        let server_pk = [2u8; 32];

        let mut keys = SessionKeys::derive(secret, &client_pk, &server_pk);

        // Try to decrypt with wrong nonce length
        let result = keys.decrypt(&[0u8; 10], &[0u8; 16]);
        assert!(result.is_err());
    }
}
