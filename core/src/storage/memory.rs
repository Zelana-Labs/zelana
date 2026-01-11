use crate::storage::StateStore;
use anyhow::Result;
use blake3::Hasher;
use sha2::{Digest, Sha256};
use std::{cell::RefCell, collections::HashMap};
use zelana_account::{AccountId, AccountState};

/// Deterministic in-memory state store.
/// Used by the Prover and batch verification logic.
pub struct ZkMemStore {
    accounts: RefCell<HashMap<AccountId, AccountState>>,
}

impl ZkMemStore {
    // Initialize from sequencer-provided witness
    /// Witness MUST represent the full post-state of the block
    pub fn new(witness: HashMap<AccountId, AccountState>) -> Self {
        Self {
            accounts: RefCell::new(witness),
        }
    }

    // Computes the cryptographic commitment (state root)
    //
    // Logic:
    // Hash( Sort( [AccountId || balance || nonce] ) )
    //  pub fn compute_root(&self) -> [u8; 32] {
    //     let accounts = self.accounts.borrow();

    // let mut entries: Vec<_> = accounts.iter().collect();
    // entries.sort_by_key(|(id, _)| id.0);

    // let mut hasher = Sha256::new();
    // for (id, state) in entries {
    //     hasher.update(&id.0);
    //     hasher.update(&state.balance.to_be_bytes());
    //     hasher.update(&state.nonce.to_be_bytes());
    // }

    // hasher.finalize().into()
    // }
}

impl StateStore for ZkMemStore {
    fn get_account_state(&self, id: &AccountId) -> Result<AccountState> {
        Ok(self.accounts.borrow().get(id).cloned().unwrap_or_default())
    }

    fn set_account_state(&self, id: AccountId, state: AccountState) -> Result<()> {
        self.accounts.borrow_mut().insert(id, state);
        Ok(())
    }
}
