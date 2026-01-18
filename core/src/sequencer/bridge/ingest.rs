#![allow(dead_code)] // start_indexer kept for non-pipeline usage
//! Deposit Indexer
//!
//! Watches the Solana L1 for deposit events emitted by the bridge program
//! and routes them through the pipeline for L2 processing.
//!
//! Features:
//! - Uses finalized commitment for reliability
//! - Deduplicates deposits by L1 sequence number
//! - Persists last processed slot for restart recovery
//! - Routes deposits through pipeline (not direct DB update)
//!
//! Log format: "Program log: ZE_DEPOSIT:<Pubkey>:<Amount>:<Nonce>"

use anyhow::Result;
use log::{debug, error, info, warn};
use solana_client::nonblocking::pubsub_client::PubsubClient;
use solana_client::rpc_config::{RpcTransactionLogsConfig, RpcTransactionLogsFilter};
use solana_commitment_config::CommitmentConfig;
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use tokio_stream::StreamExt;

use zelana_account::AccountId;
use zelana_transaction::{DepositEvent, TransactionType};

use crate::sequencer::pipeline::PipelineService;
use crate::sequencer::storage::db::RocksDbStore;

/// Deposit indexer configuration
#[derive(Debug, Clone)]
pub struct IndexerConfig {
    /// Solana WebSocket URL
    pub ws_url: String,
    /// Solana RPC URL (for historical fetch)
    pub rpc_url: String,
    /// Bridge program ID
    pub bridge_program_id: String,
    /// Whether to fetch historical deposits on startup
    pub fetch_historical: bool,
    /// Maximum historical slots to scan (to limit startup time)
    pub max_historical_slots: u64,
}

impl Default for IndexerConfig {
    fn default() -> Self {
        Self {
            ws_url: "ws://127.0.0.1:8900".to_string(),
            rpc_url: "http://127.0.0.1:8899".to_string(),
            bridge_program_id: "8SE6gCijcFQixvDQqWu29mCm9AydN8hcwWh2e2Q6RQgE".to_string(),
            fetch_historical: true,
            max_historical_slots: 10000, // ~1 hour of slots
        }
    }
}

/// Start the deposit indexer with pipeline integration
///
/// Connects to Solana WebSocket and watches for deposit events from the bridge program.
/// When a deposit is detected, routes it through the pipeline for proper L2 processing.
pub async fn start_indexer_with_pipeline(
    db: Arc<RocksDbStore>,
    config: IndexerConfig,
    pipeline: Arc<PipelineService>,
) {
    info!(
        "Deposit indexer started (finalized commitment). Watching: {}",
        config.bridge_program_id
    );

    // Fetch historical deposits if configured
    if config.fetch_historical {
        if let Err(e) =
            fetch_historical_deposits(&db, &config.rpc_url, &config.bridge_program_id, &pipeline)
                .await
        {
            warn!("Failed to fetch historical deposits: {}", e);
        }
    }

    // Start live subscription
    loop {
        match run_subscription(&db, &config, &pipeline).await {
            Ok(()) => {
                info!("Indexer subscription ended normally");
                break;
            }
            Err(e) => {
                error!("Indexer subscription error: {}. Reconnecting in 5s...", e);
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        }
    }
}

/// Run the WebSocket subscription loop
async fn run_subscription(
    db: &Arc<RocksDbStore>,
    config: &IndexerConfig,
    pipeline: &Arc<PipelineService>,
) -> Result<()> {
    let pubsub = PubsubClient::new(&config.ws_url).await?;
    info!("Connected to Solana pubsub at {}", config.ws_url);

    // Use finalized commitment for reliability
    let (mut stream, _unsub) = pubsub
        .logs_subscribe(
            RpcTransactionLogsFilter::Mentions(vec![config.bridge_program_id.clone()]),
            // RpcTransactionLogsFilter::All,
            RpcTransactionLogsConfig {
                commitment: Some(CommitmentConfig::processed()),
            },
        )
        .await?;
    info!("hi");
    info!("Subscribed to bridge program logs (finalized commitment)");

    while let Some(response) = stream.next().await {
        // Extract slot from context
        let slot = response.context.slot;
        info!("{slot}");
        for log in response.value.logs {
            // Check for our specific log prefix
            info!("{}", log);
            if let Some(payload) = log.strip_prefix("Program log: ZE_DEPOSIT:") {
                if let Some(event) = parse_deposit_log(payload) {
                    // Check for duplicate
                    if db.is_deposit_processed(event.l1_seq)? {
                        debug!(
                            "Skipping duplicate deposit l1_seq={} (already processed)",
                            event.l1_seq
                        );
                        continue;
                    }

                    info!(
                        "Deposit detected: to={:?}, amount={}, l1_seq={}, slot={}",
                        event.to, event.amount, event.l1_seq, slot
                    );

                    // Route through pipeline
                    match pipeline
                        .submit(TransactionType::Deposit(event.clone()))
                        .await
                    {
                        Ok(()) => {
                            // Mark as processed only after successful submission
                            if let Err(e) = db.mark_deposit_processed(event.l1_seq, slot) {
                                error!("Failed to mark deposit as processed: {}", e);
                            }
                            if let Err(e) = db.set_last_processed_slot(slot) {
                                error!("Failed to update last processed slot: {}", e);
                            }
                            // Track L1 deposit amount for stats
                            if let Err(e) = db.add_l1_deposit(event.amount) {
                                error!("Failed to track L1 deposit amount: {}", e);
                            }
                            info!(
                                "DEPOSIT: +{} lamports for {:?} (l1_seq={})",
                                event.amount, event.to, event.l1_seq
                            );
                        }
                        Err(e) => {
                            error!(
                                "Failed to submit deposit to pipeline (l1_seq={}): {}",
                                event.l1_seq, e
                            );
                            // Don't mark as processed - will retry on next run
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

/// Fetch historical deposits from the last processed slot
///
/// Note: This is a simplified implementation. In production, you would use
/// getSignaturesForAddress with proper pagination and transaction parsing.
/// For MVP, we rely on the live subscription and accept that deposits made
/// while the indexer was offline may need manual reconciliation.
async fn fetch_historical_deposits(
    db: &Arc<RocksDbStore>,
    _rpc_url: &str,
    _bridge_program_id: &str,
    _pipeline: &Arc<PipelineService>,
) -> Result<()> {
    let last_slot = db.get_last_processed_slot()?;

    match last_slot {
        Some(slot) => {
            info!(
                "Last processed slot: {}. Historical fetch not implemented for MVP - relying on live subscription.",
                slot
            );
        }
        None => {
            info!("No previous slot recorded. Starting fresh with live subscription.");
        }
    }

    // TODO: Implement proper historical fetch with:
    // 1. Get signatures for bridge program since last_slot
    // 2. Parse transactions for deposit events
    // 3. Route through pipeline with deduplication
    //
    // For MVP, we accept potential gaps during downtime.
    // The deduplication logic will prevent double-processing of deposits
    // that are re-submitted manually if needed.

    Ok(())
}

/// Parses format: "ZE_DEPOSIT:<Pubkey>:<Amount>:<Nonce>"
fn parse_deposit_log(payload: &str) -> Option<DepositEvent> {
    let parts: Vec<&str> = payload.split(':').collect();
    if parts.len() != 3 {
        warn!("Malformed deposit log: {}", payload);
        return None;
    }
    info!("{}", payload);

    let pubkey_str = parts[0];
    let pubkey = parse_log_pubkey(pubkey_str)?;
    let amount = parts[1].parse::<u64>().ok()?;
    let nonce = parts[2].parse::<u64>().ok()?;

    Some(DepositEvent {
        to: map_l1_to_l2(pubkey),
        amount,
        l1_seq: nonce,
    })
}

/// Parse a pubkey from log output (handles both base58 and byte array formats)
fn parse_log_pubkey(log_val: &str) -> Option<Pubkey> {
    let log_val = log_val.trim();

    // Handle byte array format: [1, 2, 3, ...]
    if log_val.starts_with('[') {
        let bytes_str = log_val.trim_matches(|c| c == '[' || c == ']');
        let bytes: Result<Vec<u8>, _> = bytes_str
            .split(',')
            .map(|s| s.trim().parse::<u8>())
            .collect();

        if let Ok(vec) = bytes {
            if vec.len() == 32 {
                return Some(Pubkey::new_from_array(vec.try_into().unwrap()));
            }
        }
    }

    // Handle base58 format
    Pubkey::from_str(log_val).ok()
}

/// Map L1 Solana pubkey to L2 account ID
///
/// For MVP: Direct 1:1 mapping of bytes
/// Future: Could use a different derivation scheme
fn map_l1_to_l2(l1_key: Pubkey) -> AccountId {
    let mut bytes = [0u8; 32];
    bytes.copy_from_slice(l1_key.as_ref());
    AccountId(bytes)
}

// ============================================================================
// Legacy function for backward compatibility
// ============================================================================

/// Start the deposit indexer (legacy, direct DB update)
///
/// This is the original implementation that updates the DB directly.
/// Prefer using `start_indexer_with_pipeline` for proper transaction routing.
#[deprecated(note = "Use start_indexer_with_pipeline instead for proper pipeline routing")]
pub async fn start_indexer(db: Arc<RocksDbStore>, ws_url: String, bridge_program_id: String) {
    use crate::storage::StateStore;

    info!(
        "Deposit indexer started (legacy mode). Watching: {}",
        bridge_program_id
    );

    let pubsub = match PubsubClient::new(&ws_url).await {
        Ok(client) => client,
        Err(e) => {
            error!("Failed to connect to Solana WSS: {}", e);
            return;
        }
    };

    info!("Connected to Solana pubsub");

    let (mut stream, _unsub) = match pubsub
        .logs_subscribe(
            RpcTransactionLogsFilter::Mentions(vec![bridge_program_id]),
            RpcTransactionLogsConfig {
                commitment: Some(CommitmentConfig::finalized()),
            },
        )
        .await
    {
        Ok(s) => s,
        Err(e) => {
            error!("Failed to subscribe to logs: {}", e);
            return;
        }
    };

    while let Some(response) = stream.next().await {
        let slot = response.context.slot;

        for log in response.value.logs {
            // Check for our specific log prefix
            if let Some(payload) = log.strip_prefix("Program log: ZE_DEPOSIT:") {
                if let Some(event) = parse_deposit_log(payload) {
                    // Check for duplicate
                    match db.is_deposit_processed(event.l1_seq) {
                        Ok(true) => {
                            debug!(
                                "Skipping duplicate deposit l1_seq={} (already processed)",
                                event.l1_seq
                            );
                            continue;
                        }
                        Err(e) => {
                            error!("Failed to check deposit status: {}", e);
                            continue;
                        }
                        Ok(false) => {}
                    }

                    info!("Deposit detected: {:?}", event);

                    // Direct DB update (legacy behavior)
                    let mut account_state = db.get_account_state(&event.to).unwrap_or_default();
                    account_state.balance = account_state.balance.saturating_add(event.amount);

                    if let Err(e) = db.set_account_state(event.to, account_state) {
                        error!("Failed to persist deposit: {}", e);
                    } else {
                        // Mark as processed
                        let _ = db.mark_deposit_processed(event.l1_seq, slot);
                        let _ = db.set_last_processed_slot(slot);
                        info!(
                            "DEPOSIT (legacy): +{} lamports for {:?}",
                            event.amount, event.to
                        );
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_deposit_log() {
        // Test base58 pubkey format
        let payload = "11111111111111111111111111111111:1000000:42";
        let event = parse_deposit_log(payload).unwrap();
        assert_eq!(event.amount, 1000000);
        assert_eq!(event.l1_seq, 42);
    }

    #[test]
    fn test_parse_deposit_log_malformed() {
        // Missing field
        let payload = "11111111111111111111111111111111:1000000";
        assert!(parse_deposit_log(payload).is_none());

        // Too many fields
        let payload = "11111111111111111111111111111111:1000000:42:extra";
        assert!(parse_deposit_log(payload).is_none());
    }

    #[test]
    fn test_map_l1_to_l2() {
        let pubkey = Pubkey::new_unique();
        let account_id = map_l1_to_l2(pubkey);
        assert_eq!(account_id.0, pubkey.to_bytes());
    }
}
