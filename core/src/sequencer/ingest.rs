use log::{info,error,warn};
use regex::Regex;
use solana_sdk::pubkey::Pubkey;
use solana_client::rpc_config::{
    RpcTransactionLogsConfig,RpcTransactionLogsFilter
};
use tokio_stream::StreamExt;
use solana_client::nonblocking::pubsub_client::PubsubClient;
use solana_commitment_config::CommitmentConfig;
use std::str::FromStr;
use zelana_core::{DepositEvent,AccountId,IdentityKeys};
use zelana_execution::StateStore;

use crate::db::RocksDbStore;

pub async fn start_indexer(
    db: RocksDbStore, 
    ws_url: String, 
    bridge_program_id: String
) {
    info!("🔭 Indexer started. Watching: {}", bridge_program_id);

    let pubsub = match PubsubClient::new(&ws_url).await {
        Ok(client) => client,
        Err(e) => {
            error!("Failed to connect to Solana WSS: {}", e);
            return;
        }
    };

    info!("{:?}",pubsub);

    let (mut stream, _unsub) = match pubsub.logs_subscribe(
        RpcTransactionLogsFilter::Mentions(vec![bridge_program_id]),
        RpcTransactionLogsConfig { commitment: Some(CommitmentConfig::confirmed()) }
    ).await {
        Ok(s) => s,
        Err(e) => {
            error!("Failed to subscribe to logs: {}", e);
            return;
        }
    };

    while let Some(response) = stream.next().await {
        for log in response.value.logs {
            // Check for our specific log prefix
            info!("{}",log);
            if let Some(payload) = log.strip_prefix("Program log: ZE_DEPOSIT:") {
                info!("{}",payload);
                if let Some(event) = parse_deposit_log(payload) {
                    info!("{:?}",event);
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

    info!("hi");
    Some(DepositEvent {
        to: map_l1_to_l2(pubkey), // We need this mapping function
        amount,
        l1_seq: nonce,
    })
}

fn process_deposit(db: &RocksDbStore, event: DepositEvent) {
    // 1. Load Account (or create new)
    let mut account = db.get_account(&event.to).unwrap_or_default();
    
    // 2. Credit Balance
    account.balance = account.balance.saturating_add(event.amount);
    
    // 3. Save
    // Note: In production, store the 'l1_seq' to prevent re-processing the same deposit!
    // For MVP, direct addition is fine.
    let mut db_mut = db.clone(); // Clone Arc for mutability trait
    if let Err(e) = db_mut.set_account(event.to, account) {
        error!("Failed to persist deposit: {}", e);
    } else {
        info!("DEPOSIT: +{} for {:?}", event.amount, event.to);
    }
}

fn parse_log_pubkey(log_val: &str) -> Option<Pubkey> {
    let log_val = log_val.trim();
    
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
    
    Pubkey::from_str(log_val).ok()
}

// Temporary MVP Mapping: L1 Pubkey bytes -> L2 Account ID
fn map_l1_to_l2(l1_key: Pubkey) -> AccountId {
    let mut bytes = [0u8; 32];
    bytes.copy_from_slice(l1_key.as_ref());
    AccountId(bytes)
}