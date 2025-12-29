use crate::storage::StateStore;
use anyhow::{Context, Result};
use rocksdb::{ColumnFamilyDescriptor, DB, Options, WriteBatch};
use std::path::Path;
use std::sync::Arc;
use zelana_account::{AccountId, AccountState};
use zelana_transaction::{Transaction, TransactionType};

const CF_ACCOUNTS: &str = "accounts";
const CF_TRANSACTIONS: &str = "transactions";
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

        let cf_opts = Options::default();
        let families = vec![
            ColumnFamilyDescriptor::new(CF_ACCOUNTS, Options::default()),
            ColumnFamilyDescriptor::new(CF_TRANSACTIONS, Options::default()),
            ColumnFamilyDescriptor::new(CF_NULLIFIERS, Options::default()),
        ];

        let db = DB::open_cf_descriptors(&opts, path, families)
            .map_err(|e| anyhow::anyhow!("Failed to open RocksDB: {}", e))?;

        Ok(Self { db: Arc::new(db) })
    }

    ///Check if Nullifier has already been used
    pub fn nullifier_exists(&self,nullifier: &[u8])->bool{
        if let Ok(cf) = self.db.cf_handle(CF_NULLIFIERS).context("CF missing") {
            self.db.get_cf(cf, nullifier).unwrap_or(None).is_some()
        } else {
            false
        }
    }

    ///Atomically persists a tx and marks the nullifier usage
    pub fn add_transaction(&self,tx: Transaction) -> Result<()>{
        let cf_txs = self.db.cf_handle(CF_TRANSACTIONS).context("No txs CF")?;
        let cf_nullifiers = self.db.cf_handle(CF_NULLIFIERS).context("No nullifiers CF")?;

        let tx_bytes = wincode::serialize(&tx)?;
        let tx_id_key = tx.signature.as_ref();

        let mut batch = WriteBatch::default();
        batch.put_cf(cf_txs, tx_id_key, tx_bytes);

        // handle privacy logic (nullifiers)

        if let TransactionType::Shielded(ref blob) = tx.tx_type{
            // Mark the nullifier as spent!
            // Value = 1 (just existence check)
            batch.put_cf(cf_nullifiers, &blob.nullifier, b"1");
        }
        self.db.write(batch)?;
        Ok(())
    }
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

    fn set_account_state(&mut self, id: AccountId, state: AccountState) -> Result<()> {
        let cf = self
            .db
            .cf_handle(CF_ACCOUNTS)
            .context("Column family 'accounts' missing")?;

        let bytes = wincode::serialize(&state)?;

        self.db.put_cf(cf, id.0, bytes)?;
        Ok(())
    }
}
