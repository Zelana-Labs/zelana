// Ownership Circuit - Client-Side Proof
//
// This is a lightweight circuit that runs in the user's browser/mobile.
// It proves the user owns a note (knows the spending key) without revealing
// the spending key to anyone.
//
// The proof is then sent to the Swarm (zelana-forge prover workers) which
// performs the heavy Merkle membership verification in a separate circuit.
//
// Performance: ~500ms proving time in WASM, ~5ms verification
// Proof size: ~4KB (UltraHonk)
//
// === Inputs ===
// Private:
//   - spending_key: The user's secret spending key (derived from Solana wallet)
//   - note_value: The value of the note in lamports
//   - note_blinding: Random blinding factor that makes the commitment unique
//   - note_position: Position of this note in the commitment Merkle tree
//
// Public:
//   - commitment: The note commitment = H(owner_pk, value, blinding)
//   - nullifier: Unique identifier revealed when spending
//   - blinded_proxy: Allows Swarm to fetch Merkle path

use zelana_lib::poseidon::hash_3;
use zelana_lib::nullifier::compute_nullifier;

// Domain separators (Must match Rust SDK and TypeScript SDK)
// These prevent cross-protocol attacks by ensuring hashes are unique per use case
global DELEGATE_DOMAIN: Field = 0x44454c45; // "DELE" in ASCII (68, 69, 76, 69)
global PK_DOMAIN: Field = 0x504b;           // "PK" in ASCII (80, 75)

/// Main entry point for the ownership proof
fn main(
    // Private Inputs (Never leave the user's device)
    spending_key: Field,
    note_value: Field,
    note_blinding: Field,
    note_position: Field,

    // Public Inputs (Revealed to Swarm)
    commitment: pub Field,
    nullifier: pub Field,
    blinded_proxy: pub Field
) {
    // =========================================================================
    // Step 1: Derive Public Key from Spending Key
    // =========================================================================
    // We re-derive the public key inside the circuit to prove that the
    // spending_key is the correct preimage. This is a one-way derivation.
    //
    // pk = H(PK_DOMAIN, spending_key, 0)
    let owner_pk = hash_3(PK_DOMAIN, spending_key, 0);

    // =========================================================================
    // Step 2: Verify Note Commitment
    // =========================================================================
    // The commitment binds the note's value and blinding to the owner.
    // commitment = H(owner_pk, value, blinding)
    let computed_commitment = hash_3(owner_pk, note_value, note_blinding);
    assert(computed_commitment == commitment, "Commitment mismatch");

    // =========================================================================
    // Step 3: Verify Nullifier
    // =========================================================================
    // The nullifier is deterministically derived from the spending key and
    // commitment. It's unique per note and can only be computed by the owner.
    // nullifier = H(NULLIFIER_DOMAIN, spending_key, commitment, position)
    let computed_nullifier = compute_nullifier(spending_key, commitment, note_position);
    assert(nullifier == computed_nullifier, "Nullifier mismatch");

    // =========================================================================
    // Step 4: Verify Blinded Proxy
    // =========================================================================
    // The blinded proxy allows the Swarm to fetch the Merkle path for this
    // note without the user having to send additional data.
    // blinded_proxy = H(DELEGATE_DOMAIN, commitment, position)
    let computed_proxy = hash_3(DELEGATE_DOMAIN, commitment, note_position);
    assert(blinded_proxy == computed_proxy, "Blinded proxy mismatch");
}

// =============================================================================
// Test Cases
// =============================================================================

#[test]
fn test_basic_ownership_proof() {
    // Test values
    let spending_key = 12345;
    let note_value = 1000000000; // 1 SOL in lamports
    let note_blinding = 9999999;
    let note_position = 0;
    
    // Derive expected public outputs
    let owner_pk = hash_3(PK_DOMAIN, spending_key, 0);
    let commitment = hash_3(owner_pk, note_value, note_blinding);
    let nullifier = compute_nullifier(spending_key, commitment, note_position);
    let blinded_proxy = hash_3(DELEGATE_DOMAIN, commitment, note_position);
    
    // This should not panic
    main(
        spending_key,
        note_value,
        note_blinding,
        note_position,
        commitment,
        nullifier,
        blinded_proxy
    );
}

#[test]
fn test_different_positions_different_nullifiers() {
    let spending_key = 12345;
    let note_value = 1000000000;
    let note_blinding = 9999999;
    
    // Same note, different positions
    let owner_pk = hash_3(PK_DOMAIN, spending_key, 0);
    let commitment = hash_3(owner_pk, note_value, note_blinding);
    
    let nullifier_0 = compute_nullifier(spending_key, commitment, 0);
    let nullifier_1 = compute_nullifier(spending_key, commitment, 1);
    
    // Different positions must produce different nullifiers
    assert(nullifier_0 != nullifier_1);
}

#[test]
fn test_different_spending_keys_different_nullifiers() {
    let note_value = 1000000000;
    let note_blinding = 9999999;
    let note_position = 0;
    
    // Two different spending keys
    let sk1 = 12345;
    let sk2 = 67890;
    
    let pk1 = hash_3(PK_DOMAIN, sk1, 0);
    let pk2 = hash_3(PK_DOMAIN, sk2, 0);
    
    let cm1 = hash_3(pk1, note_value, note_blinding);
    let cm2 = hash_3(pk2, note_value, note_blinding);
    
    let nf1 = compute_nullifier(sk1, cm1, note_position);
    let nf2 = compute_nullifier(sk2, cm2, note_position);
    
    // Different keys produce different nullifiers (and different commitments)
    assert(cm1 != cm2);
    assert(nf1 != nf2);
}
