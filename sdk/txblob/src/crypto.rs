use chacha20poly1305::{
    ChaCha20Poly1305,
    aead::{Aead, KeyInit, Payload},
};
use hkdf::Hkdf;
use sha2::Sha256;
// use rand_core::{OsRng, RngCore};
use chacha20poly1305::aead::rand_core::{OsRng, RngCore};
use wincode::{SchemaRead, SchemaWrite};
use x25519_dalek::{PublicKey, StaticSecret};

use crate::types::{EncryptedTxBlobV1, TX_BLOB_VERSION_V1, sender_hint_from_pubkey};
use zelana_transaction::SignedTransaction;

#[derive(Debug, SchemaRead, SchemaWrite)]
pub enum TxBlobError {
    DecryptionFailed,
    DeserializationFailed,
    EncryptionFailed,
    SerializationFailed,
}

fn derive_aead_key(my_secret: &StaticSecret, their_pub: &PublicKey) -> [u8; 32] {
    let shared = my_secret.diffie_hellman(their_pub);
    let hk = Hkdf::<Sha256>::new(None, shared.as_bytes());

    let mut key = [0u8; 32];
    hk.expand(b"zelana-tx-v1", &mut key)
        .expect("HKDF expand failed");
    key
}

pub fn encrypt_signed_tx(
    signed_tx: &SignedTransaction,
    sender_pubkey: &[u8; 32],
    client_secret: &StaticSecret,
    sequencer_pub: &PublicKey,
    flags: u8,
) -> Result<EncryptedTxBlobV1, TxBlobError> {
    let plaintext = wincode::serialize(signed_tx).map_err(|_| TxBlobError::SerializationFailed)?;

    let sender_hint = sender_hint_from_pubkey(sender_pubkey);

    let mut rng = OsRng;
    let mut nonce = [0u8; 12];
    rng.fill_bytes(&mut nonce);

    let key = derive_aead_key(client_secret, sequencer_pub);
    let cipher = ChaCha20Poly1305::new(&key.into());

    let mut aad = [0u8; 34];
    aad[0] = TX_BLOB_VERSION_V1;
    aad[1] = flags;
    aad[2..].copy_from_slice(&sender_hint);

    let encrypted = cipher
        .encrypt(
            &nonce.into(),
            Payload {
                msg: &plaintext,
                aad: &aad,
            },
        )
        .map_err(|_| TxBlobError::EncryptionFailed)?;

    let split = encrypted.len() - 16;
    let (ciphertext, tag) = encrypted.split_at(split);

    let encrypted = EncryptedTxBlobV1 {
        version: TX_BLOB_VERSION_V1,
        flags,
        sender_hint,
        nonce,
        ciphertext: ciphertext.to_vec(),
        tag: tag.try_into().unwrap(),
    };
    Ok(encrypted)
}

pub fn decrypt_signed_tx(
    blob: &EncryptedTxBlobV1,
    sequencer_secret: &StaticSecret,
    client_pub: &PublicKey,
) -> Result<SignedTransaction, TxBlobError> {
    let key = derive_aead_key(sequencer_secret, client_pub);
    let cipher = ChaCha20Poly1305::new(&key.into());

    let mut aad = [0u8; 34];
    aad[0] = blob.version;
    aad[1] = blob.flags;
    aad[2..].copy_from_slice(&blob.sender_hint);

    let mut combined = blob.ciphertext.clone();
    combined.extend_from_slice(&blob.tag);

    let plaintext = cipher
        .decrypt(
            &blob.nonce.into(),
            Payload {
                msg: &combined,
                aad: &aad,
            },
        )
        .map_err(|_| TxBlobError::DecryptionFailed)?;

    wincode::deserialize(&plaintext).map_err(|_| TxBlobError::DeserializationFailed)
}
