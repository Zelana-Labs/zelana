use serde::{Serialize, Deserialize};
use wincode_derive::{SchemaRead, SchemaWrite};
use zelana_signature::Signature;
use zelana_pubkey::Pubkey;

pub mod bridge;

/// The enum for all inputs to the L2 State Machine.
#[derive(Debug, Clone, SchemaRead, SchemaWrite)]
pub enum TransactionType {
    /// A standard transfer or interaction submitted by a user via UDP.
    Transfer(SignedTransaction),

    /// A deposit event detected on L1 (Solana) and bridged to L2.
    Deposit(DepositEvent),

    /// A withdrawal request to move funds back to L1.
    Withdraw(WithdrawRequest),
}

/// A single transaction 
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Transaction {
    pub sender: Pubkey,
    pub recipient: Pubkey,
    pub tx_type: TransactionType,
    pub signature: Signature,
}

/// The payload a user signs.
#[derive(Debug, Clone, PartialEq, SchemaRead, SchemaWrite)]
pub struct TransactionData {
    pub from: AccountId,
    pub to: AccountId,
    pub amount: u64,
    pub nonce: u64,
    /// Replay protection ID (e.g. 1 for Mainnet, 2 for Devnet)
    pub chain_id: u64,
}

/// The authenticated wrapper around TransactionData.
#[derive(Debug, Clone, SchemaRead, SchemaWrite)]
pub struct SignedTransaction {
    pub data: TransactionData,
    /// The Ed25519 signature of the serialized `data`.
    pub signature: Vec<u8>,
    /// The raw public key of the signer.
    pub signer_pubkey: [u8; 32],
}