//! Zelana Sequencer
//!
//! Main entry point for the L2 sequencer with shielded transaction support.
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                    Zelana Sequencer                             │
//! │                                                                  │
//! │  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────────┐  │
//! │  │  HTTP API   │  │  Batch Mgr  │  │    Prover Service       │  │
//! │  │  (axum)     │  │  (pipeline) │  │    (ZK proofs)          │  │
//! │  └──────┬──────┘  └──────┬──────┘  └───────────┬─────────────┘  │
//! │         │                │                     │               │
//! │         ▼                ▼                     ▼               │
//! │  ┌─────────────────────────────────────────────────────────┐   │
//! │  │                    Transaction Router                    │   │
//! │  │  • Transfer execution                                    │   │
//! │  │  • Shielded execution                                    │   │
//! │  │  • Deposit/Withdrawal processing                         │   │
//! │  └─────────────────────────────────────────────────────────┘   │
//! │         │                │                     │               │
//! │         ▼                ▼                     ▼               │
//! │  ┌───────────┐  ┌─────────────┐  ┌──────────────────────────┐  │
//! │  │  RocksDB  │  │  Withdrawal │  │     L1 Settler           │  │
//! │  │  (state)  │  │    Queue    │  │     (Solana)             │  │
//! │  └───────────┘  └─────────────┘  └──────────────────────────┘  │
//! └─────────────────────────────────────────────────────────────────┘
//! ```

use anyhow::Result;
use log::info;
use std::env;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::signal;
use tokio::sync::Mutex;

use crate::api::handlers::ApiState;
use crate::api::routes::create_router;
use crate::sequencer::batch::{BatchConfig, BatchService};
use crate::sequencer::db::RocksDbStore;
use crate::sequencer::ingest::start_indexer;
use crate::sequencer::shielded_state::ShieldedState;
use crate::sequencer::withdrawals::WithdrawalQueue;

mod api;
mod sequencer;
mod storage;

/// Sequencer configuration
#[derive(Debug, Clone)]
struct SequencerConfig {
    /// Path to RocksDB database
    db_path: String,
    /// HTTP API port
    api_port: u16,
    /// Solana WebSocket URL for deposit indexing
    solana_ws_url: String,
    /// Bridge program ID on Solana
    bridge_program_id: String,
    /// Batch configuration
    batch: BatchConfig,
}

impl SequencerConfig {
    /// Load configuration from environment variables
    fn from_env() -> Self {
        Self {
            db_path: env::var("ZELANA_DB_PATH").unwrap_or_else(|_| "./zelana-db".to_string()),
            api_port: env::var("ZELANA_API_PORT")
                .unwrap_or_else(|_| "8080".to_string())
                .parse()
                .expect("invalid API port"),
            solana_ws_url: env::var("SOLANA_WS_URL")
                .unwrap_or_else(|_| "wss://api.devnet.solana.com/".to_string()),
            bridge_program_id: env::var("ZELANA_BRIDGE_PROGRAM")
                .unwrap_or_else(|_| "11111111111111111111111111111111".to_string()),
            batch: BatchConfig {
                max_transactions: env::var("ZELANA_BATCH_MAX_TXS")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(100),
                max_batch_age_secs: env::var("ZELANA_BATCH_MAX_AGE")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(60),
                max_shielded: env::var("ZELANA_BATCH_MAX_SHIELDED")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(10),
                min_transactions: 1,
            },
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    env_logger::init();

    // Load configuration
    let config = SequencerConfig::from_env();

    info!("============================================");
    info!(
        "          ZELANA SEQUENCER v{}             ",
        env!("CARGO_PKG_VERSION")
    );
    info!("============================================");
    info!("DB path           : {}", config.db_path);
    info!("API port          : {}", config.api_port);
    info!("Solana WS         : {}", config.solana_ws_url);
    info!("Bridge program    : {}", config.bridge_program_id);
    info!("Batch max txs     : {}", config.batch.max_transactions);
    info!("Batch max age     : {}s", config.batch.max_batch_age_secs);
    info!("Batch max shielded: {}", config.batch.max_shielded);
    info!("============================================");

    // Open database
    let db = Arc::new(RocksDbStore::open(&config.db_path).expect("failed to open RocksDB"));
    info!("Database opened at {}", config.db_path);

    // Initialize shielded state
    let shielded_state = Arc::new(Mutex::new(
        ShieldedState::load(&db).unwrap_or_else(|_| ShieldedState::new()),
    ));
    {
        let state = shielded_state.lock().await;
        info!(
            "Shielded state loaded: {} commitments, {} nullifiers",
            state.commitment_count(),
            state.nullifier_count()
        );
    }

    // Initialize withdrawal queue
    let withdrawal_queue = Arc::new(Mutex::new(
        WithdrawalQueue::load(db.clone()).unwrap_or_else(|_| WithdrawalQueue::new(db.clone())),
    ));
    info!("Withdrawal queue initialized");

    // Start batch service
    let batch_service = Arc::new(
        BatchService::start(db.clone(), config.batch.clone())
            .expect("failed to start batch service"),
    );
    info!("Batch service started");

    // Create API state
    let api_state = ApiState {
        db: db.clone(),
        batch_service: batch_service.clone(),
        shielded_state: shielded_state.clone(),
        withdrawal_queue: withdrawal_queue.clone(),
        start_time: std::time::Instant::now(),
    };

    // Create and start HTTP server
    let router = create_router(api_state);
    let addr = SocketAddr::from(([0, 0, 0, 0], config.api_port));
    let listener = TcpListener::bind(addr).await?;
    info!("HTTP API listening on {}", addr);

    // Spawn HTTP server
    let http_handle = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });

    // Spawn Solana indexer for deposits
    {
        let db_clone = db.clone();
        let ws_url = config.solana_ws_url.clone();
        let program_id = config.bridge_program_id.clone();

        tokio::spawn(async move {
            start_indexer(db_clone, ws_url, program_id).await;
        });
    }
    info!("Deposit indexer started");

    info!("============================================");
    info!("  Zelana sequencer is ready!");
    info!("  API: http://0.0.0.0:{}", config.api_port);
    info!("============================================");

    // Wait for shutdown signal
    signal::ctrl_c().await?;
    info!("Shutdown signal received");

    // Graceful shutdown
    info!("Shutting down batch service...");
    if let Err(e) = batch_service.shutdown().await {
        log::error!("Error shutting down batch service: {}", e);
    }

    info!("Zelana sequencer stopped");
    Ok(())
}
