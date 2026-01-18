#![allow(dead_code)] // Legacy executor - superseded by tx_router.rs
use crate::sequencer::storage::db::RocksDbStore;
use crate::storage::state::StateStore;
use anyhow::Result;
use log::info;
use sha2::{Digest, Sha256};
use std::{collections::HashMap, sync::Arc};
use zelana_account::{AccountId, AccountState};
use zelana_transaction::{SignedTransaction, TransactionData};

/// Compute the state root from a set of accounts
pub fn compute_state_root(base_state: &HashMap<AccountId, AccountState>) -> [u8; 32] {
    let mut items: Vec<_> = base_state.iter().collect();
    items.sort_by_key(|(id, _)| id.to_hex());

    let mut hasher = Sha256::new();
    for (id, st) in items {
        hasher.update(id.as_ref());
        hasher.update(&st.balance.to_be_bytes());
        hasher.update(&st.nonce.to_be_bytes());
    }

    hasher.finalize().into()
}
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

#[derive(Debug, Clone)]
pub struct ExecutionResult {
    pub tx_hash: [u8; 32],
    pub state_diff: StateDiff,
}

//in memory State cache
#[derive(Debug, Default, Clone)]
struct InMemoryState {
    accounts: HashMap<AccountId, AccountState>,
    touched: HashMap<AccountId, AccountState>,
}

impl InMemoryState {
    fn load_account(&mut self, db: &RocksDbStore, id: &AccountId) -> Result<AccountState> {
        info!(
            "CACHE LOOKUP: id={}, has_cached={}",
            id.to_hex(),
            self.accounts.contains_key(id)
        );

        if let Some(st) = self.accounts.get(id) {
            return Ok(st.clone());
        }

        // Load from DB, but DO NOT cache
        Ok(db.get_account_state(id).unwrap_or_default())
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

impl Executor {
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
    pub fn execute_signed_tx(
        &mut self,
        tx: SignedTransaction,
        tx_hash: [u8; 32],
    ) -> Result<ExecutionResult, ExecutionError> {
        info!(
            "EXEC CACHE DEBUG: accounts={}, touched={}",
            self.state.accounts.len(),
            self.state.touched.len()
        );

        let TransactionData {
            to, amount, nonce, ..
        } = tx.data;

        let from = AccountId(tx.signer_pubkey);

        //load sender and receiver state
        let mut from_state = self
            .state
            .load_account(&self.db, &from)
            .map_err(|_| ExecutionError::AccountNotFound)?;

        if from == to {
            // Self-transfer: only nonce changes
            if from_state.balance < amount {
                return Err(ExecutionError::InsufficientBalance);
            }
            if from_state.nonce != nonce {
                return Err(ExecutionError::InvalidNonce);
            }

            from_state.nonce += 1;

            self.state.set_account(from, from_state);
        } else {
            let mut to_state = self
                .state
                .load_account(&self.db, &to)
                .map_err(|_| ExecutionError::AccountNotFound)?;

            if from_state.balance < amount {
                return Err(ExecutionError::InsufficientBalance);
            }
            if from_state.nonce != nonce {
                return Err(ExecutionError::InvalidNonce);
            }

            from_state.balance -= amount;
            from_state.nonce += 1;
            to_state.balance += amount;

            self.state.set_account(from, from_state);
            self.state.set_account(to, to_state);
        }

        Ok(ExecutionResult {
            tx_hash,
            state_diff: self.state.diff(),
        })
    }

    /// Reset in-memory cache (after block finalization)
    pub fn reset(&mut self) {
        self.state.accounts.clear();
        self.state.touched.clear();
    }
    pub fn state_root(&self) -> [u8; 32] {
        compute_state_root(&self.state.accounts)
    }
    pub fn apply_state_diff(&mut self) -> Result<(), anyhow::Error> {
        // Apply all touched account states to persistent storage
        let diff = self.state.diff();

        for (id, state) in diff.updates {
            self.db.set_account_state(id, state)?;
        }

        Ok(())
    }
}
