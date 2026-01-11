use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};
use serde::Serialize;
use std::io::{Cursor, Read, Write};
use wincode::{SchemaRead, SchemaWrite};

// Block header
pub const HEADER_MAGIC: [u8; 4] = *b"ZLNA";
pub const HEADER_VERSION: u16 = 1;
pub const HEADER_SIZE: usize = 96;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, SchemaRead, SchemaWrite)]
pub struct BlockHeader {
    #[serde(with = "hex::serde")]
    pub magic: [u8; 4],
    pub hdr_version: u16,
    pub batch_id: u64,
    #[serde(with = "hex::serde")]
    pub prev_root: [u8; 32],
    #[serde(with = "hex::serde")]
    pub new_root: [u8; 32],
    pub tx_count: u32,
    pub open_at: u64,
    pub flags: u32,
}
impl BlockHeader {
    pub fn to_bytes(&self) -> Result<[u8; HEADER_SIZE], std::io::Error> {
        let mut bytes = [0u8; HEADER_SIZE];
        let mut cursor = Cursor::new(&mut bytes[..]);
        cursor.write_all(&self.magic)?;
        cursor.write_u16::<BigEndian>(self.hdr_version)?;
        cursor.write_u16::<BigEndian>(0)?; // Reserved
        cursor.write_u64::<BigEndian>(self.batch_id)?;
        cursor.write_all(&self.prev_root)?;
        cursor.write_all(&self.new_root)?;
        cursor.write_u32::<BigEndian>(self.tx_count)?;
        cursor.write_u64::<BigEndian>(self.open_at)?;
        cursor.write_u32::<BigEndian>(self.flags)?;

        Ok(bytes)
    }

    pub fn from_bytes(bytes: &[u8; HEADER_SIZE]) -> Result<Self, std::io::Error> {
        let mut cursor = Cursor::new(&bytes[..]);
        let mut magic = [0u8; 4];
        cursor.read_exact(&mut magic)?;

        let hdr_version = cursor.read_u16::<BigEndian>()?;
        cursor.read_u16::<BigEndian>()?;

        let batch_id = cursor.read_u64::<BigEndian>()?;
        let mut prev_root = [0u8; 32];
        cursor.read_exact(&mut prev_root)?;
        let mut new_root = [0u8; 32];
        cursor.read_exact(&mut new_root)?;
        let tx_count = cursor.read_u32::<BigEndian>()?;
        let open_at = cursor.read_u64::<BigEndian>()?;
        let flags = cursor.read_u32::<BigEndian>()?;
        Ok(Self {
            magic,
            hdr_version,
            batch_id,
            prev_root,
            new_root,
            tx_count,
            open_at,
            flags,
        })
    }
    pub fn genesis() -> Self {
        Self {
            magic: HEADER_MAGIC,
            hdr_version: HEADER_VERSION,
            batch_id: 0,
            prev_root: [0; 32],
            new_root: [0; 32],
            tx_count: 0,
            open_at: 0,
            flags: 0,
        }
    }
}
