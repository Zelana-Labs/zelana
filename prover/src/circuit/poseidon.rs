use ark_bls12_381::Fr;
use ark_crypto_primitives::sponge::poseidon::{PoseidonConfig, find_poseidon_ark_and_mds};

/// Poseidon configuration for ZELANA
///
/// Field: BLS12-381 Fr (255 bits)
/// Rate: 2
/// Capacity: 1
/// Security: 128 bits
pub fn poseidon_config() -> PoseidonConfig<Fr> {
    // === Poseidon parameter choices (standard) ===
    let prime_bits: u64 = 255; // Fr is a 255-bit prime field
    let rate: usize = 2;
    let capacity: usize = 1;

    let full_rounds: u64 = 8;
    let partial_rounds: u64 = 57;

    // alpha = 5 is standard for Poseidon over large prime fields
    let alpha: u64 = 5;

    // number of matrices to skip (0 = use first)
    let skip_matrices: u64 = 0;

    let (ark, mds) = find_poseidon_ark_and_mds::<Fr>(
        prime_bits,
        rate,
        full_rounds,
        partial_rounds,
        skip_matrices,
    );

    PoseidonConfig::new(
        full_rounds as usize,
        partial_rounds as usize,
        alpha,
        mds,
        ark,
        rate,
        capacity,
    )
}
