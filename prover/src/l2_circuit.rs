use ark_bn254::{Fr, fr};
use ark_crypto_primitives::sponge::poseidon::{PoseidonConfig, constraints::PoseidonSpongeVar, find_poseidon_ark_and_mds};
use ark_ff::PrimeField;
use ark_r1cs_std::{
    prelude::*,
    fields::fp::FpVar,
    uint8::UInt8,
};
use ark_relations::r1cs::{ConstraintSynthesizer, ConstraintSystemRef, SynthesisError};
use std::collections::BTreeMap;
use ark_crypto_primitives::sponge::constraints::CryptographicSpongeVar;

pub type PubkeyBytes = [u8; 32]; // Plain bytes for map keys

// Represents an Account (balance only for MVP) in the field Fr
#[derive(Clone, Debug)]
pub struct AccountVar {
    pub balance: FpVar<Fr>,
    // NonceVar would be added here later
}

// Represents a Transaction (Transfer only for MVP) in the field Fr
#[derive(Clone, Debug)]
pub struct TransactionWitness {
    // Witness data provided to the circuit
    pub sender_pk: PubkeyBytes,
    pub recipient_pk: PubkeyBytes,
    pub amount: u64,
}

// --- Helper: Poseidon Parameters ---
// Define or load Poseidon parameters suitable for arkworks 0.4 and BN254.
// These parameters (MDS matrix, ARK constants) are crucial for security.
// Using standard parameters for rate=2 as an example.
fn get_poseidon_config() -> PoseidonConfig<Fr> {
     // Example parameters (adjust based on security requirements and arkworks version specifics)
    let full_rounds = 8;
    // Partial rounds calculation depends on field size and security level, e.g., 56 for BN254 often used
    let partial_rounds = 56;
    let alpha = 5u64;
    let rate = 2; // Arity of the hash (absorbs 2 elements at a time)
    let capacity = 1; // Security parameter
    let security_level = 128; // Example security level

    let (ark, mds) = find_poseidon_ark_and_mds::<Fr>(
         Fr::MODULUS_BIT_SIZE as u64, // Field size
         rate , // Rate
         full_rounds as u64,
         partial_rounds as u64,
         0, // skip_matrices, adjust if needed
     );
    PoseidonConfig::new(full_rounds, partial_rounds, alpha, mds, ark, rate, capacity)
}

// Represents the state commitment hash (32 bytes)
pub type StateRootVar = Vec<UInt8<Fr>>;

// Circuit definition
#[derive(Clone)]
pub struct L2BlockCircuit {
    // --- Public Inputs ---
    /// The state root before the batch, provided as bytes.
    pub prev_root: Option<[u8; 32]>,
    /// The state root after the batch, provided as bytes.
    pub new_root: Option<[u8; 32]>,

    // --- Private Witness ---
    /// The list of transactions included in the batch.
    pub transactions: Option<Vec<TransactionWitness>>,
    /// The state of all accounts *before* this batch was applied.
    /// Uses raw bytes as keys for easier witness generation outside the circuit.
    pub initial_accounts: Option<BTreeMap<PubkeyBytes, u64>>,
    pub batch_id: Option<u64>,
    pub poseidon_config: PoseidonConfig<Fr>,
}

impl ConstraintSynthesizer<Fr> for L2BlockCircuit {
    fn generate_constraints(self, cs: ConstraintSystemRef<Fr>) -> Result<(), SynthesisError> {
        println!("Generating L2 Block Constraints...");

        let poseidon_config = get_poseidon_config();
       // --- Allocate Public Inputs ---
        let prev_root_bytes = self.prev_root.ok_or(SynthesisError::AssignmentMissing)?;
        let _prev_root_var = FpVar::new_input(cs.clone(), || Ok(Fr::from_le_bytes_mod_order(&prev_root_bytes)))?;
 
        let new_root_bytes = self.new_root.ok_or(SynthesisError::AssignmentMissing)?;
        let expected_new_root_var = FpVar::new_input(cs.clone(), || Ok(Fr::from_le_bytes_mod_order(&new_root_bytes)))?;
        println!("   ✅ Allocated public inputs (prev_root, new_root).");
 
       // --- Allocate Private Witness ---
        let transactions_witness = self.transactions.ok_or(SynthesisError::AssignmentMissing)?;
        let initial_accounts_witness = self.initial_accounts.ok_or(SynthesisError::AssignmentMissing)?;
        let batch_id_val = self.batch_id.ok_or(SynthesisError::AssignmentMissing)?;
        let batch_id_fr = Fr::from(batch_id_val);
        let batch_id_var = FpVar::new_witness(cs.clone(), || Ok(batch_id_fr))?;

        // Allocate initial accounts
        let mut account_vars = BTreeMap::new();
        for (pk_bytes, balance_u64) in initial_accounts_witness.iter() {
             let balance_var = FpVar::new_witness(cs.clone(), || Ok(Fr::from(*balance_u64)))?;
             account_vars.insert(*pk_bytes, AccountVar { balance: balance_var });
        }
        println!("   Allocated {} initial account witnesses.", account_vars.len());

        // --- Apply Transaction Logic Constraints ---
        println!("   Applying transaction constraints...");
        let mut current_account_vars = account_vars.clone(); // State evolves per transaction

        for (i, tx_witness) in transactions_witness.iter().enumerate() {
            println!("      -> Processing Tx {}", i);

            let sender_pk_bytes = tx_witness.sender_pk;
            let recipient_pk_bytes = tx_witness.recipient_pk;
            let amount_var = FpVar::new_witness(cs.clone(), || Ok(Fr::from(tx_witness.amount)))?;
            let mut sender_acc = current_account_vars.get(&sender_pk_bytes).cloned().ok_or(SynthesisError::AssignmentMissing)?;
            let recipient_acc_initial = current_account_vars.get(&recipient_pk_bytes).cloned().unwrap_or(AccountVar { balance: FpVar::zero() });
            let mut recipient_acc = recipient_acc_initial.clone();
            sender_acc.balance.enforce_cmp(&amount_var, core::cmp::Ordering::Greater, true)?;
            let new_sender_balance = &sender_acc.balance - &amount_var;
            let new_recipient_balance = &recipient_acc.balance + &amount_var;
            sender_acc.balance = new_sender_balance;
            recipient_acc.balance = new_recipient_balance;
            current_account_vars.insert(sender_pk_bytes, sender_acc);
            current_account_vars.insert(recipient_pk_bytes, recipient_acc);
        }
        println!("   Transaction constraints applied.");

        // Instantiate the Poseidon sponge gadget
        // Instantiate the Poseidon sponge gadget (Needs ConstraintSystemRef)
         let mut sponge_var = PoseidonSpongeVar::new(cs.clone(), &self.poseidon_config);
 
         // Calculate initial state S0 = Poseidon(ds, batch_id)
         // Ensure domain separator matches EXACTLY the off-chain version
         let ds_bytes: &[u8] = b"zelana:accounts-fold:v1";
         let ds_fr = Fr::from_le_bytes_mod_order(ds_bytes); // Convert bytes to field element
         let domain_separator_var = FpVar::new_constant(cs.clone(), ds_fr)?; // Allocate as constant
 
         // Absorb requires a slice `&[T]` where T implements AbsorbGadget. FpVar does.
        let inputs: Vec<&FpVar<Fr>> = vec![&domain_separator_var, &batch_id_var];
        sponge_var.absorb(&inputs)?;
         let mut current_state_vars = sponge_var.squeeze_field_elements(1)?; // Squeeze 1 Fr element
         let mut current_state_var = current_state_vars.remove(0); // This is S0
         println!("      Computed initial hash state S0.");
 
         // Iterate over final account states (BTreeMap ensures sorted order by PubkeyBytes)
         for (pk_bytes, final_acc_var) in current_account_vars.iter() {
             // Convert pubkey bytes to Field element variable for hashing
             // Allocate as witness because the prover knows the bytes
             let pk_var = FpVar::new_witness(cs.clone(), || Ok(Fr::from_le_bytes_mod_order(pk_bytes)))?;
 
             // Simplified MVP Leaf hash: leaf = Poseidon(pk_var, balance_var)
             let mut leaf_sponge = PoseidonSpongeVar::new(cs.clone(), &self.poseidon_config);
             // Absorb pk_var and the final balance variable

             let inputs1 : Vec<&FpVar<Fr>>  = vec![&pk_var, &final_acc_var.balance];
             leaf_sponge.absorb(&inputs1)?; // Pass as slice
             let mut leaf_hash_vars = leaf_sponge.squeeze_field_elements(1)?;
             let leaf_hash_var = leaf_hash_vars.remove(0);
 
             // Fold: S_{i+1} = Poseidon(S_i, leaf_i)
             let mut fold_sponge = PoseidonSpongeVar::new(cs.clone(), &self.poseidon_config);
             let inputs2 : Vec<&FpVar<Fr>> = vec![&current_state_var, &leaf_hash_var];
             fold_sponge.absorb(&inputs2)?; // Pass as slice
             let mut next_state_vars = fold_sponge.squeeze_field_elements(1)?;
             current_state_var = next_state_vars.remove(0); // Update S_i
         }
         println!("      Folded {} accounts using Poseidon gadget.", current_account_vars.len());
 
         // Finalize with count: computed_new_root = Poseidon(S_last, Fr(account_count))
         let account_count_fr = Fr::from(current_account_vars.len() as u64);
         let account_count_var = FpVar::new_witness(cs.clone(), || Ok(account_count_fr))?;
 
         let mut final_sponge = PoseidonSpongeVar::new(cs.clone(), &self.poseidon_config);
         let inputs4:Vec<&FpVar<Fr>> = vec![&current_state_var, &account_count_var];
         final_sponge.absorb(&inputs4)?; // Pass as slice
         let mut final_root_vars = final_sponge.squeeze_field_elements(1)?;
         let computed_new_root_var = final_root_vars.remove(0); // This is the computed new_root
         println!("      Computed final state root hash variable using Poseidon gadget.");
 
         // --- Final Equality Constraint ---
         // Enforce computed_new_root (from gadget) == expected_new_root (public input)
         computed_new_root_var.enforce_equal(&expected_new_root_var)?;
         println!("   ✅ Constraint Added: computed_poseidon_root == expected_new_root");
 
         println!("✅ L2 Block Constraints Generation Complete!");
         Ok(())

    }
}