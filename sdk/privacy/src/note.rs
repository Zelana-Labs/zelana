//! Shielded Notes
//!
//! A Note represents value held privately on Zelana L2.
//!
//! ```text
//! Note = {
//!     value: u64,           // Amount in the smallest unit
//!     randomness: [u8; 32], // Blinding factor
//!     owner_pk: [u8; 32],   // Owner's public key
//!     position: u64,        // Position in commitment tree (set on insertion)
//! }
//! ```

use ark_std::rand::Rng;
use serde::{Deserialize, Serialize};

use crate::commitment::{Commitment, CommitmentScheme};
use crate::nullifier::{Nullifier, NullifierKey};

/// A shielded note representing privately held value
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Note {
    /// The value (amount) held in this note
    pub value: NoteValue,
    /// Random blinding factor for hiding commitment
    pub randomness: [u8; 32],
    /// Owner's public key (who can spend this note)
    pub owner_pk: [u8; 32],
    /// Position in the commitment Merkle tree (None if not yet inserted)
    pub position: Option<u64>,
}

/// Note value with overflow protection
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoteValue(pub u64);

impl NoteValue {
    pub const ZERO: Self = Self(0);
    pub const MAX: Self = Self(u64::MAX);

    pub fn new(value: u64) -> Self {
        Self(value)
    }

    pub fn as_u64(&self) -> u64 {
        self.0
    }

    /// Checked addition
    pub fn checked_add(self, rhs: Self) -> Option<Self> {
        self.0.checked_add(rhs.0).map(Self)
    }

    /// Checked subtraction
    pub fn checked_sub(self, rhs: Self) -> Option<Self> {
        self.0.checked_sub(rhs.0).map(Self)
    }
}

impl Note {
    /// Create a new note with random blinding
    pub fn new<R: Rng>(value: u64, owner_pk: [u8; 32], rng: &mut R) -> Self {
        let mut randomness = [0u8; 32];
        rng.fill_bytes(&mut randomness);

        Self {
            value: NoteValue(value),
            randomness,
            owner_pk,
            position: None,
        }
    }

    /// Create a note with explicit randomness (for testing/recovery)
    pub fn with_randomness(value: u64, owner_pk: [u8; 32], randomness: [u8; 32]) -> Self {
        Self {
            value: NoteValue(value),
            randomness,
            owner_pk,
            position: None,
        }
    }

    /// Compute the commitment for this note
    pub fn commitment(&self) -> Commitment {
        let scheme = CommitmentScheme::new();
        scheme.commit(self.value.0, &self.randomness, &self.owner_pk)
    }

    /// Derive the nullifier for spending this note
    ///
    /// Requires the spending key and that position is set
    pub fn nullifier(&self, spending_key: &SpendingKey) -> Option<Nullifier> {
        let position = self.position?;
        let nk = spending_key.nullifier_key();
        Some(nk.derive_nullifier(&self.commitment(), position))
    }

    /// Set the Merkle tree position (called after insertion)
    pub fn with_position(mut self, position: u64) -> Self {
        self.position = Some(position);
        self
    }

    /// Check if this note has been inserted into the tree
    pub fn is_inserted(&self) -> bool {
        self.position.is_some()
    }
}

/// Spending key - allows spending notes
///
/// This is the most sensitive key. Loss = loss of funds.
/// Compromise = theft of funds.
#[derive(Debug, Clone)]
pub struct SpendingKey {
    key: [u8; 32],
}

impl SpendingKey {
    /// Generate a random spending key
    pub fn random<R: Rng>(rng: &mut R) -> Self {
        let mut key = [0u8; 32];
        rng.fill_bytes(&mut key);
        Self { key }
    }

    /// Create from raw bytes
    pub fn from_bytes(key: [u8; 32]) -> Self {
        Self { key }
    }

    /// Get raw bytes
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.key
    }

    /// Derive the nullifier key
    pub fn nullifier_key(&self) -> NullifierKey {
        NullifierKey::from_bytes(self.key)
    }

    /// Derive the viewing key (for read-only access)
    pub fn viewing_key(&self) -> ViewingKey {
        // ivk = Poseidon("ZelanaIVK", ask)
        use ark_bls12_381::Fr;
        use ark_crypto_primitives::sponge::{
            CryptographicSponge,
            poseidon::{PoseidonConfig, PoseidonSponge, find_poseidon_ark_and_mds},
        };
        use ark_ff::{BigInteger, PrimeField};

        let (ark, mds) = find_poseidon_ark_and_mds::<Fr>(255, 2, 8, 57, 0);
        let config = PoseidonConfig::new(8, 57, 5, mds, ark, 2, 1);

        let mut sponge = PoseidonSponge::new(&config);

        let domain =
            Fr::from_le_bytes_mod_order(b"ZelanaIVK\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0");
        sponge.absorb(&domain);

        let key_f = Fr::from_le_bytes_mod_order(&self.key);
        sponge.absorb(&key_f);

        let result: Fr = sponge.squeeze_field_elements(1)[0];
        let bytes = result.into_bigint().to_bytes_le();
        let mut arr = [0u8; 32];
        arr[..bytes.len()].copy_from_slice(&bytes);

        ViewingKey { key: arr }
    }

    /// Derive the public key (address)
    pub fn public_key(&self) -> [u8; 32] {
        use ark_bls12_381::Fr;
        use ark_crypto_primitives::sponge::{
            CryptographicSponge,
            poseidon::{PoseidonConfig, PoseidonSponge, find_poseidon_ark_and_mds},
        };
        use ark_ff::{BigInteger, PrimeField};

        let (ark, mds) = find_poseidon_ark_and_mds::<Fr>(255, 2, 8, 57, 0);
        let config = PoseidonConfig::new(8, 57, 5, mds, ark, 2, 1);

        let mut sponge = PoseidonSponge::new(&config);

        let domain = Fr::from_le_bytes_mod_order(
            b"ZelanaPK\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
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

/// Viewing key - allows viewing but not spending notes
///
/// Share this with auditors, wallets, etc. for read-only access.
#[derive(Debug, Clone)]
pub struct ViewingKey {
    key: [u8; 32],
}

impl ViewingKey {
    /// Create from raw bytes
    pub fn from_bytes(key: [u8; 32]) -> Self {
        Self { key }
    }

    /// Get raw bytes
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.key
    }

    /// Check if a note belongs to this viewing key
    ///
    /// This is used to scan the blockchain for owned notes.
    pub fn owns_note(&self, note: &Note, expected_pk: &[u8; 32]) -> bool {
        &note.owner_pk == expected_pk
    }
}

/// Full key bundle for a shielded address
#[derive(Debug, Clone)]
pub struct ShieldedKeyBundle {
    /// Spending key (secret - allows spending)
    pub spending_key: SpendingKey,
    /// Viewing key (allows viewing but not spending)
    pub viewing_key: ViewingKey,
    /// Public key / address (can be shared publicly)
    pub public_key: [u8; 32],
}

impl ShieldedKeyBundle {
    /// Generate a new random key bundle
    pub fn random<R: Rng>(rng: &mut R) -> Self {
        let spending_key = SpendingKey::random(rng);
        let viewing_key = spending_key.viewing_key();
        let public_key = spending_key.public_key();

        Self {
            spending_key,
            viewing_key,
            public_key,
        }
    }

    /// Restore from spending key
    pub fn from_spending_key(spending_key: SpendingKey) -> Self {
        let viewing_key = spending_key.viewing_key();
        let public_key = spending_key.public_key();

        Self {
            spending_key,
            viewing_key,
            public_key,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ark_std::rand::rngs::OsRng;

    #[test]
    fn test_note_commitment() {
        let mut rng = OsRng;
        let owner_pk = [1u8; 32];
        let note = Note::new(1000, owner_pk, &mut rng);

        let c1 = note.commitment();
        let c2 = note.commitment();

        assert_eq!(c1, c2, "commitment should be deterministic");
    }

    #[test]
    fn test_note_nullifier_requires_position() {
        let mut rng = OsRng;
        let spending_key = SpendingKey::random(&mut rng);
        let note = Note::new(1000, spending_key.public_key(), &mut rng);

        // Without position, nullifier should be None
        assert!(note.nullifier(&spending_key).is_none());

        // With position, nullifier should exist
        let note_with_pos = note.with_position(42);
        assert!(note_with_pos.nullifier(&spending_key).is_some());
    }

    #[test]
    fn test_key_derivation() {
        let mut rng = OsRng;
        let bundle = ShieldedKeyBundle::random(&mut rng);

        // Same spending key should derive same viewing key and public key
        let bundle2 = ShieldedKeyBundle::from_spending_key(SpendingKey::from_bytes(
            *bundle.spending_key.as_bytes(),
        ));

        assert_eq!(bundle.public_key, bundle2.public_key);
        assert_eq!(
            bundle.viewing_key.as_bytes(),
            bundle2.viewing_key.as_bytes()
        );
    }

    #[test]
    fn test_note_value_checked_ops() {
        let v1 = NoteValue::new(100);
        let v2 = NoteValue::new(50);

        assert_eq!(v1.checked_add(v2), Some(NoteValue::new(150)));
        assert_eq!(v1.checked_sub(v2), Some(NoteValue::new(50)));
        assert_eq!(v2.checked_sub(v1), None); // Underflow
        assert_eq!(NoteValue::MAX.checked_add(NoteValue::new(1)), None); // Overflow
    }
}
