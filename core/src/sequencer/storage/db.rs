#![allow(dead_code)] // Some methods reserved for shielded/encrypted features
//! # RocksDB Storage Layer
//!
//! Persistent storage for the Zelana L2 sequencer using RocksDB.
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────────────┐
//! │                           RocksDB Column Families                            │
//! ├─────────────────────────────────────────────────────────────────────────────┤
//! │                                                                              │
//! │  ┌─────────────────┐  ┌─────────────────┐  ┌─────────────────────────────┐  │
//! │  │    ACCOUNTS     │  │     BLOCKS      │  │         TX_INDEX            │  │
//! │  │  Key: [u8;32]   │  │  Key: u64 (BE)  │  │  Key: [u8;32] (tx_hash)     │  │
//! │  │  Val: AccState  │  │  Val: BlockHdr  │  │  Val: TxSummary (JSON)      │  │
//! │  └─────────────────┘  └─────────────────┘  └─────────────────────────────┘  │
//! │                                                                              │
//! │  ┌─────────────────┐  ┌─────────────────┐  ┌─────────────────────────────┐  │
//! │  │   NULLIFIERS    │  │   COMMITMENTS   │  │      ENCRYPTED_NOTES        │  │
//! │  │  Key: [u8;32]   │  │  Key: u32 (BE)  │  │  Key: [u8;32] (commitment)  │  │
//! │  │  Val: [] (flag) │  │  Val: [u8;32]   │  │  Val: EncNote (JSON)        │  │
//! │  └─────────────────┘  └─────────────────┘  └─────────────────────────────┘  │
//! │                                                                              │
//! │  ┌─────────────────┐  ┌─────────────────┐  ┌─────────────────────────────┐  │
//! │  │   WITHDRAWALS   │  │    TREE_META    │  │     PROCESSED_DEPOSITS      │  │
//! │  │  Key: [u8;32]   │  │  Key: string    │  │  Key: u64 (L1 seq, BE)      │  │
//! │  │  Val: bytes     │  │  Val: varies    │  │  Val: u64 (slot, BE)        │  │
//! │  └─────────────────┘  └─────────────────┘  └─────────────────────────────┘  │
//! │                                                                              │
//! │  ┌─────────────────┐  ┌─────────────────┐  ┌─────────────────────────────┐  │
//! │  │     BATCHES     │  │    TX_BLOBS     │  │       INDEXER_META          │  │
//! │  │  Key: u64 (BE)  │  │  Key: [u8;32]   │  │  Key: string                │  │
//! │  │  Val: JSON      │  │  Val: enc blob  │  │  Val: u64 (slot)            │  │
//! │  └─────────────────┘  └─────────────────┘  └─────────────────────────────┘  │
//! │                                                                              │
//! └─────────────────────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Column Families Reference
//!
//! | CF Name            | Key Format              | Value Format              | Purpose                                    |
//! |--------------------|-------------------------|---------------------------|--------------------------------------------|
//! | `accounts`         | `[u8; 32]` (AccountId)  | `wincode(AccountState)`   | L2 account balances and nonces             |
//! | `blocks`           | `u64` (batch_id, BE)    | `wincode(BlockHeader)`    | Finalized block headers                    |
//! | `tx_index`         | `[u8; 32]` (tx_hash)    | `JSON(TxSummary)`         | Transaction metadata for queries           |
//! | `tx_blobs`         | `[u8; 32]` (tx_hash)    | `Vec<u8>` (encrypted)     | Encrypted transaction blobs                |
//! | `batches`          | `u64` (batch_id, BE)    | `JSON(BatchSummary)`      | Batch metadata for queries                 |
//! | `nullifiers`       | `[u8; 32]`              | `[]` (empty)              | Spent nullifiers (double-spend prevention) |
//! | `commitments`      | `u32` (position, BE)    | `[u8; 32]`                | Note commitments in Merkle tree            |
//! | `encrypted_notes`  | `[u8; 32]` (commitment) | `JSON(EncryptedNote)`     | Encrypted notes for viewing key scanning   |
//! | `tree_meta`        | `string` (key name)     | varies                    | Merkle tree frontier for fast restart      |
//! | `withdrawals`      | `[u8; 32]` (tx_hash)    | `Vec<u8>` (serialized)    | Pending L2→L1 withdrawals                  |
//! | `processed_deposits`| `u64` (L1 seq, BE)     | `u64` (slot, BE)          | Dedupe L1→L2 deposits                      |
//! | `indexer_meta`     | `string` (key name)     | `u64` (slot)              | Deposit indexer checkpoint                 |
//!
//! ## Key Format Details
//!
//! - **Big-Endian (BE)**: Numeric keys use big-endian encoding for lexicographic ordering
//! - **AccountId**: 32-byte public key (ed25519)
//! - **tx_hash**: 32-byte SHA256 hash of transaction
//! - **commitment**: 32-byte Poseidon hash of note
//! - **nullifier**: 32-byte hash derived from note and spending key
//!
//! ## Serialization
//!
//! - **wincode**: Binary serialization for fixed-size structs (AccountState, BlockHeader)
//! - **JSON**: For variable-size structs with optional fields (TxSummary, BatchSummary)
//! - **Raw bytes**: For opaque data (encrypted blobs, commitments)
//!
//! ## Thread Safety
//!
//! `RocksDbStore` is `Clone` and thread-safe via `Arc<DB>`. RocksDB handles
//! internal locking for concurrent reads/writes.
//!
//! ## Atomic Batching
//!
//! Use `DbBatch` with `apply_batch()` for atomic multi-key updates. This is
//! critical for state transitions that update accounts, nullifiers, and
//! commitments together.

use super::shielded_state::TreeFrontier;
use crate::api::types::{BatchSummary, TxStatus, TxSummary, TxType};
use crate::storage::StateStore;
use anyhow::{Context, Result};
use rocksdb::{ColumnFamilyDescriptor, DB, Options, WriteBatch};
use std::path::Path;
use std::sync::Arc;
use zelana_account::{AccountId, AccountState};
use zelana_block::BlockHeader;
use zelana_privacy::{EncryptedNote, Nullifier, TREE_DEPTH};

// =============================================================================
// Column Family Names
// =============================================================================

/// Account state: balance and nonce per L2 address
/// Key: [u8; 32] (AccountId), Value: wincode(AccountState)
const CF_ACCOUNTS: &str = "accounts";

/// Encrypted transaction blobs (threshold-encrypted)
/// Key: [u8; 32] (tx_hash), Value: Vec<u8> (encrypted blob)
const CF_TX_BLOBS: &str = "tx_blobs";

/// Finalized block headers
/// Key: u64 BE (batch_id), Value: wincode(BlockHeader)
const CF_BLOCKS: &str = "blocks";

/// Spent nullifiers for double-spend prevention
/// Key: [u8; 32] (nullifier), Value: [] (empty, presence = spent)
const CF_NULLIFIERS: &str = "nullifiers";

/// Note commitments in Merkle tree order
/// Key: u32 BE (position), Value: [u8; 32] (commitment)
const CF_COMMITMENTS: &str = "commitments";

/// Encrypted notes for viewing key scanning
/// Key: [u8; 32] (commitment), Value: JSON(EncryptedNote)
const CF_ENCRYPTED_NOTES: &str = "encrypted_notes";

/// Pending withdrawals awaiting L1 settlement
/// Key: [u8; 32] (tx_hash), Value: serialized withdrawal data
const CF_WITHDRAWALS: &str = "withdrawals";

/// Merkle tree metadata (frontier nodes, next position)
/// Keys: "next_position", "frontier_0", "frontier_1", ..., "frontier_31"
const CF_TREE_META: &str = "tree_meta";

/// Processed L1 deposits (for deduplication)
/// Key: u64 BE (L1 sequence number), Value: u64 BE (slot processed)
const CF_PROCESSED_DEPOSITS: &str = "processed_deposits";

/// Batch metadata for queries
/// Key: u64 BE (batch_id), Value: JSON(BatchSummary)
const CF_BATCHES: &str = "batches";

/// Transaction index for queries
/// Key: [u8; 32] (tx_hash), Value: JSON(TxSummary)
const CF_TX_INDEX: &str = "tx_index";

/// Deposit indexer metadata
/// Keys: "last_processed_slot"
const CF_INDEXER_META: &str = "indexer_meta";

/// Statistics metadata
/// Keys: "dev_deposits_total", "l1_deposits_total", "withdrawals_total"
const CF_STATS: &str = "stats";

// =============================================================================
// RocksDbStore
// =============================================================================

/// A thread-safe wrapper around RocksDB for L2 state persistence.
///
/// # Example
///
/// ```ignore
/// let db = RocksDbStore::open("./zelana-db")?;
///
/// // Store account state
/// db.set_account_state(account_id, AccountState { balance: 1000, nonce: 0 })?;
///
/// // Atomic batch update
/// let mut batch = DbBatch::default();
/// batch.account_updates.push((account_id, new_state));
/// batch.nullifiers.push(nullifier);
/// db.apply_batch(batch)?;
/// ```
#[derive(Clone)]
pub struct RocksDbStore {
    db: Arc<DB>,
}

impl RocksDbStore {
    /// Opens the database at the specified path, creating it if missing.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let mut opts = Options::default();
        opts.create_if_missing(true);
        opts.create_missing_column_families(true);

        let families = vec![
            ColumnFamilyDescriptor::new(CF_ACCOUNTS, Options::default()),
            ColumnFamilyDescriptor::new(CF_BLOCKS, Options::default()),
            ColumnFamilyDescriptor::new(CF_NULLIFIERS, Options::default()),
            ColumnFamilyDescriptor::new(CF_TX_BLOBS, Options::default()),
            ColumnFamilyDescriptor::new(CF_COMMITMENTS, Options::default()),
            ColumnFamilyDescriptor::new(CF_ENCRYPTED_NOTES, Options::default()),
            ColumnFamilyDescriptor::new(CF_WITHDRAWALS, Options::default()),
            ColumnFamilyDescriptor::new(CF_TREE_META, Options::default()),
            ColumnFamilyDescriptor::new(CF_PROCESSED_DEPOSITS, Options::default()),
            ColumnFamilyDescriptor::new(CF_BATCHES, Options::default()),
            ColumnFamilyDescriptor::new(CF_TX_INDEX, Options::default()),
            ColumnFamilyDescriptor::new(CF_INDEXER_META, Options::default()),
            ColumnFamilyDescriptor::new(CF_STATS, Options::default()),
        ];

        let db = DB::open_cf_descriptors(&opts, path, families)
            .map_err(|e| anyhow::anyhow!("Failed to open RocksDB: {}", e))?;

        Ok(Self { db: Arc::new(db) })
    }

    ///Check if Nullifier has already been used
    pub fn nullifier_exists(&self, nullifier: &[u8]) -> Result<bool> {
        let cf = self
            .db
            .cf_handle(CF_NULLIFIERS)
            .context("Nullifiers column family not found")?;

        Ok(self.db.get_cf(cf, nullifier)?.is_some())
    }

    pub fn add_encrypted_tx(&self, tx_hash: [u8; 32], blob: Vec<u8>) -> Result<()> {
        let cf = self
            .db
            .cf_handle(CF_TX_BLOBS)
            .context("tx_blobs CF missing")?;

        self.db.put_cf(cf, tx_hash, blob)?;
        Ok(())
    }

    pub fn store_block_header(&self, header: BlockHeader) -> Result<()> {
        let cf = self.db.cf_handle(CF_BLOCKS).context("blocks CF missing")?;

        let key = header.batch_id.to_be_bytes();
        let value = wincode::serialize(&header)?;

        self.db.put_cf(cf, key, value)?;
        Ok(())
    }

    pub fn get_latest_state_root(&self) -> Result<[u8; 32]> {
        // Try CF_BATCHES first (where BatchSummary is stored by pipeline)
        let cf = self
            .db
            .cf_handle(CF_BATCHES)
            .context("batches CF missing")?;
        let mut iter = self.db.iterator_cf(cf, rocksdb::IteratorMode::End);
        if let Some(Ok((_k, v))) = iter.next() {
            let summary: crate::api::types::BatchSummary = serde_json::from_slice(&v)?;
            let root_bytes = hex::decode(&summary.state_root)?;
            if root_bytes.len() == 32 {
                let mut arr = [0u8; 32];
                arr.copy_from_slice(&root_bytes);
                return Ok(arr);
            }
        }

        // Fallback to CF_BLOCKS (legacy BlockHeader storage)
        let cf = self.db.cf_handle(CF_BLOCKS).context("blocks CF missing")?;
        let mut iter = self.db.iterator_cf(cf, rocksdb::IteratorMode::End);
        if let Some(Ok((_k, v))) = iter.next() {
            let header: BlockHeader = wincode::deserialize(&v)?;
            return Ok(header.new_root);
        }

        Ok([0u8; 32]) // genesis
    }

    pub fn mark_nullifier(&self, nullifier: &[u8]) -> Result<()> {
        let cf = self
            .db
            .cf_handle(CF_NULLIFIERS)
            .context("nullifiers CF missing")?;

        self.db.put_cf(cf, nullifier, [])?;
        Ok(())
    }

    // =========================================================================
    // Shielded State Methods
    // =========================================================================

    /// Insert a commitment at a position
    pub fn insert_commitment(&self, position: u32, commitment: [u8; 32]) -> Result<()> {
        let cf = self
            .db
            .cf_handle(CF_COMMITMENTS)
            .context("commitments CF missing")?;

        self.db.put_cf(cf, position.to_be_bytes(), commitment)?;
        Ok(())
    }

    /// Get commitment at position
    pub fn get_commitment(&self, position: u32) -> Result<Option<[u8; 32]>> {
        let cf = self
            .db
            .cf_handle(CF_COMMITMENTS)
            .context("commitments CF missing")?;

        match self.db.get_cf(cf, position.to_be_bytes())? {
            Some(bytes) => {
                let arr: [u8; 32] = bytes
                    .as_slice()
                    .try_into()
                    .context("invalid commitment length")?;
                Ok(Some(arr))
            }
            None => Ok(None),
        }
    }

    /// Get all commitments (for tree reconstruction on startup)
    pub fn get_all_commitments(&self) -> Result<Vec<(u32, [u8; 32])>> {
        let cf = self
            .db
            .cf_handle(CF_COMMITMENTS)
            .context("commitments CF missing")?;

        let mut commitments = Vec::new();
        let iter = self.db.iterator_cf(cf, rocksdb::IteratorMode::Start);

        for item in iter {
            let (key, value) = item?;
            let position = u32::from_be_bytes(
                key.as_ref()
                    .try_into()
                    .context("invalid position key length")?,
            );
            let commitment: [u8; 32] = value
                .as_ref()
                .try_into()
                .context("invalid commitment length")?;
            commitments.push((position, commitment));
        }

        Ok(commitments)
    }

    /// Get all nullifiers (for loading on startup)
    pub fn get_all_nullifiers(&self) -> Result<Vec<Nullifier>> {
        let cf = self
            .db
            .cf_handle(CF_NULLIFIERS)
            .context("nullifiers CF missing")?;

        let mut nullifiers = Vec::new();
        let iter = self.db.iterator_cf(cf, rocksdb::IteratorMode::Start);

        for item in iter {
            let (key, _) = item?;
            let nullifier: [u8; 32] = key
                .as_ref()
                .try_into()
                .context("invalid nullifier length")?;
            nullifiers.push(Nullifier(nullifier));
        }

        Ok(nullifiers)
    }

    /// Store encrypted note for viewing key scanning
    pub fn store_encrypted_note(&self, commitment: [u8; 32], note: &EncryptedNote) -> Result<()> {
        let cf = self
            .db
            .cf_handle(CF_ENCRYPTED_NOTES)
            .context("encrypted_notes CF missing")?;

        // Use serde_json for EncryptedNote (it implements Serialize/Deserialize, not SchemaWrite)
        let value = serde_json::to_vec(note)?;
        self.db.put_cf(cf, commitment, value)?;
        Ok(())
    }

    /// Get encrypted note by commitment
    pub fn get_encrypted_note(&self, commitment: &[u8; 32]) -> Result<Option<EncryptedNote>> {
        let cf = self
            .db
            .cf_handle(CF_ENCRYPTED_NOTES)
            .context("encrypted_notes CF missing")?;

        match self.db.get_cf(cf, commitment)? {
            Some(bytes) => {
                let note: EncryptedNote = serde_json::from_slice(&bytes)?;
                Ok(Some(note))
            }
            None => Ok(None),
        }
    }

    /// Get all encrypted notes (for scanning)
    pub fn get_all_encrypted_notes(&self) -> Result<Vec<([u8; 32], EncryptedNote)>> {
        let cf = self
            .db
            .cf_handle(CF_ENCRYPTED_NOTES)
            .context("encrypted_notes CF missing")?;

        let mut notes = Vec::new();
        let iter = self.db.iterator_cf(cf, rocksdb::IteratorMode::Start);

        for item in iter {
            let (key, value) = item?;
            let commitment: [u8; 32] = key
                .as_ref()
                .try_into()
                .context("invalid commitment key length")?;
            let note: EncryptedNote = serde_json::from_slice(&value)?;
            notes.push((commitment, note));
        }

        Ok(notes)
    }

    /// Store tree frontier for fast restart
    pub fn store_tree_frontier(&self, frontier: &TreeFrontier) -> Result<()> {
        let cf = self
            .db
            .cf_handle(CF_TREE_META)
            .context("tree_meta CF missing")?;

        // Store next_position
        self.db
            .put_cf(cf, b"next_position", frontier.next_position.to_be_bytes())?;

        // Store frontier nodes
        for (level, node) in frontier.frontier.iter().enumerate() {
            let key = format!("frontier_{}", level);
            match node {
                Some(hash) => self.db.put_cf(cf, key.as_bytes(), hash)?,
                None => self.db.delete_cf(cf, key.as_bytes())?,
            }
        }

        Ok(())
    }

    /// Load tree frontier
    pub fn load_tree_frontier(&self) -> Result<Option<TreeFrontier>> {
        let cf = self
            .db
            .cf_handle(CF_TREE_META)
            .context("tree_meta CF missing")?;

        // Get next_position
        let next_position = match self.db.get_cf(cf, b"next_position")? {
            Some(bytes) => {
                let arr: [u8; 8] = bytes
                    .as_slice()
                    .try_into()
                    .context("invalid next_position length")?;
                u64::from_be_bytes(arr)
            }
            None => return Ok(None),
        };

        // Load frontier nodes
        let mut frontier = vec![None; TREE_DEPTH];
        for level in 0..TREE_DEPTH {
            let key = format!("frontier_{}", level);
            if let Some(bytes) = self.db.get_cf(cf, key.as_bytes())? {
                let hash: [u8; 32] = bytes
                    .as_slice()
                    .try_into()
                    .context("invalid frontier hash length")?;
                frontier[level] = Some(hash);
            }
        }

        Ok(Some(TreeFrontier {
            frontier,
            next_position,
        }))
    }

    /// Get next commitment position
    pub fn next_commitment_position(&self) -> Result<u32> {
        let cf = self
            .db
            .cf_handle(CF_TREE_META)
            .context("tree_meta CF missing")?;

        match self.db.get_cf(cf, b"next_position")? {
            Some(bytes) => {
                let arr: [u8; 8] = bytes
                    .as_slice()
                    .try_into()
                    .context("invalid next_position length")?;
                Ok(u64::from_be_bytes(arr) as u32)
            }
            None => Ok(0),
        }
    }

    // =========================================================================
    // Withdrawal Methods
    // =========================================================================

    /// Store a pending withdrawal
    pub fn store_withdrawal(&self, tx_hash: [u8; 32], data: &[u8]) -> Result<()> {
        let cf = self
            .db
            .cf_handle(CF_WITHDRAWALS)
            .context("withdrawals CF missing")?;

        self.db.put_cf(cf, tx_hash, data)?;
        Ok(())
    }

    /// Get withdrawal by tx hash
    pub fn get_withdrawal(&self, tx_hash: &[u8; 32]) -> Result<Option<Vec<u8>>> {
        let cf = self
            .db
            .cf_handle(CF_WITHDRAWALS)
            .context("withdrawals CF missing")?;

        Ok(self.db.get_cf(cf, tx_hash)?)
    }

    /// Delete withdrawal after it's been settled
    pub fn delete_withdrawal(&self, tx_hash: &[u8; 32]) -> Result<()> {
        let cf = self
            .db
            .cf_handle(CF_WITHDRAWALS)
            .context("withdrawals CF missing")?;

        self.db.delete_cf(cf, tx_hash)?;
        Ok(())
    }

    /// Get all withdrawals from database (for loading on startup)
    pub fn get_all_withdrawals(&self) -> Result<Vec<([u8; 32], Vec<u8>)>> {
        let cf = self
            .db
            .cf_handle(CF_WITHDRAWALS)
            .context("withdrawals CF missing")?;

        let mut withdrawals = Vec::new();
        let iter = self.db.iterator_cf(cf, rocksdb::IteratorMode::Start);

        for item in iter {
            let (key, value) = item?;
            let tx_hash: [u8; 32] = key.as_ref().try_into().context("invalid tx_hash length")?;
            withdrawals.push((tx_hash, value.to_vec()));
        }

        Ok(withdrawals)
    }

    // =========================================================================
    // Deposit Indexer Methods
    // =========================================================================

    /// Check if a deposit has already been processed (by L1 sequence number)
    pub fn is_deposit_processed(&self, l1_seq: u64) -> Result<bool> {
        let cf = self
            .db
            .cf_handle(CF_PROCESSED_DEPOSITS)
            .context("processed_deposits CF missing")?;

        Ok(self.db.get_cf(cf, l1_seq.to_be_bytes())?.is_some())
    }

    /// Mark a deposit as processed
    pub fn mark_deposit_processed(&self, l1_seq: u64, slot: u64) -> Result<()> {
        let cf = self
            .db
            .cf_handle(CF_PROCESSED_DEPOSITS)
            .context("processed_deposits CF missing")?;

        // Store the slot at which the deposit was processed
        self.db
            .put_cf(cf, l1_seq.to_be_bytes(), slot.to_be_bytes())?;
        Ok(())
    }

    /// Get the last processed L1 slot for the deposit indexer
    pub fn get_last_processed_slot(&self) -> Result<Option<u64>> {
        let cf = self
            .db
            .cf_handle(CF_INDEXER_META)
            .context("indexer_meta CF missing")?;

        match self.db.get_cf(cf, b"last_processed_slot")? {
            Some(bytes) => {
                let arr: [u8; 8] = bytes.as_slice().try_into().context("invalid slot length")?;
                Ok(Some(u64::from_be_bytes(arr)))
            }
            None => Ok(None),
        }
    }

    /// Set the last processed L1 slot for the deposit indexer
    pub fn set_last_processed_slot(&self, slot: u64) -> Result<()> {
        let cf = self
            .db
            .cf_handle(CF_INDEXER_META)
            .context("indexer_meta CF missing")?;

        self.db
            .put_cf(cf, b"last_processed_slot", slot.to_be_bytes())?;
        Ok(())
    }

    // =========================================================================
    // Batch Operations
    // =========================================================================

    /// Atomically apply a batch of operations
    pub fn apply_batch(&self, operations: DbBatch) -> Result<()> {
        let mut batch = WriteBatch::default();

        let cf_accounts = self
            .db
            .cf_handle(CF_ACCOUNTS)
            .context("accounts CF missing")?;
        let cf_nullifiers = self
            .db
            .cf_handle(CF_NULLIFIERS)
            .context("nullifiers CF missing")?;
        let cf_commitments = self
            .db
            .cf_handle(CF_COMMITMENTS)
            .context("commitments CF missing")?;
        let cf_enc_notes = self
            .db
            .cf_handle(CF_ENCRYPTED_NOTES)
            .context("encrypted_notes CF missing")?;

        // Account updates
        for (id, state) in &operations.account_updates {
            let bytes = wincode::serialize(state)?;
            batch.put_cf(cf_accounts, id.0, bytes);
        }

        // Nullifiers
        for nullifier in &operations.nullifiers {
            batch.put_cf(cf_nullifiers, &nullifier.0, []);
        }

        // Commitments
        for (position, commitment) in &operations.commitments {
            batch.put_cf(cf_commitments, position.to_be_bytes(), commitment);
        }

        // Encrypted notes (use serde_json, not wincode)
        for (commitment, note) in &operations.encrypted_notes {
            let bytes = serde_json::to_vec(note)?;
            batch.put_cf(cf_enc_notes, commitment, bytes);
        }

        self.db.write(batch)?;
        Ok(())
    }

    // =========================================================================
    // Batch Query Methods
    // =========================================================================

    /// Store a batch summary
    pub fn store_batch_summary(&self, summary: &BatchSummary) -> Result<()> {
        let cf = self
            .db
            .cf_handle(CF_BATCHES)
            .context("batches CF missing")?;

        let key = summary.batch_id.to_be_bytes();
        let value = serde_json::to_vec(summary)?;
        self.db.put_cf(cf, key, value)?;
        Ok(())
    }

    /// Get a batch summary by ID
    pub fn get_batch_summary(&self, batch_id: u64) -> Result<Option<BatchSummary>> {
        let cf = self
            .db
            .cf_handle(CF_BATCHES)
            .context("batches CF missing")?;

        match self.db.get_cf(cf, batch_id.to_be_bytes())? {
            Some(bytes) => {
                let summary: BatchSummary = serde_json::from_slice(&bytes)?;
                Ok(Some(summary))
            }
            None => Ok(None),
        }
    }

    /// List batches with pagination (newest first)
    pub fn list_batches(&self, offset: usize, limit: usize) -> Result<(Vec<BatchSummary>, usize)> {
        let cf = self
            .db
            .cf_handle(CF_BATCHES)
            .context("batches CF missing")?;

        let mut batches = Vec::new();
        let mut total = 0usize;

        // Iterate in reverse order (newest first)
        let iter = self.db.iterator_cf(cf, rocksdb::IteratorMode::End);

        for item in iter {
            let (_key, value) = item?;
            total += 1;

            // Skip until offset
            if total <= offset {
                continue;
            }

            // Collect up to limit
            if batches.len() < limit {
                let summary: BatchSummary = serde_json::from_slice(&value)?;
                batches.push(summary);
            }
        }

        Ok((batches, total))
    }

    /// Get the latest batch ID
    pub fn get_latest_batch_id(&self) -> Result<Option<u64>> {
        let cf = self
            .db
            .cf_handle(CF_BATCHES)
            .context("batches CF missing")?;

        let mut iter = self.db.iterator_cf(cf, rocksdb::IteratorMode::End);
        if let Some(Ok((key, _))) = iter.next() {
            let arr: [u8; 8] = key
                .as_ref()
                .try_into()
                .context("invalid batch_id key length")?;
            return Ok(Some(u64::from_be_bytes(arr)));
        }

        Ok(None)
    }

    /// Count total batches
    pub fn count_batches(&self) -> Result<u64> {
        let cf = self
            .db
            .cf_handle(CF_BATCHES)
            .context("batches CF missing")?;

        let mut count = 0u64;
        let iter = self.db.iterator_cf(cf, rocksdb::IteratorMode::Start);
        for _ in iter {
            count += 1;
        }

        Ok(count)
    }

    // =========================================================================
    // Transaction Index Methods
    // =========================================================================

    /// Store a transaction summary
    pub fn store_tx_summary(&self, tx_hash: &[u8; 32], summary: &TxSummary) -> Result<()> {
        let cf = self
            .db
            .cf_handle(CF_TX_INDEX)
            .context("tx_index CF missing")?;

        let value = serde_json::to_vec(summary)?;
        self.db.put_cf(cf, tx_hash, value)?;
        Ok(())
    }

    /// Get a transaction summary by hash
    pub fn get_tx_summary(&self, tx_hash: &[u8; 32]) -> Result<Option<TxSummary>> {
        let cf = self
            .db
            .cf_handle(CF_TX_INDEX)
            .context("tx_index CF missing")?;

        match self.db.get_cf(cf, tx_hash)? {
            Some(bytes) => {
                let summary: TxSummary = serde_json::from_slice(&bytes)?;
                Ok(Some(summary))
            }
            None => Ok(None),
        }
    }

    /// List transactions with pagination and optional filters (newest first)
    pub fn list_transactions(
        &self,
        offset: usize,
        limit: usize,
        batch_id_filter: Option<u64>,
        tx_type_filter: Option<TxType>,
        status_filter: Option<TxStatus>,
    ) -> Result<(Vec<TxSummary>, usize)> {
        let cf = self
            .db
            .cf_handle(CF_TX_INDEX)
            .context("tx_index CF missing")?;

        let mut transactions = Vec::new();

        // Iterate in reverse order (newest first based on key order)
        // Note: tx_hash keys aren't naturally time-ordered, so we collect all and sort
        let iter = self.db.iterator_cf(cf, rocksdb::IteratorMode::Start);

        let mut all_txs: Vec<TxSummary> = Vec::new();
        for item in iter {
            let (_key, value) = item?;
            let summary: TxSummary = serde_json::from_slice(&value)?;

            // Apply filters
            if let Some(bid) = batch_id_filter {
                if summary.batch_id != Some(bid) {
                    continue;
                }
            }
            if let Some(tt) = tx_type_filter {
                if summary.tx_type != tt {
                    continue;
                }
            }
            if let Some(st) = status_filter {
                if summary.status != st {
                    continue;
                }
            }

            all_txs.push(summary);
        }

        // Sort by received_at descending (newest first)
        all_txs.sort_by(|a, b| b.received_at.cmp(&a.received_at));

        let total = all_txs.len();

        // Apply pagination
        for tx in all_txs.into_iter().skip(offset).take(limit) {
            transactions.push(tx);
        }

        Ok((transactions, total))
    }

    /// Count total transactions
    pub fn count_transactions(&self) -> Result<u64> {
        let cf = self
            .db
            .cf_handle(CF_TX_INDEX)
            .context("tx_index CF missing")?;

        let mut count = 0u64;
        let iter = self.db.iterator_cf(cf, rocksdb::IteratorMode::Start);
        for _ in iter {
            count += 1;
        }

        Ok(count)
    }

    /// Update transaction status
    pub fn update_tx_status(
        &self,
        tx_hash: &[u8; 32],
        status: TxStatus,
        batch_id: Option<u64>,
    ) -> Result<()> {
        if let Some(mut summary) = self.get_tx_summary(tx_hash)? {
            summary.status = status;
            if let Some(bid) = batch_id {
                summary.batch_id = Some(bid);
            }
            if status == TxStatus::Executed || status == TxStatus::Settled {
                summary.executed_at = Some(
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs())
                        .unwrap_or(0),
                );
            }
            self.store_tx_summary(tx_hash, &summary)?;
        }
        Ok(())
    }

    // =========================================================================
    // Statistics Methods
    // =========================================================================

    /// Get global statistics
    pub fn get_global_stats(&self) -> Result<(u64, u64)> {
        let total_batches = self.count_batches()?;
        let total_transactions = self.count_transactions()?;
        Ok((total_batches, total_transactions))
    }

    /// Count active accounts (accounts with non-zero balance)
    pub fn count_active_accounts(&self) -> Result<u64> {
        let cf = self
            .db
            .cf_handle(CF_ACCOUNTS)
            .context("accounts CF missing")?;

        let mut count = 0u64;
        let iter = self.db.iterator_cf(cf, rocksdb::IteratorMode::Start);
        for item in iter {
            let (_key, value) = item?;
            let state: AccountState = wincode::deserialize(&value)?;
            if state.balance > 0 {
                count += 1;
            }
        }

        Ok(count)
    }

    // =========================================================================
    // Deposit/Withdrawal Statistics
    // =========================================================================

    /// Add to dev deposits total
    pub fn add_dev_deposit(&self, amount: u64) -> Result<()> {
        let cf = self.db.cf_handle(CF_STATS).context("stats CF missing")?;

        let current = self.get_dev_deposits_total()?;
        let new_total = current.saturating_add(amount);
        self.db
            .put_cf(cf, b"dev_deposits_total", new_total.to_be_bytes())?;
        Ok(())
    }

    /// Get dev deposits total
    pub fn get_dev_deposits_total(&self) -> Result<u64> {
        let cf = self.db.cf_handle(CF_STATS).context("stats CF missing")?;

        match self.db.get_cf(cf, b"dev_deposits_total")? {
            Some(bytes) => {
                let arr: [u8; 8] = bytes.as_slice().try_into().context("invalid u64 length")?;
                Ok(u64::from_be_bytes(arr))
            }
            None => Ok(0),
        }
    }

    /// Add to L1 deposits total
    pub fn add_l1_deposit(&self, amount: u64) -> Result<()> {
        let cf = self.db.cf_handle(CF_STATS).context("stats CF missing")?;

        let current = self.get_l1_deposits_total()?;
        let new_total = current.saturating_add(amount);
        self.db
            .put_cf(cf, b"l1_deposits_total", new_total.to_be_bytes())?;
        Ok(())
    }

    /// Get L1 deposits total
    pub fn get_l1_deposits_total(&self) -> Result<u64> {
        let cf = self.db.cf_handle(CF_STATS).context("stats CF missing")?;

        match self.db.get_cf(cf, b"l1_deposits_total")? {
            Some(bytes) => {
                let arr: [u8; 8] = bytes.as_slice().try_into().context("invalid u64 length")?;
                Ok(u64::from_be_bytes(arr))
            }
            None => Ok(0),
        }
    }

    /// Get total deposits (dev + L1)
    pub fn get_total_deposits(&self) -> Result<u64> {
        let dev = self.get_dev_deposits_total()?;
        let l1 = self.get_l1_deposits_total()?;
        Ok(dev.saturating_add(l1))
    }

    /// Add to withdrawals total
    pub fn add_withdrawal(&self, amount: u64) -> Result<()> {
        let cf = self.db.cf_handle(CF_STATS).context("stats CF missing")?;

        let current = self.get_withdrawals_total()?;
        let new_total = current.saturating_add(amount);
        self.db
            .put_cf(cf, b"withdrawals_total", new_total.to_be_bytes())?;
        Ok(())
    }

    /// Get withdrawals total
    pub fn get_withdrawals_total(&self) -> Result<u64> {
        let cf = self.db.cf_handle(CF_STATS).context("stats CF missing")?;

        match self.db.get_cf(cf, b"withdrawals_total")? {
            Some(bytes) => {
                let arr: [u8; 8] = bytes.as_slice().try_into().context("invalid u64 length")?;
                Ok(u64::from_be_bytes(arr))
            }
            None => Ok(0),
        }
    }
}

/// Batch of database operations for atomic commit
#[derive(Default)]
pub struct DbBatch {
    pub account_updates: Vec<(AccountId, AccountState)>,
    pub nullifiers: Vec<Nullifier>,
    pub commitments: Vec<(u32, [u8; 32])>,
    pub encrypted_notes: Vec<([u8; 32], EncryptedNote)>,
}

impl StateStore for RocksDbStore {
    fn get_account_state(&self, id: &AccountId) -> Result<AccountState> {
        let cf = self
            .db
            .cf_handle(CF_ACCOUNTS)
            .context("Column family 'accounts' missing")?;

        // Key is the 32-byte AccountId directly
        match self.db.get_cf(cf, id.0)? {
            Some(bytes) => {
                let state: AccountState = wincode::deserialize::<AccountState>(&bytes)?;
                Ok(state)
            }
            None => Ok(AccountState::default()), // Non-existent accounts have 0 balance
        }
    }

    fn set_account_state(&self, id: AccountId, state: AccountState) -> Result<()> {
        let cf = self
            .db
            .cf_handle(CF_ACCOUNTS)
            .context("Column family 'accounts' missing")?;

        let bytes = wincode::serialize(&state)?;

        self.db.put_cf(cf, id.0, bytes)?;
        Ok(())
    }
}
