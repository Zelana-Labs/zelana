//! Zelana Sequencer
//!
//! Main entry point for the L2 sequencer with shielded transaction support.
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────────┐
//! │                         Zelana Sequencer                                │
//! │                                                                         │
//! │  ┌─────────────┐  ┌─────────────────────────────────────────────────┐   │
//! │  │  HTTP API   │  │              Pipeline Orchestrator               │  │
//! │  │  (axum)     │  │  ┌─────────┐  ┌─────────┐  ┌─────────────────┐  │   │
//! │  └──────┬──────┘  │  │ Batch   │─▶│ Prover │─▶│    Settler      │  │ │
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
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::signal;
use tokio::sync::Mutex;

use crate::api::handlers::ApiState;
use crate::api::routes::create_router;
use crate::config::{ZelanaConfig, ZelanaConfigExt};
use crate::sequencer::{
    IndexerConfig, PipelineService, RocksDbStore, ShieldedState, WithdrawalQueue,
    start_indexer_with_pipeline,
};

mod api;
mod config;
mod sequencer;
mod storage;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    env_logger::init();

    // Load configuration from ~/.zelana/config.toml + env vars
    let config = ZelanaConfig::load().expect("Failed to load configuration");

    // Convert to pipeline config
    let pipeline_config = config.to_pipeline_config();
    let batch_config = config.to_batch_config();

    info!("============================================");
    info!(
        "          ZELANA SEQUENCER v{}             ",
        env!("CARGO_PKG_VERSION")
    );
    info!("============================================");
    info!("DB path           : {}", config.database.path);
    info!("API port          : {}", config.api.port);
    info!(
        "UDP port          : {}",
        config
            .api
            .udp_port
            .map(|p| p.to_string())
            .unwrap_or_else(|| "disabled".to_string())
    );
    info!("Solana WS         : {}", config.solana.ws_url);
    info!("Bridge program    : {}", config.solana.bridge_program_id);
    info!("--------------------------------------------");
    info!("Batch max txs     : {}", batch_config.max_transactions);
    info!("Batch max age     : {}s", batch_config.max_batch_age_secs);
    info!("Batch max shielded: {}", batch_config.max_shielded);
    info!("--------------------------------------------");
    info!("Prover mode       : {:?}", pipeline_config.prover_mode);
    match &pipeline_config.prover_mode {
        crate::sequencer::ProverMode::Groth16 => {
            info!(
                "Proving key       : {}",
                pipeline_config
                    .proving_key_path
                    .as_deref()
                    .unwrap_or("<not set>")
            );
            info!(
                "Verifying key     : {}",
                pipeline_config
                    .verifying_key_path
                    .as_deref()
                    .unwrap_or("<not set>")
            );
        }
        crate::sequencer::ProverMode::Noir => {
            info!(
                "Noir coordinator  : {}",
                pipeline_config
                    .noir_coordinator_url
                    .as_deref()
                    .unwrap_or("<not set>")
            );
        }
        crate::sequencer::ProverMode::Mock => {}
    }
    info!("Settlement enabled: {}", pipeline_config.settlement_enabled);
    if pipeline_config.settlement_enabled {
        info!("  Solana RPC      : {}", config.solana.rpc_url);
        info!("  Bridge program  : {}", config.solana.bridge_program_id);
        info!(
            "  Keypair path    : {}",
            pipeline_config
                .sequencer_keypair_path
                .as_deref()
                .unwrap_or("<not set>")
        );
    }
    info!(
        "Settlement retries: {}",
        pipeline_config.max_settlement_retries
    );
    info!("Dev mode          : {}", config.features.dev_mode);
    info!("============================================");

    // Open database
    let db = Arc::new(RocksDbStore::open(&config.database.path).expect("failed to open RocksDB"));
    info!("Database opened at {}", config.database.path);

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
        PipelineService::start(db.clone(), pipeline_config.clone(), None)
            .expect("failed to start pipeline service"),
    );
    info!(
        "Pipeline service started (prover: {:?}, settlement: {})",
        pipeline_config.prover_mode,
        if pipeline_config.settlement_enabled {
            "enabled"
        } else {
            "mock"
        }
    );

    // Initialize fast withdrawal service (optional)
    let fast_withdraw = if config.features.fast_withdrawals {
        use crate::sequencer::{FastWithdrawConfig, FastWithdrawManager};
        let fw = Arc::new(FastWithdrawManager::new(FastWithdrawConfig::default()));
        info!("Fast withdrawal service enabled");
        Some(fw)
    } else {
        info!("Fast withdrawal service disabled");
        None
    };

    // Initialize threshold encryption mempool (optional)
    let threshold_mempool = if config.features.threshold_encryption {
        use crate::sequencer::{
            EncryptedMempoolConfig, ThresholdMempoolManager, create_test_committee,
        };

        let threshold = config.features.threshold_k;
        let total = config.features.threshold_n;

        let mempool_config = EncryptedMempoolConfig {
            enabled: true,
            threshold,
            total_members: total,
            max_pending: 1000,
        };

        let manager = Arc::new(ThresholdMempoolManager::new(mempool_config));

        // For development: create a test committee
        // In production, this would be loaded from configuration or DKG
        if config.features.threshold_dev {
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
        info!("Threshold encryption disabled");
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
        dev_mode: config.features.dev_mode,
    };

    // Create and start HTTP server
    let router = create_router(api_state.clone());
    let addr = SocketAddr::from(([0, 0, 0, 0], config.api.port));
    let listener = TcpListener::bind(addr).await?;
    info!("HTTP API listening on {}", addr);

    // Spawn HTTP server
    let _http_handle = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });

    // Spawn Zephyr UDP server if configured
    if let Some(udp_port) = config.api.udp_port {
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

    // Spawn Solana indexer for deposits (with pipeline integration)
    {
        let db_clone = db.clone();
        let pipeline_clone = pipeline_service.clone();
        let indexer_config = IndexerConfig {
            ws_url: config.solana.ws_url.clone(),
            rpc_url: config.solana.rpc_url.clone(),
            bridge_program_id: config.solana.bridge_program_id.clone(),
            fetch_historical: true,
            max_historical_slots: 10000,
        };

        tokio::spawn(async move {
            start_indexer_with_pipeline(db_clone, indexer_config, pipeline_clone).await;
        });
    }
    info!("Deposit indexer started (finalized commitment, pipeline routing)");

    info!("============================================");
    info!("  Zelana sequencer is ready!");
    info!("  HTTP API: http://0.0.0.0:{}", config.api.port);
    if let Some(udp_port) = config.api.udp_port {
        info!("  UDP API:  udp://0.0.0.0:{}", udp_port);
    }
    if config.features.dev_mode {
        info!("  Dev endpoints enabled:");
        info!("    POST /dev/deposit - Simulate L1 deposit");
        info!("    POST /dev/seal    - Force seal current batch");
    }
    info!("============================================");

    // Wait for shutdown signal
    signal::ctrl_c().await?;
    info!("Shutdown signal received");

    // Graceful shutdown with timeout
    info!("Shutting down pipeline service...");
    let shutdown_timeout = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        pipeline_service.shutdown(),
    );

    match shutdown_timeout.await {
        Ok(Ok(())) => info!("Pipeline service shutdown complete"),
        Ok(Err(e)) => log::error!("Error shutting down pipeline service: {}", e),
        Err(_) => log::warn!("Pipeline shutdown timed out after 30s"),
    }

    // Brief delay to allow async tasks to clean up
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    info!("Zelana sequencer stopped");
    Ok(())
}
