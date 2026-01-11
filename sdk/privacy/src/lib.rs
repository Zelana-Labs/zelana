//! Zelana Privacy SDK
//!
//! Zcash-style note-based privacy primitives for shielded transactions.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                    Shielded Transaction                         │
//! │  ┌──────────────┐  ┌──────────────┐  ┌───────────────────────┐ │
//! │  │  Nullifiers  │  │ Commitments  │  │   Encrypted Output    │ │
//! │  │  (spent)     │  │  (new notes) │  │   (for recipient)     │ │
//! │  └──────────────┘  └──────────────┘  └───────────────────────┘ │
//! │         │                 │                     │               │
//! │         ▼                 ▼                     ▼               │
//! │  ┌─────────────────────────────────────────────────────────┐   │
//! │  │              ZK Proof (Groth16)                          │   │
//! │  │  • Valid nullifier derivation                            │   │
//! │  │  • Valid commitment structure                            │   │
//! │  │  • Balance preservation: Σ inputs = Σ outputs            │   │
//! │  └─────────────────────────────────────────────────────────┘   │
//! └─────────────────────────────────────────────────────────────────┘
//! ```

pub mod commitment;
pub mod encryption;
pub mod merkle;
pub mod note;
pub mod nullifier;

pub use commitment::{Commitment, CommitmentScheme};
pub use encryption::{EncryptedNote, decrypt_note, encrypt_note, try_decrypt_note};
pub use merkle::{MerkleHasher, MerklePath, MerkleTree, RootHistory, TREE_DEPTH};
pub use note::{Note, NoteValue, ShieldedKeyBundle, SpendingKey, ViewingKey};
pub use nullifier::{Nullifier, NullifierKey};
