//! Note Commitments
//!
//! Implements Poseidon-based commitments for notes.
//!
//! ```text
//! Commitment = Poseidon(value || randomness || owner_pk)
//! ```
//!
//! This hides the note contents while allowing ZK proofs of knowledge.

use ark_bls12_381::Fr;
use ark_crypto_primitives::sponge::{
    CryptographicSponge,
    poseidon::{PoseidonConfig, PoseidonSponge},
};
use ark_ff::{BigInteger, PrimeField};
use ark_std::rand::Rng;
use serde::{Deserialize, Serialize};

/// A note commitment (32 bytes)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Commitment(pub [u8; 32]);

impl Commitment {
    /// Create commitment from field element
    pub fn from_field(f: Fr) -> Self {
        let bytes = f.into_bigint().to_bytes_le();
        let mut arr = [0u8; 32];
        arr[..bytes.len()].copy_from_slice(&bytes);
        Self(arr)
    }

    /// Convert to field element
    pub fn to_field(&self) -> Fr {
        Fr::from_le_bytes_mod_order(&self.0)
    }

    /// Get raw bytes
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl AsRef<[u8]> for Commitment {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

/// Commitment scheme using Poseidon hash
pub struct CommitmentScheme {
    config: PoseidonConfig<Fr>,
}

impl CommitmentScheme {
    /// Create a new commitment scheme with Zelana Poseidon parameters
    pub fn new() -> Self {
        Self {
            config: poseidon_config(),
        }
    }

    /// Commit to a note: C = Poseidon(value, randomness, owner_pk_x, owner_pk_y)
    ///
    /// # Arguments
    /// * `value` - The note value (amount)
    /// * `randomness` - Random blinding factor (32 bytes)
    /// * `owner_pk` - Owner's public key (32 bytes, compressed)
    pub fn commit(&self, value: u64, randomness: &[u8; 32], owner_pk: &[u8; 32]) -> Commitment {
        let mut sponge = PoseidonSponge::new(&self.config);

        // Convert inputs to field elements
        let value_f = Fr::from(value);
        let randomness_f = Fr::from_le_bytes_mod_order(randomness);
        let owner_f = Fr::from_le_bytes_mod_order(owner_pk);

        // Absorb all inputs
        sponge.absorb(&value_f);
        sponge.absorb(&randomness_f);
        sponge.absorb(&owner_f);

        // Squeeze commitment
        let result: Fr = sponge.squeeze_field_elements(1)[0];
        Commitment::from_field(result)
    }

    /// Commit with additional data (for extended note types)
    pub fn commit_extended(
        &self,
        value: u64,
        randomness: &[u8; 32],
        owner_pk: &[u8; 32],
        asset_id: &[u8; 32],
    ) -> Commitment {
        let mut sponge = PoseidonSponge::new(&self.config);

        let value_f = Fr::from(value);
        let randomness_f = Fr::from_le_bytes_mod_order(randomness);
        let owner_f = Fr::from_le_bytes_mod_order(owner_pk);
        let asset_f = Fr::from_le_bytes_mod_order(asset_id);

        sponge.absorb(&value_f);
        sponge.absorb(&randomness_f);
        sponge.absorb(&owner_f);
        sponge.absorb(&asset_f);

        let result: Fr = sponge.squeeze_field_elements(1)[0];
        Commitment::from_field(result)
    }

    /// Generate random blinding factor
    pub fn random_blinding<R: Rng>(rng: &mut R) -> [u8; 32] {
        let mut bytes = [0u8; 32];
        rng.fill_bytes(&mut bytes);
        bytes
    }
}

impl Default for CommitmentScheme {
    fn default() -> Self {
        Self::new()
    }
}

/// Poseidon configuration for Zelana
///
/// Field: BLS12-381 Fr (255 bits)
/// Rate: 2, Capacity: 1
/// Security: 128 bits
fn poseidon_config() -> PoseidonConfig<Fr> {
    use ark_crypto_primitives::sponge::poseidon::find_poseidon_ark_and_mds;

    let prime_bits: u64 = 255;
    let rate: usize = 2;
    let capacity: usize = 1;
    let full_rounds: u64 = 8;
    let partial_rounds: u64 = 57;
    let alpha: u64 = 5;
    let skip_matrices: u64 = 0;

    let (ark, mds) = find_poseidon_ark_and_mds::<Fr>(
        prime_bits,
        rate,
        full_rounds,
        partial_rounds,
        skip_matrices,
    );

    PoseidonConfig::new(
        full_rounds as usize,
        partial_rounds as usize,
        alpha,
        mds,
        ark,
        rate,
        capacity,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_commitment_deterministic() {
        let scheme = CommitmentScheme::new();
        let value = 1000u64;
        let randomness = [42u8; 32];
        let owner_pk = [1u8; 32];

        let c1 = scheme.commit(value, &randomness, &owner_pk);
        let c2 = scheme.commit(value, &randomness, &owner_pk);

        assert_eq!(c1, c2, "same inputs should produce same commitment");
    }

    #[test]
    fn test_commitment_hiding() {
        let scheme = CommitmentScheme::new();
        let value = 1000u64;
        let owner_pk = [1u8; 32];

        let c1 = scheme.commit(value, &[1u8; 32], &owner_pk);
        let c2 = scheme.commit(value, &[2u8; 32], &owner_pk);

        assert_ne!(
            c1, c2,
            "different randomness should produce different commitments"
        );
    }

    #[test]
    fn test_commitment_binding() {
        let scheme = CommitmentScheme::new();
        let randomness = [42u8; 32];
        let owner_pk = [1u8; 32];

        let c1 = scheme.commit(1000, &randomness, &owner_pk);
        let c2 = scheme.commit(2000, &randomness, &owner_pk);

        assert_ne!(
            c1, c2,
            "different values should produce different commitments"
        );
    }
}
