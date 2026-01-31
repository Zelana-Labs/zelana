//! MiMC Hash Implementation
//!
//! This is a Rust implementation of the MiMC hash function that matches
//! the Noir circuit in `zelana-forge/circuits/zelana_lib/src/poseidon.nr`.
//!
//! IMPORTANT: This must produce identical outputs to the Noir circuit
//! for the ownership proof to verify correctly.
//!
//! The implementation uses:
//! - BN254 scalar field (Fr) - same as Noir's default
//! - x^7 permutation (standard MiMC)
//! - 91 rounds for 256-bit security
//! - Sponge construction with domain separation

use ark_bn254::Fr;

/// Number of MiMC rounds for ~256-bit security
const MIMC_ROUNDS: u32 = 91;

/// Domain separator for delegated proving ("DELE" = 0x44454c45)
pub fn delegate_domain() -> Fr {
    Fr::from(0x44454c45u64)
}

/// Domain separator for public key derivation ("PK" = 0x504b)
pub fn pk_domain() -> Fr {
    Fr::from(0x504bu64)
}

/// Domain separator for nullifiers
pub fn domain_nullifier() -> Fr {
    Fr::from(3u64)
}

// Re-export as constants for convenience (computed at call time)
pub const DELEGATE_DOMAIN: fn() -> Fr = delegate_domain;
pub const PK_DOMAIN: fn() -> Fr = pk_domain;

/// Compute round constant for round i
///
/// RC[i] = (i+1)^3 + (i+1)
///
/// This matches the Noir circuit exactly.
fn round_constant(i: u32) -> Fr {
    let idx = Fr::from((i + 1) as u64);
    let idx_cubed = idx * idx * idx;
    idx_cubed + idx
}

/// MiMC round function: x -> (x + k + c)^7
fn mimc_round(x: Fr, k: Fr, c: Fr) -> Fr {
    let t = x + k + c;
    let t2 = t * t; // t^2
    let t4 = t2 * t2; // t^4
    let t6 = t4 * t2; // t^6
    t6 * t // t^7
}

/// MiMC permutation: encrypts x with key k
fn mimc_permute(x: Fr, k: Fr) -> Fr {
    let mut state = x;
    for i in 0..MIMC_ROUNDS {
        let c = round_constant(i);
        state = mimc_round(state, k, c);
    }
    state + k // Final key addition
}

/// Sponge-based hash absorption
///
/// Uses capacity=1 field element for 128-bit security margin.
fn mimc_sponge_absorb(inputs: &[Fr], capacity: Fr) -> Fr {
    let mut state = capacity;

    // Absorb phase - XOR inputs into state and permute
    for input in inputs {
        state = mimc_permute(state + input, Fr::from(0u64));
    }

    state
}

/// Hash two field elements (used for Merkle tree pairs)
///
/// This matches `hash_2` in the Noir circuit.
pub fn hash_2(left: Fr, right: Fr) -> Fr {
    let domain = Fr::from(2u64);
    mimc_sponge_absorb(&[domain, left, right], Fr::from(0u64))
}

/// Hash three field elements
///
/// This matches `hash_3` in the Noir circuit.
pub fn hash_3(a: Fr, b: Fr, c: Fr) -> Fr {
    let domain = Fr::from(3u64);
    mimc_sponge_absorb(&[domain, a, b, c], Fr::from(0u64))
}

/// Hash four field elements
///
/// This matches `hash_4` in the Noir circuit.
pub fn hash_4(a: Fr, b: Fr, c: Fr, d: Fr) -> Fr {
    let domain = Fr::from(4u64);
    mimc_sponge_absorb(&[domain, a, b, c, d], Fr::from(0u64))
}

/// Hash five field elements
pub fn hash_5(a: Fr, b: Fr, c: Fr, d: Fr, e: Fr) -> Fr {
    let domain = Fr::from(5u64);
    mimc_sponge_absorb(&[domain, a, b, c, d, e], Fr::from(0u64))
}

/// Hash six field elements
pub fn hash_6(a: Fr, b: Fr, c: Fr, d: Fr, e: Fr, f: Fr) -> Fr {
    let domain = Fr::from(6u64);
    mimc_sponge_absorb(&[domain, a, b, c, d, e, f], Fr::from(0u64))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_deterministic() {
        let a = Fr::from(123u64);
        let b = Fr::from(456u64);

        let h1 = hash_2(a, b);
        let h2 = hash_2(a, b);

        assert_eq!(h1, h2, "hash should be deterministic");
    }

    #[test]
    fn test_hash_collision_resistant() {
        let h1 = hash_2(Fr::from(1u64), Fr::from(2u64));
        let h2 = hash_2(Fr::from(2u64), Fr::from(1u64));

        assert_ne!(h1, h2, "order should matter");
    }

    #[test]
    fn test_round_constant() {
        // RC[0] = 1^3 + 1 = 2
        assert_eq!(round_constant(0), Fr::from(2u64));
        // RC[1] = 2^3 + 2 = 10
        assert_eq!(round_constant(1), Fr::from(10u64));
        // RC[2] = 3^3 + 3 = 30
        assert_eq!(round_constant(2), Fr::from(30u64));
    }

    #[test]
    fn test_mimc_round() {
        let x = Fr::from(1u64);
        let k = Fr::from(2u64);
        let c = Fr::from(3u64);

        let result = mimc_round(x, k, c);

        // (1 + 2 + 3)^7 = 6^7 = 279936
        assert_eq!(result, Fr::from(279936u64));
    }

    #[test]
    fn test_domain_separation() {
        let a = Fr::from(100u64);
        let b = Fr::from(200u64);
        let c = Fr::from(300u64);

        // hash_2 and hash_3 with same inputs should differ
        let h2 = hash_2(a, b);
        let h3 = hash_3(a, b, c);

        assert_ne!(h2, h3);
    }
}
