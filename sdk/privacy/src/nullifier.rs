//! Nullifiers
//!
//! Implements nullifier derivation for double-spend prevention.
//!
//! ```text
//! Nullifier = PRF(spending_key, note_commitment)
//! ```
//!
//! Once a nullifier is published, the corresponding note cannot be spent again.

use ark_bls12_381::Fr;
use ark_crypto_primitives::sponge::{
    CryptographicSponge,
    poseidon::{PoseidonConfig, PoseidonSponge},
};
use ark_ff::{BigInteger, PrimeField};
use serde::{Deserialize, Serialize};

use crate::commitment::Commitment;

/// A nullifier (32 bytes) - unique tag for a spent note
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Nullifier(pub [u8; 32]);

impl Nullifier {
    /// Create from field element
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

    /// Create from raw bytes
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
}

impl AsRef<[u8]> for Nullifier {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

/// Nullifier derivation key (spending key)
///
/// This is the secret key that allows spending notes.
/// Knowledge of this key is required to derive valid nullifiers.
#[derive(Debug, Clone)]
pub struct NullifierKey {
    /// The secret spending key
    key: [u8; 32],
    /// Poseidon config for PRF
    config: PoseidonConfig<Fr>,
}

impl NullifierKey {
    /// Create from raw bytes
    pub fn from_bytes(key: [u8; 32]) -> Self {
        Self {
            key,
            config: poseidon_config(),
        }
    }

    /// Derive a nullifier for a note
    ///
    /// Nullifier = PRF_nk(commitment || position)
    ///
    /// # Arguments
    /// * `commitment` - The note commitment
    /// * `position` - The note's position in the Merkle tree (prevents nullifier grinding)
    pub fn derive_nullifier(&self, commitment: &Commitment, position: u64) -> Nullifier {
        let mut sponge = PoseidonSponge::new(&self.config);

        // Domain separation
        let domain = Fr::from(0x4e554c4c_u64); // "NULL" in hex
        sponge.absorb(&domain);

        // Absorb spending key
        let key_f = Fr::from_le_bytes_mod_order(&self.key);
        sponge.absorb(&key_f);

        // Absorb commitment
        sponge.absorb(&commitment.to_field());

        // Absorb position for uniqueness
        let pos_f = Fr::from(position);
        sponge.absorb(&pos_f);

        // Squeeze nullifier
        let result: Fr = sponge.squeeze_field_elements(1)[0];
        Nullifier::from_field(result)
    }

    /// Derive the internal nullifier key (nk) from spending key (ask)
    ///
    /// nk = Poseidon("ZelanaNK", ask)
    pub fn derive_nk(&self) -> [u8; 32] {
        let mut sponge = PoseidonSponge::new(&self.config);

        // Domain separation
        let domain = Fr::from_le_bytes_mod_order(
            b"ZelanaNK\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
        );
        sponge.absorb(&domain);

        let key_f = Fr::from_le_bytes_mod_order(&self.key);
        sponge.absorb(&key_f);

        let result: Fr = sponge.squeeze_field_elements(1)[0];
        let bytes = result.into_bigint().to_bytes_le();
        let mut arr = [0u8; 32];
        arr[..bytes.len()].copy_from_slice(&bytes);
        arr
    }
}

/// Poseidon configuration for nullifier derivation
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
    fn test_nullifier_deterministic() {
        let key = NullifierKey::from_bytes([1u8; 32]);
        let commitment = Commitment([2u8; 32]);
        let position = 100u64;

        let n1 = key.derive_nullifier(&commitment, position);
        let n2 = key.derive_nullifier(&commitment, position);

        assert_eq!(n1, n2, "same inputs should produce same nullifier");
    }

    #[test]
    fn test_nullifier_unique_per_note() {
        let key = NullifierKey::from_bytes([1u8; 32]);
        let c1 = Commitment([1u8; 32]);
        let c2 = Commitment([2u8; 32]);

        let n1 = key.derive_nullifier(&c1, 0);
        let n2 = key.derive_nullifier(&c2, 0);

        assert_ne!(n1, n2, "different notes should have different nullifiers");
    }

    #[test]
    fn test_nullifier_requires_key() {
        let key1 = NullifierKey::from_bytes([1u8; 32]);
        let key2 = NullifierKey::from_bytes([2u8; 32]);
        let commitment = Commitment([3u8; 32]);

        let n1 = key1.derive_nullifier(&commitment, 0);
        let n2 = key2.derive_nullifier(&commitment, 0);

        assert_ne!(n1, n2, "different keys should produce different nullifiers");
    }

    #[test]
    fn test_position_affects_nullifier() {
        let key = NullifierKey::from_bytes([1u8; 32]);
        let commitment = Commitment([2u8; 32]);

        let n1 = key.derive_nullifier(&commitment, 0);
        let n2 = key.derive_nullifier(&commitment, 1);

        assert_ne!(
            n1, n2,
            "different positions should produce different nullifiers"
        );
    }
}
