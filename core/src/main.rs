//! Zelana Sequencer
//!
//! Main entry point for the L2 sequencer with shielded transaction support.
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────────┐
//! │                         Zelana Sequencer                                 │
//! │                                                                          │
//! │  ┌─────────────┐  ┌─────────────────────────────────────────────────┐   │
//! │  │  HTTP API   │  │              Pipeline Orchestrator               │   │
//! │  │  (axum)     │  │  ┌─────────┐  ┌─────────┐  ┌─────────────────┐  │   │
//! │  └──────┬──────┘  │  │ Batch   │─▶│ Prover  │─▶│    Settler      │  │   │
//! │         │         │  │ Manager │  │(MockZK) │  │ (Mock/Solana)   │  │   │
//! │         ▼         │  └─────────┘  └─────────┘  └─────────────────┘  │   │
//! │  ┌─────────────────────────────────────────────────────────────────────┐│
//! │  │                    Transaction Router                               ││
//! │  │  • Transfer execution   • Shielded execution                        ││
//! │  │  • Deposit processing   • Withdrawal processing                     ││
//! │  └─────────────────────────────────────────────────────────────────────┘│
//! │         │                │                     │                        │
//! │         ▼                ▼                     ▼                        │
//! │  ┌───────────┐  ┌─────────────┐  ┌──────────────────────────┐           │
//! │  │  RocksDB  │  │  Shielded   │  │     Withdrawal Queue     │           │
//! │  │  (state)  │  │    State    │  │                          │           │
//! │  └───────────┘  └─────────────┘  └──────────────────────────┘           │
//! └─────────────────────────────────────────────────────────────────────────┘
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
use crate::sequencer::batch::BatchConfig;
use crate::sequencer::db::RocksDbStore;
use crate::sequencer::ingest::start_indexer;
use crate::sequencer::pipeline::{PipelineConfig, PipelineService};
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
    /// UDP API port (Zephyr protocol)
    udp_port: Option<u16>,
    /// Solana WebSocket URL for deposit indexing
    solana_ws_url: String,
    /// Bridge program ID on Solana
    bridge_program_id: String,
    /// Batch configuration
    batch: BatchConfig,
    /// Pipeline configuration
    pipeline: PipelineConfig,
}

impl SequencerConfig {
    /// Load configuration from environment variables
    fn from_env() -> Self {
        let batch = BatchConfig {
            max_transactions: env::var("BATCH_MAX_TXS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(100),
            max_batch_age_secs: env::var("BATCH_MAX_AGE")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(60),
            max_shielded: env::var("BATCH_MAX_SHIELDED")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(10),
            min_transactions: 1,
        };

        // Pipeline configuration
        let mock_prover = env::var("ZELANA_MOCK_PROVER")
            .map(|v| v != "0" && v.to_lowercase() != "false")
            .unwrap_or(true); // Default: use mock prover for MVP

        let settlement_enabled = env::var("ZELANA_SETTLEMENT_ENABLED")
            .map(|v| v == "1" || v.to_lowercase() == "true")
            .unwrap_or(false); // Default: disabled for local testing

        let pipeline = PipelineConfig {
            mock_prover,
            settlement_enabled,
            max_settlement_retries: env::var("ZELANA_SETTLEMENT_RETRIES")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(5),
            settlement_retry_base_ms: 5000,
            poll_interval_ms: 100,
            batch_config: batch.clone(),
            settler_config: None, // TODO: load from env if settlement_enabled
        };

        Self {
            db_path: env::var("ZELANA_DB_PATH").unwrap_or_else(|_| "./zelana-db".to_string()),
            api_port: env::var("ZELANA_API_PORT")
                .unwrap_or_else(|_| "8080".to_string())
                .parse()
                .expect("invalid API port"),
            udp_port: env::var("ZELANA_UDP_PORT")
                .ok()
                .and_then(|s| s.parse().ok()),
            solana_ws_url: env::var("SOLANA_WS_URL")
                .unwrap_or_else(|_| "wss://api.devnet.solana.com/".to_string()),
            bridge_program_id: env::var("ZELANA_BRIDGE_PROGRAM")
                .unwrap_or_else(|_| "11111111111111111111111111111111".to_string()),
            batch,
            pipeline,
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
    info!(
        "UDP port          : {}",
        config
            .udp_port
            .map(|p| p.to_string())
            .unwrap_or_else(|| "disabled".to_string())
    );
    info!("Solana WS         : {}", config.solana_ws_url);
    info!("Bridge program    : {}", config.bridge_program_id);
    info!("--------------------------------------------");
    info!("Batch max txs     : {}", config.batch.max_transactions);
    info!("Batch max age     : {}s", config.batch.max_batch_age_secs);
    info!("Batch max shielded: {}", config.batch.max_shielded);
    info!("--------------------------------------------");
    info!("Mock prover       : {}", config.pipeline.mock_prover);
    info!("Settlement enabled: {}", config.pipeline.settlement_enabled);
    info!(
        "Settlement retries: {}",
        config.pipeline.max_settlement_retries
    );
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

    // Start pipeline service (includes batch manager, prover, and settler)
    let pipeline_service = Arc::new(
        PipelineService::start(db.clone(), config.pipeline.clone(), None)
            .expect("failed to start pipeline service"),
    );
    info!(
        "Pipeline service started (prover: {}, settlement: {})",
        if config.pipeline.mock_prover {
            "mock"
        } else {
            "groth16"
        },
        if config.pipeline.settlement_enabled {
            "enabled"
        } else {
            "mock"
        }
    );

    // Initialize fast withdrawal service (optional)
    let fast_withdraw = if env::var("FAST_WITHDRAW_ENABLED").is_ok() {
        use crate::sequencer::fast_withdrawals::{FastWithdrawConfig, FastWithdrawManager};
        let fw = Arc::new(FastWithdrawManager::new(FastWithdrawConfig::default()));
        info!("Fast withdrawal service enabled");
        Some(fw)
    } else {
        info!("Fast withdrawal service disabled (set FAST_WITHDRAW_ENABLED to enable)");
        None
    };

    // Initialize threshold encryption mempool (optional)
    let threshold_mempool = if env::var("THRESHOLD_ENABLED").is_ok() {
        use crate::sequencer::threshold_mempool::{
            EncryptedMempoolConfig, ThresholdMempoolManager, create_test_committee,
        };

        let threshold: usize = env::var("THRESHOLD_K")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(2);
        let total: usize = env::var("THRESHOLD_N")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(3);

        let config = EncryptedMempoolConfig {
            enabled: true,
            threshold,
            total_members: total,
            max_pending: 1000,
        };

        let manager = Arc::new(ThresholdMempoolManager::new(config));

        // For development: create a test committee
        // In production, this would be loaded from configuration or DKG
        if env::var("THRESHOLD_DEV").is_ok() {
            let (committee, local_members) = create_test_committee(threshold, total);
            manager.initialize_committee(committee).await;
            // Set this node as the first committee member (for dev)
            manager.set_local_member(local_members[0].clone()).await;
            info!(
                "Threshold mempool initialized (DEV mode): K={}, N={}",
                threshold, total
            );
        } else {
            info!(
                "Threshold mempool enabled: K={}, N={} (committee not initialized)",
                threshold, total
            );
        }

        Some(manager)
    } else {
        info!("Threshold encryption disabled (set THRESHOLD_ENABLED to enable)");
        None
    };

    // Create API state
    let api_state = ApiState {
        db: db.clone(),
        pipeline_service: pipeline_service.clone(),
        shielded_state: shielded_state.clone(),
        withdrawal_queue: withdrawal_queue.clone(),
        fast_withdraw,
        threshold_mempool,
        start_time: std::time::Instant::now(),
    };

    // Create and start HTTP server
    let router = create_router(api_state.clone());
    let addr = SocketAddr::from(([0, 0, 0, 0], config.api_port));
    let listener = TcpListener::bind(addr).await?;
    info!("HTTP API listening on {}", addr);

    // Spawn HTTP server
    let http_handle = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });

    // Spawn Zephyr UDP server if configured
    if let Some(udp_port) = config.udp_port {
        use crate::api::{UdpServerConfig, start_udp_server};

        let udp_config = UdpServerConfig {
            port: udp_port,
            max_sessions: 10000,
        };
        let udp_api_state = api_state.clone();

        tokio::spawn(async move {
            start_udp_server(udp_config, udp_api_state).await;
        });
        info!("Zephyr UDP server listening on 0.0.0.0:{}", udp_port);
    }

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
    info!("  HTTP API: http://0.0.0.0:{}", config.api_port);
    if let Some(udp_port) = config.udp_port {
        info!("  UDP API:  udp://0.0.0.0:{}", udp_port);
    }
    info!("============================================");

    // Wait for shutdown signal
    signal::ctrl_c().await?;
    info!("Shutdown signal received");

    // Graceful shutdown
    info!("Shutting down pipeline service...");
    if let Err(e) = pipeline_service.shutdown().await {
        log::error!("Error shutting down pipeline service: {}", e);
    }

    info!("Zelana sequencer stopped");
    Ok(())
}
