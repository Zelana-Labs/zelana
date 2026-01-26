//! RocksDB Reader Server
//!
//! A long-running server that provides read access to the Zelana RocksDB database
//! via a Unix socket or TCP. This allows the Bun.js frontend to query database
//! state without needing native RocksDB bindings.

use anyhow::{Context, Result};
use rocksdb::{ColumnFamilyDescriptor, DB, IteratorMode, Options};
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use zelana_account::AccountState;
use zelana_block::BlockHeader;
use zelana_privacy::EncryptedNote;

// Column family names (must match core/src/sequencer/db.rs)
const CF_ACCOUNTS: &str = "accounts";
const CF_TX_BLOBS: &str = "tx_blobs";
const CF_BLOCKS: &str = "blocks";
const CF_NULLIFIERS: &str = "nullifiers";
const CF_COMMITMENTS: &str = "commitments";
const CF_ENCRYPTED_NOTES: &str = "encrypted_notes";
const CF_WITHDRAWALS: &str = "withdrawals";
const CF_TREE_META: &str = "tree_meta";
const CF_PROCESSED_DEPOSITS: &str = "processed_deposits";
const CF_BATCHES: &str = "batches";
const CF_TX_INDEX: &str = "tx_index";
const CF_INDEXER_META: &str = "indexer_meta";

/// Request from the Bun server
#[derive(Debug, Deserialize)]
#[serde(tag = "cmd")]
enum Request {
    #[serde(rename = "stats")]
    Stats,
    #[serde(rename = "accounts")]
    Accounts { offset: usize, limit: usize },
    #[serde(rename = "account")]
    Account { id: String },
    #[serde(rename = "transactions")]
    Transactions {
        offset: usize,
        limit: usize,
        batch_id: Option<u64>,
        tx_type: Option<String>,
        status: Option<String>,
    },
    #[serde(rename = "transaction")]
    Transaction { hash: String },
    #[serde(rename = "batches")]
    Batches { offset: usize, limit: usize },
    #[serde(rename = "batch")]
    Batch { id: u64 },
    #[serde(rename = "blocks")]
    Blocks { offset: usize, limit: usize },
    #[serde(rename = "nullifiers")]
    Nullifiers { offset: usize, limit: usize },
    #[serde(rename = "commitments")]
    Commitments { offset: usize, limit: usize },
    #[serde(rename = "encrypted_notes")]
    EncryptedNotes { offset: usize, limit: usize },
    #[serde(rename = "tree_meta")]
    TreeMeta,
    #[serde(rename = "deposits")]
    Deposits { offset: usize, limit: usize },
    #[serde(rename = "withdrawals")]
    Withdrawals { offset: usize, limit: usize },
    #[serde(rename = "indexer_meta")]
    IndexerMeta,
    #[serde(rename = "ping")]
    Ping,
}

/// Response to the Bun server
#[derive(Debug, Serialize)]
struct Response {
    success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

impl Response {
    fn ok(data: serde_json::Value) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
        }
    }

    fn err(msg: impl Into<String>) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(msg.into()),
        }
    }
}

/// Database reader with cached handles
struct DbReader {
    db: Arc<DB>,
}

impl DbReader {
    fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let mut opts = Options::default();
        opts.create_if_missing(false);
        opts.create_missing_column_families(false);

        let cf_names: &[&str] = &[
            CF_ACCOUNTS,
            CF_BLOCKS,
            CF_NULLIFIERS,
            CF_TX_BLOBS,
            CF_COMMITMENTS,
            CF_ENCRYPTED_NOTES,
            CF_WITHDRAWALS,
            CF_TREE_META,
            CF_PROCESSED_DEPOSITS,
            CF_BATCHES,
            CF_TX_INDEX,
            CF_INDEXER_META,
        ];

        // Helper to create descriptors (since they don't implement Clone)
        let descriptors = || {
            cf_names
                .iter()
                .map(|name| ColumnFamilyDescriptor::new(*name, Options::default()))
                .collect::<Vec<_>>()
        };

        let secondary_path = PathBuf::from(format!("{}_secondary", path.as_ref().display()));
        // Try read-only first, then secondary
        let db = DB::open_cf_descriptors_as_secondary(
            &opts,
            path.as_ref(),
            &secondary_path,
            descriptors(),
        )
        .context("Failed to open RocksDB")?;

        Ok(Self { db: Arc::new(db) })
    }

    fn handle_request(&self, req: Request) -> Response {
        match req {
            Request::Ping => Response::ok(serde_json::json!({"pong": true})),
            Request::Stats => self.get_stats(),
            Request::Accounts { offset, limit } => self.get_accounts(offset, limit),
            Request::Account { id } => self.get_account(&id),
            Request::Transactions {
                offset,
                limit,
                batch_id,
                tx_type,
                status,
            } => self.get_transactions(offset, limit, batch_id, tx_type, status),
            Request::Transaction { hash } => self.get_transaction(&hash),
            Request::Batches { offset, limit } => self.get_batches(offset, limit),
            Request::Batch { id } => self.get_batch(id),
            Request::Blocks { offset, limit } => self.get_blocks(offset, limit),
            Request::Nullifiers { offset, limit } => self.get_nullifiers(offset, limit),
            Request::Commitments { offset, limit } => self.get_commitments(offset, limit),
            Request::EncryptedNotes { offset, limit } => self.get_encrypted_notes(offset, limit),
            Request::TreeMeta => self.get_tree_meta(),
            Request::Deposits { offset, limit } => self.get_deposits(offset, limit),
            Request::Withdrawals { offset, limit } => self.get_withdrawals(offset, limit),
            Request::IndexerMeta => self.get_indexer_meta(),
        }
    }

    fn get_stats(&self) -> Response {
        let accounts_count = self.count_cf(CF_ACCOUNTS).unwrap_or(0);
        let transactions_count = self.count_cf(CF_TX_INDEX).unwrap_or(0);
        let batches_count = self.count_cf(CF_BATCHES).unwrap_or(0);
        let blocks_count = self.count_cf(CF_BLOCKS).unwrap_or(0);
        let nullifiers_count = self.count_cf(CF_NULLIFIERS).unwrap_or(0);
        let commitments_count = self.count_cf(CF_COMMITMENTS).unwrap_or(0);
        let encrypted_notes_count = self.count_cf(CF_ENCRYPTED_NOTES).unwrap_or(0);
        let withdrawals_count = self.count_cf(CF_WITHDRAWALS).unwrap_or(0);
        let deposits_count = self.count_cf(CF_PROCESSED_DEPOSITS).unwrap_or(0);

        // Get latest state root from blocks
        let latest_state_root = self
            .get_latest_state_root()
            .unwrap_or_else(|| "0".repeat(64));
        let latest_batch_id = self.get_latest_batch_id().unwrap_or(0);

        Response::ok(serde_json::json!({
            "accounts": accounts_count,
            "transactions": transactions_count,
            "batches": batches_count,
            "blocks": blocks_count,
            "nullifiers": nullifiers_count,
            "commitments": commitments_count,
            "encrypted_notes": encrypted_notes_count,
            "withdrawals": withdrawals_count,
            "deposits": deposits_count,
            "latest_state_root": latest_state_root,
            "latest_batch_id": latest_batch_id,
        }))
    }

    fn get_accounts(&self, offset: usize, limit: usize) -> Response {
        let cf = match self.db.cf_handle(CF_ACCOUNTS) {
            Some(cf) => cf,
            None => return Response::err("accounts CF not found"),
        };

        let mut accounts = Vec::new();
        let iter = self.db.iterator_cf(&cf, IteratorMode::Start);

        for item in iter {
            let (key, value) = match item {
                Ok(kv) => kv,
                Err(e) => return Response::err(format!("Iterator error: {}", e)),
            };

            if key.len() == 32 {
                let id = hex::encode(&key);
                if let Ok(state) = wincode::deserialize::<AccountState>(&value) {
                    accounts.push(serde_json::json!({
                        "id": id,
                        "balance": state.balance,
                        "nonce": state.nonce,
                    }));
                }
            }
        }

        // Sort by balance descending
        accounts.sort_by(|a, b| {
            let bal_a = a["balance"].as_u64().unwrap_or(0);
            let bal_b = b["balance"].as_u64().unwrap_or(0);
            bal_b.cmp(&bal_a)
        });

        let total = accounts.len();
        let paginated: Vec<_> = accounts.into_iter().skip(offset).take(limit).collect();

        Response::ok(serde_json::json!({
            "items": paginated,
            "total": total,
            "offset": offset,
            "limit": limit,
        }))
    }

    fn get_account(&self, id: &str) -> Response {
        let cf = match self.db.cf_handle(CF_ACCOUNTS) {
            Some(cf) => cf,
            None => return Response::err("accounts CF not found"),
        };

        let key = match hex::decode(id) {
            Ok(k) => k,
            Err(_) => return Response::err("Invalid hex ID"),
        };

        match self.db.get_cf(&cf, &key) {
            Ok(Some(value)) => {
                if let Ok(state) = wincode::deserialize::<AccountState>(&value) {
                    Response::ok(serde_json::json!({
                        "id": id,
                        "balance": state.balance,
                        "nonce": state.nonce,
                    }))
                } else {
                    Response::err("Failed to deserialize account")
                }
            }
            Ok(None) => Response::ok(serde_json::json!({
                "id": id,
                "balance": 0,
                "nonce": 0,
            })),
            Err(e) => Response::err(format!("DB error: {}", e)),
        }
    }

    fn get_transactions(
        &self,
        offset: usize,
        limit: usize,
        batch_id: Option<u64>,
        tx_type: Option<String>,
        status: Option<String>,
    ) -> Response {
        let cf = match self.db.cf_handle(CF_TX_INDEX) {
            Some(cf) => cf,
            None => return Response::err("tx_index CF not found"),
        };

        let mut transactions = Vec::new();
        let iter = self.db.iterator_cf(&cf, IteratorMode::Start);

        for item in iter {
            let (key, value) = match item {
                Ok(kv) => kv,
                Err(e) => return Response::err(format!("Iterator error: {}", e)),
            };

            let tx_hash = hex::encode(&key);
            if let Ok(summary) = serde_json::from_slice::<serde_json::Value>(&value) {
                // Apply filters
                if let Some(bid) = batch_id {
                    if summary.get("batch_id").and_then(|v| v.as_u64()) != Some(bid) {
                        continue;
                    }
                }
                if let Some(ref tt) = tx_type {
                    if summary.get("tx_type").and_then(|v| v.as_str()) != Some(tt) {
                        continue;
                    }
                }
                if let Some(ref st) = status {
                    if summary.get("status").and_then(|v| v.as_str()) != Some(st) {
                        continue;
                    }
                }

                let mut tx = summary.clone();
                if let serde_json::Value::Object(ref mut map) = tx {
                    map.insert("tx_hash".to_string(), serde_json::json!(tx_hash));
                }
                transactions.push(tx);
            }
        }

        // Sort by received_at descending
        transactions.sort_by(|a, b| {
            let ts_a = a.get("received_at").and_then(|v| v.as_u64()).unwrap_or(0);
            let ts_b = b.get("received_at").and_then(|v| v.as_u64()).unwrap_or(0);
            ts_b.cmp(&ts_a)
        });

        let total = transactions.len();
        let paginated: Vec<_> = transactions.into_iter().skip(offset).take(limit).collect();

        Response::ok(serde_json::json!({
            "items": paginated,
            "total": total,
            "offset": offset,
            "limit": limit,
        }))
    }

    fn get_transaction(&self, hash: &str) -> Response {
        let cf = match self.db.cf_handle(CF_TX_INDEX) {
            Some(cf) => cf,
            None => return Response::err("tx_index CF not found"),
        };

        let key = match hex::decode(hash) {
            Ok(k) => k,
            Err(_) => return Response::err("Invalid hex hash"),
        };

        match self.db.get_cf(&cf, &key) {
            Ok(Some(value)) => {
                if let Ok(mut summary) = serde_json::from_slice::<serde_json::Value>(&value) {
                    if let serde_json::Value::Object(ref mut map) = summary {
                        map.insert("tx_hash".to_string(), serde_json::json!(hash));
                    }
                    Response::ok(summary)
                } else {
                    Response::err("Failed to deserialize transaction")
                }
            }
            Ok(None) => Response::err("Transaction not found"),
            Err(e) => Response::err(format!("DB error: {}", e)),
        }
    }

    fn get_batches(&self, offset: usize, limit: usize) -> Response {
        let cf = match self.db.cf_handle(CF_BATCHES) {
            Some(cf) => cf,
            None => return Response::err("batches CF not found"),
        };

        let mut batches = Vec::new();
        let iter = self.db.iterator_cf(&cf, IteratorMode::End);

        for item in iter {
            let (key, value) = match item {
                Ok(kv) => kv,
                Err(e) => return Response::err(format!("Iterator error: {}", e)),
            };

            if key.len() == 8 {
                let batch_id = u64::from_be_bytes(key.as_ref().try_into().unwrap_or([0; 8]));
                if let Ok(mut summary) = serde_json::from_slice::<serde_json::Value>(&value) {
                    if let serde_json::Value::Object(ref mut map) = summary {
                        map.insert("batch_id".to_string(), serde_json::json!(batch_id));
                    }
                    batches.push(summary);
                }
            }
        }

        let total = batches.len();
        let paginated: Vec<_> = batches.into_iter().skip(offset).take(limit).collect();

        Response::ok(serde_json::json!({
            "items": paginated,
            "total": total,
            "offset": offset,
            "limit": limit,
        }))
    }

    fn get_batch(&self, id: u64) -> Response {
        let cf = match self.db.cf_handle(CF_BATCHES) {
            Some(cf) => cf,
            None => return Response::err("batches CF not found"),
        };

        let key = id.to_be_bytes();
        match self.db.get_cf(&cf, &key) {
            Ok(Some(value)) => {
                if let Ok(mut summary) = serde_json::from_slice::<serde_json::Value>(&value) {
                    if let serde_json::Value::Object(ref mut map) = summary {
                        map.insert("batch_id".to_string(), serde_json::json!(id));
                    }
                    Response::ok(summary)
                } else {
                    Response::err("Failed to deserialize batch")
                }
            }
            Ok(None) => Response::err("Batch not found"),
            Err(e) => Response::err(format!("DB error: {}", e)),
        }
    }

    fn get_blocks(&self, offset: usize, limit: usize) -> Response {
        let cf = match self.db.cf_handle(CF_BLOCKS) {
            Some(cf) => cf,
            None => return Response::err("blocks CF not found"),
        };

        let mut blocks = Vec::new();
        let iter = self.db.iterator_cf(&cf, IteratorMode::End);

        for item in iter {
            let (key, value) = match item {
                Ok(kv) => kv,
                Err(e) => return Response::err(format!("Iterator error: {}", e)),
            };

            if key.len() == 8 {
                if let Ok(header) = wincode::deserialize::<BlockHeader>(&value) {
                    blocks.push(serde_json::json!({
                        "batch_id": header.batch_id,
                        "prev_root": hex::encode(header.prev_root),
                        "new_root": hex::encode(header.new_root),
                        "tx_count": header.tx_count,
                        "open_at": header.open_at,
                        "flags": header.flags,
                    }));
                }
            }
        }

        let total = blocks.len();
        let paginated: Vec<_> = blocks.into_iter().skip(offset).take(limit).collect();

        Response::ok(serde_json::json!({
            "items": paginated,
            "total": total,
            "offset": offset,
            "limit": limit,
        }))
    }

    fn get_nullifiers(&self, offset: usize, limit: usize) -> Response {
        let cf = match self.db.cf_handle(CF_NULLIFIERS) {
            Some(cf) => cf,
            None => return Response::err("nullifiers CF not found"),
        };

        let mut nullifiers = Vec::new();
        let iter = self.db.iterator_cf(&cf, IteratorMode::Start);

        for item in iter {
            let (key, _) = match item {
                Ok(kv) => kv,
                Err(e) => return Response::err(format!("Iterator error: {}", e)),
            };

            nullifiers.push(serde_json::json!({
                "nullifier": hex::encode(&key),
            }));
        }

        let total = nullifiers.len();
        let paginated: Vec<_> = nullifiers.into_iter().skip(offset).take(limit).collect();

        Response::ok(serde_json::json!({
            "items": paginated,
            "total": total,
            "offset": offset,
            "limit": limit,
        }))
    }

    fn get_commitments(&self, offset: usize, limit: usize) -> Response {
        let cf = match self.db.cf_handle(CF_COMMITMENTS) {
            Some(cf) => cf,
            None => return Response::err("commitments CF not found"),
        };

        let mut commitments = Vec::new();
        let iter = self.db.iterator_cf(&cf, IteratorMode::Start);

        for item in iter {
            let (key, value) = match item {
                Ok(kv) => kv,
                Err(e) => return Response::err(format!("Iterator error: {}", e)),
            };

            if key.len() == 4 && value.len() == 32 {
                let position = u32::from_be_bytes(key.as_ref().try_into().unwrap_or([0; 4]));
                commitments.push(serde_json::json!({
                    "position": position,
                    "commitment": hex::encode(&value),
                }));
            }
        }

        let total = commitments.len();
        let paginated: Vec<_> = commitments.into_iter().skip(offset).take(limit).collect();

        Response::ok(serde_json::json!({
            "items": paginated,
            "total": total,
            "offset": offset,
            "limit": limit,
        }))
    }

    fn get_encrypted_notes(&self, offset: usize, limit: usize) -> Response {
        let cf = match self.db.cf_handle(CF_ENCRYPTED_NOTES) {
            Some(cf) => cf,
            None => return Response::err("encrypted_notes CF not found"),
        };

        let mut notes = Vec::new();
        let iter = self.db.iterator_cf(&cf, IteratorMode::Start);

        for item in iter {
            let (key, value) = match item {
                Ok(kv) => kv,
                Err(e) => return Response::err(format!("Iterator error: {}", e)),
            };

            let commitment = hex::encode(&key);
            if let Ok(note) = serde_json::from_slice::<EncryptedNote>(&value) {
                notes.push(serde_json::json!({
                    "commitment": commitment,
                    "ciphertext_len": note.ciphertext.len(),
                    "ephemeral_pk": hex::encode(note.ephemeral_pk),
                }));
            }
        }

        let total = notes.len();
        let paginated: Vec<_> = notes.into_iter().skip(offset).take(limit).collect();

        Response::ok(serde_json::json!({
            "items": paginated,
            "total": total,
            "offset": offset,
            "limit": limit,
        }))
    }

    fn get_tree_meta(&self) -> Response {
        let cf = match self.db.cf_handle(CF_TREE_META) {
            Some(cf) => cf,
            None => return Response::err("tree_meta CF not found"),
        };

        // Get next_position
        let next_position = match self.db.get_cf(&cf, b"next_position") {
            Ok(Some(bytes)) if bytes.len() == 8 => {
                u64::from_be_bytes(bytes.as_slice().try_into().unwrap_or([0; 8]))
            }
            _ => 0,
        };

        // Get frontier nodes
        let mut frontier = Vec::new();
        for level in 0..32 {
            let key = format!("frontier_{}", level);
            if let Ok(Some(bytes)) = self.db.get_cf(&cf, key.as_bytes()) {
                frontier.push(serde_json::json!({
                    "level": level,
                    "hash": hex::encode(&bytes),
                }));
            }
        }

        Response::ok(serde_json::json!({
            "next_position": next_position,
            "frontier": frontier,
        }))
    }

    fn get_deposits(&self, offset: usize, limit: usize) -> Response {
        let cf = match self.db.cf_handle(CF_PROCESSED_DEPOSITS) {
            Some(cf) => cf,
            None => return Response::err("processed_deposits CF not found"),
        };

        let mut deposits = Vec::new();
        let iter = self.db.iterator_cf(&cf, IteratorMode::End);

        for item in iter {
            let (key, value) = match item {
                Ok(kv) => kv,
                Err(e) => return Response::err(format!("Iterator error: {}", e)),
            };

            if key.len() == 8 && value.len() == 8 {
                let l1_seq = u64::from_be_bytes(key.as_ref().try_into().unwrap_or([0; 8]));
                let slot = u64::from_be_bytes(value.as_ref().try_into().unwrap_or([0; 8]));
                deposits.push(serde_json::json!({
                    "l1_seq": l1_seq,
                    "slot": slot,
                }));
            }
        }

        let total = deposits.len();
        let paginated: Vec<_> = deposits.into_iter().skip(offset).take(limit).collect();

        Response::ok(serde_json::json!({
            "items": paginated,
            "total": total,
            "offset": offset,
            "limit": limit,
        }))
    }

    fn get_withdrawals(&self, offset: usize, limit: usize) -> Response {
        let cf = match self.db.cf_handle(CF_WITHDRAWALS) {
            Some(cf) => cf,
            None => return Response::err("withdrawals CF not found"),
        };

        let mut withdrawals = Vec::new();
        let iter = self.db.iterator_cf(&cf, IteratorMode::Start);

        for item in iter {
            let (key, value) = match item {
                Ok(kv) => kv,
                Err(e) => return Response::err(format!("Iterator error: {}", e)),
            };

            let tx_hash = hex::encode(&key);
            withdrawals.push(serde_json::json!({
                "tx_hash": tx_hash,
                "data_len": value.len(),
            }));
        }

        let total = withdrawals.len();
        let paginated: Vec<_> = withdrawals.into_iter().skip(offset).take(limit).collect();

        Response::ok(serde_json::json!({
            "items": paginated,
            "total": total,
            "offset": offset,
            "limit": limit,
        }))
    }

    fn get_indexer_meta(&self) -> Response {
        let cf = match self.db.cf_handle(CF_INDEXER_META) {
            Some(cf) => cf,
            None => return Response::err("indexer_meta CF not found"),
        };

        let last_processed_slot = match self.db.get_cf(&cf, b"last_processed_slot") {
            Ok(Some(bytes)) if bytes.len() == 8 => Some(u64::from_be_bytes(
                bytes.as_slice().try_into().unwrap_or([0; 8]),
            )),
            _ => None,
        };

        Response::ok(serde_json::json!({
            "last_processed_slot": last_processed_slot,
        }))
    }

    // Helper methods

    fn count_cf(&self, cf_name: &str) -> Option<u64> {
        let cf = self.db.cf_handle(cf_name)?;
        let mut count = 0u64;
        let iter = self.db.iterator_cf(&cf, IteratorMode::Start);
        for _ in iter {
            count += 1;
        }
        Some(count)
    }

    fn get_latest_state_root(&self) -> Option<String> {
        let cf = self.db.cf_handle(CF_BLOCKS)?;
        let mut iter = self.db.iterator_cf(&cf, IteratorMode::End);
        if let Some(Ok((_, value))) = iter.next() {
            if let Ok(header) = wincode::deserialize::<BlockHeader>(&value) {
                return Some(hex::encode(header.new_root));
            }
        }
        None
    }

    fn get_latest_batch_id(&self) -> Option<u64> {
        let cf = self.db.cf_handle(CF_BATCHES)?;
        let mut iter = self.db.iterator_cf(&cf, IteratorMode::End);
        if let Some(Ok((key, _))) = iter.next() {
            if key.len() == 8 {
                return Some(u64::from_be_bytes(key.as_ref().try_into().ok()?));
            }
        }
        None
    }

    fn start_catchup_loop(db: Arc<DB>) {
        std::thread::spawn(move || {
            loop {
                if let Err(e) = db.try_catch_up_with_primary() {
                    eprintln!("RocksDB catchup filead: {}", e)
                }
                std::thread::sleep(Duration::from_millis(500));
            }
        });
    }
}

fn main() -> Result<()> {
    let db_path = std::env::var("DB_PATH").unwrap_or_else(|_| "./zelana-db".to_string());
    println!("{}", db_path);
    let port: u16 = std::env::var("PORT")
        .unwrap_or_else(|_| "3457".to_string())
        .parse()
        .unwrap_or(3457);

    println!("Opening database at: {}", db_path);
    let reader = DbReader::open(&db_path)?;
    DbReader::start_catchup_loop(Arc::clone(&reader.db));
    println!("Database opened successfully");

    let listener = TcpListener::bind(format!("127.0.0.1:{}", port))?;
    println!("DB Reader server listening on port {}", port);

    for stream in listener.incoming() {
        match stream {
            Ok(mut stream) => {
                let reader_clone = DbReader {
                    db: Arc::clone(&reader.db),
                };

                std::thread::spawn(move || {
                    let peer = stream.peer_addr().ok();
                    println!("Client connected: {:?}", peer);

                    let mut buf_reader = BufReader::new(stream.try_clone().unwrap());
                    let mut line = String::new();

                    loop {
                        line.clear();
                        match buf_reader.read_line(&mut line) {
                            Ok(0) => {
                                println!("Client disconnected: {:?}", peer);
                                break;
                            }
                            Ok(_) => {
                                let response = match serde_json::from_str::<Request>(&line) {
                                    Ok(req) => reader_clone.handle_request(req),
                                    Err(e) => Response::err(format!("Parse error: {}", e)),
                                };

                                let mut response_json = serde_json::to_string(&response).unwrap();
                                response_json.push('\n');

                                if let Err(e) = stream.write_all(response_json.as_bytes()) {
                                    eprintln!("Write error: {}", e);
                                    break;
                                }
                            }
                            Err(e) => {
                                eprintln!("Read error: {}", e);
                                break;
                            }
                        }
                    }
                });
            }
            Err(e) => {
                eprintln!("Connection error: {}", e);
            }
        }
    }

    Ok(())
}
