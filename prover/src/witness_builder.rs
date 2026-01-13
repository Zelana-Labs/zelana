//! Witness builder for ZK proofs
//!
//! Builds witness data from execution traces for circuit proving.

use ark_bn254::Fr;
use ark_crypto_primitives::sponge::{CryptographicSponge, poseidon::PoseidonSponge};

use crate::{
    circuit::merkle::MerklePathWitness,
    circuit::poseidon::poseidon_config,
    witness::{AccountWitness, WitnessTx},
};

/// Execution trace for a single transaction (provided by sequencer)
#[derive(Clone, Debug)]
pub struct ExecutionTraceTx {
    pub tx_type: u8,
    pub sender: ExecutionTraceAccount,
    pub receiver: Option<ExecutionTraceAccount>,
    pub amount: u64,
    pub nonce: u64,
}

/// Account state from execution trace
#[derive(Clone, Debug)]
pub struct ExecutionTraceAccount {
    pub pubkey: Fr,
    pub balance: u64,
    pub nonce: u64,
    pub merkle_path: MerklePathWitness,
}

/// Full execution trace for a batch
#[derive(Clone, Debug)]
pub struct ExecutionTrace {
    pub txs: Vec<ExecutionTraceTx>,
}

fn compute_tx_hash(pubkey: Fr, nonce: u64, amount: u64, tx_type: u8) -> Fr {
    let mut sponge = PoseidonSponge::<Fr>::new(&poseidon_config());
    sponge.absorb(&pubkey);
    sponge.absorb(&Fr::from(nonce));
    sponge.absorb(&Fr::from(amount));
    sponge.absorb(&Fr::from(tx_type as u64));
    sponge.squeeze_field_elements(1)[0]
}

pub fn build_witness_txs(trace: ExecutionTrace) -> Vec<WitnessTx> {
    let mut out = Vec::new();

    for tx in trace.txs {
        let tx_hash = compute_tx_hash(tx.sender.pubkey, tx.nonce, tx.amount, tx.tx_type);

        out.push(WitnessTx {
            enabled: true,
            tx_type: tx.tx_type,
            sender: AccountWitness {
                pubkey: tx.sender.pubkey,
                balance: tx.sender.balance,
                nonce: tx.sender.nonce,
                merkle_path: tx.sender.merkle_path,
            },
            receiver: tx.receiver.map(|r| AccountWitness {
                pubkey: r.pubkey,
                balance: r.balance,
                nonce: r.nonce,
                merkle_path: r.merkle_path,
            }),
            amount: tx.amount,
            nonce: tx.nonce,
            tx_hash,
        });
    }

    out
}
