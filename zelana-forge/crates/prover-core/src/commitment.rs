//! Witness commitment primitives for privacy-preserving distributed proving.
//!
//! This module implements the commit-then-prove protocol that ensures
//! prover nodes never see the public witness during proof generation.
//!
//! ## Protocol Overview
//!
//! 1. Client commits: `Com = SHA256(public_witness || salt)`
//! 2. Provers generate fragments using only `Com`
//! 3. Verifier receives reveal: `(public_witness, salt)` and checks commitment
//!
//! ## Security Properties
//!
//! - **Hiding**: SHA-256 preimage resistance prevents witness recovery from Com
//! - **Binding**: SHA-256 collision resistance prevents commitment equivocation
//! - **Privacy**: No prover learns public_witness until verification phase

use ark_bn254::{Fr, G1Affine};
use ark_ff::PrimeField;
use ark_serialize::CanonicalSerialize;
use sha2::{Digest, Sha256};

use crate::errors::ProverError;

/// Size of commitment in bytes (SHA-256 output)
pub const COMMITMENT_SIZE: usize = 32;

/// Size of salt in bytes (256 bits for security)
pub const SALT_SIZE: usize = 32;

/// Commitment to a public witness
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WitnessCommitment {
    /// SHA-256 hash of (public_witness || salt)
    pub hash: [u8; COMMITMENT_SIZE],
}

impl WitnessCommitment {
    /// Create a new commitment from hash bytes
    pub fn from_bytes(bytes: [u8; COMMITMENT_SIZE]) -> Self {
        Self { hash: bytes }
    }

    /// Get the commitment as bytes
    pub fn as_bytes(&self) -> &[u8; COMMITMENT_SIZE] {
        &self.hash
    }

    /// Create commitment from hex string
    pub fn from_hex(hex: &str) -> Result<Self, ProverError> {
        if hex.len() != COMMITMENT_SIZE * 2 {
            return Err(ProverError::InvalidCommitment(
                "Invalid hex length".to_string(),
            ));
        }

        let mut bytes = [0u8; COMMITMENT_SIZE];
        for i in 0..COMMITMENT_SIZE {
            bytes[i] = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16)
                .map_err(|e| ProverError::InvalidCommitment(e.to_string()))?;
        }

        Ok(Self { hash: bytes })
    }

    /// Convert commitment to hex string
    pub fn to_hex(&self) -> String {
        hex::encode(self.hash)
    }
}

/// Commit to a public witness with a salt
///
/// # Security
///
/// - Salt must be 32 bytes from a CSPRNG
/// - Salt must be unique per proof
/// - Salt must be kept secret until reveal phase
///
/// # Example
///
/// ```rust
/// use prover_core::commitment::commit_witness;
///
/// let public_witness = b"transaction details";
/// let salt = [42u8; 32]; // In production: use CSPRNG
///
/// let commitment = commit_witness(public_witness, &salt);
/// ```
pub fn commit_witness(public_witness: &[u8], salt: &[u8; SALT_SIZE]) -> WitnessCommitment {
    let mut hasher = Sha256::new();
    hasher.update(public_witness);
    hasher.update(salt);
    let hash: [u8; COMMITMENT_SIZE] = hasher.finalize().into();

    WitnessCommitment { hash }
}

/// Verify a commitment reveal
///
/// Checks that `SHA256(public_witness || salt) == commitment`
///
/// Returns `true` if commitment is valid, `false` otherwise.
///
/// # Security
///
/// Uses constant-time comparison to prevent timing attacks.
///
/// # Example
///
/// ```rust
/// use prover_core::commitment::{commit_witness, verify_commitment};
///
/// let public_witness = b"transaction details";
/// let salt = [42u8; 32];
///
/// let commitment = commit_witness(public_witness, &salt);
/// let valid = verify_commitment(public_witness, &salt, &commitment);
/// assert!(valid);
/// ```
pub fn verify_commitment(
    public_witness: &[u8],
    salt: &[u8; SALT_SIZE],
    commitment: &WitnessCommitment,
) -> bool {
    let computed = commit_witness(public_witness, salt);

    // Constant-time comparison to prevent timing attacks
    use subtle::ConstantTimeEq;
    computed.hash.ct_eq(&commitment.hash).into()
}

/// Generate Fiat-Shamir challenge from commitment (not public witness)
///
/// This is the key privacy-preserving modification: the challenge is computed
/// from the commitment hash, not the actual public witness.
///
/// # Arguments
///
/// - `generator`: Generator point for the proof system
/// - `witness_commitment`: Commitment to the public witness
/// - `aggregated_commitment`: Aggregated commitment from provers (Phase 2)
/// - `session_id`: Unique session identifier
///
/// # Security
///
/// The challenge is cryptographically bound to the witness (via commitment)
/// but reveals no information about the witness itself.
pub fn generate_challenge_from_commitment(
    generator: &G1Affine,
    witness_commitment: &WitnessCommitment,
    aggregated_commitment: &G1Affine,
    session_id: &str,
) -> Result<Fr, ProverError> {
    let mut hasher = Sha256::new();

    // Hash generator
    let mut gen_bytes = Vec::new();
    generator
        .serialize_compressed(&mut gen_bytes)
        .map_err(|e| ProverError::SerializationError(e.to_string()))?;
    hasher.update(&gen_bytes);

    // Hash witness commitment (NOT public witness)
    hasher.update(witness_commitment.as_bytes());

    // Hash aggregated commitment
    let mut commit_bytes = Vec::new();
    aggregated_commitment
        .serialize_compressed(&mut commit_bytes)
        .map_err(|e| ProverError::SerializationError(e.to_string()))?;
    hasher.update(&commit_bytes);

    // Hash session ID
    hasher.update(session_id.as_bytes());

    let hash = hasher.finalize();
    Ok(hash_to_field(&hash))
}

/// Convert hash bytes to field element
fn hash_to_field(hash: &[u8]) -> Fr {
    // Take first 31 bytes to stay within field modulus
    let mut bytes = [0u8; 32];
    let len = hash.len().min(31);
    bytes[..len].copy_from_slice(&hash[..len]);
    Fr::from_le_bytes_mod_order(&bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ark_std::{test_rng, UniformRand};

    #[test]
    fn test_commit_and_verify() {
        let public_witness = b"test witness";
        let salt = [42u8; SALT_SIZE];

        let commitment = commit_witness(public_witness, &salt);
        let valid = verify_commitment(public_witness, &salt, &commitment);

        assert!(valid, "Commitment should verify correctly");
    }

    #[test]
    fn test_verify_fails_wrong_witness() {
        let public_witness = b"test witness";
        let wrong_witness = b"wrong witness";
        let salt = [42u8; SALT_SIZE];

        let commitment = commit_witness(public_witness, &salt);
        let valid = verify_commitment(wrong_witness, &salt, &commitment);

        assert!(!valid, "Commitment should fail with wrong witness");
    }

    #[test]
    fn test_verify_fails_wrong_salt() {
        let public_witness = b"test witness";
        let salt = [42u8; SALT_SIZE];
        let wrong_salt = [43u8; SALT_SIZE];

        let commitment = commit_witness(public_witness, &salt);
        let valid = verify_commitment(public_witness, &wrong_salt, &commitment);

        assert!(!valid, "Commitment should fail with wrong salt");
    }

    #[test]
    fn test_commitment_deterministic() {
        let public_witness = b"test witness";
        let salt = [42u8; SALT_SIZE];

        let commitment1 = commit_witness(public_witness, &salt);
        let commitment2 = commit_witness(public_witness, &salt);

        assert_eq!(
            commitment1, commitment2,
            "Same input should produce same commitment"
        );
    }

    #[test]
    fn test_commitment_different_salt() {
        let public_witness = b"test witness";
        let salt1 = [42u8; SALT_SIZE];
        let salt2 = [43u8; SALT_SIZE];

        let commitment1 = commit_witness(public_witness, &salt1);
        let commitment2 = commit_witness(public_witness, &salt2);

        assert_ne!(
            commitment1, commitment2,
            "Different salts should produce different commitments"
        );
    }

    #[test]
    fn test_generate_challenge_from_commitment() {
        let mut rng = test_rng();
        let generator = G1Affine::rand(&mut rng);
        let aggregated_commitment = G1Affine::rand(&mut rng);

        let public_witness = b"test witness";
        let salt = [42u8; SALT_SIZE];
        let commitment = commit_witness(public_witness, &salt);

        let challenge = generate_challenge_from_commitment(
            &generator,
            &commitment,
            &aggregated_commitment,
            "test-session",
        );

        assert!(challenge.is_ok(), "Challenge generation should succeed");

        let challenge = challenge.unwrap();
        use ark_std::Zero;
        assert!(!challenge.is_zero(), "Challenge should not be zero");
    }

    #[test]
    fn test_challenge_deterministic() {
        let mut rng = test_rng();
        let generator = G1Affine::rand(&mut rng);
        let aggregated_commitment = G1Affine::rand(&mut rng);

        let public_witness = b"test witness";
        let salt = [42u8; SALT_SIZE];
        let commitment = commit_witness(public_witness, &salt);

        let challenge1 = generate_challenge_from_commitment(
            &generator,
            &commitment,
            &aggregated_commitment,
            "test-session",
        )
        .unwrap();

        let challenge2 = generate_challenge_from_commitment(
            &generator,
            &commitment,
            &aggregated_commitment,
            "test-session",
        )
        .unwrap();

        assert_eq!(
            challenge1, challenge2,
            "Same inputs should produce same challenge"
        );
    }

    #[test]
    fn test_challenge_different_commitment() {
        let mut rng = test_rng();
        let generator = G1Affine::rand(&mut rng);
        let aggregated_commitment = G1Affine::rand(&mut rng);

        let public_witness1 = b"test witness 1";
        let public_witness2 = b"test witness 2";
        let salt = [42u8; SALT_SIZE];

        let commitment1 = commit_witness(public_witness1, &salt);
        let commitment2 = commit_witness(public_witness2, &salt);

        let challenge1 = generate_challenge_from_commitment(
            &generator,
            &commitment1,
            &aggregated_commitment,
            "test-session",
        )
        .unwrap();

        let challenge2 = generate_challenge_from_commitment(
            &generator,
            &commitment2,
            &aggregated_commitment,
            "test-session",
        )
        .unwrap();

        assert_ne!(
            challenge1, challenge2,
            "Different commitments should produce different challenges"
        );
    }

    #[test]
    fn test_hex_encoding() {
        let public_witness = b"test witness";
        let salt = [42u8; SALT_SIZE];
        let commitment = commit_witness(public_witness, &salt);

        let hex = commitment.to_hex();
        let recovered = WitnessCommitment::from_hex(&hex).unwrap();

        assert_eq!(commitment, recovered, "Hex encoding should roundtrip");
    }

    #[test]
    fn test_hex_invalid_length() {
        let result = WitnessCommitment::from_hex("00112233"); // Too short
        assert!(result.is_err(), "Should reject invalid hex length");
    }

    #[test]
    fn test_hex_invalid_chars() {
        let invalid_hex = "zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz";
        let result = WitnessCommitment::from_hex(invalid_hex);
        assert!(result.is_err(), "Should reject invalid hex characters");
    }
}
