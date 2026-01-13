// crates/zelana-prover/src/prover_input.rs

use ark_bn254::Fr;

use crate::witness::WitnessTx;

/// Public inputs fed to the Groth16 verifier (
#[derive(Clone, Debug)]
pub struct PublicInputs {
    pub prev_state_root: Fr,
    pub new_state_root: Fr,
    pub batch_hash: Fr,
}

/// Full prover input (off-circuit)
#[derive(Clone, Debug)]
pub struct ProverInput {
    pub public: PublicInputs,
    /// Always exactly MAX_TXS elements (real txs + padding)
    pub txs: Vec<WitnessTx>,
}
