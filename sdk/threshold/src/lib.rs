//! Zelana Threshold Encryption
//!
//! Implements threshold encryption for MEV-resistant encrypted mempool.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                    Encrypted Mempool Flow                        │
//! │                                                                  │
//! │  1. User                    2. Sequencer            3. Committee │
//! │  ┌──────────┐              ┌──────────────┐        ┌──────────┐ │
//! │  │ Encrypt  │──encrypted──▶│   Order      │──────▶ │ Decrypt  │ │
//! │  │ to K-of-N│   tx blob    │   (blind)    │ after  │ (K-of-N) │ │
//! │  └──────────┘              └──────────────┘ order  └──────────┘ │
//! │                                                                  │
//! │  Benefits:                                                       │
//! │  • MEV resistance (can't extract value from encrypted txs)      │
//! │  • Front-running prevention (order is fixed before decrypt)     │
//! │  • Censorship resistance (can't selectively censor)             │
//! └─────────────────────────────────────────────────────────────────┘
//! ```

pub mod committee;
pub mod encrypted_tx;
pub mod shares;

pub use committee::{Committee, CommitteeConfig, CommitteeMember, LocalCommitteeMember};
pub use encrypted_tx::{
    DecryptionCoordinator, EncryptedMempool, EncryptedTransaction, OrderedEncryptedTx,
    decrypt_transaction, encrypt_for_committee,
};
pub use shares::{Share, ShareId, combine_shares, split_secret};
