use rocksdb::{DB,Options,ColumnFamilyDescriptor};
use std::path::Path;
use std::sync::Arc;
use wincode::deserialize;
use anyhow::{Result,Context};
use zelana_core::AccountId;
use zelana_execution::{StateStore,AccountState};

const CF_ACCOUNTS: &str = "accounts";

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
            ColumnFamilyDescriptor::new(CF_ACCOUNTS, cf_opts),
        ];

        let db = DB::open_cf_descriptors(&opts, path, families)
            .map_err(|e| anyhow::anyhow!("Failed to open RocksDB: {}", e))?;

        Ok(Self {
            db: Arc::new(db),
        })
    }
}

impl StateStore for RocksDbStore {
    fn get_account(&self, id: &AccountId) -> Result<AccountState> {
        let cf = self.db.cf_handle(CF_ACCOUNTS)
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

    fn set_account(&mut self, id: AccountId, state: AccountState) -> Result<()> {
        let cf = self.db.cf_handle(CF_ACCOUNTS)
            .context("Column family 'accounts' missing")?;

        let bytes = wincode::serialize(&state)?;
        
        self.db.put_cf(cf, id.0, bytes)?;
        Ok(())
    }
}