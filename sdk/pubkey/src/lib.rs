use serde::{Serialize, Deserialize};

#[derive(Clone,Copy,PartialEq,Eq,Hash,Debug,PartialOrd,Ord,Serialize,Deserialize)]
pub struct Pubkey(pub [u8;32]);