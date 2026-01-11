use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use wincode::{SchemaRead, SchemaWrite, serialize};
pub const TX_BLOB_VERSION_V1: u8 = 1;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, SchemaRead, SchemaWrite)]
pub struct EncryptedTxBlobV1 {
    pub version: u8,
    pub flags: u8,
    pub sender_hint: [u8; 32],
    pub nonce: [u8; 12],
    pub ciphertext: Vec<u8>,
    pub tag: [u8; 16],
}

pub fn tx_blob_hash(blob: &EncryptedTxBlobV1) -> [u8; 32] {
    let bytes = serialize(blob).expect("tx blob serialization failed");
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher.finalize().into()
}

/// sender_hint = H(signer_pubkey)
pub fn sender_hint_from_pubkey(pk: &[u8; 32]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(pk);
    h.finalize().into()
}
