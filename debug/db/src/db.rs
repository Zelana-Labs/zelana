use anyhow::{Context, Result};
use hex;
use rocksdb::{ColumnFamilyDescriptor, DB, IteratorMode, Options};
use std::path::PathBuf;
use zelana_account::AccountState;
use zelana_transaction::Transaction;

const CF_ACCOUNTS: &str = "accounts";
const CF_TRANSACTIONS: &str = "transactions";
const CF_NULLIFIERS: &str = "nullifiers";

pub fn load_database(
    db_path: &str,
) -> Result<(
    Vec<(String, AccountState)>,
    Vec<(String, Transaction)>,
    Vec<String>,
)> {
    let rocks_path = PathBuf::from(db_path);
    let mut opts = Options::default();
    opts.create_if_missing(false);
    opts.create_missing_column_families(false);
    let cf_opts = Options::default();

    let db = DB::open_cf_descriptors_read_only(
        &opts,
        &rocks_path,
        vec![
            ColumnFamilyDescriptor::new(CF_ACCOUNTS, cf_opts.clone()),
            ColumnFamilyDescriptor::new(CF_TRANSACTIONS, cf_opts.clone()),
            ColumnFamilyDescriptor::new(CF_NULLIFIERS, cf_opts),
        ],
        false,
    )
    .or_else(|_| {
        let secondary_path = PathBuf::from(format!("{}_secondary", db_path));
        DB::open_cf_as_secondary(
            &opts,
            &rocks_path,
            &secondary_path,
            &[CF_ACCOUNTS, CF_TRANSACTIONS, CF_NULLIFIERS],
        )
        .context("Failed to open secondary DB")
    })?;

    // Load accounts
    let mut accounts = Vec::new();
    if let Some(cf) = db.cf_handle(CF_ACCOUNTS) {
        for entry in db.iterator_cf(&cf, IteratorMode::Start) {
            let (key_bytes, value_bytes) = entry?;
            if key_bytes.len() == 32 {
                let account_hex = hex::encode(&key_bytes);
                if let Ok(state) = wincode::deserialize::<AccountState>(&value_bytes) {
                    accounts.push((account_hex, state));
                }
            }
        }
    }
    accounts.sort_by(|a, b| b.1.balance.cmp(&a.1.balance));

    // Load transactions
    let mut transactions = Vec::new();
    if let Some(cf) = db.cf_handle(CF_TRANSACTIONS) {
        for entry in db.iterator_cf(&cf, IteratorMode::Start) {
            let (key_bytes, value_bytes) = entry?;
            let tx_id = hex::encode(&key_bytes);
            if let Ok(tx) = wincode::deserialize::<Transaction>(&value_bytes) {
                transactions.push((tx_id, tx));
            }
        }
    }
    transactions.reverse();

    // Load nullifiers
    let mut nullifiers = Vec::new();
    if let Some(cf) = db.cf_handle(CF_NULLIFIERS) {
        for entry in db.iterator_cf(&cf, IteratorMode::Start) {
            let (key_bytes, _) = entry?;
            nullifiers.push(hex::encode(&key_bytes));
        }
    }
    nullifiers.reverse();

    Ok((accounts, transactions, nullifiers))
}
