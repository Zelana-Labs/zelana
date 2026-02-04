//! Network message types for the distributed proving protocol.
//!
//! Defines all message types used for communication between coordinator and nodes.

use crate::serde_utils::{deserialize_fr, deserialize_g1, serialize_fr, serialize_g1};
use prover_core::{Fr, G1Affine, G1Projective};
use serde::{Deserialize, Serialize};
use ark_ec::CurveGroup;
/// Circuit type for the proof system
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum CircuitType {
    /// Schnorr signature circuit (prove knowledge of discrete log)
    #[default]
    Schnorr,
    /// Hash preimage circuit (prove knowledge of hash preimage)
    HashPreimage,
}

/// Share assignment message sent from coordinator to node during setup
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShareAssignment {
    /// Node ID (1-indexed)
    pub node_id: u32,

    /// Share index (x-coordinate)
    pub share_index: u32,

    /// Share value (y-coordinate)
    #[serde(serialize_with = "serialize_fr", deserialize_with = "deserialize_fr")]
    pub share_value: Fr,

    /// Generator point for the proof system
    #[serde(serialize_with = "serialize_g1", deserialize_with = "deserialize_g1")]
    pub generator: G1Affine,

    /// Public key (g^secret)
    #[serde(serialize_with = "serialize_g1", deserialize_with = "deserialize_g1")]
    pub public_key: G1Affine,
}

/// Request for a commitment from a node (Phase 1)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitmentRequest {
    /// Unique session ID for this proof
    pub session_id: String,
}

/// Response containing a commitment point (Phase 1)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitmentResponse {
    /// Node ID
    pub node_id: u32,

    /// Session ID (must match request)
    pub session_id: String,

    /// Commitment point (g^nonce)
    #[serde(serialize_with = "serialize_g1", deserialize_with = "deserialize_g1")]
    pub commitment: G1Affine,
}

/// Request for a proof fragment from a node (Phase 3)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FragmentRequest {
    /// Session ID (must match commitment phase)
    pub session_id: String,

    /// Fiat-Shamir challenge
    #[serde(serialize_with = "serialize_fr", deserialize_with = "deserialize_fr")]
    pub challenge: Fr,
}

/// Response containing a proof fragment (Phase 3)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FragmentResponse {
    /// Node ID
    pub node_id: u32,

    /// Session ID
    pub session_id: String,

    /// Response value (nonce + challenge * share)
    #[serde(serialize_with = "serialize_fr", deserialize_with = "deserialize_fr")]
    pub response: Fr,
}

/// Complete distributed proof that can be verified
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkProof {
    /// Aggregated commitment
    #[serde(serialize_with = "serialize_g1", deserialize_with = "deserialize_g1")]
    pub commitment: G1Affine,

    /// Fiat-Shamir challenge
    #[serde(serialize_with = "serialize_fr", deserialize_with = "deserialize_fr")]
    pub challenge: Fr,

    /// Aggregated response
    #[serde(serialize_with = "serialize_fr", deserialize_with = "deserialize_fr")]
    pub response: Fr,

    /// Generator used for verification
    #[serde(serialize_with = "serialize_g1", deserialize_with = "deserialize_g1")]
    pub generator: G1Affine,

    /// Public key for verification
    #[serde(serialize_with = "serialize_g1", deserialize_with = "deserialize_g1")]
    pub public_key: G1Affine,
}

/// Standardized API response wrapper
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "lowercase")]
pub enum ApiResponse<T> {
    /// Successful response
    Success {
        /// Response data
        data: T,
    },
    /// Error response
    Error {
        /// Error message
        message: String,
    },
}

impl<T> ApiResponse<T> {
    /// Create a success response
    pub fn success(data: T) -> Self {
        ApiResponse::Success { data }
    }

    /// Create an error response
    pub fn error(message: impl Into<String>) -> Self {
        ApiResponse::Error {
            message: message.into(),
        }
    }

    /// Check if response is successful
    pub fn is_success(&self) -> bool {
        matches!(self, ApiResponse::Success { .. })
    }

    /// Check if response is an error
    pub fn is_error(&self) -> bool {
        matches!(self, ApiResponse::Error { .. })
    }
}

/// Setup request to initialize the system with a secret
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetupRequest {
    /// The secret to share (in hex format)
    pub secret: String,
}

/// Setup response containing public parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetupResponse {
    /// Generator point
    #[serde(serialize_with = "serialize_g1", deserialize_with = "deserialize_g1")]
    pub generator: G1Affine,

    /// Public key (g^secret)
    #[serde(serialize_with = "serialize_g1", deserialize_with = "deserialize_g1")]
    pub public_key: G1Affine,

    /// Number of nodes in the system
    pub num_nodes: usize,

    /// Threshold required for proofs
    pub threshold: usize,
}

/// Prove request to generate a distributed proof
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProveRequest {
    /// Message to include in the proof (included in challenge computation)
    pub message: String,
}

/// Prove response containing the generated proof
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProveResponse {
    /// The distributed proof
    pub proof: NetworkProof,

    /// Number of nodes that participated
    pub participants: usize,
}

/// Verify request to check a proof
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifyRequest {
    /// The proof to verify
    pub proof: NetworkProof,
}

/// Verify response indicating if proof is valid
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifyResponse {
    /// Whether the proof is valid
    pub valid: bool,
}

/// Health check response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    /// Service status
    pub status: String,

    /// Node ID (for nodes) or coordinator identifier
    pub node_id: Option<u32>,

    /// Whether the service is ready to process requests
    pub ready: bool,
}

// Privacy-Preserving Blind Proving Messages

/// Witness commitment for privacy-preserving proving
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WitnessCommitment {
    /// Commitment hash (SHA-256 of public_witness || salt)
    #[serde(with = "hex::serde")]
    pub hash: [u8; 32],
}

/// Blind share assignment (no public witness revealed to prover)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlindShareAssignment {
    /// Session ID for this blind proving session
    pub session_id: String,

    /// Node ID (1-indexed)
    pub node_id: u32,

    /// Share index (x-coordinate)
    pub share_index: u32,

    /// Share value (y-coordinate)
    #[serde(serialize_with = "serialize_fr", deserialize_with = "deserialize_fr")]
    pub share_value: Fr,

    /// Generator point for the proof system
    #[serde(serialize_with = "serialize_g1", deserialize_with = "deserialize_g1")]
    pub generator: G1Affine,

    /// Witness commitment (NOT the actual public witness)
    pub witness_commitment: WitnessCommitment,

    /// Circuit type being proven
    pub circuit_type: CircuitType,
}

/// Setup request with witness commitment (privacy-preserving)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlindSetupRequest {
    /// Circuit type to prove
    pub circuit_type: CircuitType,

    /// Commitment to the public witness
    pub witness_commitment: WitnessCommitment,

    /// Secret to share (in hex format)
    pub secret: String,
}

/// Blind setup response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlindSetupResponse {
    /// Generator point
    #[serde(serialize_with = "serialize_g1", deserialize_with = "deserialize_g1")]
    pub generator: G1Affine,

    /// Witness commitment (echo back)
    pub witness_commitment: WitnessCommitment,

    /// Number of nodes in the system
    pub num_nodes: usize,

    /// Threshold required for proofs
    pub threshold: usize,

    /// Session ID for this blind proving session
    pub session_id: String,
}

/// Blind proof (proof with commitment, no witness revealed)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlindProof {
    /// Witness commitment
    pub witness_commitment: WitnessCommitment,

    /// Aggregated commitment
    #[serde(serialize_with = "serialize_g1", deserialize_with = "deserialize_g1")]
    pub commitment: G1Affine,

    /// Challenge (computed from witness_commitment, not witness)
    #[serde(serialize_with = "serialize_fr", deserialize_with = "deserialize_fr")]
    pub challenge: Fr,

    /// Aggregated response
    #[serde(serialize_with = "serialize_fr", deserialize_with = "deserialize_fr")]
    pub response: Fr,

    /// Generator used
    #[serde(serialize_with = "serialize_g1", deserialize_with = "deserialize_g1")]
    pub generator: G1Affine,

    /// Public key (g^secret) for verification
    #[serde(serialize_with = "serialize_g1", deserialize_with = "deserialize_g1")]
    pub public_key: G1Affine,

    /// Circuit type
    pub circuit_type: CircuitType,
}

/// Blind prove request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlindProveRequest {
    /// Session ID from blind setup
    pub session_id: String,
}

/// Blind prove response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlindProveResponse {
    /// The blind proof
    pub blind_proof: BlindProof,

    /// Number of nodes that participated
    pub participants: usize,
}

/// Verification request with witness reveal
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifyWithRevealRequest {
    /// The blind proof to verify
    pub blind_proof: BlindProof,

    /// Revealed public witness (hex-encoded)
    pub public_witness: String,

    /// Salt used in commitment (hex-encoded)
    #[serde(with = "hex::serde")]
    pub salt: [u8; 32],
}

/// Verification response for blind proofs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifyWithRevealResponse {
    /// Whether the proof is valid
    pub valid: bool,

    /// Whether the commitment matches (witness reveal is correct)
    pub commitment_valid: bool,

    /// Optional error message
    pub message: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use ark_std::{test_rng, UniformRand};

    #[test]
    fn test_share_assignment_serialization() {
        use ark_ec::CurveGroup;
        use prover_core::G1Projective;
        let mut rng = test_rng();

        let msg = ShareAssignment {
            node_id: 1,
            share_index: 1,
            share_value: Fr::rand(&mut rng),
            generator: G1Projective::rand(&mut rng).into_affine(),
            public_key: G1Projective::rand(&mut rng).into_affine(),
        };

        let json = serde_json::to_string(&msg).unwrap();
        let recovered: ShareAssignment = serde_json::from_str(&json).unwrap();

        assert_eq!(msg.node_id, recovered.node_id);
        assert_eq!(msg.share_value, recovered.share_value);
        assert_eq!(msg.generator, recovered.generator);
        assert_eq!(msg.public_key, recovered.public_key);
    }

    #[test]
    fn test_commitment_request_response() {
        use ark_ec::CurveGroup;
        use prover_core::G1Projective;
        let mut rng = test_rng();

        let req = CommitmentRequest {
            session_id: "test-session".to_string(),
        };

        let json = serde_json::to_string(&req).unwrap();
        let recovered: CommitmentRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(req.session_id, recovered.session_id);

        let resp = CommitmentResponse {
            node_id: 1,
            session_id: "test-session".to_string(),
            commitment: G1Projective::rand(&mut rng).into_affine(),
        };

        let json = serde_json::to_string(&resp).unwrap();
        let recovered: CommitmentResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(resp.node_id, recovered.node_id);
        assert_eq!(resp.commitment, recovered.commitment);
    }

    #[test]
    fn test_fragment_request_response() {
        let mut rng = test_rng();

        let req = FragmentRequest {
            session_id: "test-session".to_string(),
            challenge: Fr::rand(&mut rng),
        };

        let json = serde_json::to_string(&req).unwrap();
        let recovered: FragmentRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(req.challenge, recovered.challenge);

        let resp = FragmentResponse {
            node_id: 1,
            session_id: "test-session".to_string(),
            response: Fr::rand(&mut rng),
        };

        let json = serde_json::to_string(&resp).unwrap();
        let recovered: FragmentResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(resp.response, recovered.response);
    }

    #[test]
    fn test_network_proof_serialization() {
        use ark_ec::CurveGroup;
        use prover_core::G1Projective;
        let mut rng = test_rng();

        let proof = NetworkProof {
            commitment: G1Projective::rand(&mut rng).into_affine(),
            challenge: Fr::rand(&mut rng),
            response: Fr::rand(&mut rng),
            generator: G1Projective::rand(&mut rng).into_affine(),
            public_key: G1Projective::rand(&mut rng).into_affine(),
        };

        let json = serde_json::to_string(&proof).unwrap();
        let recovered: NetworkProof = serde_json::from_str(&json).unwrap();

        assert_eq!(proof.commitment, recovered.commitment);
        assert_eq!(proof.challenge, recovered.challenge);
        assert_eq!(proof.response, recovered.response);
        assert_eq!(proof.generator, recovered.generator);
        assert_eq!(proof.public_key, recovered.public_key);
    }

    #[test]
    fn test_api_response() {
        let success: ApiResponse<String> = ApiResponse::success("hello".to_string());
        assert!(success.is_success());
        assert!(!success.is_error());

        let json = serde_json::to_string(&success).unwrap();
        let recovered: ApiResponse<String> = serde_json::from_str(&json).unwrap();
        assert!(recovered.is_success());

        let error: ApiResponse<String> = ApiResponse::error("something went wrong");
        assert!(!error.is_success());
        assert!(error.is_error());

        let json = serde_json::to_string(&error).unwrap();
        let recovered: ApiResponse<String> = serde_json::from_str(&json).unwrap();
        assert!(recovered.is_error());
    }

    #[test]
    fn test_witness_commitment_serialization() {
        let commitment = WitnessCommitment { hash: [42u8; 32] };

        let json = serde_json::to_string(&commitment).unwrap();
        let recovered: WitnessCommitment = serde_json::from_str(&json).unwrap();

        assert_eq!(commitment.hash, recovered.hash);
    }

    #[test]
    fn test_blind_share_assignment_serialization() {
        let mut rng = test_rng();

        let witness_commitment = WitnessCommitment { hash: [42u8; 32] };

        let msg = BlindShareAssignment {
            session_id: "test-session".to_string(),
            node_id: 1,
            share_index: 1,
            share_value: Fr::rand(&mut rng),
            generator: G1Projective::rand(&mut rng).into_affine(),
            witness_commitment: witness_commitment.clone(),
            circuit_type: CircuitType::Schnorr,
        };

        let json = serde_json::to_string(&msg).unwrap();
        let recovered: BlindShareAssignment = serde_json::from_str(&json).unwrap();

        assert_eq!(msg.node_id, recovered.node_id);
        assert_eq!(msg.share_value, recovered.share_value);
        assert_eq!(msg.generator, recovered.generator);
        assert_eq!(msg.witness_commitment, recovered.witness_commitment);
    }

    #[test]
    fn test_blind_setup_request_response() {
        use ark_ec::CurveGroup;
        use prover_core::G1Projective;
        let mut rng = test_rng();

        let witness_commitment = WitnessCommitment { hash: [42u8; 32] };

        let req = BlindSetupRequest {
            circuit_type: CircuitType::Schnorr,
            witness_commitment: witness_commitment.clone(),
            secret: "deadbeef".to_string(),
        };

        let json = serde_json::to_string(&req).unwrap();
        let recovered: BlindSetupRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(req.witness_commitment, recovered.witness_commitment);

        let resp = BlindSetupResponse {
            generator: G1Projective::rand(&mut rng).into_affine(),
            witness_commitment: witness_commitment.clone(),
            num_nodes: 5,
            threshold: 3,
            session_id: "test-session".to_string(),
        };

        let json = serde_json::to_string(&resp).unwrap();
        let recovered: BlindSetupResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(resp.witness_commitment, recovered.witness_commitment);
        assert_eq!(resp.session_id, recovered.session_id);
    }

    #[test]
    fn test_blind_proof_serialization() {
        use ark_ec::CurveGroup;
        use prover_core::G1Projective;
        let mut rng = test_rng();

        let witness_commitment = WitnessCommitment { hash: [42u8; 32] };

        let proof = BlindProof {
            witness_commitment,
            commitment: G1Projective::rand(&mut rng).into_affine(),
            challenge: Fr::rand(&mut rng),
            response: Fr::rand(&mut rng),
            generator: G1Projective::rand(&mut rng).into_affine(),
            public_key: G1Projective::rand(&mut rng).into_affine(),
            circuit_type: CircuitType::Schnorr,
        };

        let json = serde_json::to_string(&proof).unwrap();
        let recovered: BlindProof = serde_json::from_str(&json).unwrap();

        assert_eq!(proof.witness_commitment, recovered.witness_commitment);
        assert_eq!(proof.commitment, recovered.commitment);
        assert_eq!(proof.challenge, recovered.challenge);
        assert_eq!(proof.response, recovered.response);
        assert_eq!(proof.public_key, recovered.public_key);
    }

    #[test]
    fn test_verify_with_reveal_request() {
        use ark_ec::CurveGroup;
        use prover_core::G1Projective;
        let mut rng = test_rng();

        let witness_commitment = WitnessCommitment { hash: [42u8; 32] };

        let blind_proof = BlindProof {
            witness_commitment,
            commitment: G1Projective::rand(&mut rng).into_affine(),
            challenge: Fr::rand(&mut rng),
            response: Fr::rand(&mut rng),
            generator: G1Projective::rand(&mut rng).into_affine(),
            public_key: G1Projective::rand(&mut rng).into_affine(),
            circuit_type: CircuitType::Schnorr,
        };

        let req = VerifyWithRevealRequest {
            blind_proof,
            public_witness: "deadbeef".to_string(),
            salt: [99u8; 32],
        };

        let json = serde_json::to_string(&req).unwrap();
        let recovered: VerifyWithRevealRequest = serde_json::from_str(&json).unwrap();

        assert_eq!(req.public_witness, recovered.public_witness);
        assert_eq!(req.salt, recovered.salt);
    }
}
