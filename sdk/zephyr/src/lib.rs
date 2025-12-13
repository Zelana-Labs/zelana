pub mod client;
pub mod keys;
pub mod packet;
pub use keys::{EphemeralKeyPair, SessionKeys};

#[cfg(test)]
mod tests {
    use crate::keys::{EphemeralKeyPair, SessionKeys};

    #[test]
    fn test_handshake_derivation() {
        // 1. Simulate two parties
        let client_keys = EphemeralKeyPair::generate();
        let server_keys = EphemeralKeyPair::generate();

        let client_pub = *client_keys.pk.as_bytes();
        let server_pub = *server_keys.pk.as_bytes();

        // 2. Perform ECDH (X25519)
        let client_shared = client_keys.sk.diffie_hellman(&server_keys.pk).to_bytes();

        let server_shared = server_keys.sk.diffie_hellman(&client_keys.pk).to_bytes();

        assert_eq!(
            client_shared, server_shared,
            "ECDH shared secrets must match"
        );

        // 3. Derive Session Keys
        let mut client_session = SessionKeys::derive(client_shared, &client_pub, &server_pub);
        let mut server_session = SessionKeys::derive(server_shared, &client_pub, &server_pub);

        // 4. Test Encryption Loop
        let msg = b"Hello Zelana";
        let encrypted = client_session.encrypt(msg).unwrap();

        // Extract nonce (first 12 bytes) and ciphertext
        let nonce = &encrypted[0..12];
        let cipher = &encrypted[12..];

        let decrypted = server_session.decrypt(nonce, cipher).unwrap();
        assert_eq!(msg, decrypted.as_slice());
    }

    #[test]
    fn test_nonce_increment() {
        // Test that encryption changes every time even for same message
        let keys = EphemeralKeyPair::generate();
        let shared = [0u8; 32]; // Dummy shared
        let pk = *keys.pk.as_bytes();
        let mut session = SessionKeys::derive(shared, &pk, &pk);

        let msg = b"replay attack test";
        let c1 = session.encrypt(msg).unwrap();
        let c2 = session.encrypt(msg).unwrap();

        // The first 12 bytes (nonce) MUST be different
        assert_ne!(&c1[0..12], &c2[0..12]);
    }
}
