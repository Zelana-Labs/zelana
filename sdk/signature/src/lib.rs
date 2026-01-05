// zelana_signature/src/lib.rs
use wincode::{SchemaRead, SchemaWrite};


#[derive(Clone, Copy, PartialEq, Debug, Eq, Hash, SchemaRead, SchemaWrite)]
pub struct Signature(pub [u8; 64]);  // ✅ Changed from 32 to 64

impl Signature {
    pub const LEN: usize = 64;  // ✅ Changed from 32 to 64

    pub fn as_bytes(&self) -> &[u8; 64] {
        &self.0
    }
    
    pub fn from_bytes(bytes: &[u8; 64]) -> Self {
        Self(*bytes)
    }
}

impl Default for Signature {
    fn default() -> Self {
        Self([0u8; 64])
    }
}

impl AsRef<[u8]> for Signature {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl From<[u8; 64]> for Signature {
    fn from(bytes: [u8; 64]) -> Self {
        Self(bytes)
    }
}

impl TryFrom<&[u8]> for Signature {
    type Error = std::array::TryFromSliceError;
    
    fn try_from(slice: &[u8]) -> Result<Self, Self::Error> {
        let bytes: [u8; 64] = slice.try_into()?;
        Ok(Self(bytes))
    }
}

impl TryFrom<Vec<u8>> for Signature {
    type Error = String;
    
    fn try_from(vec: Vec<u8>) -> Result<Self, Self::Error> {
        if vec.len() != 64 {
            return Err(format!("Invalid signature length: expected 64, got {}", vec.len()));
        }
        let mut bytes = [0u8; 64];
        bytes.copy_from_slice(&vec);
        Ok(Self(bytes))
    }
}