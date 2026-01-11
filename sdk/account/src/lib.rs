use serde::{Deserialize, Serialize};
use std::fmt;
use wincode::{SchemaRead, SchemaWrite};

/// The state of an account.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize, SchemaRead, SchemaWrite,
)]
pub struct AccountState {
    pub balance: u64,
    pub nonce: u64,
}

/// The canonical identifier for a user on L2 (32 bytes).
/// Derived from H(SignerPK || PrivacyPK)
#[derive(
    Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, Default, SchemaWrite, SchemaRead,
)]
pub struct AccountId(pub [u8; 32]);

impl AccountId {
    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }
}

impl AsRef<[u8]> for AccountId {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl fmt::Debug for AccountId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "AccountId({})", self.to_hex())
    }
}

impl fmt::Display for AccountId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Example: if AccountId wraps [u8; 32]
        write!(f, "{}", hex::encode(&self.0))
    }
}
