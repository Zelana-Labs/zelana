use serde::{Deserialize, Serialize};
use wincode::{SchemaRead, SchemaWrite};

#[derive(
    Clone, Copy, PartialEq, Debug, Eq, Hash, SchemaRead, SchemaWrite, Serialize, Deserialize,
)]
pub struct Signature(pub [u8; 32]);

impl Signature {
    pub const LEN: usize = 32;

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl AsRef<[u8]> for Signature {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}
