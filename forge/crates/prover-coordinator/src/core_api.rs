//! Core API Module
//!
//! Provides HTTP endpoints for the Zelana Core Sequencer integration.
//! Uses SSE (Server-Sent Events) for async proof status updates.
//!
//! ## Endpoints
//!
//! - `POST /v2/batch/prove` - Submit batch for proving, returns job_id
//! - `GET /v2/batch/:job_id/status` - Get proof job status (SSE stream)
//! - `GET /v2/batch/:job_id/proof` - Get completed proof
//! - `DELETE /v2/batch/:job_id` - Cancel proof job

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::{
        IntoResponse, Response,
        sse::{Event, Sse},
    },
    routing::{delete, get, post},
};
use futures::stream::Stream;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    convert::Infallible,
    path::PathBuf,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::sync::{RwLock, broadcast};
use tokio_stream::StreamExt;
use tokio_stream::wrappers::BroadcastStream;
use tracing::{error, info, warn};

// Types matching Core Sequencer

/// Request from core sequencer to prove a batch
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoreBatchProveRequest {
    /// Batch ID from sequencer
    pub batch_id: u64,

// State roots (32-byte hex strings)
    pub pre_state_root: String,
    pub post_state_root: String,
    pub pre_shielded_root: String,
    pub post_shielded_root: String,

// Transaction data
    #[serde(default)]
    pub transfers: Vec<CoreTransferWitness>,
    #[serde(default)]
    pub withdrawals: Vec<CoreWithdrawalWitness>,
    #[serde(default)]
    pub shielded: Vec<CoreShieldedWitness>,
}

/// Transfer witness from core sequencer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoreTransferWitness {
    pub sender_pubkey: String,
    pub sender_balance: u64,
    pub sender_nonce: u64,
    pub sender_merkle_path: Vec<String>,
    pub sender_path_indices: Vec<u8>,
    pub receiver_pubkey: String,
    pub receiver_balance: u64,
    pub receiver_nonce: u64,
    pub receiver_merkle_path: Vec<String>,
    pub receiver_path_indices: Vec<u8>,
    pub amount: u64,
    pub signature: String,
}

/// Withdrawal witness from core sequencer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoreWithdrawalWitness {
    pub sender_pubkey: String,
    pub sender_balance: u64,
    pub sender_nonce: u64,
    pub sender_merkle_path: Vec<String>,
    pub sender_path_indices: Vec<u8>,
    pub l1_recipient: String, // 32-byte Solana address
    pub amount: u64,
    pub signature: String,
}

/// Shielded witness from core sequencer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoreShieldedWitness {
    pub input_commitment: String,
    pub input_value: u64,
    pub input_blinding: String,
    pub input_position: u64,
    pub input_merkle_path: Vec<String>,
    pub input_path_indices: Vec<u8>,
    pub spending_key: String,
    pub output_owner: String,
    pub output_value: u64,
    pub output_blinding: String,
    pub output_commitment: String, // Pre-computed commitment for pass-through mode
    pub nullifier: String,
}

/// Response after submitting a batch for proving
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoreBatchProveResponse {
    /// Unique job ID for tracking
    pub job_id: String,
    /// Batch ID (echoed back)
    pub batch_id: u64,
    /// Estimated proving time (ms)
    pub estimated_time_ms: u64,
    /// SSE endpoint for status updates
    pub status_url: String,
}

/// Proof result response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoreProofResult {
    /// Job ID
    pub job_id: String,
    /// Batch ID
    pub batch_id: u64,
    /// Raw proof bytes (388 bytes, hex encoded)
    pub proof_bytes: String,
    /// Raw public witness bytes (236 bytes, hex encoded)
    pub public_witness_bytes: String,
    /// Circuit-computed batch hash (MiMC)
    pub batch_hash: String,
    /// Circuit-computed withdrawal root (MiMC)
    pub withdrawal_root: String,
    /// Proving time in milliseconds
    pub proving_time_ms: u64,
}

/// Job status for SSE updates
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProofJobStatus {
    pub job_id: String,
    pub batch_id: u64,
    pub state: ProofJobState,
    pub progress_pct: u8,
    pub message: String,
    pub created_at: u64,
    pub updated_at: u64,
    pub completed_at: Option<u64>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProofJobState {
    Pending,
    Preparing,
    Proving,
    Completed,
    Failed,
    Cancelled,
}

/// SSE event for proof status
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ProofStatusEvent {
    Status(ProofJobStatus),
    Progress {
        job_id: String,
        progress_pct: u8,
        message: String,
    },
    Completed(CoreProofResult),
    Failed {
        job_id: String,
        error: String,
    },
}

// API Response Wrapper

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "lowercase")]
pub enum ApiResponse<T> {
    Success {
        data: T,
    },
    Error {
        message: String,
        code: Option<String>,
    },
}

impl<T> ApiResponse<T> {
    pub fn success(data: T) -> Self {
        ApiResponse::Success { data }
    }

    pub fn error(message: impl Into<String>) -> Self {
        ApiResponse::Error {
            message: message.into(),
            code: None,
        }
    }

    pub fn error_with_code(message: impl Into<String>, code: impl Into<String>) -> Self {
        ApiResponse::Error {
            message: message.into(),
            code: Some(code.into()),
        }
    }
}

// Proof Cache

/// Cached proof entry
#[derive(Debug, Clone)]
pub struct CachedProof {
    pub result: CoreProofResult,
    pub cached_at: u64,
    /// Expires after this duration (seconds)
    pub ttl_secs: u64,
}

impl CachedProof {
    pub fn is_expired(&self) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        now > self.cached_at + self.ttl_secs
    }
}

/// Proof cache with TTL
#[derive(Debug, Clone, Default)]
pub struct ProofCache {
    /// Map of batch_id -> cached proof
    proofs: HashMap<u64, CachedProof>,
    /// Map of job_id -> batch_id (for lookup)
    job_to_batch: HashMap<String, u64>,
}

impl ProofCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a proof into the cache
    pub fn insert(
        &mut self,
        job_id: String,
        batch_id: u64,
        result: CoreProofResult,
        ttl_secs: u64,
    ) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        self.proofs.insert(
            batch_id,
            CachedProof {
                result,
                cached_at: now,
                ttl_secs,
            },
        );
        self.job_to_batch.insert(job_id, batch_id);
    }

    /// Get a proof by job_id
    pub fn get_by_job(&self, job_id: &str) -> Option<&CoreProofResult> {
        let batch_id = self.job_to_batch.get(job_id)?;
        self.get_by_batch(*batch_id)
    }

    /// Get a proof by batch_id
    pub fn get_by_batch(&self, batch_id: u64) -> Option<&CoreProofResult> {
        let cached = self.proofs.get(&batch_id)?;
        if cached.is_expired() {
            None
        } else {
            Some(&cached.result)
        }
    }

    /// Clean up expired entries
    pub fn cleanup_expired(&mut self) {
        let expired_batches: Vec<u64> = self
            .proofs
            .iter()
            .filter(|(_, v)| v.is_expired())
            .map(|(k, _)| *k)
            .collect();

        for batch_id in expired_batches {
            self.proofs.remove(&batch_id);
        }

        // Also clean up job mappings
        self.job_to_batch
            .retain(|_, batch_id| self.proofs.contains_key(batch_id));
    }
}

// Core API State

/// Configuration for Core API
#[derive(Debug, Clone)]
pub struct CoreApiConfig {
    /// Path to Noir circuit directory
    pub circuit_path: PathBuf,
    /// Use mock prover (for testing)
    pub mock_prover: bool,
    /// Mock proving delay (ms)
    pub mock_delay_ms: u64,
    /// Proof cache TTL (seconds)
    pub cache_ttl_secs: u64,
    /// Maximum concurrent proving jobs
    pub max_concurrent_jobs: usize,
}

impl Default for CoreApiConfig {
    fn default() -> Self {
        Self {
            circuit_path: PathBuf::from("../../circuits/zelana_batch"),
            mock_prover: true,
            cache_ttl_secs: 3600, // 1 hour
            max_concurrent_jobs: 4,
            mock_delay_ms: 1000,
        }
    }
}

/// Internal job tracking
#[derive(Debug)]
pub struct ProofJob {
    pub status: ProofJobStatus,
    pub request: CoreBatchProveRequest,
    /// Channel to broadcast status updates
    pub status_tx: broadcast::Sender<ProofStatusEvent>,
}

/// Core API shared state
pub struct CoreApiState {
    pub config: CoreApiConfig,
    /// Active proof jobs
    pub jobs: HashMap<String, ProofJob>,
    /// Completed proof cache
    pub cache: ProofCache,
    /// Current active job count
    pub active_jobs: usize,
}

pub type SharedCoreApiState = Arc<RwLock<CoreApiState>>;

impl CoreApiState {
    pub fn new(config: CoreApiConfig) -> Self {
        Self {
            config,
            jobs: HashMap::new(),
            cache: ProofCache::new(),
            active_jobs: 0,
        }
    }
}

// Router

/// Create the Core API router
pub fn core_api_router(state: SharedCoreApiState) -> Router {
    Router::new()
        .route("/v2/batch/prove", post(prove_handler))
        .route("/v2/batch/:job_id/status", get(status_sse_handler))
        .route("/v2/batch/:job_id/proof", get(get_proof_handler))
        .route("/v2/batch/:job_id", delete(cancel_handler))
        .route("/v2/health", get(health_handler))
        .with_state(state)
}

// Handlers

/// Health check for core API
async fn health_handler(
    State(state): State<SharedCoreApiState>,
) -> Json<ApiResponse<serde_json::Value>> {
    let api_state = state.read().await;

    let health = serde_json::json!({
        "status": "ok",
        "active_jobs": api_state.active_jobs,
        "max_concurrent_jobs": api_state.config.max_concurrent_jobs,
        "cached_proofs": api_state.cache.proofs.len(),
        "mock_prover": api_state.config.mock_prover,
    });

    Json(ApiResponse::success(health))
}

/// Submit a batch for proving
async fn prove_handler(
    State(state): State<SharedCoreApiState>,
    Json(request): Json<CoreBatchProveRequest>,
) -> Result<Json<ApiResponse<CoreBatchProveResponse>>, StatusCode> {
    // Check if we have a cached proof for this batch
    {
        let api_state = state.read().await;
        if let Some(cached) = api_state.cache.get_by_batch(request.batch_id) {
            info!("Returning cached proof for batch {}", request.batch_id);
            // Return existing job_id if we have it
            return Ok(Json(ApiResponse::success(CoreBatchProveResponse {
                job_id: cached.job_id.clone(),
                batch_id: request.batch_id,
                estimated_time_ms: 0,
                status_url: format!("/v2/batch/{}/status", cached.job_id),
            })));
        }
    }

    // Check capacity
    {
        let api_state = state.read().await;
        if api_state.active_jobs >= api_state.config.max_concurrent_jobs {
            return Ok(Json(ApiResponse::error_with_code(
                format!(
                    "Prover at capacity ({}/{})",
                    api_state.active_jobs, api_state.config.max_concurrent_jobs
                ),
                "CAPACITY_EXCEEDED",
            )));
        }
    }

    // Generate job ID
    let job_id = format!("pj_{}", uuid::Uuid::new_v4().simple());
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    // Create broadcast channel for status updates
    let (status_tx, _) = broadcast::channel::<ProofStatusEvent>(16);

    // Create job
    let job = ProofJob {
        status: ProofJobStatus {
            job_id: job_id.clone(),
            batch_id: request.batch_id,
            state: ProofJobState::Pending,
            progress_pct: 0,
            message: "Proof job created".to_string(),
            created_at: now,
            updated_at: now,
            completed_at: None,
            error: None,
        },
        request: request.clone(),
        status_tx: status_tx.clone(),
    };

    // Store job and increment active count
    let (mock_prover, mock_delay, circuit_path, cache_ttl) = {
        let mut api_state = state.write().await;
        api_state.jobs.insert(job_id.clone(), job);
        api_state.active_jobs += 1;
        (
            api_state.config.mock_prover,
            api_state.config.mock_delay_ms,
            api_state.config.circuit_path.clone(),
            api_state.config.cache_ttl_secs,
        )
    };

    info!(
        "Created proof job {} for batch {}",
        job_id, request.batch_id
    );

    // Spawn async proving task
    let state_clone = state.clone();
    let job_id_clone = job_id.clone();
    let batch_id = request.batch_id;

    // Calculate estimated time before moving request
    let num_txs = request.transfers.len() + request.withdrawals.len() + request.shielded.len();
    let estimated_time_ms = if mock_prover {
        mock_delay
    } else {
        // Rough estimate: base time + time per transaction
        30_000 + (num_txs as u64 * 5_000)
    };

    tokio::spawn(async move {
        execute_proof_job(
            state_clone,
            job_id_clone,
            batch_id,
            request,
            status_tx,
            mock_prover,
            mock_delay,
            circuit_path,
            cache_ttl,
        )
        .await;
    });

    Ok(Json(ApiResponse::success(CoreBatchProveResponse {
        job_id: job_id.clone(),
        batch_id,
        estimated_time_ms,
        status_url: format!("/v2/batch/{}/status", job_id),
    })))
}

/// SSE endpoint for proof status updates
async fn status_sse_handler(
    State(state): State<SharedCoreApiState>,
    Path(job_id): Path<String>,
) -> Response {
    // Get the broadcast receiver for this job
    let rx = {
        let api_state = state.read().await;
        match api_state.jobs.get(&job_id) {
            Some(job) => job.status_tx.subscribe(),
            None => {
                // Job not found - check cache
                if let Some(cached) = api_state.cache.get_by_job(&job_id) {
                    // Return completed event as SSE
                    let event = ProofStatusEvent::Completed(cached.clone());
                    let stream = futures::stream::once(async move {
                        Ok::<_, Infallible>(
                            Event::default()
                                .event("completed")
                                .json_data(&event)
                                .unwrap(),
                        )
                    });
                    return Sse::new(stream)
                        .keep_alive(axum::response::sse::KeepAlive::default())
                        .into_response();
                }
                return (
                    StatusCode::NOT_FOUND,
                    Json(ApiResponse::<()>::error("Job not found")),
                )
                    .into_response();
            }
        }
    };

    // Convert broadcast receiver to SSE stream
    let stream = BroadcastStream::new(rx).filter_map(|result| {
        match result {
            Ok(event) => {
                let event_type = match &event {
                    ProofStatusEvent::Status(_) => "status",
                    ProofStatusEvent::Progress { .. } => "progress",
                    ProofStatusEvent::Completed(_) => "completed",
                    ProofStatusEvent::Failed { .. } => "failed",
                };
                Some(Ok::<_, Infallible>(
                    Event::default()
                        .event(event_type)
                        .json_data(&event)
                        .unwrap(),
                ))
            }
            Err(_) => None, // Channel lagged, skip
        }
    });

    Sse::new(stream)
        .keep_alive(
            axum::response::sse::KeepAlive::new()
                .interval(Duration::from_secs(15))
                .text("ping"),
        )
        .into_response()
}

/// Get completed proof
async fn get_proof_handler(
    State(state): State<SharedCoreApiState>,
    Path(job_id): Path<String>,
) -> Result<Json<ApiResponse<CoreProofResult>>, StatusCode> {
    let api_state = state.read().await;

    // Check cache
    if let Some(cached) = api_state.cache.get_by_job(&job_id) {
        return Ok(Json(ApiResponse::success(cached.clone())));
    }

    // Check active jobs
    if let Some(job) = api_state.jobs.get(&job_id) {
        match job.status.state {
            ProofJobState::Completed => {
                // Should be in cache, but check anyway
                return Ok(Json(ApiResponse::error_with_code(
                    "Proof completed but not in cache",
                    "INTERNAL_ERROR",
                )));
            }
            ProofJobState::Failed => {
                return Ok(Json(ApiResponse::error_with_code(
                    job.status
                        .error
                        .clone()
                        .unwrap_or("Unknown error".to_string()),
                    "PROOF_FAILED",
                )));
            }
            _ => {
                return Ok(Json(ApiResponse::error_with_code(
                    format!("Proof job still in progress: {:?}", job.status.state),
                    "NOT_READY",
                )));
            }
        }
    }

    Ok(Json(ApiResponse::error_with_code(
        "Job not found",
        "NOT_FOUND",
    )))
}

/// Cancel a proof job
async fn cancel_handler(
    State(state): State<SharedCoreApiState>,
    Path(job_id): Path<String>,
) -> Result<Json<ApiResponse<serde_json::Value>>, StatusCode> {
    let mut api_state = state.write().await;

    if let Some(job) = api_state.jobs.get_mut(&job_id) {
        // Can only cancel pending or preparing jobs
        if matches!(
            job.status.state,
            ProofJobState::Pending | ProofJobState::Preparing
        ) {
            job.status.state = ProofJobState::Cancelled;
            job.status.message = "Cancelled by user".to_string();
            job.status.updated_at = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();

            // Notify subscribers
            let _ = job.status_tx.send(ProofStatusEvent::Failed {
                job_id: job_id.clone(),
                error: "Cancelled by user".to_string(),
            });

            api_state.active_jobs = api_state.active_jobs.saturating_sub(1);

            return Ok(Json(ApiResponse::success(serde_json::json!({
                "job_id": job_id,
                "cancelled": true
            }))));
        } else {
            return Ok(Json(ApiResponse::error_with_code(
                format!("Cannot cancel job in state {:?}", job.status.state),
                "INVALID_STATE",
            )));
        }
    }

    Ok(Json(ApiResponse::error_with_code(
        "Job not found",
        "NOT_FOUND",
    )))
}

// Proof Execution

/// Execute the proof job (runs in background task)
async fn execute_proof_job(
    state: SharedCoreApiState,
    job_id: String,
    batch_id: u64,
    request: CoreBatchProveRequest,
    status_tx: broadcast::Sender<ProofStatusEvent>,
    mock_prover: bool,
    mock_delay: u64,
    circuit_path: PathBuf,
    cache_ttl: u64,
) {
    let start = std::time::Instant::now();

    // Update status: Preparing
    update_job_status(
        &state,
        &job_id,
        ProofJobState::Preparing,
        10,
        "Preparing witness",
    )
    .await;
    let _ = status_tx.send(ProofStatusEvent::Progress {
        job_id: job_id.clone(),
        progress_pct: 10,
        message: "Preparing witness".to_string(),
    });

    // Check if cancelled
    if is_job_cancelled(&state, &job_id).await {
        return;
    }

    // Convert request to Noir BatchInputs format
    let batch_inputs = convert_to_noir_inputs(&request);

    // Update status: Proving
    update_job_status(
        &state,
        &job_id,
        ProofJobState::Proving,
        30,
        "Generating proof",
    )
    .await;
    let _ = status_tx.send(ProofStatusEvent::Progress {
        job_id: job_id.clone(),
        progress_pct: 30,
        message: "Generating proof".to_string(),
    });

    // Execute proof
    let proof_result = if mock_prover {
        // Mock proving
        tokio::time::sleep(Duration::from_millis(mock_delay / 2)).await;

        // Send progress update
        let _ = status_tx.send(ProofStatusEvent::Progress {
            job_id: job_id.clone(),
            progress_pct: 60,
            message: "Proof computation in progress".to_string(),
        });

        tokio::time::sleep(Duration::from_millis(mock_delay / 2)).await;

        // Generate mock proof
        generate_mock_proof(&job_id, batch_id, &request)
    } else {
        // Real Noir proving using prover-worker
        info!(
            "Using real NoirProver with circuit_path: {:?}",
            circuit_path
        );

        // Send progress update for witness preparation
        let _ = status_tx.send(ProofStatusEvent::Progress {
            job_id: job_id.clone(),
            progress_pct: 40,
            message: "Executing nargo witness generation".to_string(),
        });

        let prover = prover_worker::NoirProver::new(circuit_path.clone());
        match prover.generate_batch_proof(batch_inputs).await {
            Ok(proof_result) => {
                // Send progress update for proof completion
                let _ = status_tx.send(ProofStatusEvent::Progress {
                    job_id: job_id.clone(),
                    progress_pct: 90,
                    message: "Proof generated, finalizing".to_string(),
                });

                // Extract batch_hash and withdrawal_root from public witness
                // The public witness contains 7 field elements after a 12-byte header:
                // [0-3]: count, [4-11]: padding, [12-43]: input 0, ..., [204-235]: input 6
                // Typically: pre_state, post_state, pre_shielded, post_shielded, withdrawal_root, batch_hash, batch_id
                let (batch_hash, withdrawal_root) =
                    extract_hashes_from_witness(&proof_result.public_witness_bytes);

                // Convert prover-worker result to CoreProofResult
                Ok(CoreProofResult {
                    job_id: job_id.clone(),
                    batch_id,
                    proof_bytes: hex::encode(&proof_result.proof_bytes),
                    public_witness_bytes: hex::encode(&proof_result.public_witness_bytes),
                    batch_hash,
                    withdrawal_root,
                    proving_time_ms: 0, // Will be set later
                })
            }
            Err(e) => {
                error!("NoirProver failed: {:?}", e);
                Err(format!("Noir proving failed: {}", e))
            }
        }
    };

    let proving_time_ms = start.elapsed().as_millis() as u64;

    match proof_result {
        Ok(mut result) => {
            result.proving_time_ms = proving_time_ms;

            // Cache the result
            {
                let mut api_state = state.write().await;
                api_state
                    .cache
                    .insert(job_id.clone(), batch_id, result.clone(), cache_ttl);
                api_state.active_jobs = api_state.active_jobs.saturating_sub(1);

                // Update job status
                if let Some(job) = api_state.jobs.get_mut(&job_id) {
                    job.status.state = ProofJobState::Completed;
                    job.status.progress_pct = 100;
                    job.status.message = "Proof completed".to_string();
                    job.status.completed_at = Some(
                        SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .unwrap()
                            .as_secs(),
                    );
                    job.status.updated_at = job.status.completed_at.unwrap();
                }
            }

            info!("Proof job {} completed in {}ms", job_id, proving_time_ms);

            // Send completion event
            let _ = status_tx.send(ProofStatusEvent::Completed(result));
        }
        Err(e) => {
            error!("Proof job {} failed: {}", job_id, e);

            {
                let mut api_state = state.write().await;
                api_state.active_jobs = api_state.active_jobs.saturating_sub(1);

                if let Some(job) = api_state.jobs.get_mut(&job_id) {
                    job.status.state = ProofJobState::Failed;
                    job.status.error = Some(e.to_string());
                    job.status.message = "Proof failed".to_string();
                    job.status.updated_at = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_secs();
                }
            }

            // Send failure event
            let _ = status_tx.send(ProofStatusEvent::Failed {
                job_id: job_id.clone(),
                error: e.to_string(),
            });
        }
    }
}

/// Update job status in state
async fn update_job_status(
    state: &SharedCoreApiState,
    job_id: &str,
    new_state: ProofJobState,
    progress: u8,
    message: &str,
) {
    let mut api_state = state.write().await;
    if let Some(job) = api_state.jobs.get_mut(job_id) {
        job.status.state = new_state;
        job.status.progress_pct = progress;
        job.status.message = message.to_string();
        job.status.updated_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Also broadcast status update
        let _ = job
            .status_tx
            .send(ProofStatusEvent::Status(job.status.clone()));
    }
}

/// Check if job is cancelled
async fn is_job_cancelled(state: &SharedCoreApiState, job_id: &str) -> bool {
    let api_state = state.read().await;
    if let Some(job) = api_state.jobs.get(job_id) {
        return job.status.state == ProofJobState::Cancelled;
    }
    false
}

/// Convert CoreBatchProveRequest to Noir BatchInputs
fn convert_to_noir_inputs(request: &CoreBatchProveRequest) -> prover_worker::BatchInputs {
    use prover_worker::{
        BatchInputs, Fr, MERKLE_DEPTH, MiMC, ShieldedData, ShieldedWitness, TransferData,
        TransferWitness, WithdrawalData, WithdrawalWitness, compute_batch_hash,
        compute_withdrawal_root, field_to_hex, hex_to_field,
    };

    // Create MiMC hasher for computing batch_hash and withdrawal_root
    let mimc = MiMC::new();
    let batch_id_fr = Fr::from(request.batch_id);

    // Convert transfer data for hashing
    let transfer_data: Vec<TransferData> = request
        .transfers
        .iter()
        .map(|tx| TransferData {
            sender_pubkey: hex_to_field(&tx.sender_pubkey).unwrap_or(Fr::from(0u64)),
            receiver_pubkey: hex_to_field(&tx.receiver_pubkey).unwrap_or(Fr::from(0u64)),
            amount: Fr::from(tx.amount),
            sender_nonce: Fr::from(tx.sender_nonce),
        })
        .collect();

    // Convert withdrawal data for hashing
    let withdrawal_data: Vec<WithdrawalData> = request
        .withdrawals
        .iter()
        .map(|wd| WithdrawalData {
            sender_pubkey: hex_to_field(&wd.sender_pubkey).unwrap_or(Fr::from(0u64)),
            l1_recipient: hex_to_field(&wd.l1_recipient).unwrap_or(Fr::from(0u64)),
            amount: Fr::from(wd.amount),
        })
        .collect();

    // Convert shielded data for hashing
    let shielded_data: Vec<ShieldedData> = request
        .shielded
        .iter()
        .map(|sh| ShieldedData {
            nullifier: hex_to_field(&sh.nullifier).unwrap_or(Fr::from(0u64)),
            output_commitment: hex_to_field(&sh.output_commitment).unwrap_or(Fr::from(0u64)),
        })
        .collect();

    // Compute batch_hash and withdrawal_root using MiMC (matching circuit)
    let batch_hash = compute_batch_hash(
        &mimc,
        batch_id_fr,
        &transfer_data,
        &withdrawal_data,
        &shielded_data,
    );
    let withdrawal_root = compute_withdrawal_root(&mimc, batch_id_fr, &withdrawal_data);

    // Convert to hex strings for BatchInputs
    let batch_hash_hex = field_to_hex(batch_hash);
    let withdrawal_root_hex = field_to_hex(withdrawal_root);

    let mut batch = BatchInputs::empty_batch(
        &request.pre_state_root,
        &request.pre_shielded_root,
        request.batch_id,
        &batch_hash_hex,
        &withdrawal_root_hex,
    );
    let withdrawal_root = compute_withdrawal_root(&mimc, batch_id_fr, &withdrawal_data);

    // Convert to hex strings for BatchInputs
    let batch_hash_hex = field_to_hex(batch_hash);
    let withdrawal_root_hex = field_to_hex(withdrawal_root);

    let mut batch = BatchInputs::empty_batch(
        &request.pre_state_root,
        &request.pre_shielded_root,
        request.batch_id,
        &batch_hash_hex,
        &withdrawal_root_hex,
    );

    batch.post_state_root = request.post_state_root.clone();
    batch.post_shielded_root = request.post_shielded_root.clone();

    // Convert transfers
    for (i, tx) in request.transfers.iter().take(8).enumerate() {
        batch.transfers[i] = TransferWitness {
            sender_pubkey: tx.sender_pubkey.clone(),
            sender_balance: tx.sender_balance.to_string(),
            sender_nonce: tx.sender_nonce.to_string(),
            sender_path: array_from_vec(&tx.sender_merkle_path, MERKLE_DEPTH),
            sender_path_indices: array_from_indices(&tx.sender_path_indices, MERKLE_DEPTH),
            receiver_pubkey: tx.receiver_pubkey.clone(),
            receiver_balance: tx.receiver_balance.to_string(),
            receiver_nonce: tx.receiver_nonce.to_string(),
            receiver_path: array_from_vec(&tx.receiver_merkle_path, MERKLE_DEPTH),
            receiver_path_indices: array_from_indices(&tx.receiver_path_indices, MERKLE_DEPTH),
            amount: tx.amount.to_string(),
            signature: tx.signature.clone(),
            is_valid: true,
        };
    }
    batch.num_transfers = request.transfers.len().min(8).to_string();

    // Convert withdrawals
    for (i, wd) in request.withdrawals.iter().take(4).enumerate() {
        batch.withdrawals[i] = WithdrawalWitness {
            sender_pubkey: wd.sender_pubkey.clone(),
            sender_balance: wd.sender_balance.to_string(),
            sender_nonce: wd.sender_nonce.to_string(),
            sender_path: array_from_vec(&wd.sender_merkle_path, MERKLE_DEPTH),
            sender_path_indices: array_from_indices(&wd.sender_path_indices, MERKLE_DEPTH),
            l1_recipient: wd.l1_recipient.clone(),
            amount: wd.amount.to_string(),
            signature: wd.signature.clone(),
            is_valid: true,
        };
    }
    batch.num_withdrawals = request.withdrawals.len().min(4).to_string();

    // Convert shielded (with skip_verification = true for pass-through mode)
    for (i, sh) in request.shielded.iter().take(4).enumerate() {
        batch.shielded[i] = ShieldedWitness {
            input_owner: sh.input_commitment.clone(), // Note: field mapping
            input_value: sh.input_value.to_string(),
            input_blinding: sh.input_blinding.clone(),
            input_position: sh.input_position.to_string(),
            input_path: array_from_vec(&sh.input_merkle_path, MERKLE_DEPTH),
            input_path_indices: array_from_indices(&sh.input_path_indices, MERKLE_DEPTH),
            spending_key: sh.spending_key.clone(),
            output_owner: sh.output_owner.clone(),
            output_value: sh.output_value.to_string(),
            output_blinding: sh.output_blinding.clone(),
            output_commitment: sh.output_commitment.clone(),
            nullifier: sh.nullifier.clone(),
            is_valid: true,
            skip_verification: true, // Pass-through mode: trust user's proof
        };
    }
    batch.num_shielded = request.shielded.len().min(4).to_string();

    batch
}

fn array_from_vec(v: &[String], size: usize) -> [String; 32] {
    let mut arr: [String; 32] = std::array::from_fn(|_| "0".to_string());
    for (i, s) in v.iter().take(size).enumerate() {
        arr[i] = s.clone();
    }
    arr
}

fn array_from_indices(v: &[u8], size: usize) -> [String; 32] {
    let mut arr: [String; 32] = std::array::from_fn(|_| "0".to_string());
    for (i, &b) in v.iter().take(size).enumerate() {
        arr[i] = b.to_string();
    }
    arr
}

/// Generate mock proof for testing
fn generate_mock_proof(
    job_id: &str,
    batch_id: u64,
    request: &CoreBatchProveRequest,
) -> Result<CoreProofResult, String> {
    use sha2::{Digest, Sha256};

    // Generate deterministic mock proof based on inputs
    let mut hasher = Sha256::new();
    hasher.update(request.pre_state_root.as_bytes());
    hasher.update(request.post_state_root.as_bytes());
    hasher.update(batch_id.to_le_bytes());
    let hash = hasher.finalize();

    // Create mock proof (388 bytes)
    // 388 = 12 * 32 + 4 = 384 + 4
    let mut proof_bytes = Vec::with_capacity(388);
    for _ in 0..13 {
        proof_bytes.extend_from_slice(&hash);
    }
    proof_bytes.truncate(388);

    // Create mock public witness (236 bytes for 7 inputs)
    // Header: count (4) + padding (8) + 7 * 32 bytes
    let mut pw_bytes = Vec::with_capacity(236);
    pw_bytes.extend_from_slice(&[0, 0, 0, 7]); // count = 7 (big-endian)
    pw_bytes.extend_from_slice(&[0; 8]); // padding
    for _ in 0..7 {
        pw_bytes.extend_from_slice(&hash);
    }
    pw_bytes.truncate(236);

    // Mock batch hash and withdrawal root
    let batch_hash = format!("0x{}", hex::encode(&hash));
    let withdrawal_root = format!("0x{}", hex::encode(&hash[..16]));

    Ok(CoreProofResult {
        job_id: job_id.to_string(),
        batch_id,
        proof_bytes: hex::encode(&proof_bytes),
        public_witness_bytes: hex::encode(&pw_bytes),
        batch_hash,
        withdrawal_root,
        proving_time_ms: 0, // Will be set by caller
    })
}

/// Extract batch_hash and withdrawal_root from public witness bytes
///
/// The public witness format (236 bytes):
/// - [0-3]: count (4 bytes, big-endian u32) = 7
/// - [4-11]: padding (8 bytes)
/// - [12-43]: public input 0 = pre_state_root (32 bytes)
/// - [44-75]: public input 1 = post_state_root (32 bytes)
/// - [76-107]: public input 2 = pre_shielded_root (32 bytes)
/// - [108-139]: public input 3 = post_shielded_root (32 bytes)
/// - [140-171]: public input 4 = withdrawal_root (32 bytes)
/// - [172-203]: public input 5 = batch_hash (32 bytes)
/// - [204-235]: public input 6 = batch_id (32 bytes, low bits only)
fn extract_hashes_from_witness(pw_bytes: &[u8]) -> (String, String) {
    if pw_bytes.len() < 204 {
        // Witness too short, return zeros
        return (
            "0x".to_string() + &"0".repeat(64),
            "0x".to_string() + &"0".repeat(64),
        );
    }

    // Extract withdrawal_root from bytes 140-171 (public input 4)
    let withdrawal_root = if pw_bytes.len() >= 172 {
        format!("0x{}", hex::encode(&pw_bytes[140..172]))
    } else {
        "0x".to_string() + &"0".repeat(64)
    };

    // Extract batch_hash from bytes 172-203 (public input 5)
    let batch_hash = if pw_bytes.len() >= 204 {
        format!("0x{}", hex::encode(&pw_bytes[172..204]))
    } else {
        "0x".to_string() + &"0".repeat(64)
    };

    (batch_hash, withdrawal_root)
}

// Tests

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_proof_cache() {
        let mut cache = ProofCache::new();

        let result = CoreProofResult {
            job_id: "job1".to_string(),
            batch_id: 1,
            proof_bytes: "abcd".to_string(),
            public_witness_bytes: "1234".to_string(),
            batch_hash: "0x0".to_string(),
            withdrawal_root: "0x0".to_string(),
            proving_time_ms: 100,
        };

        cache.insert("job1".to_string(), 1, result.clone(), 3600);

        assert!(cache.get_by_job("job1").is_some());
        assert!(cache.get_by_batch(1).is_some());
        assert!(cache.get_by_job("job2").is_none());
        assert!(cache.get_by_batch(2).is_none());
    }

    #[test]
    fn test_mock_proof_generation() {
        let request = CoreBatchProveRequest {
            batch_id: 1,
            pre_state_root: "0x1234".to_string(),
            post_state_root: "0x5678".to_string(),
            pre_shielded_root: "0xaaaa".to_string(),
            post_shielded_root: "0xbbbb".to_string(),
            transfers: vec![],
            withdrawals: vec![],
            shielded: vec![],
        };

        let result = generate_mock_proof("job1", 1, &request).unwrap();

        // Proof should be 388 bytes (776 hex chars)
        assert_eq!(result.proof_bytes.len(), 388 * 2);
        // Public witness should be 236 bytes (472 hex chars)
        assert_eq!(result.public_witness_bytes.len(), 236 * 2);
    }
}
