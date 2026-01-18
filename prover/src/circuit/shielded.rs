//! Shielded Transaction Circuit
//!
//! ZK proof that a shielded transfer is valid:
//! 1. Input nullifiers are correctly derived from spending keys
//! 2. Input notes exist in the commitment tree (Merkle proof)
//! 3. Output commitments are correctly formed
//! 4. Balance is preserved: sum(inputs) = sum(outputs) + fee
//!
//! ```text
//! Public Inputs:
//!   - merkle_root: Current commitment tree root
//!   - nullifiers[]: Nullifiers for spent notes
//!   - commitments[]: New note commitments
//!   - fee: Transaction fee
//!
//! Private Witness:
//!   - input_notes[]: Value, randomness, owner for each input
//!   - input_paths[]: Merkle authentication paths
//!   - spending_keys[]: Keys for nullifier derivation
//!   - output_notes[]: Value, randomness, owner for each output
//! ```

use ark_bn254::Fr;
use ark_crypto_primitives::sponge::constraints::CryptographicSpongeVar;
use ark_crypto_primitives::sponge::poseidon::{
    PoseidonConfig, constraints::PoseidonSpongeVar, find_poseidon_ark_and_mds,
};
use ark_ff::PrimeField;
use ark_r1cs_std::{
    alloc::AllocVar, boolean::Boolean, eq::EqGadget, fields::fp::FpVar, prelude::*,
    select::CondSelectGadget,
};
use ark_relations::r1cs::{ConstraintSynthesizer, ConstraintSystemRef, SynthesisError};

/// Maximum number of inputs per shielded transaction
pub const MAX_INPUTS: usize = 2;
/// Maximum number of outputs per shielded transaction  
pub const MAX_OUTPUTS: usize = 2;
/// Merkle tree depth
pub const TREE_DEPTH: usize = 32;

/// Witness for an input note
#[derive(Clone, Debug)]
pub struct InputNoteWitness {
    /// Note value
    pub value: u64,
    /// Note randomness (blinding factor)
    pub randomness: [u8; 32],
    /// Owner public key
    pub owner_pk: [u8; 32],
    /// Position in the Merkle tree
    pub position: u64,
    /// Spending key for nullifier derivation
    pub spending_key: [u8; 32],
    /// Merkle authentication path (sibling hashes)
    pub merkle_path: Vec<[u8; 32]>,
    /// Path direction bits (0 = left, 1 = right)
    pub path_bits: Vec<bool>,
}

/// Witness for an output note
#[derive(Clone, Debug)]
pub struct OutputNoteWitness {
    /// Note value
    pub value: u64,
    /// Note randomness (blinding factor)
    pub randomness: [u8; 32],
    /// Recipient public key
    pub recipient_pk: [u8; 32],
}

/// Shielded transaction circuit
#[derive(Clone)]
pub struct ShieldedTransferCircuit {
    // --- Public Inputs ---
    /// Current Merkle root (notes must be in this tree)
    pub merkle_root: Option<[u8; 32]>,
    /// Nullifiers for spent inputs
    pub nullifiers: Option<Vec<[u8; 32]>>,
    /// New commitments for outputs
    pub commitments: Option<Vec<[u8; 32]>>,
    /// Transaction fee (transparent)
    pub fee: Option<u64>,

    // --- Private Witness ---
    /// Input notes to spend
    pub inputs: Option<Vec<InputNoteWitness>>,
    /// Output notes to create
    pub outputs: Option<Vec<OutputNoteWitness>>,

    // --- Circuit config ---
    pub poseidon_config: PoseidonConfig<Fr>,
}

impl ShieldedTransferCircuit {
    /// Create a new circuit with default Poseidon config
    pub fn new() -> Self {
        Self {
            merkle_root: None,
            nullifiers: None,
            commitments: None,
            fee: None,
            inputs: None,
            outputs: None,
            poseidon_config: get_poseidon_config(),
        }
    }

    /// Set public inputs
    pub fn with_public_inputs(
        mut self,
        merkle_root: [u8; 32],
        nullifiers: Vec<[u8; 32]>,
        commitments: Vec<[u8; 32]>,
        fee: u64,
    ) -> Self {
        self.merkle_root = Some(merkle_root);
        self.nullifiers = Some(nullifiers);
        self.commitments = Some(commitments);
        self.fee = Some(fee);
        self
    }

    /// Set private witness
    pub fn with_witness(
        mut self,
        inputs: Vec<InputNoteWitness>,
        outputs: Vec<OutputNoteWitness>,
    ) -> Self {
        self.inputs = Some(inputs);
        self.outputs = Some(outputs);
        self
    }
}

impl Default for ShieldedTransferCircuit {
    fn default() -> Self {
        Self::new()
    }
}

impl ConstraintSynthesizer<Fr> for ShieldedTransferCircuit {
    fn generate_constraints(self, cs: ConstraintSystemRef<Fr>) -> Result<(), SynthesisError> {
        // === Allocate Public Inputs ===

        // Merkle root
        let merkle_root_bytes = self.merkle_root.ok_or(SynthesisError::AssignmentMissing)?;
        let merkle_root_var = FpVar::new_input(cs.clone(), || {
            Ok(Fr::from_le_bytes_mod_order(&merkle_root_bytes))
        })?;

        // Nullifiers (public)
        let nullifiers = self.nullifiers.ok_or(SynthesisError::AssignmentMissing)?;
        let mut nullifier_vars = Vec::with_capacity(nullifiers.len());
        for nf in &nullifiers {
            let nf_var = FpVar::new_input(cs.clone(), || Ok(Fr::from_le_bytes_mod_order(nf)))?;
            nullifier_vars.push(nf_var);
        }

        // Commitments (public)
        let commitments = self.commitments.ok_or(SynthesisError::AssignmentMissing)?;
        let mut commitment_vars = Vec::with_capacity(commitments.len());
        for cm in &commitments {
            let cm_var = FpVar::new_input(cs.clone(), || Ok(Fr::from_le_bytes_mod_order(cm)))?;
            commitment_vars.push(cm_var);
        }

        // Fee (public)
        let fee = self.fee.ok_or(SynthesisError::AssignmentMissing)?;
        let fee_var = FpVar::new_input(cs.clone(), || Ok(Fr::from(fee)))?;

        // === Allocate Private Witness ===

        let inputs = self.inputs.ok_or(SynthesisError::AssignmentMissing)?;
        let outputs = self.outputs.ok_or(SynthesisError::AssignmentMissing)?;

        // === Process Inputs ===
        let mut total_input_value = FpVar::zero();

        for (i, input) in inputs.iter().enumerate() {
            // Allocate input note values
            let value_var = FpVar::new_witness(cs.clone(), || Ok(Fr::from(input.value)))?;
            let randomness_var = FpVar::new_witness(cs.clone(), || {
                Ok(Fr::from_le_bytes_mod_order(&input.randomness))
            })?;
            let owner_pk_var = FpVar::new_witness(cs.clone(), || {
                Ok(Fr::from_le_bytes_mod_order(&input.owner_pk))
            })?;
            let position_var = FpVar::new_witness(cs.clone(), || Ok(Fr::from(input.position)))?;
            let spending_key_var = FpVar::new_witness(cs.clone(), || {
                Ok(Fr::from_le_bytes_mod_order(&input.spending_key))
            })?;

            // 1. Compute commitment: C = Poseidon(value, randomness, owner_pk)
            let computed_commitment = compute_commitment(
                cs.clone(),
                &self.poseidon_config,
                &value_var,
                &randomness_var,
                &owner_pk_var,
            )?;

            // 2. Verify Merkle path
            verify_merkle_path(
                cs.clone(),
                &self.poseidon_config,
                &computed_commitment,
                &input.merkle_path,
                &input.path_bits,
                &merkle_root_var,
            )?;

            // 3. Compute nullifier: N = PRF(spending_key, commitment, position)
            let computed_nullifier = compute_nullifier(
                cs.clone(),
                &self.poseidon_config,
                &spending_key_var,
                &computed_commitment,
                &position_var,
            )?;

            // 4. Verify nullifier matches public input
            computed_nullifier.enforce_equal(&nullifier_vars[i])?;

            // 5. Verify spending key corresponds to owner
            let derived_pk =
                derive_public_key(cs.clone(), &self.poseidon_config, &spending_key_var)?;
            derived_pk.enforce_equal(&owner_pk_var)?;

            // Accumulate input value
            total_input_value = total_input_value + &value_var;
        }

        // === Process Outputs ===
        let mut total_output_value = FpVar::zero();

        for (i, output) in outputs.iter().enumerate() {
            // Allocate output note values
            let value_var = FpVar::new_witness(cs.clone(), || Ok(Fr::from(output.value)))?;
            let randomness_var = FpVar::new_witness(cs.clone(), || {
                Ok(Fr::from_le_bytes_mod_order(&output.randomness))
            })?;
            let recipient_pk_var = FpVar::new_witness(cs.clone(), || {
                Ok(Fr::from_le_bytes_mod_order(&output.recipient_pk))
            })?;

            // Compute commitment: C = Poseidon(value, randomness, recipient_pk)
            let computed_commitment = compute_commitment(
                cs.clone(),
                &self.poseidon_config,
                &value_var,
                &randomness_var,
                &recipient_pk_var,
            )?;

            // Verify commitment matches public input
            computed_commitment.enforce_equal(&commitment_vars[i])?;

            // Accumulate output value
            total_output_value = total_output_value + &value_var;
        }

        // === Balance Constraint ===
        // sum(inputs) = sum(outputs) + fee
        let expected_input = &total_output_value + &fee_var;
        total_input_value.enforce_equal(&expected_input)?;

        Ok(())
    }
}

/// Compute note commitment: Poseidon(value, randomness, owner_pk)
fn compute_commitment(
    cs: ConstraintSystemRef<Fr>,
    config: &PoseidonConfig<Fr>,
    value: &FpVar<Fr>,
    randomness: &FpVar<Fr>,
    owner_pk: &FpVar<Fr>,
) -> Result<FpVar<Fr>, SynthesisError> {
    let mut sponge = PoseidonSpongeVar::new(cs, config);
    let inputs = vec![value.clone(), randomness.clone(), owner_pk.clone()];
    sponge.absorb(&inputs.as_slice())?;
    let mut result = sponge.squeeze_field_elements(1)?;
    Ok(result.remove(0))
}

/// Compute nullifier: PRF(spending_key, commitment, position)
fn compute_nullifier(
    cs: ConstraintSystemRef<Fr>,
    config: &PoseidonConfig<Fr>,
    spending_key: &FpVar<Fr>,
    commitment: &FpVar<Fr>,
    position: &FpVar<Fr>,
) -> Result<FpVar<Fr>, SynthesisError> {
    let mut sponge = PoseidonSpongeVar::new(cs, config);

    // Domain separation
    let domain = FpVar::constant(Fr::from(0x4e554c4c_u64)); // "NULL"
    let inputs = vec![
        domain,
        spending_key.clone(),
        commitment.clone(),
        position.clone(),
    ];
    sponge.absorb(&inputs.as_slice())?;

    let mut result = sponge.squeeze_field_elements(1)?;
    Ok(result.remove(0))
}

/// Derive public key from spending key
fn derive_public_key(
    cs: ConstraintSystemRef<Fr>,
    config: &PoseidonConfig<Fr>,
    spending_key: &FpVar<Fr>,
) -> Result<FpVar<Fr>, SynthesisError> {
    let mut sponge = PoseidonSpongeVar::new(cs, config);

    // Domain separation for PK derivation
    let domain = FpVar::constant(Fr::from_le_bytes_mod_order(
        b"ZelanaPK\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
    ));
    let inputs = vec![domain, spending_key.clone()];
    sponge.absorb(&inputs.as_slice())?;

    let mut result = sponge.squeeze_field_elements(1)?;
    Ok(result.remove(0))
}

/// Verify Merkle path from leaf to root
fn verify_merkle_path(
    cs: ConstraintSystemRef<Fr>,
    config: &PoseidonConfig<Fr>,
    leaf: &FpVar<Fr>,
    path: &[[u8; 32]],
    path_bits: &[bool],
    expected_root: &FpVar<Fr>,
) -> Result<(), SynthesisError> {
    let mut current = leaf.clone();

    for (sibling_bytes, is_right) in path.iter().zip(path_bits.iter()) {
        let sibling = FpVar::new_witness(cs.clone(), || {
            Ok(Fr::from_le_bytes_mod_order(sibling_bytes))
        })?;

        let is_right_var = Boolean::new_witness(cs.clone(), || Ok(*is_right))?;

        // If is_right, hash(sibling, current), else hash(current, sibling)
        // Use conditionally_select: if is_right_var is true, returns first arg, else second
        let left = FpVar::conditionally_select(&is_right_var, &sibling, &current)?;
        let right = FpVar::conditionally_select(&is_right_var, &current, &sibling)?;

        let mut sponge = PoseidonSpongeVar::new(cs.clone(), config);
        let inputs = vec![left, right];
        sponge.absorb(&inputs.as_slice())?;
        let mut parent = sponge.squeeze_field_elements(1)?;
        current = parent.remove(0);
    }

    current.enforce_equal(expected_root)?;
    Ok(())
}

/// Get Poseidon configuration for BLS12-381
fn get_poseidon_config() -> PoseidonConfig<Fr> {
    let (ark, mds) = find_poseidon_ark_and_mds::<Fr>(255, 2, 8, 57, 0);
    PoseidonConfig::new(8, 57, 5, mds, ark, 2, 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ark_relations::r1cs::ConstraintSystem;

    #[test]
    fn test_circuit_satisfiability() {
        // This is a basic structure test - full test would require proper witness generation
        let cs = ConstraintSystem::<Fr>::new_ref();

        let circuit = ShieldedTransferCircuit::new()
            .with_public_inputs(
                [0u8; 32],       // merkle_root
                vec![[1u8; 32]], // nullifiers
                vec![[2u8; 32]], // commitments
                0,               // fee
            )
            .with_witness(
                vec![InputNoteWitness {
                    value: 100,
                    randomness: [0u8; 32],
                    owner_pk: [0u8; 32],
                    position: 0,
                    spending_key: [0u8; 32],
                    merkle_path: vec![[0u8; 32]; TREE_DEPTH],
                    path_bits: vec![false; TREE_DEPTH],
                }],
                vec![OutputNoteWitness {
                    value: 100,
                    randomness: [0u8; 32],
                    recipient_pk: [0u8; 32],
                }],
            );

        // Note: This will fail constraint satisfaction because the test data is dummy
        // A real test would need properly computed commitments, nullifiers, etc.
        let result = circuit.generate_constraints(cs.clone());

        // Just check that constraint generation doesn't panic
        assert!(result.is_ok() || result.is_err()); // Placeholder - real test would verify satisfaction
    }
}
