//! # Prover Worker Library
//!
//! Provides distributed proving functionality for Zelana L2.
//!
//! ## Modules
//!
//! - `prover` - Noir circuit proving (nargo + sunspot)
//! - `mimc` - MiMC hash implementation matching circuit

pub mod mimc;
pub mod prover;

// Re-export ark_bn254::Fr for coordinator
pub use ark_bn254::Fr;

pub use mimc::{
    MiMC, ShieldedData, TransferData, WithdrawalData, compute_batch_hash, compute_withdrawal_root,
    field_to_hex, hex_to_field,
};
pub use prover::{
    BatchInputs, ChunkInputs, MAX_SHIELDED, MAX_TRANSFERS, MAX_WITHDRAWALS, MERKLE_DEPTH,
    MockProver, NoirProver, ProofResult, ProverError, ShieldedWitness, TransferWitness,
    WithdrawalWitness,
};
