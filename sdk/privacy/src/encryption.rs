//! Note Encryption
//!
//! Encrypts note data for the recipient using ECDH + ChaCha20-Poly1305.
//!
//! ```text
//! Flow:
//! 1. Sender generates ephemeral keypair (epk, esk)
//! 2. Shared secret = ECDH(esk, recipient_pk)
//! 3. Encryption key = HKDF(shared_secret, "zelana-note-v1")
//! 4. Ciphertext = ChaCha20-Poly1305(key, nonce, plaintext)
//! 5. Output = (epk, nonce, ciphertext, tag)
//! ```

use chacha20poly1305::{
    ChaCha20Poly1305, Nonce,
    aead::{Aead, KeyInit},
};
use serde::{Deserialize, Serialize};
use x25519_dalek::{EphemeralSecret, PublicKey, StaticSecret};

use crate::note::Note;

/// An encrypted note (sent on-chain)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedNote {
    /// Ephemeral public key for ECDH
    pub ephemeral_pk: [u8; 32],
    /// Nonce for ChaCha20-Poly1305
    pub nonce: [u8; 12],
    /// Encrypted note data with authentication tag
    pub ciphertext: Vec<u8>,
}

/// Note plaintext format for encryption
#[derive(Debug, Clone, Serialize, Deserialize)]
struct NotePlaintext {
    /// Note value
    value: u64,
    /// Randomness (blinding factor)
    randomness: [u8; 32],
    /// Optional memo (up to 512 bytes)
    memo: Vec<u8>,
}

impl EncryptedNote {
    /// Size of encrypted note (fixed overhead)
    pub const OVERHEAD: usize = 32 + 12 + 16; // epk + nonce + tag

    /// Get the ephemeral public key
    pub fn ephemeral_public_key(&self) -> &[u8; 32] {
        &self.ephemeral_pk
    }
}

/// Encrypt a note for a recipient
///
/// # Arguments
/// * `note` - The note to encrypt
/// * `recipient_pk` - Recipient's X25519 public key
/// * `memo` - Optional memo (max 512 bytes)
///
/// # Returns
/// Encrypted note that can be published on-chain
pub fn encrypt_note(note: &Note, recipient_pk: &[u8; 32], memo: Option<&[u8]>) -> EncryptedNote {
    // Generate ephemeral keypair using thread_rng which is compatible with x25519-dalek
    let mut rng = rand::thread_rng();
    let ephemeral_secret = EphemeralSecret::random_from_rng(&mut rng);
    let ephemeral_pk = PublicKey::from(&ephemeral_secret);

    // ECDH shared secret
    let recipient_key = PublicKey::from(*recipient_pk);
    let shared_secret = ephemeral_secret.diffie_hellman(&recipient_key);

    // Derive encryption key using HKDF
    let encryption_key = derive_note_key(shared_secret.as_bytes(), ephemeral_pk.as_bytes());

    // Create plaintext
    let plaintext = NotePlaintext {
        value: note.value.0,
        randomness: note.randomness,
        memo: memo
            .map(|m| m[..m.len().min(512)].to_vec())
            .unwrap_or_default(),
    };

    let plaintext_bytes = serialize_plaintext(&plaintext);

    // Generate random nonce
    let mut nonce_bytes = [0u8; 12];
    use rand::RngCore;
    rng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    // Encrypt with ChaCha20-Poly1305
    let cipher = ChaCha20Poly1305::new_from_slice(&encryption_key).expect("valid key length");

    let ciphertext = cipher
        .encrypt(nonce, plaintext_bytes.as_slice())
        .expect("encryption should not fail");

    EncryptedNote {
        ephemeral_pk: *ephemeral_pk.as_bytes(),
        nonce: nonce_bytes,
        ciphertext,
    }
}

/// Decrypt a note using recipient's secret key
///
/// # Arguments
/// * `encrypted` - The encrypted note
/// * `recipient_sk` - Recipient's X25519 secret key
/// * `expected_owner_pk` - The expected owner public key (for note reconstruction)
///
/// # Returns
/// The decrypted note, or None if decryption fails
pub fn decrypt_note(
    encrypted: &EncryptedNote,
    recipient_sk: &[u8; 32],
    expected_owner_pk: [u8; 32],
) -> Option<(Note, Vec<u8>)> {
    // Reconstruct shared secret
    let secret = StaticSecret::from(*recipient_sk);
    let ephemeral_pk = PublicKey::from(encrypted.ephemeral_pk);
    let shared_secret = secret.diffie_hellman(&ephemeral_pk);

    // Derive decryption key
    let decryption_key = derive_note_key(shared_secret.as_bytes(), &encrypted.ephemeral_pk);

    // Decrypt
    let cipher = ChaCha20Poly1305::new_from_slice(&decryption_key).ok()?;
    let nonce = Nonce::from_slice(&encrypted.nonce);

    let plaintext_bytes = cipher
        .decrypt(nonce, encrypted.ciphertext.as_slice())
        .ok()?;

    // Deserialize
    let plaintext = deserialize_plaintext(&plaintext_bytes)?;

    // Reconstruct note
    let note = Note::with_randomness(plaintext.value, expected_owner_pk, plaintext.randomness);

    Some((note, plaintext.memo))
}

/// Try to decrypt a note (scan mode - for wallet scanning)
///
/// Returns the note if decryption succeeds and commitment matches
pub fn try_decrypt_note(
    encrypted: &EncryptedNote,
    recipient_sk: &[u8; 32],
    expected_owner_pk: [u8; 32],
    expected_commitment: &[u8; 32],
) -> Option<(Note, Vec<u8>)> {
    let (note, memo) = decrypt_note(encrypted, recipient_sk, expected_owner_pk)?;

    // Verify commitment matches
    let computed_commitment = note.commitment();
    if computed_commitment.as_bytes() == expected_commitment {
        Some((note, memo))
    } else {
        None
    }
}

/// Derive encryption key from shared secret
fn derive_note_key(shared_secret: &[u8], ephemeral_pk: &[u8]) -> [u8; 32] {
    // HKDF using blake3
    let mut hasher = blake3::Hasher::new_derive_key("zelana-note-v1");
    hasher.update(shared_secret);
    hasher.update(ephemeral_pk);
    *hasher.finalize().as_bytes()
}

/// Serialize plaintext for encryption
fn serialize_plaintext(pt: &NotePlaintext) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(8 + 32 + 2 + pt.memo.len());

    // Value (8 bytes, little-endian)
    bytes.extend_from_slice(&pt.value.to_le_bytes());

    // Randomness (32 bytes)
    bytes.extend_from_slice(&pt.randomness);

    // Memo length (2 bytes) + memo
    let memo_len = pt.memo.len() as u16;
    bytes.extend_from_slice(&memo_len.to_le_bytes());
    bytes.extend_from_slice(&pt.memo);

    bytes
}

/// Deserialize plaintext after decryption
fn deserialize_plaintext(bytes: &[u8]) -> Option<NotePlaintext> {
    if bytes.len() < 42 {
        return None; // 8 + 32 + 2 minimum
    }

    let value = u64::from_le_bytes(bytes[0..8].try_into().ok()?);
    let randomness: [u8; 32] = bytes[8..40].try_into().ok()?;
    let memo_len = u16::from_le_bytes(bytes[40..42].try_into().ok()?) as usize;

    if bytes.len() < 42 + memo_len {
        return None;
    }

    let memo = bytes[42..42 + memo_len].to_vec();

    Some(NotePlaintext {
        value,
        randomness,
        memo,
    })
}

/// Batch encryption for multiple outputs
pub fn encrypt_notes(
    notes: &[(Note, [u8; 32], Option<Vec<u8>>)], // (note, recipient_pk, memo)
) -> Vec<EncryptedNote> {
    notes
        .iter()
        .map(|(note, recipient_pk, memo)| encrypt_note(note, recipient_pk, memo.as_deref()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn generate_keypair() -> ([u8; 32], [u8; 32]) {
        let mut rng = rand::thread_rng();
        let secret = StaticSecret::random_from_rng(&mut rng);
        let public = PublicKey::from(&secret);
        (*secret.as_bytes(), *public.as_bytes())
    }

    #[test]
    fn test_encrypt_decrypt_note() {
        let (recipient_sk, recipient_pk) = generate_keypair();

        let note = Note::with_randomness(1000, recipient_pk, [42u8; 32]);
        let memo = b"test memo";

        let encrypted = encrypt_note(&note, &recipient_pk, Some(memo));
        let (decrypted, decrypted_memo) = decrypt_note(&encrypted, &recipient_sk, recipient_pk)
            .expect("decryption should succeed");

        assert_eq!(decrypted.value.0, note.value.0);
        assert_eq!(decrypted.randomness, note.randomness);
        assert_eq!(decrypted_memo, memo);
    }

    #[test]
    fn test_wrong_key_fails() {
        let (_, recipient_pk) = generate_keypair();
        let (wrong_sk, _) = generate_keypair();

        let note = Note::with_randomness(1000, recipient_pk, [42u8; 32]);
        let encrypted = encrypt_note(&note, &recipient_pk, None);

        let result = decrypt_note(&encrypted, &wrong_sk, recipient_pk);
        assert!(result.is_none(), "wrong key should fail decryption");
    }

    #[test]
    fn test_commitment_verification() {
        let (recipient_sk, recipient_pk) = generate_keypair();

        let note = Note::with_randomness(1000, recipient_pk, [42u8; 32]);
        let commitment = note.commitment();

        let encrypted = encrypt_note(&note, &recipient_pk, None);

        // Should succeed with correct commitment
        let result = try_decrypt_note(
            &encrypted,
            &recipient_sk,
            recipient_pk,
            commitment.as_bytes(),
        );
        assert!(result.is_some());

        // Should fail with wrong commitment
        let wrong_commitment = [0u8; 32];
        let result = try_decrypt_note(&encrypted, &recipient_sk, recipient_pk, &wrong_commitment);
        assert!(result.is_none());
    }
}
