//! Deposit Indexer
//!
//! Watches the Solana L1 for deposit events emitted by the bridge program
//! and credits the corresponding L2 accounts.
//!
//! Log format: "Program log: ZE_DEPOSIT:<Pubkey>:<Amount>:<Nonce>"

use anyhow::Result;
use log::{error, info, warn};
use solana_client::nonblocking::pubsub_client::PubsubClient;
use solana_client::rpc_config::{RpcTransactionLogsConfig, RpcTransactionLogsFilter};
use solana_commitment_config::CommitmentConfig;
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;
use std::sync::Arc;
use tokio_stream::StreamExt;

use zelana_account::AccountId;
use zelana_transaction::DepositEvent;

use super::db::RocksDbStore;
use crate::storage::StateStore;

/// Start the deposit indexer
///
/// Connects to Solana WebSocket and watches for deposit events from the bridge program.
/// When a deposit is detected, credits the L2 account with the deposited amount.
pub async fn start_indexer(db: Arc<RocksDbStore>, ws_url: String, bridge_program_id: String) {
    info!("Deposit indexer started. Watching: {}", bridge_program_id);

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
                commitment: Some(CommitmentConfig::confirmed()),
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
        for log in response.value.logs {
            // Check for our specific log prefix
            if let Some(payload) = log.strip_prefix("Program log: ZE_DEPOSIT:") {
                if let Some(event) = parse_deposit_log(payload) {
                    info!("Deposit detected: {:?}", event);
                    process_deposit(&db, event);
                }
            }
        }
    }
}

/// Parses format: "ZE_DEPOSIT:<Pubkey>:<Amount>:<Nonce>"
fn parse_deposit_log(payload: &str) -> Option<DepositEvent> {
    let parts: Vec<&str> = payload.split(':').collect();
    if parts.len() != 3 {
        warn!("Malformed deposit log: {}", payload);
        return None;
    }

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

/// Process a deposit event by crediting the L2 account
fn process_deposit(db: &RocksDbStore, event: DepositEvent) {
    let mut account_state = db.get_account_state(&event.to).unwrap_or_default();

    account_state.balance = account_state.balance.saturating_add(event.amount);

    if let Err(e) = db.set_account_state(event.to, account_state) {
        error!("Failed to persist deposit: {}", e);
    } else {
        info!("DEPOSIT: +{} lamports for {:?}", event.amount, event.to);
    }
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
