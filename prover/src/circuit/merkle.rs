use crate::circuit::hash::hash2;
use ark_bn254::Fr;
use ark_r1cs_std::alloc::AllocVar;
use ark_r1cs_std::eq::EqGadget;
use ark_r1cs_std::select::CondSelectGadget;
use ark_r1cs_std::{boolean::Boolean, fields::fp::FpVar};
use ark_relations::r1cs::{ConstraintSystemRef, SynthesisError};

/// Merkle path witness for inclusion proofs
#[derive(Clone, Debug)]
pub struct MerklePathWitness {
    /// Sibling hashes along the path
    pub siblings: Vec<Fr>,
    /// Direction bits: true = current node is left child
    pub is_left: Vec<bool>,
}

impl MerklePathWitness {
    /// Create an empty path
    pub fn empty(depth: usize) -> Self {
        Self {
            siblings: vec![Fr::from(0u64); depth],
            is_left: vec![false; depth],
        }
    }
}

/// Compute leaf hash: Poseidon(pubkey || balance || nonce)
fn compute_leaf(
    cs: ConstraintSystemRef<Fr>,
    pubkey: &FpVar<Fr>,
    balance: &FpVar<Fr>,
    nonce: &FpVar<Fr>,
) -> Result<FpVar<Fr>, SynthesisError> {
    let h1 = hash2(cs.clone(), pubkey, balance)?;
    hash2(cs, &h1, nonce)
}

/// Verify Merkle inclusion proof
pub fn verify_merkle_path(
    cs: ConstraintSystemRef<Fr>,
    root: &FpVar<Fr>,
    pubkey: &FpVar<Fr>,
    balance: &FpVar<Fr>,
    nonce: &FpVar<Fr>,
    path: &MerklePathWitness,
) -> Result<(), SynthesisError> {
    let mut current = compute_leaf(cs.clone(), pubkey, balance, nonce)?;

    for (i, sibling) in path.siblings.iter().enumerate() {
        let sibling_var = FpVar::new_witness(cs.clone(), || Ok(*sibling))?;

        let is_left = Boolean::new_witness(cs.clone(), || Ok(path.is_left[i]))?;

        let left = FpVar::conditionally_select(&is_left, &current, &sibling_var)?;

        let right = FpVar::conditionally_select(&is_left, &sibling_var, &current)?;

        current = hash2(cs.clone(), &left, &right)?;
    }

    current.enforce_equal(root)?;
    Ok(())
}

/// Update Merkle root after modifying a leaf
pub fn update_merkle_root(
    cs: ConstraintSystemRef<Fr>,
    _old_root: &FpVar<Fr>,
    pubkey: &FpVar<Fr>,
    new_balance: &FpVar<Fr>,
    new_nonce: &FpVar<Fr>,
    path: &MerklePathWitness,
) -> Result<FpVar<Fr>, SynthesisError> {
    // Recompute new leaf
    let mut current = compute_leaf(cs.clone(), pubkey, new_balance, new_nonce)?;

    for (i, sibling) in path.siblings.iter().enumerate() {
        let sibling_var = FpVar::new_witness(cs.clone(), || Ok(*sibling))?;

        let is_left = Boolean::new_witness(cs.clone(), || Ok(path.is_left[i]))?;

        let left = FpVar::conditionally_select(&is_left, &current, &sibling_var)?;

        let right = FpVar::conditionally_select(&is_left, &sibling_var, &current)?;

        current = hash2(cs.clone(), &left, &right)?;
    }

    // old_root is not enforced here (already checked earlier)
    Ok(current)
}
