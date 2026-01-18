pub mod constants;
pub mod prover_inputs;
pub mod witness;
pub mod witness_builder;

pub mod circuit;
pub mod l2_circuit;

// Re-export key types for external usage
pub use l2_circuit::{
    L2BlockCircuit, PubkeyBytes, ShieldedCommitmentWitness, TransactionWitness, WithdrawalWitness,
    get_poseidon_config,
};
