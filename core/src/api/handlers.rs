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
use crate::sequencer::{
    FastWithdrawManager, PipelineService, RocksDbStore, ShieldedState, ThresholdMempoolManager,
    WithdrawalQueue, WithdrawalState,
};
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
    pub pipeline_service: Arc<PipelineService>,
    pub shielded_state: Arc<Mutex<ShieldedState>>,
    pub withdrawal_queue: Arc<Mutex<WithdrawalQueue>>,
    pub fast_withdraw: Option<Arc<FastWithdrawManager>>,
    pub threshold_mempool: Option<Arc<ThresholdMempoolManager>>,
    pub start_time: std::time::Instant,
    /// Dev mode enables testing endpoints like /dev/deposit and /dev/seal
    pub dev_mode: bool,
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

    // Get latest batch ID from database
    let batch_id = state.db.get_latest_batch_id().unwrap_or(None).unwrap_or(0);

    // Get latest state root from database
    let state_root = state.db.get_latest_state_root().unwrap_or([0u8; 32]);

    Json(StateRootsResponse {
        batch_id,
        state_root: hex::encode(state_root),
        shielded_root: hex::encode(shielded.root()),
        commitment_count: shielded.commitment_count(),
    })
}

/// Get batch status
pub async fn get_batch_status(State(state): State<ApiState>) -> impl IntoResponse {
    match state.pipeline_service.stats().await {
        Ok(stats) => Json(BatchStatusResponse {
            current_batch_id: stats.batch_stats.next_batch_id.saturating_sub(1),
            current_batch_txs: stats.batch_stats.current_batch_txs,
            proving_count: stats.batch_stats.proving_count,
            pending_settlement: stats.batch_stats.pending_settlement_count,
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
        Ok(account_state) => {
            log::debug!(
                "[GET_ACCOUNT] Account {}: balance={}, nonce={}",
                hex::encode(&account_id.0[..8]),
                account_state.balance,
                account_state.nonce
            );
            Json(AccountStateResponse {
                account_id: req.account_id,
                balance: account_state.balance,
                nonce: account_state.nonce,
            })
            .into_response()
        }
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
// Transfer Operations
// ============================================================================

/// Submit a transparent transfer transaction
pub async fn submit_transfer(
    State(state): State<ApiState>,
    Json(req): Json<super::types::TransferRequest>,
) -> impl IntoResponse {
    use zelana_account::AccountId;
    use zelana_transaction::{SignedTransaction, TransactionData};

    // Build the signed transaction
    let tx_data = TransactionData {
        from: AccountId(req.from),
        to: AccountId(req.to),
        amount: req.amount,
        nonce: req.nonce,
        chain_id: req.chain_id,
    };

    let signed_tx = SignedTransaction {
        data: tx_data,
        signature: req.signature,
        signer_pubkey: req.signer_pubkey,
    };

    let tx = TransactionType::Transfer(signed_tx);

    // Compute tx hash (matches TxRouter::compute_tx_hash)
    let tx_hash = {
        let mut hasher = blake3::Hasher::new();
        hasher.update(&req.signer_pubkey);
        hasher.update(&req.nonce.to_le_bytes());
        *hasher.finalize().as_bytes()
    };

    // Submit to pipeline service
    match state.pipeline_service.submit(tx).await {
        Ok(()) => {
            info!(
                "Transfer accepted: {} -> {} amount={} tx_hash={}",
                hex::encode(req.from),
                hex::encode(req.to),
                req.amount,
                hex::encode(tx_hash)
            );
            Json(super::types::TransferResponse {
                tx_hash: hex::encode(tx_hash),
                accepted: true,
                message: "Transfer accepted".to_string(),
            })
            .into_response()
        }
        Err(e) => {
            warn!("Transfer rejected: {}", e);
            (
                StatusCode::BAD_REQUEST,
                Json(super::types::TransferResponse {
                    tx_hash: hex::encode(tx_hash),
                    accepted: false,
                    message: e.to_string(),
                }),
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

    // Submit to pipeline service
    match state.pipeline_service.submit(tx).await {
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
    // Get the range to scan
    let from = req.from_position.unwrap_or(0);
    let limit = req.limit.unwrap_or(1000);

    // Load all encrypted notes from DB
    let encrypted_notes = match state.db.get_all_encrypted_notes() {
        Ok(notes) => notes,
        Err(e) => {
            error!("Failed to load encrypted notes: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::internal("Failed to load notes")),
            )
                .into_response();
        }
    };

    // Also need positions - get commitments to map positions
    let commitments = match state.db.get_all_commitments() {
        Ok(c) => c,
        Err(e) => {
            error!("Failed to load commitments: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::internal("Failed to load commitments")),
            )
                .into_response();
        }
    };

    // Build commitment -> position map
    let commitment_to_pos: std::collections::HashMap<[u8; 32], u32> =
        commitments.into_iter().map(|(pos, cm)| (cm, pos)).collect();

    let mut scanned_notes = Vec::new();
    let mut max_pos = from;
    let total_notes = encrypted_notes.len();

    for (commitment, encrypted_note) in encrypted_notes {
        // Get position for this commitment
        let position = match commitment_to_pos.get(&commitment) {
            Some(&pos) => pos,
            None => continue, // Skip if not in tree yet
        };

        // Skip if before requested range
        if position < from {
            continue;
        }

        // Update max position seen
        if position > max_pos {
            max_pos = position;
        }

        // Try to decrypt the note
        if let Some((note, memo)) = zelana_privacy::try_decrypt_note(
            &encrypted_note,
            &req.decryption_key,
            req.owner_pk,
            &commitment,
        ) {
            // Successfully decrypted - this note belongs to us
            let memo_str = if memo.is_empty() {
                None
            } else {
                String::from_utf8(memo).ok()
            };

            scanned_notes.push(ScannedNote {
                position,
                commitment: hex::encode(commitment),
                value: note.value.0,
                memo: memo_str,
            });

            // Check limit
            if scanned_notes.len() >= limit {
                break;
            }
        }
    }

    // Sort by position
    scanned_notes.sort_by_key(|n| n.position);

    info!(
        "Scanned {} notes, found {} owned",
        total_notes,
        scanned_notes.len()
    );

    Json(ScanNotesResponse {
        notes: scanned_notes,
        scanned_to: max_pos,
    })
    .into_response()
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

    // Submit to pipeline service
    match state.pipeline_service.submit(tx).await {
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

    // First check in-memory queue
    let queue = state.withdrawal_queue.lock().await;

    if let Some(withdrawal) = queue.get(&arr) {
        let state_str = match &withdrawal.state {
            WithdrawalState::Pending => "pending",
            WithdrawalState::InBatch { .. } => "in_batch",
            WithdrawalState::Submitted { .. } => "submitted",
            WithdrawalState::Finalized => "finalized",
            WithdrawalState::Failed { .. } => "failed",
        };

        let l1_sig = if let WithdrawalState::Submitted { l1_tx_sig } = &withdrawal.state {
            Some(l1_tx_sig.clone())
        } else {
            None
        };

        return Json(WithdrawalStatusResponse {
            tx_hash: req.tx_hash,
            state: state_str.to_string(),
            amount: withdrawal.amount,
            to_l1_address: hex::encode(withdrawal.to_l1_address),
            l1_tx_sig: l1_sig,
        })
        .into_response();
    }
    drop(queue);

    // Fall back to database lookup for withdrawals stored during batch execution
    if let Ok(Some(data)) = state.db.get_withdrawal(&arr) {
        // Deserialize the PendingWithdrawal stored by tx_router.commit()
        if let Ok(pw) = serde_json::from_slice::<crate::sequencer::PendingWithdrawal>(&data) {
            return Json(WithdrawalStatusResponse {
                tx_hash: req.tx_hash,
                state: "executed".to_string(), // Withdrawal was executed in a batch
                amount: pw.amount,
                to_l1_address: hex::encode(pw.to_l1_address),
                l1_tx_sig: None,
            })
            .into_response();
        }
    }

    (
        StatusCode::NOT_FOUND,
        Json(ErrorResponse::not_found("Withdrawal not found")),
    )
        .into_response()
}

// ============================================================================
// Fast Withdrawal Operations
// ============================================================================

/// Get quote for fast withdrawal
pub async fn fast_withdraw_quote(
    State(state): State<ApiState>,
    Json(req): Json<FastWithdrawQuoteRequest>,
) -> impl IntoResponse {
    let fast_withdraw = match &state.fast_withdraw {
        Some(fw) => fw,
        None => {
            return Json(FastWithdrawQuoteResponse {
                available: false,
                amount: req.amount,
                fee: 0,
                amount_received: 0,
                fee_bps: 0,
                lp_address: None,
            })
            .into_response();
        }
    };

    match fast_withdraw.get_quote(req.amount).await {
        Some(quote) => Json(FastWithdrawQuoteResponse {
            available: true,
            amount: quote.amount,
            fee: quote.fee,
            amount_received: quote.amount_received,
            fee_bps: quote.fee_bps,
            lp_address: Some(hex::encode(quote.lp_address)),
        })
        .into_response(),
        None => Json(FastWithdrawQuoteResponse {
            available: false,
            amount: req.amount,
            fee: 0,
            amount_received: 0,
            fee_bps: 0,
            lp_address: None,
        })
        .into_response(),
    }
}

/// Execute fast withdrawal
pub async fn execute_fast_withdraw(
    State(state): State<ApiState>,
    Json(req): Json<FastWithdrawRequest>,
) -> impl IntoResponse {
    let fast_withdraw = match &state.fast_withdraw {
        Some(fw) => fw,
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(FastWithdrawResponse {
                    success: false,
                    claim_id: None,
                    amount_fronted: 0,
                    fee: 0,
                    message: "Fast withdrawals not enabled".to_string(),
                }),
            )
                .into_response();
        }
    };

    match fast_withdraw
        .execute_fast_withdraw(
            req.withdrawal_tx_hash,
            req.user_l1_address,
            req.amount,
            req.lp_address,
        )
        .await
    {
        Ok(claim) => {
            info!(
                "Fast withdrawal executed: claim_id={}",
                hex::encode(claim.claim_id)
            );
            Json(FastWithdrawResponse {
                success: true,
                claim_id: Some(hex::encode(claim.claim_id)),
                amount_fronted: claim.amount_fronted,
                fee: claim.fee,
                message: "Fast withdrawal executed successfully".to_string(),
            })
            .into_response()
        }
        Err(e) => {
            warn!("Fast withdrawal failed: {}", e);
            (
                StatusCode::BAD_REQUEST,
                Json(FastWithdrawResponse {
                    success: false,
                    claim_id: None,
                    amount_fronted: 0,
                    fee: 0,
                    message: e.to_string(),
                }),
            )
                .into_response()
        }
    }
}

/// Register as liquidity provider
pub async fn register_lp(
    State(state): State<ApiState>,
    Json(req): Json<RegisterLpRequest>,
) -> impl IntoResponse {
    let fast_withdraw = match &state.fast_withdraw {
        Some(fw) => fw,
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(RegisterLpResponse {
                    success: false,
                    message: "Fast withdrawals not enabled".to_string(),
                }),
            )
                .into_response();
        }
    };

    match fast_withdraw
        .register_lp(
            req.l1_address,
            req.l2_address,
            req.collateral,
            req.custom_fee_bps,
        )
        .await
    {
        Ok(()) => {
            info!("LP registered: {}", hex::encode(req.l1_address));
            Json(RegisterLpResponse {
                success: true,
                message: "LP registered successfully".to_string(),
            })
            .into_response()
        }
        Err(e) => {
            warn!("LP registration failed: {}", e);
            (
                StatusCode::BAD_REQUEST,
                Json(RegisterLpResponse {
                    success: false,
                    message: e.to_string(),
                }),
            )
                .into_response()
        }
    }
}

// ============================================================================
// Encrypted Mempool Operations (Threshold Encryption)
// ============================================================================

/// Submit an encrypted transaction to the threshold mempool
pub async fn submit_encrypted_tx(
    State(state): State<ApiState>,
    Json(req): Json<SubmitEncryptedTxRequest>,
) -> impl IntoResponse {
    let threshold_mempool = match &state.threshold_mempool {
        Some(tm) => tm,
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(SubmitEncryptedTxResponse {
                    accepted: false,
                    tx_id: hex::encode(req.tx_id),
                    position: 0,
                    message: "Threshold encryption not enabled".to_string(),
                }),
            )
                .into_response();
        }
    };

    // Check if active
    if !threshold_mempool.is_active().await {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(SubmitEncryptedTxResponse {
                accepted: false,
                tx_id: hex::encode(req.tx_id),
                position: 0,
                message: "Threshold encryption committee not initialized".to_string(),
            }),
        )
            .into_response();
    }

    // Convert API types to SDK types
    use zelana_threshold::EncryptedTransaction;
    use zelana_threshold::committee::EncryptedShare;

    let encrypted_shares: Vec<EncryptedShare> = req
        .encrypted_shares
        .into_iter()
        .map(|s| EncryptedShare {
            member_id: s.member_id,
            ephemeral_pk: s.ephemeral_pk,
            nonce: s.nonce,
            ciphertext: s.ciphertext,
        })
        .collect();

    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);

    let encrypted_tx = EncryptedTransaction {
        tx_id: req.tx_id,
        epoch: req.epoch,
        nonce: req.nonce,
        ciphertext: req.ciphertext,
        encrypted_shares,
        timestamp,
        sender_hint: req.sender_hint,
    };

    // Add to mempool
    match threshold_mempool.add_encrypted_tx(encrypted_tx).await {
        Ok(()) => {
            let pending = threshold_mempool.pending_count().await;
            info!(
                "Encrypted tx accepted: {}, pending={}",
                hex::encode(req.tx_id),
                pending
            );
            Json(SubmitEncryptedTxResponse {
                accepted: true,
                tx_id: hex::encode(req.tx_id),
                position: pending as u64,
                message: "Encrypted transaction accepted".to_string(),
            })
            .into_response()
        }
        Err(e) => {
            warn!("Encrypted tx rejected: {}", e);
            (
                StatusCode::BAD_REQUEST,
                Json(SubmitEncryptedTxResponse {
                    accepted: false,
                    tx_id: hex::encode(req.tx_id),
                    position: 0,
                    message: e.to_string(),
                }),
            )
                .into_response()
        }
    }
}

/// Get committee information for clients to encrypt transactions
pub async fn get_committee_info(State(state): State<ApiState>) -> impl IntoResponse {
    let threshold_mempool = match &state.threshold_mempool {
        Some(tm) => tm,
        None => {
            return Json(CommitteeInfoResponse {
                enabled: false,
                threshold: 0,
                total_members: 0,
                epoch: 0,
                members: vec![],
                pending_count: 0,
            })
            .into_response();
        }
    };

    let committee = match threshold_mempool.committee().await {
        Some(c) => c,
        None => {
            return Json(CommitteeInfoResponse {
                enabled: false,
                threshold: 0,
                total_members: 0,
                epoch: 0,
                members: vec![],
                pending_count: 0,
            })
            .into_response();
        }
    };

    let members: Vec<CommitteeMemberInfo> = committee
        .members
        .iter()
        .map(|m| CommitteeMemberInfo {
            id: m.id,
            public_key: hex::encode(m.public_key),
            endpoint: m.endpoint.clone(),
        })
        .collect();

    let pending = threshold_mempool.pending_count().await;

    Json(CommitteeInfoResponse {
        enabled: true,
        threshold: committee.config.threshold,
        total_members: committee.config.total_members,
        epoch: committee.config.epoch,
        members,
        pending_count: pending,
    })
    .into_response()
}

// ============================================================================
// Batch & Transaction Query Operations
// ============================================================================

/// Get a batch by ID
pub async fn get_batch(
    State(state): State<ApiState>,
    Json(req): Json<GetBatchRequest>,
) -> impl IntoResponse {
    match state.db.get_batch_summary(req.batch_id) {
        Ok(batch) => Json(GetBatchResponse { batch }).into_response(),
        Err(e) => {
            error!("Failed to get batch: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::internal("Failed to get batch")),
            )
                .into_response()
        }
    }
}

/// List batches with pagination
pub async fn list_batches(
    State(state): State<ApiState>,
    Json(req): Json<ListBatchesRequest>,
) -> impl IntoResponse {
    let limit = req.pagination.clamped_limit();
    let offset = req.pagination.offset;

    match state.db.list_batches(offset, limit) {
        Ok((batches, total)) => Json(ListBatchesResponse {
            batches,
            total,
            offset,
            limit,
        })
        .into_response(),
        Err(e) => {
            error!("Failed to list batches: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::internal("Failed to list batches")),
            )
                .into_response()
        }
    }
}

/// Get a transaction by hash
pub async fn get_transaction(
    State(state): State<ApiState>,
    Json(req): Json<GetTxRequest>,
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

    match state.db.get_tx_summary(&arr) {
        Ok(tx) => Json(GetTxResponse { tx }).into_response(),
        Err(e) => {
            error!("Failed to get transaction: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::internal("Failed to get transaction")),
            )
                .into_response()
        }
    }
}

/// List transactions with pagination and filters
pub async fn list_transactions(
    State(state): State<ApiState>,
    Json(req): Json<ListTxsRequest>,
) -> impl IntoResponse {
    let limit = req.pagination.clamped_limit();
    let offset = req.pagination.offset;

    match state
        .db
        .list_transactions(offset, limit, req.batch_id, req.tx_type, req.status)
    {
        Ok((transactions, total)) => Json(ListTxsResponse {
            transactions,
            total,
            offset,
            limit,
        })
        .into_response(),
        Err(e) => {
            error!("Failed to list transactions: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::internal("Failed to list transactions")),
            )
                .into_response()
        }
    }
}

/// Get global statistics
pub async fn get_global_stats(State(state): State<ApiState>) -> impl IntoResponse {
    let uptime = state.start_time.elapsed().as_secs();

    // Get stats from DB
    let (total_batches, total_transactions) = match state.db.get_global_stats() {
        Ok(stats) => stats,
        Err(e) => {
            error!("Failed to get global stats: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::internal("Failed to get stats")),
            )
                .into_response();
        }
    };

    let active_accounts = state.db.count_active_accounts().unwrap_or(0);

    let shielded = state.shielded_state.lock().await;
    let shielded_commitments = shielded.commitment_count();
    drop(shielded);

    // Get current batch info from pipeline
    let current_batch_id = match state.pipeline_service.stats().await {
        Ok(stats) => stats.batch_stats.next_batch_id.saturating_sub(1),
        Err(_) => 0,
    };

    // Get deposit/withdrawal totals from DB
    let total_deposited = state.db.get_total_deposits().unwrap_or(0);
    let total_withdrawn = state.db.get_withdrawals_total().unwrap_or(0);

    Json(GlobalStats {
        total_batches,
        total_transactions,
        total_deposited,
        total_withdrawn,
        current_batch_id,
        active_accounts,
        shielded_commitments,
        uptime_secs: uptime,
    })
    .into_response()
}

// ============================================================================
// Development/Testing Operations (only available in dev mode)
// ============================================================================

/// Simulate a deposit (dev mode only)
/// This allows testing the full pipeline without a real L1 indexer.
pub async fn dev_deposit(
    State(state): State<ApiState>,
    Json(req): Json<super::types::DevDepositRequest>,
) -> impl IntoResponse {
    // Check dev mode
    if !state.dev_mode {
        return (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::not_found("Endpoint not available")),
        )
            .into_response();
    }

    // Parse destination address
    let to_bytes = match hex::decode(&req.to) {
        Ok(bytes) if bytes.len() == 32 => bytes,
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::bad_request(
                    "Invalid 'to' address format (expected 32-byte hex)",
                )),
            )
                .into_response();
        }
    };

    let mut to_arr = [0u8; 32];
    to_arr.copy_from_slice(&to_bytes);
    let to = AccountId(to_arr);

    // Generate a mock L1 sequence number (timestamp-based for uniqueness)
    let l1_seq = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_micros() as u64)
        .unwrap_or(0);

    // Create deposit event
    use zelana_transaction::{DepositEvent, TransactionType};
    let deposit = DepositEvent {
        to: to.clone(),
        amount: req.amount,
        l1_seq,
    };

    // Compute tx hash (same as TxRouter for deposits)
    let tx_hash = {
        let mut hasher = blake3::Hasher::new();
        hasher.update(&to_arr);
        hasher.update(&l1_seq.to_le_bytes());
        *hasher.finalize().as_bytes()
    };

    // Submit to pipeline
    match state
        .pipeline_service
        .submit(TransactionType::Deposit(deposit))
        .await
    {
        Ok(()) => {
            info!(
                "[DEV] Deposit accepted: to={} amount={} tx_hash={}",
                req.to,
                req.amount,
                hex::encode(tx_hash)
            );

            // Track dev deposit amount
            if let Err(e) = state.db.add_dev_deposit(req.amount) {
                warn!("[DEV] Failed to track deposit amount: {}", e);
            }

            // Query new balance
            let new_balance = state
                .db
                .get_account_state(&to)
                .map(|acc| acc.balance)
                .unwrap_or(0);

            Json(super::types::DevDepositResponse {
                tx_hash: hex::encode(tx_hash),
                accepted: true,
                new_balance,
                message: "Dev deposit accepted".to_string(),
            })
            .into_response()
        }
        Err(e) => {
            warn!("[DEV] Deposit rejected: {}", e);
            (
                StatusCode::BAD_REQUEST,
                Json(super::types::DevDepositResponse {
                    tx_hash: hex::encode(tx_hash),
                    accepted: false,
                    new_balance: 0,
                    message: e.to_string(),
                }),
            )
                .into_response()
        }
    }
}

/// Force seal the current batch (dev mode only)
/// This allows testing batch settlement without waiting for the timer.
pub async fn dev_seal(
    State(state): State<ApiState>,
    Json(req): Json<super::types::DevSealRequest>,
) -> impl IntoResponse {
    // Check dev mode
    if !state.dev_mode {
        return (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::not_found("Endpoint not available")),
        )
            .into_response();
    }

    // Force seal the batch
    match state.pipeline_service.force_seal().await {
        Ok(seal_result) => {
            info!(
                "[DEV] Batch sealed: batch_id={} tx_count={}",
                seal_result.batch_id, seal_result.tx_count
            );

            // Optionally wait for proof
            if req.wait_for_proof && seal_result.tx_count > 0 {
                // Poll for proof completion (with timeout)
                let timeout = tokio::time::Duration::from_secs(30);
                let start = std::time::Instant::now();

                while start.elapsed() < timeout {
                    if let Ok(stats) = state.pipeline_service.stats().await {
                        // Check if this batch is no longer in proving queue
                        if stats.batch_stats.proving_count == 0
                            || stats.batch_stats.next_batch_id > seal_result.batch_id + 1
                        {
                            break;
                        }
                    }
                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                }
            }

            Json(super::types::DevSealResponse {
                batch_id: seal_result.batch_id,
                tx_count: seal_result.tx_count,
                message: format!(
                    "Batch {} sealed with {} transactions",
                    seal_result.batch_id, seal_result.tx_count
                ),
            })
            .into_response()
        }
        Err(e) => {
            warn!("[DEV] Seal failed: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::internal(format!(
                    "Failed to seal batch: {}",
                    e
                ))),
            )
                .into_response()
        }
    }
}
