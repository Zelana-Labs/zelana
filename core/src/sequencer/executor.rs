use anyhow::{Result, bail};
use log::{error, info};
use zelana_account::{AccountId, AccountState};
use zelana_transaction::{SignedTransaction, TransactionType,Transaction,TransactionData};
use std::{collections::HashMap, sync::Arc};
use super::db::RocksDbStore;
use crate::storage::state::StateStore;
// use crate::storage::BatchExecutor;

#[derive(Clone)]
pub struct Executor {
    db: Arc<RocksDbStore>,
    state: InMemoryState,
}


#[derive(Debug)]
pub enum ExecutionError {
    InsufficientBalance,
    InvalidNonce,
    AccountNotFound,
    InvalidTransaction,
}


#[derive(Debug, Clone)]
pub struct StateDiff {
    /// Updated account states
    pub updates: HashMap<AccountId, AccountState>,
}

#[derive(Debug,Clone)]
pub struct ExecutionResult {
    pub tx_hash: [u8; 32],
    pub state_diff: StateDiff,
}

//in memory State cache
#[derive(Debug, Default,Clone)]
struct InMemoryState {
    accounts: HashMap<AccountId, AccountState>,
    touched: HashMap<AccountId, AccountState>,
}


impl InMemoryState {
    fn load_account(
        &mut self,
        db: &RocksDbStore,
        id: &AccountId,
    ) -> Result<AccountState> {
        if let Some(st) = self.accounts.get(id) {
            return Ok(st.clone());
        }

        let st = db.get_account_state(id).unwrap_or_default();
        self.accounts.insert(*id, st.clone());
        Ok(st)
    }

    fn set_account(&mut self, id: AccountId, state: AccountState) {
        self.accounts.insert(id, state.clone());
        self.touched.insert(id, state);
    }

    fn diff(&self) -> StateDiff {
        StateDiff {
            updates: self.touched.clone(),
        }
    }
}


impl Executor{
    pub fn new(db: Arc<RocksDbStore>) -> Self {
        Self {
            db,
            state: InMemoryState::default(),
        }
    }

    /// Execute a decrypted SignedTransaction
    ///
    /// - Mutates ONLY in-memory state
    /// - Returns a StateDiff
    /// - Does NOT commit to DB
    pub fn execute_signed_tx(
        &mut self,
        tx: SignedTransaction,
        tx_hash: [u8;32]
    )->Result<ExecutionResult,ExecutionError>{
        let TransactionData {
            to,
            amount,
            nonce,
            ..
        } = tx.data;

        let from = AccountId(tx.signer_pubkey);
        
        //load sender and receiver state
        let mut from_state = self
            .state
            .load_account(&self.db, &from)
            .map_err(|_| ExecutionError::AccountNotFound)?;

        let mut to_state = self
            .state
            .load_account(&self.db, &to)
            .map_err(|_| ExecutionError::AccountNotFound)?;

        //validate nonce
        if from_state.nonce != nonce{
            return Err(ExecutionError::InvalidNonce);
        }

        //validate balance
        if from_state.balance < amount{
            return Err(ExecutionError::InsufficientBalance);
        }

        from_state.balance -= amount;
        from_state.nonce += 1;

        to_state.balance += amount;

        self.state.set_account(from, from_state);
        self.state.set_account(to, to_state);

        Ok(ExecutionResult {
            tx_hash,
            state_diff: self.state.diff(),
        })
    }

    /// Apply a state diff to persistent storage
    ///
    /// Later this must be gated behind:
    /// - block finalization
    /// - prover success
     pub fn apply_state_diff(&self, diff: StateDiff) -> Result<()> {
        for (id, state) in diff.updates {
            self.db.set_account_state(id, state)?;
        }
        Ok(())
    }
    /// Reset in-memory cache (after block finalization)
    pub fn reset(&mut self) {
        self.state = InMemoryState::default();
    }
}

// pub struct TransactionExecutor {
//     pub db: RocksDbStore,
// }



// impl TransactionExecutor {
//     pub fn new(db_path: &str) -> Result<Self> {
//         let db = RocksDbStore::open(db_path)?;
//         Ok(Self { db })
//     }
//     /// Takes a signed transaction, validates logic, and persists to DB.
//     pub  fn process(&self, tx: SignedTransaction) -> anyhow::Result<()> {
//         // SVM Execution
//         // 1. Load Account
//         // 2. Check Balance
//         // 3. Update State

//         //wrap in the execution engin
//         let mut executor = BatchExecutor::new(&mut self.db);

//         //wrap as TransactionType
//         let l2_tx = Transaction {
//         sender: tx.data., 
//         tx_type: TransactionType::Transfer(tx),
//         signature: tx.signature.clone(),
//     };


//         match executor.execute(&l2_tx) {
//             Ok(_) => {
//                 info!(
//                     "COMMITTED: {} -> {} | Amt: {}",
//                     tx.data.from.to_hex(),
//                     tx.data.to.to_hex(),
//                     tx.data.amount
//                 );
//                 Ok(())
//             }
//             Err(e) => {
//                 error!("REVERTED: {}", e);
//                 Err(e)
//             }
//         }
//     }
// }
