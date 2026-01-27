// Nullifier Computation
//
// Nullifiers are used to prevent double-spending in shielded transactions.
// Each note can only be spent once - when spent, its nullifier is revealed
// and added to a public nullifier set.
//
// Nullifier = Poseidon(spending_key, note_commitment, position)
//
// The nullifier is:
// - Deterministic: Same note always produces same nullifier
// - Unlinkable: Cannot link nullifier to commitment without spending key
// - Unique: Each note has exactly one valid nullifier

use crate::poseidon::{hash_3, hash_4, domain_nullifier};

/// Compute a nullifier for a note
///
/// # Arguments
/// * `spending_key` - The secret spending key
/// * `commitment` - The note commitment
/// * `position` - The position in the commitment tree
///
/// # Returns
/// The nullifier that can be revealed when spending this note
pub fn compute_nullifier(
    spending_key: Field,
    commitment: Field,
    position: Field
) -> Field {
    // Domain-separated nullifier: H(domain, sk, cm, pos)
    hash_4(domain_nullifier(), spending_key, commitment, position)
}

/// Note structure for shielded transactions
pub struct Note {
    /// Recipient's public key
    pub owner: Field,
    /// Value in lamports
    pub value: Field,
    /// Random blinding factor
    pub blinding: Field,
}

impl Note {
    /// Create a new note
    pub fn new(owner: Field, value: Field, blinding: Field) -> Self {
        Note { owner, value, blinding }
    }
    
    /// Compute the note commitment
    /// commitment = Poseidon(owner, value, blinding)
    pub fn commitment(self) -> Field {
        hash_3(self.owner, self.value, self.blinding)
    }
}

/// Shielded transaction witness
pub struct ShieldedWitness {
    /// Input note being spent
    pub input_note: Note,
    /// Position in commitment tree
    pub input_position: Field,
    /// Merkle path for input note
    pub input_path: [Field; 32],
    /// Path indices
    pub input_path_indices: [Field; 32],
    /// Spending key (secret)
    pub spending_key: Field,
    /// Output note being created
    pub output_note: Note,
}

/// Verify a shielded transaction
///
/// Checks:
/// 1. Input note exists in commitment tree (Merkle proof)
/// 2. Nullifier is correctly computed
/// 3. Output note commitment is valid
/// 4. Value conservation (input value == output value)
pub fn verify_shielded_tx(
    witness: ShieldedWitness,
    commitment_root: Field,
    nullifier: Field
) -> (Field, Field) {
    // 1. Compute input commitment
    let input_commitment = witness.input_note.commitment();
    
    // 2. Verify input exists in tree
    let computed_root = crate::merkle::compute_merkle_root(
        input_commitment,
        witness.input_path,
        witness.input_path_indices
    );
    assert(computed_root == commitment_root, "Input note not in tree");
    
    // 3. Verify nullifier
    let computed_nullifier = compute_nullifier(
        witness.spending_key,
        input_commitment,
        witness.input_position
    );
    assert(computed_nullifier == nullifier, "Invalid nullifier");
    
    // 4. Verify value conservation
    assert(
        witness.input_note.value == witness.output_note.value,
        "Value not conserved"
    );
    
    // 5. Compute output commitment
    let output_commitment = witness.output_note.commitment();
    
    // Return (nullifier, new_commitment) for state updates
    (computed_nullifier, output_commitment)
}

#[test]
fn test_nullifier_computation() {
    let sk = 12345;
    let cm = 67890;
    let pos = 0;
    
    let nf1 = compute_nullifier(sk, cm, pos);
    let nf2 = compute_nullifier(sk, cm, pos);
    
    // Deterministic
    assert(nf1 == nf2);
    
    // Different position = different nullifier
    let nf3 = compute_nullifier(sk, cm, 1);
    assert(nf1 != nf3);
}

#[test]
fn test_note_commitment() {
    let note = Note::new(123, 1000, 999);
    let cm1 = note.commitment();
    let cm2 = note.commitment();
    
    assert(cm1 == cm2);
    
    // Different blinding = different commitment (privacy!)
    let note2 = Note::new(123, 1000, 888);
    let cm3 = note2.commitment();
    assert(cm1 != cm3);
}
