use zelana_account::AccountId;
use wincode_derive::{SchemaRead, SchemaWrite};
use anyhow::Result;
use zelana_account::AccountState;

/// decoupling logic from the db 
pub trait StateStore {
    /// Retrieve an account. Returns Default if not found.
    fn get_account(&self, id: &AccountId) -> Result<AccountState>;
    
    /// Update an account's state.
    fn set_account(&mut self, id: AccountId, state: AccountState) -> Result<()>;
}