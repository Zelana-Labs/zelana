#![allow(dead_code)] // Trait for future state abstraction
use anyhow::Result;
use zelana_account::AccountId;
use zelana_account::AccountState;

/// decoupling logic from the db
pub trait StateStore {
    /// Retrieve an account. Returns Default if not found.
    fn get_account_state(&self, id: &AccountId) -> Result<AccountState>;

    /// Update an account's state.
    fn set_account_state(&self, id: AccountId, state: AccountState) -> Result<()>;
}
