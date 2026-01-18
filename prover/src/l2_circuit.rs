//! L2 Block Circuit for Zelana
//!
//! This circuit proves the validity of a batch of L2 transactions.
//! It uses Groth16 proving system on BN254 curve.
//!
//! Public Inputs (7 field elements, order matters for verifier):
//! 1. pre_state_root      - Account state root before batch
//! 2. post_state_root     - Account state root after batch  
//! 3. pre_shielded_root   - Shielded commitment tree root before batch
//! 4. post_shielded_root  - Shielded commitment tree root after batch
//! 5. withdrawal_root     - Merkle root of withdrawals in this batch
//! 6. batch_hash          - Hash of all transactions in the batch
//! 7. batch_id            - Batch sequence number
//!
//! Private Witness:
//! - Initial account states (balances)
//! - Transactions (transfers)
//! - Shielded commitments added
//! - Withdrawals processed

use ark_bn254::Fr;
use ark_crypto_primitives::sponge::constraints::CryptographicSpongeVar;
use ark_crypto_primitives::sponge::poseidon::{
    PoseidonConfig, constraints::PoseidonSpongeVar, find_poseidon_ark_and_mds,
};
use ark_ff::PrimeField;
use ark_r1cs_std::{fields::fp::FpVar, prelude::*};
use ark_relations::r1cs::{ConstraintSynthesizer, ConstraintSystemRef, SynthesisError};
use std::collections::BTreeMap;

pub type PubkeyBytes = [u8; 32];

// ============================================================================
// Account State
// ============================================================================

/// Account state variable in the circuit
#[derive(Clone, Debug)]
pub struct AccountVar {
    pub balance: FpVar<Fr>,
}

// ============================================================================
// Transaction Witness
// ============================================================================

/// A transfer transaction witness
#[derive(Clone, Debug)]
pub struct TransactionWitness {
    pub sender_pk: PubkeyBytes,
    pub recipient_pk: PubkeyBytes,
    pub amount: u64,
}

/// A shielded commitment (for shield operations)
#[derive(Clone, Debug)]
pub struct ShieldedCommitmentWitness {
    pub commitment: [u8; 32],
}

/// A withdrawal request
#[derive(Clone, Debug)]
pub struct WithdrawalWitness {
    pub recipient: [u8; 32], // L1 address
    pub amount: u64,
}

// ============================================================================
// Poseidon Config
// ============================================================================

/// Get Poseidon hash configuration for BN254
/// Parameters chosen for 128-bit security
pub fn get_poseidon_config() -> PoseidonConfig<Fr> {
    let full_rounds = 8;
    let partial_rounds = 56;
    let alpha = 5u64;
    let rate = 2;
    let capacity = 1;

    let (ark, mds) = find_poseidon_ark_and_mds::<Fr>(
        Fr::MODULUS_BIT_SIZE as u64,
        rate,
        full_rounds as u64,
        partial_rounds as u64,
        0,
    );
    PoseidonConfig::new(full_rounds, partial_rounds, alpha, mds, ark, rate, capacity)
}

// ============================================================================
// L2 Block Circuit
// ============================================================================

/// The main L2 block circuit
///
/// Proves that:
/// 1. All transfers are valid (sender has sufficient balance)
/// 2. Account state transitions correctly (pre_state -> post_state)
/// 3. Shielded commitment tree updated correctly
/// 4. Withdrawal merkle root is correct
/// 5. Batch hash matches the transactions
#[derive(Clone)]
pub struct L2BlockCircuit {
    // === Public Inputs (7 field elements) ===
    /// Account state root before batch
    pub pre_state_root: Option<[u8; 32]>,
    /// Account state root after batch
    pub post_state_root: Option<[u8; 32]>,
    /// Shielded commitment tree root before batch
    pub pre_shielded_root: Option<[u8; 32]>,
    /// Shielded commitment tree root after batch  
    pub post_shielded_root: Option<[u8; 32]>,
    /// Merkle root of withdrawals in this batch
    pub withdrawal_root: Option<[u8; 32]>,
    /// Hash of all transactions in the batch
    pub batch_hash: Option<[u8; 32]>,
    /// Batch sequence number
    pub batch_id: Option<u64>,

    // === Private Witness ===
    /// Transfers in this batch
    pub transactions: Option<Vec<TransactionWitness>>,
    /// Initial account states
    pub initial_accounts: Option<BTreeMap<PubkeyBytes, u64>>,
    /// Shielded commitments added in this batch
    pub shielded_commitments: Option<Vec<ShieldedCommitmentWitness>>,
    /// Withdrawals in this batch
    pub withdrawals: Option<Vec<WithdrawalWitness>>,
    /// Poseidon hash configuration
    pub poseidon_config: PoseidonConfig<Fr>,
}

impl L2BlockCircuit {
    /// Create a new circuit with default Poseidon config
    pub fn new() -> Self {
        Self {
            pre_state_root: None,
            post_state_root: None,
            pre_shielded_root: None,
            post_shielded_root: None,
            withdrawal_root: None,
            batch_hash: None,
            batch_id: None,
            transactions: None,
            initial_accounts: None,
            shielded_commitments: None,
            withdrawals: None,
            poseidon_config: get_poseidon_config(),
        }
    }

    /// Create a dummy circuit for key generation
    /// Uses minimal constraints but same structure as real proofs
    pub fn dummy() -> Self {
        let mut accounts = BTreeMap::new();
        accounts.insert([1u8; 32], 1000u64);
        accounts.insert([2u8; 32], 0u64);

        Self {
            pre_state_root: Some([0u8; 32]),
            post_state_root: Some([0u8; 32]),
            pre_shielded_root: Some([0u8; 32]),
            post_shielded_root: Some([0u8; 32]),
            withdrawal_root: Some([0u8; 32]),
            batch_hash: Some([0u8; 32]),
            batch_id: Some(0),
            transactions: Some(vec![TransactionWitness {
                sender_pk: [1u8; 32],
                recipient_pk: [2u8; 32],
                amount: 100,
            }]),
            initial_accounts: Some(accounts),
            shielded_commitments: Some(vec![]),
            withdrawals: Some(vec![]),
            poseidon_config: get_poseidon_config(),
        }
    }
}

impl Default for L2BlockCircuit {
    fn default() -> Self {
        Self::new()
    }
}

impl ConstraintSynthesizer<Fr> for L2BlockCircuit {
    fn generate_constraints(self, cs: ConstraintSystemRef<Fr>) -> Result<(), SynthesisError> {
        // =====================================================================
        // Allocate Public Inputs (ORDER MATTERS - must match verifier)
        // =====================================================================

        // 1. pre_state_root
        let pre_state_bytes = self
            .pre_state_root
            .ok_or(SynthesisError::AssignmentMissing)?;
        let pre_state_root_var = FpVar::new_input(cs.clone(), || {
            Ok(Fr::from_le_bytes_mod_order(&pre_state_bytes))
        })?;

        // 2. post_state_root
        let post_state_bytes = self
            .post_state_root
            .ok_or(SynthesisError::AssignmentMissing)?;
        let expected_post_state_var = FpVar::new_input(cs.clone(), || {
            Ok(Fr::from_le_bytes_mod_order(&post_state_bytes))
        })?;

        // 3. pre_shielded_root
        let pre_shielded_bytes = self
            .pre_shielded_root
            .ok_or(SynthesisError::AssignmentMissing)?;
        let pre_shielded_root_var = FpVar::new_input(cs.clone(), || {
            Ok(Fr::from_le_bytes_mod_order(&pre_shielded_bytes))
        })?;

        // 4. post_shielded_root
        let post_shielded_bytes = self
            .post_shielded_root
            .ok_or(SynthesisError::AssignmentMissing)?;
        let expected_post_shielded_var = FpVar::new_input(cs.clone(), || {
            Ok(Fr::from_le_bytes_mod_order(&post_shielded_bytes))
        })?;

        // 5. withdrawal_root
        let withdrawal_bytes = self
            .withdrawal_root
            .ok_or(SynthesisError::AssignmentMissing)?;
        let expected_withdrawal_root_var = FpVar::new_input(cs.clone(), || {
            Ok(Fr::from_le_bytes_mod_order(&withdrawal_bytes))
        })?;

        // 6. batch_hash
        let batch_hash_bytes = self.batch_hash.ok_or(SynthesisError::AssignmentMissing)?;
        let expected_batch_hash_var = FpVar::new_input(cs.clone(), || {
            Ok(Fr::from_le_bytes_mod_order(&batch_hash_bytes))
        })?;

        // 7. batch_id
        let batch_id_val = self.batch_id.ok_or(SynthesisError::AssignmentMissing)?;
        let batch_id_var = FpVar::new_input(cs.clone(), || Ok(Fr::from(batch_id_val)))?;

        // =====================================================================
        // Allocate Private Witness
        // =====================================================================

        let transactions = self.transactions.ok_or(SynthesisError::AssignmentMissing)?;
        let initial_accounts = self
            .initial_accounts
            .ok_or(SynthesisError::AssignmentMissing)?;
        let shielded_commitments = self.shielded_commitments.unwrap_or_default();
        let withdrawals = self.withdrawals.unwrap_or_default();

        // Allocate initial account balances
        let mut account_vars: BTreeMap<PubkeyBytes, AccountVar> = BTreeMap::new();
        for (pk_bytes, balance) in initial_accounts.iter() {
            let balance_var = FpVar::new_witness(cs.clone(), || Ok(Fr::from(*balance)))?;
            account_vars.insert(
                *pk_bytes,
                AccountVar {
                    balance: balance_var,
                },
            );
        }

        // =====================================================================
        // Process Transfers
        // =====================================================================

        let mut current_accounts = account_vars.clone();

        for tx in transactions.iter() {
            let amount_var = FpVar::new_witness(cs.clone(), || Ok(Fr::from(tx.amount)))?;

            // Get sender account
            let sender_acc = current_accounts
                .get(&tx.sender_pk)
                .cloned()
                .ok_or(SynthesisError::AssignmentMissing)?;

            // Get or create recipient account
            let recipient_acc =
                current_accounts
                    .get(&tx.recipient_pk)
                    .cloned()
                    .unwrap_or(AccountVar {
                        balance: FpVar::zero(),
                    });

            // Enforce sender has sufficient balance: sender.balance >= amount
            sender_acc
                .balance
                .enforce_cmp(&amount_var, core::cmp::Ordering::Greater, true)?;

            // Update balances
            let new_sender_balance = &sender_acc.balance - &amount_var;
            let new_recipient_balance = &recipient_acc.balance + &amount_var;

            current_accounts.insert(
                tx.sender_pk,
                AccountVar {
                    balance: new_sender_balance,
                },
            );
            current_accounts.insert(
                tx.recipient_pk,
                AccountVar {
                    balance: new_recipient_balance,
                },
            );
        }

        // =====================================================================
        // Compute Account State Root (Poseidon fold)
        // =====================================================================

        let mut sponge = PoseidonSpongeVar::new(cs.clone(), &self.poseidon_config);

        // Domain separator for account folding
        let ds_bytes: &[u8] = b"zelana:accounts-fold:v1";
        let ds_fr = Fr::from_le_bytes_mod_order(ds_bytes);
        let domain_separator_var = FpVar::new_constant(cs.clone(), ds_fr)?;

        // Initial state: S0 = Poseidon(domain_separator, batch_id)
        sponge.absorb(&vec![&domain_separator_var, &batch_id_var])?;
        let mut state_vars = sponge.squeeze_field_elements(1)?;
        let mut current_state = state_vars.remove(0);

        // Fold each account: S_{i+1} = Poseidon(S_i, leaf_i)
        // where leaf_i = Poseidon(pk, balance)
        for (pk_bytes, acc_var) in current_accounts.iter() {
            let pk_var =
                FpVar::new_witness(cs.clone(), || Ok(Fr::from_le_bytes_mod_order(pk_bytes)))?;

            // Compute leaf hash
            let mut leaf_sponge = PoseidonSpongeVar::new(cs.clone(), &self.poseidon_config);
            leaf_sponge.absorb(&vec![&pk_var, &acc_var.balance])?;
            let mut leaf_vars = leaf_sponge.squeeze_field_elements(1)?;
            let leaf_hash = leaf_vars.remove(0);

            // Fold
            let mut fold_sponge = PoseidonSpongeVar::new(cs.clone(), &self.poseidon_config);
            fold_sponge.absorb(&vec![&current_state, &leaf_hash])?;
            let mut next_vars = fold_sponge.squeeze_field_elements(1)?;
            current_state = next_vars.remove(0);
        }

        // Finalize with count
        let account_count = Fr::from(current_accounts.len() as u64);
        let count_var = FpVar::new_witness(cs.clone(), || Ok(account_count))?;

        let mut final_sponge = PoseidonSpongeVar::new(cs.clone(), &self.poseidon_config);
        final_sponge.absorb(&vec![&current_state, &count_var])?;
        let mut final_vars = final_sponge.squeeze_field_elements(1)?;
        let computed_post_state = final_vars.remove(0);

        // Enforce: computed_post_state == expected_post_state
        computed_post_state.enforce_equal(&expected_post_state_var)?;

        // =====================================================================
        // Compute Shielded Root (simplified for MVP)
        // =====================================================================

        // For MVP: Just verify the roots are provided correctly
        // In full implementation: build merkle tree from commitments

        // Compute expected shielded root from commitments
        let mut shielded_sponge = PoseidonSpongeVar::new(cs.clone(), &self.poseidon_config);

        // Start with pre_shielded_root
        shielded_sponge.absorb(&vec![&pre_shielded_root_var])?;
        let mut shielded_state_vars = shielded_sponge.squeeze_field_elements(1)?;
        let mut shielded_state = shielded_state_vars.remove(0);

        // Fold in new commitments
        for commitment in shielded_commitments.iter() {
            let commitment_var = FpVar::new_witness(cs.clone(), || {
                Ok(Fr::from_le_bytes_mod_order(&commitment.commitment))
            })?;

            let mut fold_sponge = PoseidonSpongeVar::new(cs.clone(), &self.poseidon_config);
            fold_sponge.absorb(&vec![&shielded_state, &commitment_var])?;
            let mut next_vars = fold_sponge.squeeze_field_elements(1)?;
            shielded_state = next_vars.remove(0);
        }

        // If no new commitments, shielded root stays same
        if shielded_commitments.is_empty() {
            // pre == post when no shielded txs
            pre_shielded_root_var.enforce_equal(&expected_post_shielded_var)?;
        } else {
            shielded_state.enforce_equal(&expected_post_shielded_var)?;
        }

        // =====================================================================
        // Compute Withdrawal Root
        // =====================================================================

        // Build merkle root of withdrawals
        let mut withdrawal_sponge = PoseidonSpongeVar::new(cs.clone(), &self.poseidon_config);

        // Domain separator for withdrawals
        let wd_ds = Fr::from_le_bytes_mod_order(b"zelana:withdrawals:v1");
        let wd_ds_var = FpVar::new_constant(cs.clone(), wd_ds)?;
        withdrawal_sponge.absorb(&vec![&wd_ds_var])?;
        let mut wd_state_vars = withdrawal_sponge.squeeze_field_elements(1)?;
        let mut wd_state = wd_state_vars.remove(0);

        for wd in withdrawals.iter() {
            let recipient_var = FpVar::new_witness(cs.clone(), || {
                Ok(Fr::from_le_bytes_mod_order(&wd.recipient))
            })?;
            let amount_var = FpVar::new_witness(cs.clone(), || Ok(Fr::from(wd.amount)))?;

            // Leaf = Poseidon(recipient, amount)
            let mut leaf_sponge = PoseidonSpongeVar::new(cs.clone(), &self.poseidon_config);
            leaf_sponge.absorb(&vec![&recipient_var, &amount_var])?;
            let mut leaf_vars = leaf_sponge.squeeze_field_elements(1)?;
            let leaf = leaf_vars.remove(0);

            // Fold
            let mut fold_sponge = PoseidonSpongeVar::new(cs.clone(), &self.poseidon_config);
            fold_sponge.absorb(&vec![&wd_state, &leaf])?;
            let mut next_vars = fold_sponge.squeeze_field_elements(1)?;
            wd_state = next_vars.remove(0);
        }

        // Finalize withdrawal root
        let wd_count = Fr::from(withdrawals.len() as u64);
        let wd_count_var = FpVar::new_witness(cs.clone(), || Ok(wd_count))?;

        let mut final_wd_sponge = PoseidonSpongeVar::new(cs.clone(), &self.poseidon_config);
        final_wd_sponge.absorb(&vec![&wd_state, &wd_count_var])?;
        let mut final_wd_vars = final_wd_sponge.squeeze_field_elements(1)?;
        let computed_wd_root = final_wd_vars.remove(0);

        // Enforce withdrawal root
        computed_wd_root.enforce_equal(&expected_withdrawal_root_var)?;

        // =====================================================================
        // Verify Batch Hash
        // =====================================================================

        // Compute batch hash from transactions
        let mut batch_sponge = PoseidonSpongeVar::new(cs.clone(), &self.poseidon_config);

        let batch_ds = Fr::from_le_bytes_mod_order(b"zelana:batch-hash:v1");
        let batch_ds_var = FpVar::new_constant(cs.clone(), batch_ds)?;
        batch_sponge.absorb(&vec![&batch_ds_var, &batch_id_var])?;
        let mut batch_state_vars = batch_sponge.squeeze_field_elements(1)?;
        let mut batch_state = batch_state_vars.remove(0);

        // Fold each transaction
        for tx in transactions.iter() {
            let sender_var = FpVar::new_witness(cs.clone(), || {
                Ok(Fr::from_le_bytes_mod_order(&tx.sender_pk))
            })?;
            let recipient_var = FpVar::new_witness(cs.clone(), || {
                Ok(Fr::from_le_bytes_mod_order(&tx.recipient_pk))
            })?;
            let amount_var = FpVar::new_witness(cs.clone(), || Ok(Fr::from(tx.amount)))?;

            // tx_hash = Poseidon(sender, recipient, amount)
            let mut tx_sponge = PoseidonSpongeVar::new(cs.clone(), &self.poseidon_config);
            tx_sponge.absorb(&vec![&sender_var, &recipient_var, &amount_var])?;
            let mut tx_vars = tx_sponge.squeeze_field_elements(1)?;
            let tx_hash = tx_vars.remove(0);

            // Fold
            let mut fold_sponge = PoseidonSpongeVar::new(cs.clone(), &self.poseidon_config);
            fold_sponge.absorb(&vec![&batch_state, &tx_hash])?;
            let mut next_vars = fold_sponge.squeeze_field_elements(1)?;
            batch_state = next_vars.remove(0);
        }

        // Finalize batch hash
        let tx_count = Fr::from(transactions.len() as u64);
        let tx_count_var = FpVar::new_witness(cs.clone(), || Ok(tx_count))?;

        let mut final_batch_sponge = PoseidonSpongeVar::new(cs.clone(), &self.poseidon_config);
        final_batch_sponge.absorb(&vec![&batch_state, &tx_count_var])?;
        let mut final_batch_vars = final_batch_sponge.squeeze_field_elements(1)?;
        let computed_batch_hash = final_batch_vars.remove(0);

        // Enforce batch hash matches
        computed_batch_hash.enforce_equal(&expected_batch_hash_var)?;

        // =====================================================================
        // Verify pre_state_root (anchor constraint)
        // =====================================================================

        // Compute pre_state_root from initial_accounts using same algorithm
        let mut pre_sponge = PoseidonSpongeVar::new(cs.clone(), &self.poseidon_config);

        // Use same domain separator
        pre_sponge.absorb(&vec![&domain_separator_var, &batch_id_var])?;
        let mut pre_state_vars = pre_sponge.squeeze_field_elements(1)?;
        let mut pre_state = pre_state_vars.remove(0);

        // Fold initial accounts (before transactions)
        for (pk_bytes, acc_var) in account_vars.iter() {
            let pk_var =
                FpVar::new_witness(cs.clone(), || Ok(Fr::from_le_bytes_mod_order(pk_bytes)))?;

            let mut leaf_sponge = PoseidonSpongeVar::new(cs.clone(), &self.poseidon_config);
            leaf_sponge.absorb(&vec![&pk_var, &acc_var.balance])?;
            let mut leaf_vars = leaf_sponge.squeeze_field_elements(1)?;
            let leaf_hash = leaf_vars.remove(0);

            let mut fold_sponge = PoseidonSpongeVar::new(cs.clone(), &self.poseidon_config);
            fold_sponge.absorb(&vec![&pre_state, &leaf_hash])?;
            let mut next_vars = fold_sponge.squeeze_field_elements(1)?;
            pre_state = next_vars.remove(0);
        }

        // Finalize with count
        let pre_count = Fr::from(account_vars.len() as u64);
        let pre_count_var = FpVar::new_witness(cs.clone(), || Ok(pre_count))?;

        let mut final_pre_sponge = PoseidonSpongeVar::new(cs.clone(), &self.poseidon_config);
        final_pre_sponge.absorb(&vec![&pre_state, &pre_count_var])?;
        let mut final_pre_vars = final_pre_sponge.squeeze_field_elements(1)?;
        let computed_pre_state = final_pre_vars.remove(0);

        // Enforce pre_state_root matches
        computed_pre_state.enforce_equal(&pre_state_root_var)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ark_relations::r1cs::ConstraintSystem;

    #[test]
    fn test_circuit_dummy() {
        let circuit = L2BlockCircuit::dummy();
        let cs = ConstraintSystem::<Fr>::new_ref();

        // This will fail because the dummy values don't actually compute to valid roots
        // But it tests that the circuit structure is correct
        let result = circuit.generate_constraints(cs.clone());

        // Check constraint count
        println!("Number of constraints: {}", cs.num_constraints());
        println!("Number of public inputs: {}", cs.num_instance_variables());

        // Should have 7 public inputs (+ 1 for the constant "one")
        assert_eq!(
            cs.num_instance_variables(),
            8,
            "Expected 7 public inputs + 1 constant"
        );
    }

    #[test]
    fn test_public_input_count() {
        let circuit = L2BlockCircuit::dummy();
        let cs = ConstraintSystem::<Fr>::new_ref();
        let _ = circuit.generate_constraints(cs.clone());

        // 7 public inputs + 1 (arkworks adds a constant "1" as first input)
        assert_eq!(cs.num_instance_variables(), 8);
    }
}
