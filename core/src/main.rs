use anyhow::Result;
use log::info;
use std::env;
use std::sync::Arc;
use tokio::signal;

use x25519_dalek::{PublicKey, StaticSecret};

use crate::sequencer::db::RocksDbStore;
use crate::sequencer::ingest::{start_indexer, state_ingest_server};
use crate::sequencer::session::SessionManager;

mod sequencer;
mod storage;

#[tokio::main]
async fn main() -> Result<()> {
    // -----------------------------
    // Logging
    // -----------------------------
    env_logger::init();

    // -----------------------------
    // Config
    // -----------------------------
    let db_path = env::var("ZELANA_DB_PATH").unwrap_or_else(|_| "./zelana-db".to_string());

    let ingest_port: u16 = env::var("ZELANA_INGEST_PORT")
        .unwrap_or_else(|_| "8080".to_string())
        .parse()
        .expect("invalid port");

    let solana_ws =
        env::var("SOLANA_WS_URL").unwrap_or_else(|_| "wss://api.devnet.solana.com/".to_string());

    let bridge_program_id = env::var("ZELANA_BRIDGE_PROGRAM")
        .unwrap_or_else(|_| "11111111111111111111111111111111".to_string());

    info!("DB path           : {}", db_path);
    info!("Ingest port       : {}", ingest_port);
    info!("Solana WS         : {}", solana_ws);
    info!("Bridge program ID : {}", bridge_program_id);

    // -----------------------------
    // Database (single instance)
    // -----------------------------
    let db = Arc::new(RocksDbStore::open(&db_path).expect("failed to open RocksDB"));

    // -----------------------------
    // Sequencer secret
    // -----------------------------
    let sequencer_secret = StaticSecret::from([42u8; 32]);
    // -----------------------------
    // Network session manager
    // (used later for Zephyr / UDP)
    // -----------------------------
    let session_manager = Arc::new(SessionManager::new());

    // -----------------------------
    // Spawn ingest server (HTTP)
    // -----------------------------
    {
        let db_clone = db.clone();
        let secret = sequencer_secret.clone();

        tokio::spawn(async move {
            state_ingest_server((*db_clone).clone(), secret, ingest_port).await;
        });
    }

    // -----------------------------
    // Spawn Solana indexer
    // -----------------------------
    {
        let db_clone = db.clone();
        let ws = solana_ws.clone();
        let program_id = bridge_program_id.clone();

        tokio::spawn(async move {
            start_indexer(db_clone, ws, program_id).await;
        });
    }

    info!("Zelana sequencer started (HTTP mode, no prover)");

    // -----------------------------
    // Keep process alive
    // -----------------------------
    signal::ctrl_c().await?;
    info!("Shutting down sequencer");

    Ok(())
}
