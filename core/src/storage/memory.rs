use crate::storage::StateStore;
use anyhow::Result;
use blake3::Hasher;
use std::collections::HashMap;
use zelana_account::{AccountId, AccountState};

/// A lightweight, verifiable state store.
/// Used by the Prover (Guest) AND the Batch Generator (Host).
pub struct ZkMemStore {
    accounts: HashMap<AccountId, AccountState>,
}

impl ZkMemStore {
    /// Initialize from the witness data provided by the Sequencer.
    pub fn new(witness: HashMap<AccountId, AccountState>) -> Self {
        let mut accounts = HashMap::new();
        for (id, data) in witness {
            accounts.insert(
                id,
                AccountState {
                    balance: data.balance,
                    nonce: data.nonce,
                },
            );
        }
        Self { accounts }
    }

    /// Computes the cryptographic commitment (Root) of the current state.
    /// Logic: Hash( Sort( [ID || Balance || Nonce] ) )
    pub fn compute_root(&self) -> [u8; 32] {
        // 1. Collect all entries
        let mut entries: Vec<(&AccountId, &AccountState)> = self.accounts.iter().collect();

        // 2. Sort by ID (Critical for determinism)
        entries.sort_by_key(|(id, _)| id.0);

        // 3. Hash them all
        let mut hasher = Hasher::new();
        for (id, state) in entries {
            hasher.update(&id.0);
            hasher.update(&state.balance.to_le_bytes());
            hasher.update(&state.nonce.to_le_bytes());
        }

        hasher.finalize().into()
    }
}

impl StateStore for ZkMemStore {
    fn get_account_state(&self, id: &AccountId) -> Result<AccountState> {
        Ok(self.accounts.get(id).cloned().unwrap_or_default())
    }

    fn set_account_state(&mut self, id: AccountId, state: AccountState) -> Result<()> {
        self.accounts.insert(id, state);
        Ok(())
    }
}
