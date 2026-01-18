use crate::{
    circuit::{hash::hash2, merkle::verify_merkle_path},
    witness::WitnessTx,
};
use ark_bn254::Fr;
use ark_ff::One;
use ark_r1cs_std::alloc::AllocVar;
use ark_r1cs_std::eq::EqGadget;
use ark_r1cs_std::fields::FieldVar;
use ark_r1cs_std::{boolean::Boolean, fields::fp::FpVar};
use ark_relations::r1cs::{ConstraintSystemRef, SynthesisError};
pub fn apply_l2_block(
    cs: ConstraintSystemRef<Fr>,
    txs: &[WitnessTx],
    initial_root: FpVar<Fr>,
) -> Result<FpVar<Fr>, SynthesisError> {
    let mut current_root = initial_root;

    for tx in txs {
        let _enabled = Boolean::new_witness(cs.clone(), || Ok(tx.enabled))?;

        // -------------------------------
        // Allocate sender fields
        // -------------------------------
        let sender_pubkey = FpVar::new_witness(cs.clone(), || Ok(tx.sender.pubkey))?;
        let sender_balance = FpVar::new_witness(cs.clone(), || Ok(Fr::from(tx.sender.balance)))?;
        let sender_nonce = FpVar::new_witness(cs.clone(), || Ok(Fr::from(tx.sender.nonce)))?;

        let amount = FpVar::new_witness(cs.clone(), || Ok(Fr::from(tx.amount)))?;

        // -------------------------------
        // Merkle inclusion (sender)
        // -------------------------------
        verify_merkle_path(
            cs.clone(),
            &current_root,
            &sender_pubkey,
            &sender_balance,
            &sender_nonce,
            &tx.sender.merkle_path,
        )?;

        // -------------------------------
        // Nonce check
        // -------------------------------
        sender_nonce.enforce_equal(&FpVar::constant(Fr::from(tx.nonce)))?;

        // -------------------------------
        // Transaction hash verification
        // -------------------------------
        let tx_hash = FpVar::new_witness(cs.clone(), || Ok(tx.tx_hash))?;

        let expected_tx_hash = hash2(
            cs.clone(),
            &sender_pubkey,
            &(sender_nonce.clone() + amount.clone() + FpVar::constant(Fr::from(tx.tx_type as u64))),
        )?;

        tx_hash.enforce_equal(&expected_tx_hash)?;

        // -------------------------------
        // Balance update
        // -------------------------------
        let new_sender_balance = sender_balance.clone() - amount.clone();

        // -------------------------------
        // Update Merkle root
        // -------------------------------
        current_root = crate::circuit::merkle::update_merkle_root(
            cs.clone(),
            &current_root,
            &sender_pubkey,
            &new_sender_balance,
            &(sender_nonce + FpVar::constant(Fr::one())),
            &tx.sender.merkle_path,
        )?;
    }

    Ok(current_root)
}
