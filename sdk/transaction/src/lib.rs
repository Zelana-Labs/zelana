use serde::{Serialize, Deserialize};
use zelana_signature::Signature;
use zelana_pubkey::Pubkey;

/// A single transaction 
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Transaction {
    pub sender: Pubkey,
    pub recipient: Pubkey,
    pub tx_type: TransactionType,
    pub signature: Signature,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum TransactionType {
    Transfer { amount: u64 },
    Deposit { amount: u64 },
}