use anyhow::{Result,bail};
use log::{error, info};
use wincode_derive::{SchemaRead, SchemaWrite};
use std::fmt;
use zelana_transaction::{SignedTransaction, TransactionData, TransactionType, DepositEvent, WithdrawRequest};
use zelana_account::{AccountId, AccountState};

use crate::storage::BatchExecutor;
use super::db::RocksDbStore;

pub struct TransactionExecutor {
   pub db:RocksDbStore
}

impl TransactionExecutor {
    pub fn new(db_path:&str) -> Result<Self> {
        let db = RocksDbStore::open(db_path)?;
        Ok(Self{db})
    }
    /// Takes a signed transaction, validates logic, and persists to DB.
    pub async fn process(&self, tx: SignedTransaction) -> anyhow::Result<()> {
        // SVM Execution
        // 1. Load Account
        // 2. Check Balance
        // 3. Update State  

        let mut store = self.db.clone();

        //wrap in the execution engin
        let mut executor = BatchExecutor::new(&mut store);
        
        //wrap as TransactionType
        let l2_tx = TransactionType::Transfer(tx.clone());

        match executor.execute(&l2_tx){
            Ok(_) => {
                info!(
                    "COMMITTED: {} -> {} | Amt: {}", 
                    tx.data.from.to_hex(), 
                    tx.data.to.to_hex(), 
                    tx.data.amount
                );
                Ok(())
            }
            Err(e) => {
                error!("REVERTED: {}", e);
                Err(e)
            }
        }
    }
}