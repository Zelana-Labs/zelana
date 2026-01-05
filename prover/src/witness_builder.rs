use ark_bls12_381::Fr;
use ark_crypto_primitives::sponge::{
    poseidon::PoseidonSponge,
    CryptographicSponge,
};

use crate::{
    circuit::poseidon::poseidon_config,
    witness::{WitnessTx, AccountWitness},
    executor::ExecutionTrace,
};

fn compute_tx_hash(
    pubkey: Fr,
    nonce: u64,
    amount: u64,
    tx_type: u8,
) -> Fr {
    let mut sponge = PoseidonSponge::<Fr>::new(&poseidon_config());
    sponge.absorb(&pubkey);
    sponge.absorb(&Fr::from(nonce));
    sponge.absorb(&Fr::from(amount));
    sponge.absorb(&Fr::from(tx_type as u64));
    sponge.squeeze_field_elements(1)[0]
}

pub fn build_witness_txs(
    trace: ExecutionTrace,
) -> Vec<WitnessTx> {
    let mut out = Vec::new();

    for tx in trace.txs {
        let tx_hash = compute_tx_hash(
            tx.sender.pubkey,
            tx.nonce,
            tx.amount,
            tx.tx_type,
        );

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
