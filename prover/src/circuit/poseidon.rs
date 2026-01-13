use ark_bn254::Fr;
use ark_crypto_primitives::sponge::poseidon::{PoseidonConfig, find_poseidon_ark_and_mds};

/// Poseidon configuration for ZELANA
///
/// Field: BN254 Fr (254 bits) - compatible with Solana's alt_bn128 syscalls
/// Rate: 2
/// Capacity: 1
/// Security: 128 bits
pub fn poseidon_config() -> PoseidonConfig<Fr> {
    // === Poseidon parameter choices (standard) ===
    let prime_bits: u64 = 254; // BN254 Fr is a 254-bit prime field
    let rate: usize = 2;
    let capacity: usize = 1;

    let full_rounds: u64 = 8;
    let partial_rounds: u64 = 56; // Adjusted for 254-bit field

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
