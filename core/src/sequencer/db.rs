use crate::storage::StateStore;
use anyhow::{Context, Result};
use rocksdb::{ColumnFamilyDescriptor, DB, Options, WriteBatch};
use zelana_block::BlockHeader;
use std::path::Path;
use std::sync::Arc;
use zelana_account::{AccountId, AccountState};
use zelana_transaction::{Transaction};

const CF_ACCOUNTS: &str = "accounts";
const CF_TX_BLOBS: &str = "tx_blobs";
const CF_BLOCKS: &str = "blocks";
const CF_NULLIFIERS: &str = "nullifiers";

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
            ColumnFamilyDescriptor::new(CF_TX_BLOBS, Options::default())
        ];

        let db = DB::open_cf_descriptors(&opts, path, families)
            .map_err(|e| anyhow::anyhow!("Failed to open RocksDB: {}", e))?;

        Ok(Self { db: Arc::new(db) })
    }

    ///Check if Nullifier has already been used
    pub fn nullifier_exists(&self, nullifier: &[u8]) -> Result<bool> {
    let cf = self.db.cf_handle(CF_NULLIFIERS)
        .context("Nullifiers column family not found")?;
    
    Ok(self.db.get_cf(cf, nullifier)?.is_some())
    }
    
    pub fn add_encrypted_tx(
        &self,
        tx_hash: [u8; 32],
        blob: Vec<u8>,
    ) -> Result<()> {
        let cf = self
            .db
            .cf_handle(CF_TX_BLOBS)
            .context("tx_blobs CF missing")?;

        self.db.put_cf(cf, tx_hash, blob)?;
        Ok(())
    }

    pub fn store_block_header(&self, header: BlockHeader) -> Result<()> {
        let cf = self
            .db
            .cf_handle(CF_BLOCKS)
            .context("blocks CF missing")?;

        let key = header.batch_id.to_be_bytes();
        let value = wincode::serialize(&header)?;

        self.db.put_cf(cf, key, value)?;
        Ok(())
    }

    pub fn get_latest_state_root(&self) -> Result<[u8; 32]> {
        let cf = self
            .db
            .cf_handle(CF_BLOCKS)
            .context("blocks CF missing")?;

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
    // ///Atomically persists a tx and marks the nullifier usage
    // pub fn add_transaction(&self,tx: Transaction) -> Result<()>{
    //     let cf_txs = self.db.cf_handle(CF_TRANSACTIONS).context("No txs CF")?;
    //     let cf_nullifiers = self.db.cf_handle(CF_NULLIFIERS).context("No nullifiers CF")?;

    //     let mut batch = WriteBatch::default();

    //     batch.put_cf(cf_txs, tx.signature.as_ref(), wincode::serialize(&tx)?);

    //     tx.tx_type.apply_storage_effects(&mut batch, cf_nullifiers);

    //     self.db.write(batch)?;

    //     Ok(())
    // }
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
