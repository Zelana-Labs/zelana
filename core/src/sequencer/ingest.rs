use anyhow::Result;
use log::{error, info, warn};
use solana_client::nonblocking::pubsub_client::PubsubClient;
use solana_client::rpc_config::{RpcTransactionLogsConfig, RpcTransactionLogsFilter};
use solana_commitment_config::CommitmentConfig;
use solana_sdk::pubkey::Pubkey;
use tokio::net::TcpListener;
use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::{Arc};
use tokio::sync::Mutex;
use tokio_stream::StreamExt;
use zelana_account::AccountId;
use zelana_transaction::{DepositEvent};
use super::executor::Executor;
use super::db::RocksDbStore;
use crate::sequencer::executor::StateDiff;
use crate::sequencer::session::Session;
use crate::storage::StateStore;

use axum::{
    extract::{Json, State},
    http::StatusCode,
    routing::post,
    Router,
};
use tower_http::cors::CorsLayer;

use txblob::{
    EncryptedTxBlobV1,
    decrypt_signed_tx,
    tx_blob_hash,
};

use x25519_dalek::{StaticSecret, PublicKey};


// shared state
#[derive(Clone)]
struct AppState {
    db: RocksDbStore,
    executor: Arc<Mutex<Executor>>,
    sequencer_secret: Arc<StaticSecret>,
    session: Arc<Mutex<Session>>
}

#[derive(serde::Deserialize)]
pub struct SubmitBlobRequest {
    /// Serialized EncryptedTxBlobV1 (wincode)
    pub blob: Vec<u8>,
    /// Client X25519 public key
    pub client_pubkey: [u8; 32],
}

// ingest server ( recieves user TX)
pub async fn state_ingest_server(
    db: RocksDbStore,
    sequencer_secret: StaticSecret,
    port: u16,
) {

    let executor = Arc::new(Mutex::new(
    Executor::new(db.clone())
    ));

    let session = Arc::new(Mutex::new(Session::new(0)));
    let state = AppState {
        db,
        sequencer_secret: Arc::new(sequencer_secret),
        executor,
        session
    };

    let app = Router::new()
        .route("/submit_tx", post(handle_submit_tx))
        .layer(CorsLayer::permissive()) // allow from wallet or webpage
        .with_state(state);

    let listener = TcpListener::bind(SocketAddr::from(([0, 0, 0, 0], port))).await.unwrap();
    info!("Ingest server listening on {}", port);
    axum::serve(listener, app).await.unwrap();
}

//Handle Encrypted TX submission
async fn handle_submit_tx(
    State(state): State<AppState>,
    Json(req): Json<SubmitBlobRequest>,
) -> StatusCode {
    let blob: EncryptedTxBlobV1 = match wincode::deserialize(&req.blob) {
        Ok(b) => b,
        Err(_) => {
            warn!("Invalid tx blob serialization");
            return StatusCode::BAD_REQUEST;
        }
    };
    //Hash (canonical tx ID)
    let tx_hash = tx_blob_hash(&blob);

    //Decrypt in memory ONLY
    let client_pub = PublicKey::from(req.client_pubkey);
    let signed_tx = match decrypt_signed_tx(
        &blob,
        &state.sequencer_secret,
        &client_pub,
    ) {
        Ok(tx) => tx,
        Err(_) => {
            warn!("Tx decryption failed");
            return StatusCode::BAD_REQUEST;
        }
    };

    let mut executor = state.executor.lock().await;

    let exec_result = match executor.execute_signed_tx(
    signed_tx,
    tx_hash,
    ) {
        Ok(r) => r,
        Err(e) => {
            warn!("Execution failed: {:?}", e);
            return StatusCode::BAD_REQUEST;
        }
    };

    {
        let mut session = state.session.lock().await;
        session.push_execution(exec_result);
    }
    // (MVP) Optional nullifier checks would go here
    // if blob.flags & FLAG_SHIELDED != 0 { ... }
    if let Err(e) = state.db.add_encrypted_tx(tx_hash, req.blob) {
        error!("Failed to persist encrypted tx: {}", e);
        return StatusCode::INTERNAL_SERVER_ERROR;
    }

    {
        let mut session = state.session.lock().await;

        if session.tx_count() >= 100 {
            let prev_root = state
                .db
                .get_latest_state_root()
                .unwrap_or([0u8; 32]);

            let closed = session.clone().close(prev_root);

            // Commit state (MVP: no prover gate yet)
            if let Err(e) = executor.apply_state_diff(
                StateDiff {
        updates: closed.merged_state.clone(),
    }
            ) {
                error!("Failed to apply state diff: {}", e);
                return StatusCode::INTERNAL_SERVER_ERROR;
            }

            executor.reset();

            if let Err(e) = state.db.store_block_header(closed.header) {
                error!("Failed to store block header: {}", e);
                return StatusCode::INTERNAL_SERVER_ERROR;
            }

            *session = Session::new(closed.header.batch_id + 1);

            info!("Block {} committed", closed.header.batch_id);
        }
    }

    info!("Encrypted tx accepted: {:?}", tx_hash);
    StatusCode::ACCEPTED
}

// async fn handle_submit_tx(
//     State(state) : State<AppState>,
//     Json(tx) : Json<Transaction>
// )->StatusCode{


//     // Check Double Spend for SHIELDED txs
//     if let TransactionType::Shielded(ref blob) = tx.tx_type {
//         match state.db.nullifier_exists(&blob.nullifier) {
//             Ok(true) => {
//                 warn!("Double spend detected!");
//                 return StatusCode::BAD_REQUEST;
//             }
//             Ok(false)=>{}
//             Err(e)=>{
//                 error!("Failed to check nullifier: {}",e);
//                 return StatusCode::INTERNAL_SERVER_ERROR;
//             }
//         }
//     }
//     //Persist to Mempool (RocksDB)
//     if let Err(e) = state.db.add_transaction(tx) {
//         error!("Failed to persist transaction: {}", e);
//         return StatusCode::INTERNAL_SERVER_ERROR;
//     }   


//     info!("Tx Accepted into Mempool");
//     StatusCode::ACCEPTED
// }

pub async fn start_indexer(db: Arc<RocksDbStore>, ws_url: String, bridge_program_id: String) {
    info!(" Indexer started. Watching: {}", bridge_program_id);

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

    info!("hi");
    Some(DepositEvent {
        to: map_l1_to_l2(pubkey), // We need this mapping function
        amount,
        l1_seq: nonce,
    })
}

fn process_deposit(db: &RocksDbStore, event: DepositEvent) {
    let mut account_state =
        db.get_account_state(&event.to).unwrap_or_default();

    account_state.balance =
        account_state.balance.saturating_add(event.amount);

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
