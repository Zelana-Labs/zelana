//! # Prover Core
//!
//! Core cryptographic primitives for distributed zero-knowledge proving.
//!
//! This crate provides:
//! - Shamir's Secret Sharing for distributing secrets
//! - Schnorr proof generation and verification
//! - Hash Preimage proof generation and verification
//! - Lagrange interpolation for aggregating proof fragments
//!
//! ## Example
//!
//! ```rust
//! use prover_core::{shamir, schnorr, ShareSet};
//! use ark_std::test_rng;
//!
//! let mut rng = test_rng();
//!
//! // Split a secret among 5 parties with threshold 3
//! let secret = ark_bn254::Fr::from(42u64);
//! let shares = shamir::share_secret(secret, 5, 3, &mut rng);
//!
//! // Reconstruct from any 3 shares
//! let recovered = shamir::reconstruct_secret(&shares[0..3]);
//! assert_eq!(secret, recovered);
//! ```

pub mod commitment;
pub mod errors;
pub mod hash_preimage;
pub mod schnorr;
pub mod shamir;

#[cfg(test)]
mod integration_test;

#[cfg(test)]
mod rng_test;

pub use commitment::{
    commit_witness, generate_challenge_from_commitment, verify_commitment, WitnessCommitment,
    COMMITMENT_SIZE, SALT_SIZE,
};
pub use errors::{ProverError, Result};
pub use hash_preimage::{
    compute_sha256, hash_to_field, HashCommitment, HashPreimageProof, HashProofFragment,
    HashPublicParams,
};
pub use schnorr::{generate_challenge, Commitment, DistributedProof, ProofFragment, PublicParams};
pub use shamir::{SecretShare, ShareSet};

/// Re-export arkworks types
pub use ark_bn254::{Fr, G1Affine, G1Projective};

/// Re-export ark_std::rand for RNG types
pub use ark_std::rand;
