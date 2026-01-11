use rocksdb::{ColumnFamily, WriteBatch};
use serde::{Deserialize, Serialize};
use wincode::{SchemaRead, SchemaWrite};
use zelana_account::AccountId;
use zelana_pubkey::Pubkey;
use zelana_signature::Signature;
pub mod bridge;
pub use bridge::{DepositEvent, DepositParams, InitParams, WithdrawRequest};

/// The enum for all inputs to the L2 State Machine.
#[derive(Debug, Clone, SchemaRead, SchemaWrite, Serialize, Deserialize)]
pub enum TransactionType {
    /// PRIVACY: An opaque shielded transaction (The Blob).
    /// Sender/Receiver are hidden. Validity is proven via ZK.
    Shielded(PrivateTransaction),
    /// A standard transfer or interaction submitted by a user via UDP.
    Transfer(SignedTransaction),

    /// A deposit event detected on L1 (Solana) and bridged to L2.
    Deposit(DepositEvent),

    /// A withdrawal request to move funds back to L1.
    Withdraw(WithdrawRequest),
}

/// The Opaque Blob for Privacy
#[derive(Debug, Clone, Serialize, Deserialize, SchemaRead, SchemaWrite)]
pub struct PrivateTransaction {
    /// The ZK Proof (Groth16 bytes) attesting validity.
    pub proof: Vec<u8>,
    /// The unique tag preventing double-spends.
    pub nullifier: [u8; 32],
    /// The new note created (Encrypted Hash).
    pub commitment: [u8; 32],
    /// The encrypted data for the recipient to decrypt.
    pub ciphertext: Vec<u8>,
    /// Optional: Ephemeral public key for ECDH shared secret derivation.
    pub ephemeral_key: [u8; 32],
}
/// The Wrapper Structure
#[derive(Clone, Debug, SchemaWrite, SchemaRead, Serialize, Deserialize)]
pub struct Transaction {
    /// For Shielded txs, this might be all zeros or a "Relayer" key.
    pub sender: Pubkey,
    pub tx_type: TransactionType,
    /// Signature is used for Transparent txs.
    /// For Shielded, the authentication is inside the ZK Proof.
    pub signature: Signature,
}

/// The payload a user signs.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, SchemaRead, SchemaWrite, Default)]
pub struct TransactionData {
    pub from: AccountId,
    pub to: AccountId,
    pub amount: u64,
    pub nonce: u64,
    /// Replay protection ID (e.g. 1 for Mainnet, 2 for Devnet)
    pub chain_id: u64,
}

/// The authenticated wrapper around TransactionData.
#[derive(Debug, Clone, Serialize, Deserialize, SchemaRead, SchemaWrite)]
pub struct SignedTransaction {
    pub data: TransactionData,
    /// The Ed25519 signature of the serialized `data`.
    pub signature: Vec<u8>,
    /// The raw public key of the signer.
    pub signer_pubkey: [u8; 32],
}

impl TransactionType {
    pub fn apply_storage_effects(&self, batch: &mut WriteBatch, cf_nullifiers: &ColumnFamily) {
        match self {
            TransactionType::Shielded(blob) => {
                batch.put_cf(cf_nullifiers, &blob.nullifier, b"1");
            }
            _ => {}
        }
    }
}
