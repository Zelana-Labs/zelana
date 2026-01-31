//! Zelana Ownership Prover - WASM Client Library
//!
//! This crate provides client-side cryptographic primitives for the ownership proof.
//! It implements the exact same MiMC hash function as the Noir circuit to ensure
//! that proofs generated client-side will verify correctly.
//!
//! # Usage
//!
//! ```ignore
//! use zelana_ownership_prover::{compute_commitment, compute_nullifier, compute_blinded_proxy};
//!
//! let owner_pk = derive_public_key(spending_key);
//! let commitment = compute_commitment(owner_pk, value, blinding);
//! let nullifier = compute_nullifier(spending_key, commitment, position);
//! let blinded_proxy = compute_blinded_proxy(commitment, position);
//! ```

pub mod mimc;

#[cfg(feature = "wasm")]
pub mod wasm;

use ark_bn254::Fr;
use ark_ff::PrimeField;
use mimc::{delegate_domain, domain_nullifier, hash_3, hash_4, pk_domain};

/// A 32-byte value (field element serialized)
pub type Bytes32 = [u8; 32];

/// Convert bytes to BN254 field element
pub fn bytes_to_field(bytes: &[u8; 32]) -> Fr {
    Fr::from_le_bytes_mod_order(bytes)
}

/// Convert field element to bytes
pub fn field_to_bytes(f: Fr) -> Bytes32 {
    use ark_ff::BigInteger;
    let bigint = f.into_bigint();
    let le_bytes = bigint.to_bytes_le();
    let mut result = [0u8; 32];
    result[..le_bytes.len().min(32)].copy_from_slice(&le_bytes[..le_bytes.len().min(32)]);
    result
}

/// Derive public key from spending key
///
/// pk = MiMC_hash3(PK_DOMAIN, spending_key, 0)
pub fn derive_public_key(spending_key: Fr) -> Fr {
    hash_3(pk_domain(), spending_key, Fr::from(0u64))
}

/// Derive public key from spending key (bytes version)
pub fn derive_public_key_bytes(spending_key: &Bytes32) -> Bytes32 {
    let sk = bytes_to_field(spending_key);
    let pk = derive_public_key(sk);
    field_to_bytes(pk)
}

/// Compute note commitment
///
/// commitment = MiMC_hash3(owner_pk, value, blinding)
pub fn compute_commitment(owner_pk: Fr, value: u64, blinding: Fr) -> Fr {
    hash_3(owner_pk, Fr::from(value), blinding)
}

/// Compute note commitment (bytes version)
pub fn compute_commitment_bytes(owner_pk: &Bytes32, value: u64, blinding: &Bytes32) -> Bytes32 {
    let pk = bytes_to_field(owner_pk);
    let b = bytes_to_field(blinding);
    let cm = compute_commitment(pk, value, b);
    field_to_bytes(cm)
}

/// Compute nullifier
///
/// nullifier = MiMC_hash4(NULLIFIER_DOMAIN, spending_key, commitment, position)
pub fn compute_nullifier(spending_key: Fr, commitment: Fr, position: u64) -> Fr {
    hash_4(
        domain_nullifier(),
        spending_key,
        commitment,
        Fr::from(position),
    )
}

/// Compute nullifier (bytes version)
pub fn compute_nullifier_bytes(
    spending_key: &Bytes32,
    commitment: &Bytes32,
    position: u64,
) -> Bytes32 {
    let sk = bytes_to_field(spending_key);
    let cm = bytes_to_field(commitment);
    let nf = compute_nullifier(sk, cm, position);
    field_to_bytes(nf)
}

/// Compute blinded proxy for delegation
///
/// blinded_proxy = MiMC_hash3(DELEGATE_DOMAIN, commitment, position)
pub fn compute_blinded_proxy(commitment: Fr, position: u64) -> Fr {
    hash_3(delegate_domain(), commitment, Fr::from(position))
}

/// Compute blinded proxy (bytes version)
pub fn compute_blinded_proxy_bytes(commitment: &Bytes32, position: u64) -> Bytes32 {
    let cm = bytes_to_field(commitment);
    let bp = compute_blinded_proxy(cm, position);
    field_to_bytes(bp)
}

/// Complete ownership proof inputs
///
/// This struct contains all the values needed to generate an ownership proof.
/// The private inputs stay on the client; public inputs are revealed.
#[derive(Debug, Clone)]
pub struct OwnershipWitness {
    /// Private: spending key
    pub spending_key: Fr,
    /// Private: note value in lamports
    pub note_value: u64,
    /// Private: note blinding factor
    pub note_blinding: Fr,
    /// Private: position in commitment tree
    pub note_position: u64,
    /// Public: note commitment
    pub commitment: Fr,
    /// Public: nullifier
    pub nullifier: Fr,
    /// Public: blinded proxy
    pub blinded_proxy: Fr,
}

impl OwnershipWitness {
    /// Create witness from private inputs, computing all public outputs
    pub fn from_private_inputs(
        spending_key: Fr,
        note_value: u64,
        note_blinding: Fr,
        note_position: u64,
    ) -> Self {
        // Derive public key
        let owner_pk = derive_public_key(spending_key);

        // Compute commitment
        let commitment = compute_commitment(owner_pk, note_value, note_blinding);

        // Compute nullifier
        let nullifier = compute_nullifier(spending_key, commitment, note_position);

        // Compute blinded proxy
        let blinded_proxy = compute_blinded_proxy(commitment, note_position);

        Self {
            spending_key,
            note_value,
            note_blinding,
            note_position,
            commitment,
            nullifier,
            blinded_proxy,
        }
    }

    /// Verify that the public outputs match the private inputs
    /// This is useful for sanity checking before generating a proof
    pub fn verify(&self) -> bool {
        let owner_pk = derive_public_key(self.spending_key);
        let computed_commitment = compute_commitment(owner_pk, self.note_value, self.note_blinding);
        let computed_nullifier =
            compute_nullifier(self.spending_key, computed_commitment, self.note_position);
        let computed_proxy = compute_blinded_proxy(computed_commitment, self.note_position);

        computed_commitment == self.commitment
            && computed_nullifier == self.nullifier
            && computed_proxy == self.blinded_proxy
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_witness_creation() {
        let spending_key = Fr::from(12345u64);
        let note_value = 1_000_000_000u64; // 1 SOL
        let note_blinding = Fr::from(9999999u64);
        let note_position = 0u64;

        let witness = OwnershipWitness::from_private_inputs(
            spending_key,
            note_value,
            note_blinding,
            note_position,
        );

        assert!(witness.verify(), "Witness should be valid");
    }

    #[test]
    fn test_different_positions_different_nullifiers() {
        let spending_key = Fr::from(12345u64);
        let note_value = 1_000_000_000u64;
        let note_blinding = Fr::from(9999999u64);

        let witness_0 =
            OwnershipWitness::from_private_inputs(spending_key, note_value, note_blinding, 0);
        let witness_1 =
            OwnershipWitness::from_private_inputs(spending_key, note_value, note_blinding, 1);

        // Same commitment (same note)
        assert_eq!(witness_0.commitment, witness_1.commitment);
        // Different nullifiers (different positions)
        assert_ne!(witness_0.nullifier, witness_1.nullifier);
        // Different blinded proxies
        assert_ne!(witness_0.blinded_proxy, witness_1.blinded_proxy);
    }

    #[test]
    fn test_bytes_roundtrip() {
        let spending_key = [42u8; 32];
        let pk_bytes = derive_public_key_bytes(&spending_key);

        // Should produce a valid 32-byte output
        assert_eq!(pk_bytes.len(), 32);

        // Same input should produce same output
        let pk_bytes_2 = derive_public_key_bytes(&spending_key);
        assert_eq!(pk_bytes, pk_bytes_2);
    }

    /// This test prints the expected values for verifying against the Noir circuit.
    /// Run with: cargo test print_noir_test_values -- --nocapture
    #[test]
    fn print_noir_test_values() {
        use ark_ff::BigInteger;

        let spending_key = Fr::from(12345u64);
        let note_value = 1_000_000_000u64; // 1 SOL in lamports
        let note_blinding = Fr::from(9999999u64);
        let note_position = 0u64;

        let witness = OwnershipWitness::from_private_inputs(
            spending_key,
            note_value,
            note_blinding,
            note_position,
        );

        // Convert to hex for Noir Prover.toml
        let cm_hex = hex::encode(field_to_bytes(witness.commitment));
        let nf_hex = hex::encode(field_to_bytes(witness.nullifier));
        let bp_hex = hex::encode(field_to_bytes(witness.blinded_proxy));

        // Print as Field values (decimal) for Noir
        println!("\n=== Noir Circuit Test Values ===");
        println!("spending_key = \"12345\"");
        println!("note_value = \"1000000000\"");
        println!("note_blinding = \"9999999\"");
        println!("note_position = \"0\"");
        println!();
        println!("commitment = \"0x{}\"", cm_hex);
        println!("nullifier = \"0x{}\"", nf_hex);
        println!("blinded_proxy = \"0x{}\"", bp_hex);
        println!();

        // Also print the raw decimal values for Noir (Field is decimal in Noir)
        println!("# As decimal (for Noir):");
        println!("commitment = \"{}\"", witness.commitment.into_bigint());
        println!("nullifier = \"{}\"", witness.nullifier.into_bigint());
        println!(
            "blinded_proxy = \"{}\"",
            witness.blinded_proxy.into_bigint()
        );
    }
}
