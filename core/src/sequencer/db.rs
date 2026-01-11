use crate::sequencer::shielded_state::TreeFrontier;
use crate::storage::StateStore;
use anyhow::{Context, Result};
use rocksdb::{ColumnFamilyDescriptor, DB, Options, WriteBatch};
use std::path::Path;
use std::sync::Arc;
use zelana_account::{AccountId, AccountState};
use zelana_block::BlockHeader;
use zelana_privacy::{EncryptedNote, Nullifier, TREE_DEPTH};

const CF_ACCOUNTS: &str = "accounts";
const CF_TX_BLOBS: &str = "tx_blobs";
const CF_BLOCKS: &str = "blocks";
const CF_NULLIFIERS: &str = "nullifiers";
const CF_COMMITMENTS: &str = "commitments";
const CF_ENCRYPTED_NOTES: &str = "encrypted_notes";
const CF_WITHDRAWALS: &str = "withdrawals";
const CF_TREE_META: &str = "tree_meta";

/// A thread-safe wrapper around RocksDB.
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
