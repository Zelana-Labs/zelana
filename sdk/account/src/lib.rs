use serde::{Serialize, Deserialize};

/// The state of an account.
#[derive(Clone,Debug,PartialEq,Serialize,Deserialize)]
pub struct Account{
    pub balance:u64,
    pub nonce: u64
}