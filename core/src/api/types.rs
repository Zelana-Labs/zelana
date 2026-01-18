//! API Types
//!
//! Request/response types for the HTTP API.

use serde::{Deserialize, Serialize};

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
    /// X25519 secret key for decryption (derived from spending key)
    pub decryption_key: [u8; 32],
    /// Owner's public key (to verify ownership)
    pub owner_pk: [u8; 32],
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
// Transfer Operations
// ============================================================================

/// Request to submit a transparent transfer
#[derive(Debug, Deserialize)]
pub struct TransferRequest {
    /// Source account (signer's public key)
    pub from: [u8; 32],
    /// Destination account
    pub to: [u8; 32],
    /// Amount to transfer (in lamports)
    pub amount: u64,
    /// Account nonce (for replay protection)
    pub nonce: u64,
    /// Chain ID (for replay protection across networks)
    pub chain_id: u64,
    /// Ed25519 signature over the serialized TransactionData
    pub signature: Vec<u8>,
    /// Signer's public key (must match 'from')
    pub signer_pubkey: [u8; 32],
}

/// Response after submitting a transfer
#[derive(Debug, Serialize)]
pub struct TransferResponse {
    pub tx_hash: String,
    pub accepted: bool,
    pub message: String,
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
// Fast Withdrawal Types
// ============================================================================

/// Request for fast withdrawal quote
#[derive(Debug, Deserialize)]
pub struct FastWithdrawQuoteRequest {
    /// Amount to withdraw
    pub amount: u64,
}

/// Quote response for fast withdrawal
#[derive(Debug, Serialize)]
pub struct FastWithdrawQuoteResponse {
    pub available: bool,
    pub amount: u64,
    pub fee: u64,
    pub amount_received: u64,
    pub fee_bps: u16,
    pub lp_address: Option<String>,
}

/// Request to execute fast withdrawal
#[derive(Debug, Deserialize)]
pub struct FastWithdrawRequest {
    /// Original withdrawal tx hash
    pub withdrawal_tx_hash: [u8; 32],
    /// User's L1 destination address
    pub user_l1_address: [u8; 32],
    /// Amount to withdraw
    pub amount: u64,
    /// LP address to use (from quote)
    pub lp_address: [u8; 32],
}

/// Response after fast withdrawal execution
#[derive(Debug, Serialize)]
pub struct FastWithdrawResponse {
    pub success: bool,
    pub claim_id: Option<String>,
    pub amount_fronted: u64,
    pub fee: u64,
    pub message: String,
}

/// LP registration request
#[derive(Debug, Deserialize)]
pub struct RegisterLpRequest {
    pub l1_address: [u8; 32],
    pub l2_address: [u8; 32],
    pub collateral: u64,
    pub custom_fee_bps: Option<u16>,
}

/// LP registration response
#[derive(Debug, Serialize)]
pub struct RegisterLpResponse {
    pub success: bool,
    pub message: String,
}

// ============================================================================
// Encrypted Mempool Types (Threshold Encryption)
// ============================================================================

/// Request to submit a threshold-encrypted transaction
#[derive(Debug, Deserialize)]
pub struct SubmitEncryptedTxRequest {
    /// Transaction ID
    pub tx_id: [u8; 32],
    /// Epoch when encrypted
    pub epoch: u64,
    /// Nonce for symmetric encryption
    pub nonce: [u8; 12],
    /// Encrypted transaction data
    pub ciphertext: Vec<u8>,
    /// Encrypted shares for each committee member
    pub encrypted_shares: Vec<EncryptedShareData>,
    /// Optional sender hint for fee tracking
    pub sender_hint: Option<[u8; 32]>,
}

/// An encrypted share for a committee member
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EncryptedShareData {
    /// Target member ID
    pub member_id: u8,
    /// Ephemeral public key
    pub ephemeral_pk: [u8; 32],
    /// Nonce
    pub nonce: [u8; 12],
    /// Encrypted share ciphertext
    pub ciphertext: Vec<u8>,
}

/// Response after submitting encrypted transaction
#[derive(Debug, Serialize)]
pub struct SubmitEncryptedTxResponse {
    pub accepted: bool,
    pub tx_id: String,
    pub position: u64,
    pub message: String,
}

/// Committee member info (public)
#[derive(Debug, Clone, Serialize)]
pub struct CommitteeMemberInfo {
    pub id: u8,
    pub public_key: String,
    pub endpoint: Option<String>,
}

/// Committee info response
#[derive(Debug, Serialize)]
pub struct CommitteeInfoResponse {
    pub enabled: bool,
    pub threshold: usize,
    pub total_members: usize,
    pub epoch: u64,
    pub members: Vec<CommitteeMemberInfo>,
    pub pending_count: usize,
}

// ============================================================================
// Error Response
// ============================================================================

// ============================================================================
// Development/Testing Endpoints
// ============================================================================

/// Request to simulate a deposit (dev mode only)
#[derive(Debug, Deserialize)]
pub struct DevDepositRequest {
    /// Destination account (hex-encoded 32-byte pubkey)
    pub to: String,
    /// Amount to deposit (in lamports)
    pub amount: u64,
}

/// Response after simulating a deposit
#[derive(Debug, Serialize)]
pub struct DevDepositResponse {
    pub tx_hash: String,
    pub accepted: bool,
    pub new_balance: u64,
    pub message: String,
}

/// Request to force seal current batch (dev mode only)
#[derive(Debug, Deserialize)]
pub struct DevSealRequest {
    /// Optional: wait for batch to be proved before returning
    #[serde(default)]
    pub wait_for_proof: bool,
}

/// Response after sealing a batch
#[derive(Debug, Serialize)]
pub struct DevSealResponse {
    pub batch_id: u64,
    pub tx_count: usize,
    pub message: String,
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

// ============================================================================
// Batch & Transaction Query Types
// ============================================================================

/// Pagination parameters for list queries
#[derive(Debug, Deserialize)]
pub struct PaginationParams {
    /// Page offset (0-based)
    #[serde(default)]
    pub offset: usize,
    /// Number of items per page (default: 20, max: 100)
    #[serde(default = "default_limit")]
    pub limit: usize,
}

fn default_limit() -> usize {
    20
}

impl PaginationParams {
    /// Clamp limit to max 100
    pub fn clamped_limit(&self) -> usize {
        self.limit.min(100)
    }
}

impl Default for PaginationParams {
    fn default() -> Self {
        Self {
            offset: 0,
            limit: 20,
        }
    }
}

/// Summary of a settled batch
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchSummary {
    /// Batch ID
    pub batch_id: u64,
    /// Number of transactions in this batch
    pub tx_count: usize,
    /// State root after this batch
    pub state_root: String,
    /// Shielded root after this batch
    pub shielded_root: String,
    /// L1 transaction signature (if settled)
    pub l1_tx_sig: Option<String>,
    /// Settlement status
    pub status: BatchStatus,
    /// Unix timestamp when batch was created
    pub created_at: u64,
    /// Unix timestamp when batch was settled on L1
    pub settled_at: Option<u64>,
}

/// Batch settlement status
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BatchStatus {
    /// Batch is being built
    Building,
    /// Batch is being proved
    Proving,
    /// Batch is pending L1 submission
    PendingSettlement,
    /// Batch has been settled on L1
    Settled,
    /// Batch settlement failed
    Failed,
}

/// Summary of a transaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxSummary {
    /// Transaction hash (hex)
    pub tx_hash: String,
    /// Transaction type
    pub tx_type: TxType,
    /// Batch ID this transaction was included in
    pub batch_id: Option<u64>,
    /// Status of the transaction
    pub status: TxStatus,
    /// Unix timestamp when transaction was received
    pub received_at: u64,
    /// Unix timestamp when transaction was executed
    pub executed_at: Option<u64>,
    /// Amount (for transfers/withdrawals)
    pub amount: Option<u64>,
    /// From account (for transparent txs)
    pub from: Option<String>,
    /// To account/address
    pub to: Option<String>,
}

/// Transaction type
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TxType {
    /// Deposit from L1
    Deposit,
    /// Transparent transfer
    Transfer,
    /// Shielded transaction
    Shielded,
    /// Withdrawal to L1
    Withdrawal,
}

/// Transaction status
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TxStatus {
    /// Transaction is in mempool
    Pending,
    /// Transaction is included in a batch
    Included,
    /// Transaction has been executed
    Executed,
    /// Transaction has been settled on L1
    Settled,
    /// Transaction failed
    Failed,
}

/// Global statistics response
#[derive(Debug, Clone, Serialize)]
pub struct GlobalStats {
    /// Total number of batches settled
    pub total_batches: u64,
    /// Total number of transactions processed
    pub total_transactions: u64,
    /// Total value deposited (lamports)
    pub total_deposited: u64,
    /// Total value withdrawn (lamports)
    pub total_withdrawn: u64,
    /// Current batch being built
    pub current_batch_id: u64,
    /// Number of active accounts
    pub active_accounts: u64,
    /// Number of shielded commitments
    pub shielded_commitments: u64,
    /// Sequencer uptime in seconds
    pub uptime_secs: u64,
}

/// Request to get batch by ID
#[derive(Debug, Deserialize)]
pub struct GetBatchRequest {
    pub batch_id: u64,
}

/// Response with batch details
#[derive(Debug, Serialize)]
pub struct GetBatchResponse {
    pub batch: Option<BatchSummary>,
}

/// Request to list batches
#[derive(Debug, Deserialize)]
pub struct ListBatchesRequest {
    #[serde(flatten)]
    pub pagination: PaginationParams,
}

/// Response with list of batches
#[derive(Debug, Serialize)]
pub struct ListBatchesResponse {
    pub batches: Vec<BatchSummary>,
    pub total: usize,
    pub offset: usize,
    pub limit: usize,
}

/// Request to get transaction by hash
#[derive(Debug, Deserialize)]
pub struct GetTxRequest {
    pub tx_hash: String,
}

/// Response with transaction details
#[derive(Debug, Serialize)]
pub struct GetTxResponse {
    pub tx: Option<TxSummary>,
}

/// Request to list transactions
#[derive(Debug, Deserialize)]
pub struct ListTxsRequest {
    #[serde(flatten)]
    pub pagination: PaginationParams,
    /// Filter by batch ID
    pub batch_id: Option<u64>,
    /// Filter by transaction type
    pub tx_type: Option<TxType>,
    /// Filter by status
    pub status: Option<TxStatus>,
}

/// Response with list of transactions
#[derive(Debug, Serialize)]
pub struct ListTxsResponse {
    pub transactions: Vec<TxSummary>,
    pub total: usize,
    pub offset: usize,
    pub limit: usize,
}
