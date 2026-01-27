//! # Prover Worker
//!
//! A distributed prover worker that executes Noir circuits via nargo/sunspot
//! and returns proofs. Part of the Parallel Swarm Architecture.
//!
//! ## Endpoints
//!
//! - `GET /health` - Health check and worker status
//! - `POST /prove` - Execute Noir circuit and return proof
//! - `GET /status/:job_id` - Check job status (for async proving)
//!
//! ## Architecture
//!
//! ```text
//! Coordinator (Brain)
//!       │
//!       ├──► Worker 1 ──► POST /prove ──► nargo execute ──► sunspot prove ──► Proof
//!       ├──► Worker 2 ──► POST /prove ──► nargo execute ──► sunspot prove ──► Proof
//!       ├──► Worker 3 ──► POST /prove ──► nargo execute ──► sunspot prove ──► Proof
//!       └──► Worker 4 ──► POST /prove ──► nargo execute ──► sunspot prove ──► Proof
//! ```

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
};
use clap::Parser;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::PathBuf, sync::Arc, time::Instant};
use tokio::sync::RwLock;
use tower_http::trace::TraceLayer;
use tracing::{error, info, warn};

mod mimc;
mod prover;
use prover::{ChunkInputs, MockProver, NoirProver};

/// Command-line arguments
#[derive(Parser, Debug, Clone)]
#[command(name = "prover-worker")]
#[command(about = "Distributed prover worker for Parallel Swarm Architecture", long_about = None)]
pub struct Args {
    /// Worker ID (unique identifier)
    #[arg(long, env = "WORKER_ID", default_value = "1")]
    pub worker_id: u32,

    /// Port to listen on
    #[arg(long, default_value = "3001", env = "PORT")]
    pub port: u16,

    /// Host to bind to
    #[arg(long, default_value = "0.0.0.0", env = "HOST")]
    pub host: String,

    /// Path to the Noir circuit directory
    #[arg(
        long,
        env = "CIRCUIT_PATH",
        default_value = "../../circuits/batch_processor"
    )]
    pub circuit_path: PathBuf,

    /// Maximum concurrent proving jobs
    #[arg(long, default_value = "2", env = "MAX_CONCURRENT_JOBS")]
    pub max_concurrent_jobs: usize,

    /// Use mock prover (for demo without nargo/sunspot)
    #[arg(long, default_value = "false", env = "MOCK_PROVER")]
    pub mock_prover: bool,

    /// Mock prover delay in milliseconds (simulates proving time)
    #[arg(long, default_value = "500", env = "MOCK_DELAY_MS")]
    pub mock_delay_ms: u64,
}

/// Worker state
#[derive(Clone)]
pub struct WorkerState {
    /// Worker configuration
    pub config: Args,

    /// Active jobs (job_id -> JobStatus)
    pub jobs: HashMap<String, JobStatus>,

    /// Number of currently running jobs
    pub active_job_count: usize,

    /// Total proofs generated
    pub total_proofs: u64,

    /// Average proving time (ms)
    pub avg_proving_time_ms: u64,
}

/// Job status
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JobStatus {
    pub job_id: String,
    pub status: JobState,
    pub chunk_id: u32,
    pub started_at: u64,
    pub completed_at: Option<u64>,
    pub proof: Option<String>,
    pub error: Option<String>,
}

/// Job state enum
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum JobState {
    Pending,
    Proving,
    Completed,
    Failed,
}

type SharedState = Arc<RwLock<WorkerState>>;

// ============================================================================
// Request/Response Types
// ============================================================================

/// Prove request from coordinator
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProveRequest {
    /// Unique chunk ID (for tracking)
    pub chunk_id: u32,

    /// Pre-state root (32 bytes hex)
    pub pre_root: String,

    /// Post-state root (32 bytes hex)  
    pub post_root: String,

    /// Transactions in this chunk
    pub transactions: Vec<ChunkTransaction>,
}

/// Transaction within a chunk
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkTransaction {
    pub sender_pubkey: String,
    pub receiver_pubkey: String,
    pub amount: u64,
    pub signature: String,
    pub merkle_path: Vec<String>,
}

/// Prove response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProveResponse {
    /// Job ID for tracking
    pub job_id: String,

    /// Chunk ID (echoed back)
    pub chunk_id: u32,

    /// Worker ID
    pub worker_id: u32,

    /// Proof bytes (hex encoded)
    pub proof: String,

    /// Public inputs used
    pub public_inputs: Vec<String>,

    /// Proving time in milliseconds
    pub proving_time_ms: u64,
}

/// Health response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub worker_id: u32,
    pub ready: bool,
    pub active_jobs: usize,
    pub max_concurrent_jobs: usize,
    pub total_proofs: u64,
    pub avg_proving_time_ms: u64,
}

/// API response wrapper
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

// ============================================================================
// Main
// ============================================================================

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "prover_worker=debug,tower_http=debug".into()),
        )
        .init();

    let args = Args::parse();

    info!(
        "Starting prover worker {} on {}:{} (circuit: {:?})",
        args.worker_id, args.host, args.port, args.circuit_path
    );

    // Verify circuit path exists
    if !args.circuit_path.exists() {
        warn!(
            "Circuit path {:?} does not exist - proofs will fail until configured",
            args.circuit_path
        );
    }

    // Initialize state
    let state = Arc::new(RwLock::new(WorkerState {
        config: args.clone(),
        jobs: HashMap::new(),
        active_job_count: 0,
        total_proofs: 0,
        avg_proving_time_ms: 0,
    }));

    // Build router
    let app = Router::new()
        .route("/health", get(health_handler))
        .route("/prove", post(prove_handler))
        .route("/status/:job_id", get(status_handler))
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    // Start server
    let addr = format!("{}:{}", args.host, args.port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;

    info!("Worker {} listening on {}", args.worker_id, addr);

    axum::serve(listener, app).await?;

    Ok(())
}

// ============================================================================
// Handlers
// ============================================================================

/// Health check handler
async fn health_handler(State(state): State<SharedState>) -> Json<ApiResponse<HealthResponse>> {
    let worker_state = state.read().await;

    let ready = worker_state.active_job_count < worker_state.config.max_concurrent_jobs;

    Json(ApiResponse::success(HealthResponse {
        status: "ok".to_string(),
        worker_id: worker_state.config.worker_id,
        ready,
        active_jobs: worker_state.active_job_count,
        max_concurrent_jobs: worker_state.config.max_concurrent_jobs,
        total_proofs: worker_state.total_proofs,
        avg_proving_time_ms: worker_state.avg_proving_time_ms,
    }))
}

/// Prove handler - executes Noir circuit and returns proof
async fn prove_handler(
    State(state): State<SharedState>,
    Json(request): Json<ProveRequest>,
) -> Result<Json<ApiResponse<ProveResponse>>, StatusCode> {
    let start = Instant::now();

    // Check capacity
    {
        let worker_state = state.read().await;
        if worker_state.active_job_count >= worker_state.config.max_concurrent_jobs {
            return Ok(Json(ApiResponse::error(format!(
                "Worker {} at capacity ({}/{})",
                worker_state.config.worker_id,
                worker_state.active_job_count,
                worker_state.config.max_concurrent_jobs
            ))));
        }
    }

    // Generate job ID
    let job_id = uuid::Uuid::new_v4().to_string();

    // Get worker config
    let (worker_id, circuit_path, use_mock, mock_delay) = {
        let worker_state = state.read().await;
        (
            worker_state.config.worker_id,
            worker_state.config.circuit_path.clone(),
            worker_state.config.mock_prover,
            worker_state.config.mock_delay_ms,
        )
    };

    info!(
        "Worker {} received prove request for chunk {} (job: {})",
        worker_id, request.chunk_id, job_id
    );

    // Increment active job count
    {
        let mut worker_state = state.write().await;
        worker_state.active_job_count += 1;
        worker_state.jobs.insert(
            job_id.clone(),
            JobStatus {
                job_id: job_id.clone(),
                status: JobState::Proving,
                chunk_id: request.chunk_id,
                started_at: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
                completed_at: None,
                proof: None,
                error: None,
            },
        );
    }

    // Convert request to circuit inputs
    let inputs = ChunkInputs {
        old_root: request.pre_root.clone(),
        new_root: request.post_root.clone(),
        sender_pubkeys: request
            .transactions
            .iter()
            .map(|tx| tx.sender_pubkey.clone())
            .collect(),
        receiver_pubkeys: request
            .transactions
            .iter()
            .map(|tx| tx.receiver_pubkey.clone())
            .collect(),
        amounts: request.transactions.iter().map(|tx| tx.amount).collect(),
        signatures: request
            .transactions
            .iter()
            .map(|tx| tx.signature.clone())
            .collect(),
        merkle_paths: request
            .transactions
            .iter()
            .map(|tx| tx.merkle_path.clone())
            .collect(),
    };

    // Execute proof generation (mock or real)
    let result = if use_mock {
        info!("Using mock prover (delay: {}ms)", mock_delay);
        let prover = MockProver::new(mock_delay);
        prover.generate_proof(inputs).await
    } else {
        let prover = NoirProver::new(circuit_path);
        prover.generate_proof(inputs).await
    };

    let proving_time_ms = start.elapsed().as_millis() as u64;

    // Update state based on result
    match result {
        Ok(proof_result) => {
            let mut worker_state = state.write().await;
            worker_state.active_job_count = worker_state.active_job_count.saturating_sub(1);
            worker_state.total_proofs += 1;

            // Update average proving time
            let total = worker_state.total_proofs;
            worker_state.avg_proving_time_ms =
                ((worker_state.avg_proving_time_ms * (total - 1)) + proving_time_ms) / total;

            // Update job status
            if let Some(job) = worker_state.jobs.get_mut(&job_id) {
                job.status = JobState::Completed;
                job.completed_at = Some(
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_secs(),
                );
                job.proof = Some(proof_result.proof.clone());
            }

            info!(
                "Worker {} completed chunk {} proof in {}ms (job: {})",
                worker_id, request.chunk_id, proving_time_ms, job_id
            );

            Ok(Json(ApiResponse::success(ProveResponse {
                job_id,
                chunk_id: request.chunk_id,
                worker_id,
                proof: proof_result.proof,
                public_inputs: proof_result.public_inputs,
                proving_time_ms,
            })))
        }
        Err(e) => {
            let mut worker_state = state.write().await;
            worker_state.active_job_count = worker_state.active_job_count.saturating_sub(1);

            // Update job status
            if let Some(job) = worker_state.jobs.get_mut(&job_id) {
                job.status = JobState::Failed;
                job.completed_at = Some(
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_secs(),
                );
                job.error = Some(e.to_string());
            }

            error!(
                "Worker {} failed chunk {} proof: {} (job: {})",
                worker_id, request.chunk_id, e, job_id
            );

            Ok(Json(ApiResponse::error(format!(
                "Proof generation failed: {}",
                e
            ))))
        }
    }
}

/// Status handler - check job status
async fn status_handler(
    State(state): State<SharedState>,
    Path(job_id): Path<String>,
) -> Json<ApiResponse<JobStatus>> {
    let worker_state = state.read().await;

    match worker_state.jobs.get(&job_id) {
        Some(job) => Json(ApiResponse::success(job.clone())),
        None => Json(ApiResponse::error(format!("Job {} not found", job_id))),
    }
}
