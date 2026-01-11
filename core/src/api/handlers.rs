//! API Handlers
//!
//! Request handlers for the HTTP API.

use std::sync::Arc;

use axum::{
    extract::{Json, State},
    http::StatusCode,
    response::IntoResponse,
};
use log::{error, info, warn};
use tokio::sync::Mutex;

use super::types::*;
use crate::sequencer::batch::BatchService;
use crate::sequencer::db::RocksDbStore;
use crate::sequencer::shielded_state::ShieldedState;
use crate::sequencer::withdrawals::WithdrawalQueue;
use crate::storage::StateStore;
use zelana_account::AccountId;
use zelana_transaction::{PrivateTransaction, TransactionType};

// ============================================================================
// Shared State
// ============================================================================

/// Shared application state for API handlers
#[derive(Clone)]
pub struct ApiState {
    pub db: Arc<RocksDbStore>,
    pub batch_service: Arc<BatchService>,
    pub shielded_state: Arc<Mutex<ShieldedState>>,
    pub withdrawal_queue: Arc<Mutex<WithdrawalQueue>>,
    pub start_time: std::time::Instant,
}

// ============================================================================
// Health & Status
// ============================================================================

/// Health check endpoint
pub async fn health(State(state): State<ApiState>) -> impl IntoResponse {
    let uptime = state.start_time.elapsed().as_secs();

    Json(HealthResponse {
        healthy: true,
        version: env!("CARGO_PKG_VERSION").to_string(),
        uptime_secs: uptime,
    })
}

/// Get current state roots
pub async fn get_state_roots(State(state): State<ApiState>) -> impl IntoResponse {
    let shielded = state.shielded_state.lock().await;

    Json(StateRootsResponse {
        batch_id: 0,                        // TODO: Get from batch service
        state_root: hex::encode([0u8; 32]), // TODO: Get from state
        shielded_root: hex::encode(shielded.root()),
        commitment_count: shielded.commitment_count(),
    })
}

/// Get batch status
pub async fn get_batch_status(State(state): State<ApiState>) -> impl IntoResponse {
    match state.batch_service.stats().await {
        Ok(stats) => Json(BatchStatusResponse {
            current_batch_id: stats.next_batch_id.saturating_sub(1),
            current_batch_txs: stats.current_batch_txs,
            proving_count: stats.proving_count,
            pending_settlement: stats.pending_settlement_count,
        })
        .into_response(),
        Err(e) => {
            error!("Failed to get batch stats: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::internal("Failed to get batch status")),
            )
                .into_response()
        }
    }
}

// ============================================================================
// Account Operations
// ============================================================================

/// Get account state
pub async fn get_account(
    State(state): State<ApiState>,
    Json(req): Json<GetAccountRequest>,
) -> impl IntoResponse {
    let account_bytes = match hex::decode(&req.account_id) {
        Ok(bytes) if bytes.len() == 32 => bytes,
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::bad_request("Invalid account ID format")),
            )
                .into_response();
        }
    };

    let mut arr = [0u8; 32];
    arr.copy_from_slice(&account_bytes);
    let account_id = AccountId(arr);

    match state.db.get_account_state(&account_id) {
        Ok(account_state) => Json(AccountStateResponse {
            account_id: req.account_id,
            balance: account_state.balance,
            nonce: account_state.nonce,
        })
        .into_response(),
        Err(e) => {
            error!("Failed to get account: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::internal("Failed to get account state")),
            )
                .into_response()
        }
    }
}

// ============================================================================
// Shielded Operations
// ============================================================================

/// Submit a shielded transaction
pub async fn submit_shielded(
    State(state): State<ApiState>,
    Json(req): Json<SubmitShieldedRequest>,
) -> impl IntoResponse {
    // Build the private transaction
    let private_tx = PrivateTransaction {
        proof: req.proof,
        nullifier: req.nullifier,
        commitment: req.commitment,
        ciphertext: req.ciphertext,
        ephemeral_key: req.ephemeral_key,
    };

    let tx = TransactionType::Shielded(private_tx);

    // Compute tx hash
    let tx_hash = {
        let mut hasher = blake3::Hasher::new();
        hasher.update(&req.nullifier);
        hasher.update(&req.commitment);
        *hasher.finalize().as_bytes()
    };

    // Submit to batch service
    match state.batch_service.submit(tx).await {
        Ok(()) => {
            info!("Shielded tx accepted: {}", hex::encode(tx_hash));
            Json(SubmitShieldedResponse {
                tx_hash: hex::encode(tx_hash),
                accepted: true,
                position: None, // Position assigned after execution
                message: "Transaction accepted".to_string(),
            })
            .into_response()
        }
        Err(e) => {
            warn!("Shielded tx rejected: {}", e);
            (
                StatusCode::BAD_REQUEST,
                Json(SubmitShieldedResponse {
                    tx_hash: hex::encode(tx_hash),
                    accepted: false,
                    position: None,
                    message: e.to_string(),
                }),
            )
                .into_response()
        }
    }
}

/// Get merkle path for a commitment
pub async fn get_merkle_path(
    State(state): State<ApiState>,
    Json(req): Json<MerklePathRequest>,
) -> impl IntoResponse {
    let shielded = state.shielded_state.lock().await;

    match shielded.get_path(req.position) {
        Some(path) => {
            let path_hex: Vec<String> = path.siblings.iter().map(hex::encode).collect();

            Json(MerklePathResponse {
                position: req.position,
                path: path_hex,
                root: hex::encode(shielded.root()),
            })
            .into_response()
        }
        None => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::not_found("Position not found in tree")),
        )
            .into_response(),
    }
}

/// Scan for notes (simplified - full implementation would decrypt notes)
pub async fn scan_notes(
    State(state): State<ApiState>,
    Json(req): Json<ScanNotesRequest>,
) -> impl IntoResponse {
    // Note: Full implementation would:
    // 1. Load all encrypted notes from DB
    // 2. Try to decrypt each with the viewing key
    // 3. Return successfully decrypted notes

    // For MVP, return empty list
    // TODO: Implement full note scanning

    let from = req.from_position.unwrap_or(0);
    let shielded = state.shielded_state.lock().await;
    let to = shielded.next_position();

    Json(ScanNotesResponse {
        notes: vec![],
        scanned_to: to,
    })
}

// ============================================================================
// Withdrawal Operations
// ============================================================================

/// Submit a withdrawal request
pub async fn submit_withdrawal(
    State(state): State<ApiState>,
    Json(req): Json<WithdrawRequest>,
) -> impl IntoResponse {
    // Build withdrawal transaction
    let withdraw_req = zelana_transaction::WithdrawRequest {
        from: AccountId(req.from),
        to_l1_address: req.to_l1_address,
        amount: req.amount,
        nonce: req.nonce,
        signature: req.signature,
        signer_pubkey: req.signer_pubkey,
    };

    let tx = TransactionType::Withdraw(withdraw_req);

    // Compute tx hash
    let tx_hash = {
        let mut hasher = blake3::Hasher::new();
        hasher.update(&req.from);
        hasher.update(&req.nonce.to_le_bytes());
        *hasher.finalize().as_bytes()
    };

    // Submit to batch service
    match state.batch_service.submit(tx).await {
        Ok(()) => {
            info!("Withdrawal accepted: {}", hex::encode(tx_hash));
            Json(WithdrawResponse {
                tx_hash: hex::encode(tx_hash),
                accepted: true,
                estimated_completion: Some("~7 days (challenge period)".to_string()),
                message: "Withdrawal request accepted".to_string(),
            })
            .into_response()
        }
        Err(e) => {
            warn!("Withdrawal rejected: {}", e);
            (
                StatusCode::BAD_REQUEST,
                Json(WithdrawResponse {
                    tx_hash: hex::encode(tx_hash),
                    accepted: false,
                    estimated_completion: None,
                    message: e.to_string(),
                }),
            )
                .into_response()
        }
    }
}

/// Get withdrawal status
pub async fn get_withdrawal_status(
    State(state): State<ApiState>,
    Json(req): Json<WithdrawalStatusRequest>,
) -> impl IntoResponse {
    let tx_hash_bytes = match hex::decode(&req.tx_hash) {
        Ok(bytes) if bytes.len() == 32 => bytes,
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::bad_request("Invalid tx hash format")),
            )
                .into_response();
        }
    };

    let mut arr = [0u8; 32];
    arr.copy_from_slice(&tx_hash_bytes);

    let queue = state.withdrawal_queue.lock().await;

    match queue.get(&arr) {
        Some(withdrawal) => {
            let state_str = match &withdrawal.state {
                crate::sequencer::withdrawals::WithdrawalState::Pending => "pending",
                crate::sequencer::withdrawals::WithdrawalState::InBatch { .. } => "in_batch",
                crate::sequencer::withdrawals::WithdrawalState::Submitted { .. } => "submitted",
                crate::sequencer::withdrawals::WithdrawalState::Finalized => "finalized",
                crate::sequencer::withdrawals::WithdrawalState::Failed { .. } => "failed",
            };

            let l1_sig =
                if let crate::sequencer::withdrawals::WithdrawalState::Submitted { l1_tx_sig } =
                    &withdrawal.state
                {
                    Some(l1_tx_sig.clone())
                } else {
                    None
                };

            Json(WithdrawalStatusResponse {
                tx_hash: req.tx_hash,
                state: state_str.to_string(),
                amount: withdrawal.amount,
                to_l1_address: hex::encode(withdrawal.to_l1_address),
                l1_tx_sig: l1_sig,
            })
            .into_response()
        }
        None => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::not_found("Withdrawal not found")),
        )
            .into_response(),
    }
}
