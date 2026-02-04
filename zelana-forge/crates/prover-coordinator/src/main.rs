//! # Zelana Forge - Distributed Prover Coordinator
//!
//! The "Brain" of the Parallel Swarm Architecture.
//!
//! ## Architecture
//!
//! ```text
//!                    ┌─────────────────────────────────────────────┐
//!                    │           COORDINATOR (Brain)               │
//!                    │                                             │
//!   Batch ──────────►│  1. Slice batch into chunks                 │
//!   (100 txs)        │  2. Compute intermediate state roots        │
//!                    │  3. Dispatch chunks to workers in parallel  │
//!                    │  4. Collect proofs                          │
//!                    │  5. Submit to Solana (batched)              │
//!                    └─────────────────────────────────────────────┘
//!                              │         │         │         │
//!                              ▼         ▼         ▼         ▼
//!                         ┌────────┐ ┌────────┐ ┌────────┐ ┌────────┐
//!                         │Worker 1│ │Worker 2│ │Worker 3│ │Worker 4│
//!                         │ Chunk 0│ │ Chunk 1│ │ Chunk 2│ │ Chunk 3│
//!                         └────────┘ └────────┘ └────────┘ └────────┘
//!                              │         │         │         │
//!                              ▼         ▼         ▼         ▼
//!                         ┌─────────────────────────────────────────┐
//!                         │              SOLANA (Verifier)          │
//!                         │    Batched verification of 4 proofs    │
//!                         └─────────────────────────────────────────┘
//! ```
//!
//! ## Endpoints
//!
//! ### Parallel Swarm (New)
//! - `POST /batch/submit` - Submit a batch for parallel proving
//! - `GET /batch/:id/status` - Check batch status
//! - `GET /workers` - List available workers and their status
//!
//! ### Legacy (Threshold Schnorr)
//! - `GET /health` - Health check
//! - `POST /setup` - Initialize with witness commitment
//! - `POST /prove` - Generate distributed Schnorr proof
//! - `POST /verify` - Verify proof with witness reveal

mod core_api;
mod dispatcher;
mod ownership_api;
mod settler;
mod solana_client;

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
};
use clap::Parser;
use core_api::{CoreApiConfig, CoreApiState, SharedCoreApiState, core_api_router};
use dispatcher::{Batch, BatchProofs, Dispatcher, DispatcherConfig};
use ownership_api::{
    OwnershipProverConfig, OwnershipProverState, SharedOwnershipState, ownership_api_router,
};
use serde::{Deserialize, Serialize};
use settler::{BatchSettlement, MockSettler, SettlementMode, Settler, SettlerConfig};
use std::{collections::HashMap, sync::Arc, time::Instant};
use tokio::sync::RwLock;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing::{error, info, warn};

// ============================================================================
// CLI Arguments
// ============================================================================

#[derive(Parser, Debug, Clone)]
#[command(name = "prover-coordinator")]
#[command(about = "Parallel Swarm Coordinator - The Brain", long_about = None)]
struct Args {
    /// Port to listen on
    #[arg(long, default_value = "8080", env = "PORT")]
    port: u16,

    /// Host to bind to
    #[arg(long, default_value = "0.0.0.0", env = "HOST")]
    host: String,

    /// Comma-separated list of worker URLs
    #[arg(
        long,
        value_delimiter = ',',
        default_value = "http://localhost:9001,http://localhost:9002,http://localhost:9003,http://localhost:9004",
        env = "WORKERS"
    )]
    workers: Vec<String>,

    /// Transactions per chunk
    #[arg(long, default_value = "25", env = "CHUNK_SIZE")]
    chunk_size: usize,

    /// Proof timeout in milliseconds
    #[arg(long, default_value = "300000", env = "PROOF_TIMEOUT_MS")]
    proof_timeout_ms: u64,

    /// Solana RPC URL
    #[arg(
        long,
        default_value = "https://api.devnet.solana.com",
        env = "SOLANA_RPC"
    )]
    solana_rpc: String,

    /// Verifier program ID (deployed zelana_batch verifier)
    #[arg(
        long,
        default_value = "EZzyLrTrC4uyU488jVAs4GKeCR1s9GmoFggeiDqwDeNK",
        env = "PROGRAM_ID"
    )]
    program_id: String,

    /// Use mock settlement (for demo without Solana)
    #[arg(long, default_value = "true", env = "MOCK_SETTLEMENT", action = clap::ArgAction::Set)]
    mock_settlement: bool,

    /// Path to keypair file for Solana transactions
    #[arg(long, env = "KEYPAIR_PATH")]
    keypair_path: Option<String>,

    /// Path to circuit target directory (for proof files)
    #[arg(long, env = "CIRCUIT_TARGET_PATH")]
    circuit_target_path: Option<String>,

    /// Compute units to request for verification transactions
    #[arg(long, default_value = "500000", env = "COMPUTE_UNITS")]
    compute_units: u32,

    // === Core API Configuration ===
    /// Enable Core API endpoints (/v2/batch/prove etc.)
    #[arg(long, default_value = "true", env = "ENABLE_CORE_API", action = clap::ArgAction::Set)]
    enable_core_api: bool,

    /// Use mock prover for Core API (for testing without nargo/sunspot)
    #[arg(long, default_value = "false", env = "MOCK_PROVER", action = clap::ArgAction::Set)]
    mock_prover: bool,

    /// Mock prover delay in milliseconds
    #[arg(long, default_value = "1000", env = "MOCK_PROVER_DELAY_MS")]
    mock_prover_delay_ms: u64,

    /// Proof cache TTL in seconds
    #[arg(long, default_value = "3600", env = "PROOF_CACHE_TTL_SECS")]
    proof_cache_ttl_secs: u64,

    /// Maximum concurrent proving jobs
    #[arg(long, default_value = "4", env = "MAX_CONCURRENT_JOBS")]
    max_concurrent_jobs: usize,

    // === Ownership Prover Configuration ===
    /// Path to ownership circuit directory
    #[arg(long, env = "OWNERSHIP_CIRCUIT_PATH")]
    ownership_circuit_path: Option<String>,

    /// Use mock ownership prover (for testing without nargo/sunspot)
    #[arg(long, default_value = "false", env = "MOCK_OWNERSHIP_PROVER", action = clap::ArgAction::Set)]
    mock_ownership_prover: bool,

    /// Core API only mode - disables parallel swarm worker health checks
    /// Use this when running coordinator purely for Core API (sequencer integration)
    #[arg(long, default_value = "false", env = "CORE_API_ONLY", action = clap::ArgAction::Set)]
    core_api_only: bool,
}

// ============================================================================
// State
// ============================================================================

/// Batch processing status
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BatchStatus {
    pub batch_id: String,
    pub state: BatchState,
    pub chunks_total: usize,
    pub chunks_proved: usize,
    pub submitted_at: u64,
    pub proving_started_at: Option<u64>,
    pub proving_completed_at: Option<u64>,
    pub settled_at: Option<u64>,
    pub proofs: Option<BatchProofs>,
    pub settlement: Option<BatchSettlement>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum BatchState {
    Pending,
    Slicing,
    Proving,
    Settling,
    Completed,
    Failed,
}

/// Worker status
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorkerStatus {
    pub url: String,
    pub worker_id: Option<u32>,
    pub ready: bool,
    pub active_jobs: usize,
    pub total_proofs: u64,
    pub avg_proving_time_ms: u64,
    pub last_health_check: u64,
}

/// Coordinator state
#[derive(Clone)]
struct CoordinatorState {
    config: Args,
    batches: HashMap<String, BatchStatus>,
    workers: HashMap<String, WorkerStatus>,
    client: reqwest::Client,
}

type SharedState = Arc<RwLock<CoordinatorState>>;

// ============================================================================
// API Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "lowercase")]
pub enum ApiResponse<T> {
    Success { data: T },
    Error { message: String },
}

impl<T> ApiResponse<T> {
    pub fn success(data: T) -> Self {
        ApiResponse::Success { data }
    }

    pub fn error(message: impl Into<String>) -> Self {
        ApiResponse::Error {
            message: message.into(),
        }
    }
}

/// Health response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub workers_available: usize,
    pub workers_ready: usize,
    pub pending_batches: usize,
    pub total_batches_processed: usize,
}

/// Batch submit request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchSubmitRequest {
    pub batch: Batch,
}

/// Batch submit response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchSubmitResponse {
    pub batch_id: String,
    pub chunks: usize,
    pub workers_assigned: usize,
    pub status: BatchState,
}

/// Workers list response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkersResponse {
    pub workers: Vec<WorkerStatus>,
    pub total: usize,
    pub ready: usize,
}

// ============================================================================
// Main
// ============================================================================

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "prover_coordinator=debug,tower_http=debug".into()),
        )
        .init();

    let args = Args::parse();

    info!(
        "Starting Parallel Swarm Coordinator on {}:{}",
        args.host, args.port
    );
    info!("Core API only mode: {}", args.core_api_only);
    if !args.core_api_only {
        info!("Workers: {:?}", args.workers);
        info!("Chunk size: {} txs", args.chunk_size);
    }
    info!("Mock settlement: {}", args.mock_settlement);
    info!("Core API enabled: {}", args.enable_core_api);
    info!("Mock prover: {}", args.mock_prover);

    // Initialize state (workers only used in swarm mode)
    let workers: HashMap<String, WorkerStatus> = if args.core_api_only {
        HashMap::new() // No workers in core-api-only mode
    } else {
        args.workers
            .iter()
            .map(|url| {
                (
                    url.clone(),
                    WorkerStatus {
                        url: url.clone(),
                        worker_id: None,
                        ready: false,
                        active_jobs: 0,
                        total_proofs: 0,
                        avg_proving_time_ms: 0,
                        last_health_check: 0,
                    },
                )
            })
            .collect()
    };

    let state = Arc::new(RwLock::new(CoordinatorState {
        config: args.clone(),
        batches: HashMap::new(),
        workers,
        client: reqwest::Client::new(),
    }));

    // Spawn background task to check worker health (only in swarm mode)
    if !args.core_api_only {
        let health_state = state.clone();
        tokio::spawn(async move {
            loop {
                check_worker_health(health_state.clone()).await;
                tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
            }
        });
    }

    // Build router with CORS for dashboard
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    // Base router for Parallel Swarm endpoints
    let swarm_router = Router::new()
        // Health
        .route("/health", get(health_handler))
        // Parallel Swarm endpoints
        .route("/batch/submit", post(batch_submit_handler))
        .route("/batch/:batch_id/status", get(batch_status_handler))
        .route("/workers", get(workers_handler))
        .with_state(state);

    // Create the final app, optionally merging Core API
    let app = if args.enable_core_api {
        // Create Core API state
        // Canonicalize to absolute path to avoid working directory issues with nargo/sunspot
        let circuit_path = args
            .circuit_target_path
            .as_ref()
            .map(|p| std::path::PathBuf::from(p))
            .unwrap_or_else(|| std::path::PathBuf::from("circuits/zelana_batch"));
        let circuit_path = circuit_path.canonicalize().unwrap_or_else(|_| {
            // If canonicalize fails (path doesn't exist yet), make it absolute relative to cwd
            std::env::current_dir()
                .map(|cwd| cwd.join(&circuit_path))
                .unwrap_or(circuit_path)
        });
        info!("Circuit path (absolute): {:?}", circuit_path);

        let core_api_config = CoreApiConfig {
            circuit_path,
            mock_prover: args.mock_prover,
            mock_delay_ms: args.mock_prover_delay_ms,
            cache_ttl_secs: args.proof_cache_ttl_secs,
            max_concurrent_jobs: args.max_concurrent_jobs,
        };

        let core_api_state: SharedCoreApiState =
            Arc::new(tokio::sync::RwLock::new(CoreApiState::new(core_api_config)));

        info!(
            "Core API enabled with {} max concurrent jobs, cache TTL {}s",
            args.max_concurrent_jobs, args.proof_cache_ttl_secs
        );

        // Create Ownership API state
        // Canonicalize to absolute path to avoid working directory issues with nargo/sunspot
        let ownership_circuit_path = args
            .ownership_circuit_path
            .as_ref()
            .map(|p| std::path::PathBuf::from(p))
            .unwrap_or_else(|| std::path::PathBuf::from("circuits/ownership"));
        let ownership_circuit_path = ownership_circuit_path.canonicalize().unwrap_or_else(|_| {
            std::env::current_dir()
                .map(|cwd| cwd.join(&ownership_circuit_path))
                .unwrap_or(ownership_circuit_path)
        });
        info!(
            "Ownership circuit path (absolute): {:?}",
            ownership_circuit_path
        );

        let ownership_config = OwnershipProverConfig {
            circuit_path: ownership_circuit_path.clone(),
            mock_prover: args.mock_ownership_prover,
            mock_delay_ms: 100,
        };

        let ownership_state: SharedOwnershipState = Arc::new(tokio::sync::RwLock::new(
            OwnershipProverState::new(ownership_config),
        ));

        info!(
            "Ownership API enabled with circuit path: {:?}, mock: {}",
            ownership_circuit_path, args.mock_ownership_prover
        );

        // Merge routers
        swarm_router
            .merge(core_api_router(core_api_state))
            .merge(ownership_api_router(ownership_state))
            .layer(cors)
            .layer(TraceLayer::new_for_http())
    } else {
        swarm_router.layer(cors).layer(TraceLayer::new_for_http())
    };

    // Start server
    let addr = format!("{}:{}", args.host, args.port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;

    info!("Coordinator listening on {}", addr);

    axum::serve(listener, app).await?;

    Ok(())
}

// ============================================================================
// Handlers
// ============================================================================

/// Health check
async fn health_handler(State(state): State<SharedState>) -> Json<ApiResponse<HealthResponse>> {
    let coord_state = state.read().await;

    let workers_ready = coord_state.workers.values().filter(|w| w.ready).count();
    let pending_batches = coord_state
        .batches
        .values()
        .filter(|b| b.state != BatchState::Completed && b.state != BatchState::Failed)
        .count();
    let total_processed = coord_state
        .batches
        .values()
        .filter(|b| b.state == BatchState::Completed)
        .count();

    Json(ApiResponse::success(HealthResponse {
        status: "ok".to_string(),
        workers_available: coord_state.workers.len(),
        workers_ready,
        pending_batches,
        total_batches_processed: total_processed,
    }))
}

/// Submit a batch for parallel proving
async fn batch_submit_handler(
    State(state): State<SharedState>,
    Json(request): Json<BatchSubmitRequest>,
) -> Result<Json<ApiResponse<BatchSubmitResponse>>, StatusCode> {
    let batch = request.batch;
    let batch_id = batch.batch_id.clone();

    info!(
        "Received batch {} with {} transactions",
        batch_id,
        batch.transactions.len()
    );

    // Get config
    let (config, workers, client) = {
        let coord_state = state.read().await;
        let ready_workers: Vec<String> = coord_state
            .workers
            .values()
            .filter(|w| w.ready)
            .map(|w| w.url.clone())
            .collect();

        if ready_workers.is_empty() {
            return Ok(Json(ApiResponse::error("No workers available")));
        }

        (
            coord_state.config.clone(),
            ready_workers,
            coord_state.client.clone(),
        )
    };

    // Calculate chunks
    let num_chunks = (batch.transactions.len() + config.chunk_size - 1) / config.chunk_size;

    // Create batch status
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let status = BatchStatus {
        batch_id: batch_id.clone(),
        state: BatchState::Pending,
        chunks_total: num_chunks,
        chunks_proved: 0,
        submitted_at: now,
        proving_started_at: None,
        proving_completed_at: None,
        settled_at: None,
        proofs: None,
        settlement: None,
        error: None,
    };

    // Store batch status
    {
        let mut coord_state = state.write().await;
        coord_state.batches.insert(batch_id.clone(), status);
    }

    // Calculate workers assigned before moving
    let workers_assigned = std::cmp::min(num_chunks, workers.len());

    // Spawn async task to process the batch
    let state_clone = state.clone();
    let batch_clone = batch.clone();
    tokio::spawn(async move {
        process_batch(state_clone, batch_clone, config, workers, client).await;
    });

    Ok(Json(ApiResponse::success(BatchSubmitResponse {
        batch_id,
        chunks: num_chunks,
        workers_assigned,
        status: BatchState::Pending,
    })))
}

/// Get batch status
async fn batch_status_handler(
    State(state): State<SharedState>,
    Path(batch_id): Path<String>,
) -> Json<ApiResponse<BatchStatus>> {
    let coord_state = state.read().await;

    match coord_state.batches.get(&batch_id) {
        Some(status) => Json(ApiResponse::success(status.clone())),
        None => Json(ApiResponse::error(format!("Batch {} not found", batch_id))),
    }
}

/// Get workers status
async fn workers_handler(State(state): State<SharedState>) -> Json<ApiResponse<WorkersResponse>> {
    let coord_state = state.read().await;

    let workers: Vec<WorkerStatus> = coord_state.workers.values().cloned().collect();
    let ready = workers.iter().filter(|w| w.ready).count();

    Json(ApiResponse::success(WorkersResponse {
        total: workers.len(),
        ready,
        workers,
    }))
}

// ============================================================================
// Background Tasks
// ============================================================================

/// Process a batch: slice, dispatch, collect, settle
async fn process_batch(
    state: SharedState,
    batch: Batch,
    config: Args,
    workers: Vec<String>,
    client: reqwest::Client,
) {
    let batch_id = batch.batch_id.clone();
    let start = Instant::now();

    // Update state: Slicing
    {
        let mut coord_state = state.write().await;
        if let Some(status) = coord_state.batches.get_mut(&batch_id) {
            status.state = BatchState::Slicing;
        }
    }

    info!("Processing batch {}: slicing into chunks", batch_id);

    // Create dispatcher
    let dispatcher = Dispatcher::new(DispatcherConfig {
        worker_urls: workers.clone(),
        chunk_size: config.chunk_size,
        client: client.clone(),
        proof_timeout_ms: config.proof_timeout_ms,
    });

    // Update state: Proving
    {
        let mut coord_state = state.write().await;
        if let Some(status) = coord_state.batches.get_mut(&batch_id) {
            status.state = BatchState::Proving;
            status.proving_started_at = Some(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
            );
        }
    }

    // Dispatch batch and collect proofs
    match dispatcher.dispatch_batch(&batch, config.chunk_size).await {
        Ok(proofs) => {
            info!(
                "Batch {} proved: {} chunks in {}ms",
                batch_id,
                proofs.proofs.len(),
                proofs.total_time_ms
            );

            // Update state with proofs
            {
                let mut coord_state = state.write().await;
                if let Some(status) = coord_state.batches.get_mut(&batch_id) {
                    status.chunks_proved = proofs.proofs.len();
                    status.proofs = Some(proofs.clone());
                    status.proving_completed_at = Some(
                        std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap()
                            .as_secs(),
                    );
                    status.state = BatchState::Settling;
                }
            }

            // Settle on Solana
            let settlement_result = if config.mock_settlement {
                let settler = MockSettler::new(100);
                settler.settle_batch(&proofs).await
            } else {
                let mut settler = Settler::new(
                    SettlerConfig {
                        rpc_url: config.solana_rpc.clone(),
                        program_id: config.program_id.clone(),
                        keypair_path: config.keypair_path.clone(),
                        circuit_target_path: config
                            .circuit_target_path
                            .as_ref()
                            .map(|p| std::path::PathBuf::from(p)),
                        compute_units: config.compute_units,
                    },
                    SettlementMode::Batched,
                );
                settler.settle_batch(&proofs).await
            };

            match settlement_result {
                Ok(settlement) => {
                    info!(
                        "Batch {} settled: {}ms, tx: {:?}",
                        batch_id, settlement.settlement_time_ms, settlement.batched_tx_signature
                    );

                    let mut coord_state = state.write().await;
                    if let Some(status) = coord_state.batches.get_mut(&batch_id) {
                        status.settlement = Some(settlement);
                        status.state = BatchState::Completed;
                        status.settled_at = Some(
                            std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap()
                                .as_secs(),
                        );
                    }
                }
                Err(e) => {
                    error!("Batch {} settlement failed: {}", batch_id, e);
                    let mut coord_state = state.write().await;
                    if let Some(status) = coord_state.batches.get_mut(&batch_id) {
                        status.state = BatchState::Failed;
                        status.error = Some(format!("Settlement failed: {}", e));
                    }
                }
            }
        }
        Err(e) => {
            error!("Batch {} proving failed: {}", batch_id, e);
            let mut coord_state = state.write().await;
            if let Some(status) = coord_state.batches.get_mut(&batch_id) {
                status.state = BatchState::Failed;
                status.error = Some(format!("Proving failed: {}", e));
            }
        }
    }

    let total_time = start.elapsed();
    info!("Batch {} processing complete in {:?}", batch_id, total_time);
}

/// Check health of all workers
async fn check_worker_health(state: SharedState) {
    let (workers, client) = {
        let coord_state = state.read().await;
        (
            coord_state.workers.keys().cloned().collect::<Vec<_>>(),
            coord_state.client.clone(),
        )
    };

    for worker_url in workers {
        let health_url = format!("{}/health", worker_url);

        match client
            .get(&health_url)
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .await
        {
            Ok(response) => {
                if response.status().is_success() {
                    if let Ok(health) = response.json::<ApiResponse<WorkerHealthResponse>>().await {
                        if let ApiResponse::Success { data } = health {
                            let mut coord_state = state.write().await;
                            if let Some(worker) = coord_state.workers.get_mut(&worker_url) {
                                worker.ready = data.ready;
                                worker.worker_id = Some(data.worker_id);
                                worker.active_jobs = data.active_jobs;
                                worker.total_proofs = data.total_proofs;
                                worker.avg_proving_time_ms = data.avg_proving_time_ms;
                                worker.last_health_check = std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .unwrap()
                                    .as_secs();
                            }
                        }
                    }
                } else {
                    let mut coord_state = state.write().await;
                    if let Some(worker) = coord_state.workers.get_mut(&worker_url) {
                        worker.ready = false;
                    }
                }
            }
            Err(e) => {
                warn!("Worker {} health check failed: {}", worker_url, e);
                let mut coord_state = state.write().await;
                if let Some(worker) = coord_state.workers.get_mut(&worker_url) {
                    worker.ready = false;
                }
            }
        }
    }
}

/// Worker health response (matches prover-worker)
#[derive(Debug, Clone, Serialize, Deserialize)]
struct WorkerHealthResponse {
    status: String,
    worker_id: u32,
    ready: bool,
    active_jobs: usize,
    max_concurrent_jobs: usize,
    total_proofs: u64,
    avg_proving_time_ms: u64,
}
