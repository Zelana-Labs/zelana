use anyhow::Result;
use log::{error, info, warn};
use solana_client::nonblocking::pubsub_client::PubsubClient;
use solana_client::rpc_config::{RpcTransactionLogsConfig, RpcTransactionLogsFilter};
use solana_commitment_config::CommitmentConfig;
use solana_sdk::pubkey::Pubkey;
use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio_stream::StreamExt;
use zelana_account::AccountId;
use zelana_pubkey::Pubkey as ZelanaPubkey;
use zelana_transaction::{DepositEvent, SignedTransaction, Transaction, TransactionType};

use super::db::RocksDbStore;
use crate::sequencer::TransactionExecutor;
use crate::storage::StateStore;

use axum::{Router, body::Bytes, extract::State, http::StatusCode, routing::post};
use tower_http::cors::CorsLayer;

// shared state - NOW INCLUDES THE EXECUTOR
#[derive(Clone)]
struct AppState {
    db: RocksDbStore,
    executor: Arc<TransactionExecutor>, // ✅ Shared executor
}

// ingest server (receives user TX via HTTP)
// ✅ Updated signature to accept Arc<TransactionExecutor>
pub async fn state_ingest_server(db: RocksDbStore, executor: Arc<TransactionExecutor>, port: u16) {
    let state = AppState {
        db,
        executor, // ✅ Store the shared executor
    };

    let app = Router::new()
        .route("/submit_tx", post(handle_submit_tx))
        .layer(CorsLayer::permissive()) // allow from wallet or webpage
        .with_state(state);

    let listener = TcpListener::bind(SocketAddr::from(([0, 0, 0, 0], port)))
        .await
        .unwrap();

    info!("HTTP ingest server listening on port {}", port);
    axum::serve(listener, app).await.unwrap();
}

async fn handle_submit_tx(State(state): State<AppState>, body: Bytes) -> (StatusCode, String) {
    // Deserialize the transaction from bytes using wincode
    let tx: Transaction = match wincode::deserialize(&body) {
        Ok(t) => t,
        Err(e) => {
            error!("Failed to deserialize transaction: {}", e);
            return (
                StatusCode::BAD_REQUEST,
                format!("Invalid transaction format: {}", e),
            );
        }
    };

    // Check Double Spend for SHIELDED txs
    if let TransactionType::Shielded(ref blob) = tx.tx_type {
        if state.db.nullifier_exists(&blob.nullifier) {
            warn!("Double spend detected!");
            return (StatusCode::BAD_REQUEST, "Double spend detected".to_string());
        }
    }

    // Persist to Mempool (RocksDB)
    if let Err(e) = state.db.add_transaction(tx.clone()) {
        error!("Failed to persist transaction: {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to persist transaction: {}", e),
        );
    }

    info!("Tx Accepted into Mempool");

    // ✅ Use the shared executor from AppState
    match handle_transaction(&tx.tx_type, &state.executor).await {
        Ok(_) => {
            info!("Transaction processed successfully");
            (StatusCode::ACCEPTED, "Transaction accepted".to_string())
        }
        Err(e) => {
            error!("Failed to process transaction: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to process transaction: {}", e),
            )
        }
    }
}

pub async fn start_indexer(db: RocksDbStore, ws_url: String, bridge_program_id: String) {
    info!("Indexer started. Watching: {}", bridge_program_id);

    let pubsub = match PubsubClient::new(&ws_url).await {
        Ok(client) => client,
        Err(e) => {
            error!("Failed to connect to Solana WSS: {}", e);
            return;
        }
    };

    info!("{:?}", pubsub);

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
            info!("{}", log);
            if let Some(payload) = log.strip_prefix("Program log: ZE_DEPOSIT:") {
                info!("{}", payload);
                if let Some(event) = parse_deposit_log(payload) {
                    info!("{:?}", event);
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

    info!("Parsed deposit event");
    Some(DepositEvent {
        to: map_l1_to_l2(pubkey),
        amount,
        l1_seq: nonce,
    })
}

fn process_deposit(db: &RocksDbStore, event: DepositEvent) {
    // 1. Load AccountState from "to" address (or create new)
    let mut account_state = db.get_account_state(&event.to).unwrap_or_default();

    // 2. Credit Balance
    account_state.balance = account_state.balance.saturating_add(event.amount);

    // 3. Save
    // Note: In production, store the 'l1_seq' to prevent re-processing the same deposit!
    let mut db_mut = db.clone();
    if let Err(e) = db_mut.set_account_state(event.to, account_state) {
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

/// Decodes and routes the transaction to the executor
async fn handle_transaction(
    tx: &TransactionType,
    executor: &TransactionExecutor,
) -> anyhow::Result<()> {
    match tx {
        TransactionType::Transfer(signed_tx) => {
            // Validate Signature (Anti-Spoofing)
            verify_signature(&signed_tx)?;

            // Execute
            executor.process(signed_tx.clone()).await?;
        }
        _ => {
            // Handle Deposits/Withdrawals
            info!("Non-transfer transaction type received");
        }
    }
    Ok(())
}

/// Verifies Ed25519 signature (64 bytes)
fn verify_signature(signed_tx: &SignedTransaction) -> anyhow::Result<()> {
    use ed25519_dalek::{Signature as Ed25519Signature, Verifier, VerifyingKey};

    // 1. Check signature length
    if signed_tx.signature.0.len() != 64 {
        return Err(anyhow::anyhow!(
            "Invalid signature length: expected 64 bytes, got {}",
            signed_tx.signature.0.len()
        ));
    }

    // 2. Serialize the transaction data
    let message = wincode::serialize(&signed_tx.data)
        .map_err(|e| anyhow::anyhow!("Failed to serialize tx data: {}", e))?;

    // 3. Create Ed25519 signature from bytes
    let ed25519_sig = Ed25519Signature::from_bytes(&signed_tx.signature.0);

    // 4. Create verifying key from public key bytes
    let verifying_key = VerifyingKey::from_bytes(&signed_tx.signer_pubkey.0)
        .map_err(|e| anyhow::anyhow!("Invalid public key: {}", e))?;

    // 5. Verify the signature
    verifying_key
        .verify(&message, &ed25519_sig)
        .map_err(|_| anyhow::anyhow!("Signature verification failed"))?;

    Ok(())
}
