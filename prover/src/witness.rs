use ark_bn254::Fr;

use crate::circuit::merkle::MerklePathWitness;

#[derive(Clone, Debug)]
pub struct AccountWitness {
    /// Public key committed as a field element
    pub pubkey: Fr,
    pub balance: u64,
    pub nonce: u64,
    pub merkle_path: MerklePathWitness,
}

#[derive(Clone, Debug)]
pub struct WitnessTx {
    /// Whether this tx is real or padding
    pub enabled: bool,

    /// Transaction type (transfer, withdraw, etc.)
    pub tx_type: u8,

    pub sender: AccountWitness,
    pub receiver: Option<AccountWitness>,

    pub amount: u64,
    pub nonce: u64,

    /// Poseidon commitment to tx contents (computed off-circuit)
    pub tx_hash: Fr,
}
