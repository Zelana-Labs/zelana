use anyhow::Result;
use log::{error, info};
use zelana_core::SignedTransaction;
use zelana_execution::BatchExecutor;
use zelana_sdk::L2Transaction;

use crate::db::RocksDbStore;

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

        //wrap as l2transaction
        let l2_tx = L2Transaction::Transfer(tx.clone());

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