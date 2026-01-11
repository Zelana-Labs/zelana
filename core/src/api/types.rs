//! API Types
//!
//! Request/response types for the HTTP API.

use serde::{Deserialize, Serialize};
use zelana_account::AccountId;

// ============================================================================
// Transaction Submission
// ============================================================================

/// Request to submit an encrypted transaction blob
#[derive(Debug, Deserialize)]
pub struct SubmitTxRequest {
    /// Serialized EncryptedTxBlobV1 (wincode encoded)
    pub blob: Vec<u8>,
    /// Client X25519 public key for ECDH
    pub client_pubkey: [u8; 32],
}

/// Response after submitting a transaction
#[derive(Debug, Serialize)]
pub struct SubmitTxResponse {
    /// Transaction hash
    pub tx_hash: String,
    /// Whether the transaction was accepted
    pub accepted: bool,
    /// Status message
    pub message: String,
}

// ============================================================================
// Shielded Operations
// ============================================================================

/// Request to submit a shielded transaction
#[derive(Debug, Deserialize)]
pub struct SubmitShieldedRequest {
    /// ZK proof bytes
    pub proof: Vec<u8>,
    /// Nullifier (spent note identifier)
    pub nullifier: [u8; 32],
    /// New commitment (created note)
    pub commitment: [u8; 32],
    /// Encrypted note data for recipient
    pub ciphertext: Vec<u8>,
    /// Ephemeral public key for note decryption
    pub ephemeral_key: [u8; 32],
}

/// Response after submitting a shielded transaction
#[derive(Debug, Serialize)]
pub struct SubmitShieldedResponse {
    pub tx_hash: String,
    pub accepted: bool,
    pub position: Option<u32>,
    pub message: String,
}

/// Request to scan for notes owned by a viewing key
#[derive(Debug, Deserialize)]
pub struct ScanNotesRequest {
    /// Viewing key (X25519 secret key bytes)
    pub viewing_key: [u8; 32],
    /// Start position (for incremental scanning)
    pub from_position: Option<u32>,
    /// Maximum notes to return
    pub limit: Option<usize>,
}

/// A decrypted note found during scanning
#[derive(Debug, Serialize)]
pub struct ScannedNote {
    pub position: u32,
    pub commitment: String,
    pub value: u64,
    pub memo: Option<String>,
}

/// Response with scanned notes
#[derive(Debug, Serialize)]
pub struct ScanNotesResponse {
    pub notes: Vec<ScannedNote>,
    pub scanned_to: u32,
}

/// Request for merkle path
#[derive(Debug, Deserialize)]
pub struct MerklePathRequest {
    /// Position of the commitment
    pub position: u32,
}

/// Merkle path response
#[derive(Debug, Serialize)]
pub struct MerklePathResponse {
    pub position: u32,
    pub path: Vec<String>,
    pub root: String,
}

// ============================================================================
// Account Operations
// ============================================================================

/// Request to get account state
#[derive(Debug, Deserialize)]
pub struct GetAccountRequest {
    /// Account ID (32-byte public key, hex encoded)
    pub account_id: String,
}

/// Account state response
#[derive(Debug, Serialize)]
pub struct AccountStateResponse {
    pub account_id: String,
    pub balance: u64,
    pub nonce: u64,
}

// ============================================================================
// Withdrawal Operations
// ============================================================================

/// Request to initiate a withdrawal
#[derive(Debug, Deserialize)]
pub struct WithdrawRequest {
    /// Source account on L2
    pub from: [u8; 32],
    /// Destination address on L1 (Solana pubkey)
    pub to_l1_address: [u8; 32],
    /// Amount to withdraw
    pub amount: u64,
    /// Account nonce
    pub nonce: u64,
    /// Ed25519 signature
    pub signature: Vec<u8>,
    /// Signer public key
    pub signer_pubkey: [u8; 32],
}

/// Withdrawal status response
#[derive(Debug, Serialize)]
pub struct WithdrawResponse {
    pub tx_hash: String,
    pub accepted: bool,
    pub estimated_completion: Option<String>,
    pub message: String,
}

/// Request to get withdrawal status
#[derive(Debug, Deserialize)]
pub struct WithdrawalStatusRequest {
    pub tx_hash: String,
}

/// Withdrawal status
#[derive(Debug, Serialize)]
pub struct WithdrawalStatusResponse {
    pub tx_hash: String,
    pub state: String,
    pub amount: u64,
    pub to_l1_address: String,
    pub l1_tx_sig: Option<String>,
}

// ============================================================================
// State Queries
// ============================================================================

/// Response with current state roots
#[derive(Debug, Serialize)]
pub struct StateRootsResponse {
    pub batch_id: u64,
    pub state_root: String,
    pub shielded_root: String,
    pub commitment_count: u64,
}

/// Batch status response
#[derive(Debug, Serialize)]
pub struct BatchStatusResponse {
    pub current_batch_id: u64,
    pub current_batch_txs: usize,
    pub proving_count: usize,
    pub pending_settlement: usize,
}

/// Health check response
#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub healthy: bool,
    pub version: String,
    pub uptime_secs: u64,
}

// ============================================================================
// Error Response
// ============================================================================

/// Standard error response
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
    pub code: String,
}

impl ErrorResponse {
    pub fn new(error: impl Into<String>, code: impl Into<String>) -> Self {
        Self {
            error: error.into(),
            code: code.into(),
        }
    }

    pub fn bad_request(msg: impl Into<String>) -> Self {
        Self::new(msg, "BAD_REQUEST")
    }

    pub fn internal(msg: impl Into<String>) -> Self {
        Self::new(msg, "INTERNAL_ERROR")
    }

    pub fn not_found(msg: impl Into<String>) -> Self {
        Self::new(msg, "NOT_FOUND")
    }
}
