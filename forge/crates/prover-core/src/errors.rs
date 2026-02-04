//! Common types and error definitions for the prover system.
//!
//! This module provides type aliases for the specific elliptic curve (BN254)
//! and field elements used throughout the system, as well as a comprehensive
//! error type for all cryptographic operations.
use thiserror::Error;

/// Errors that can occur during cryptographic operations
#[derive(Error, Debug, Clone, PartialEq)]
pub enum ProverError {
    /// Not enough shares provided to reconstruct the secret
    #[error("Insufficient shares: need at least {threshold}, got {provided}")]
    InsufficientShares { threshold: usize, provided: usize },

    /// Duplicate share indices detected
    #[error("Duplicate share index detected: {0}")]
    DuplicateIndex(u32),

    /// Invalid share index (must be non-zero)
    #[error("Invalid share index: {0} (must be non-zero)")]
    InvalidIndex(u32),

    /// Threshold configuration is invalid
    #[error("Invalid threshold: must be between 1 and {max}, got {threshold}")]
    InvalidThreshold { threshold: usize, max: usize },

    /// Proof verification failed
    #[error("Proof verification failed")]
    VerificationFailed,

    /// Invalid proof fragment received from a node
    #[error("Invalid proof fragment from node {node_id}")]
    InvalidFragment { node_id: u32 },

    /// Serialization error
    #[error("Serialization error: {0}")]
    SerializationError(String),

    /// Cryptographic operation error
    #[error("Cryptographic error: {0}")]
    CryptoError(String),

    /// Not enough fragments provided to aggregate proof
    #[error("Insufficient fragments: need at least {needed}, got {got}")]
    InsufficientFragments { needed: usize, got: usize },

    /// Invalid witness commitment
    #[error("Invalid witness commitment: {0}")]
    InvalidCommitment(String),
}

/// Result type for cryptographic operations
pub type Result<T> = std::result::Result<T, ProverError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_messages() {
        let err = ProverError::InsufficientShares {
            threshold: 3,
            provided: 2,
        };
        assert_eq!(
            err.to_string(),
            "Insufficient shares: need at least 3, got 2"
        );

        let err = ProverError::DuplicateIndex(5);
        assert_eq!(err.to_string(), "Duplicate share index detected: 5");

        let err = ProverError::InvalidIndex(0);
        assert_eq!(err.to_string(), "Invalid share index: 0 (must be non-zero)");
    }

    #[test]
    fn test_error_equality() {
        let err1 = ProverError::VerificationFailed;
        let err2 = ProverError::VerificationFailed;
        assert_eq!(err1, err2);

        let err1 = ProverError::DuplicateIndex(5);
        let err2 = ProverError::DuplicateIndex(5);
        assert_eq!(err1, err2);
    }
}
