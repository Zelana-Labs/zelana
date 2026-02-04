//! MiMC Hash Implementation for ZK Proof Compatibility
//!
//! This implements the same MiMC hash function used in the Noir circuit
//! (zelana_lib/poseidon.nr). The circuit uses MiMC for updating the shielded
//! commitment tree root, so we need to compute the same values here.
//!
//! IMPORTANT: Uses big-endian byte ordering to match prover-worker's
//! hex_to_decimal_field conversion (which uses BigUint::from_bytes_be).
//!
//! Used to compute proof-compatible shielded roots that match circuit computation.

use ark_bn254::Fr;
use ark_ff::{BigInteger, Field, PrimeField};

/// Number of MiMC rounds (matches circuit)
const MIMC_ROUNDS: usize = 91;

/// MiMC hasher matching the Noir circuit implementation
#[derive(Debug, Clone)]
pub struct MiMC {
    round_constants: Vec<Fr>,
}

impl Default for MiMC {
    fn default() -> Self {
        Self::new()
    }
}

impl MiMC {
    /// Create a new MiMC hasher with precomputed round constants
    pub fn new() -> Self {
        let round_constants: Vec<Fr> = (0..MIMC_ROUNDS)
            .map(|i| Self::compute_round_constant(i))
            .collect();

        Self { round_constants }
    }

    /// Compute round constant matching Noir: RC[i] = (i+1)^3 + (i+1)
    fn compute_round_constant(i: usize) -> Fr {
        let idx = Fr::from((i + 1) as u64);
        let idx_cubed = idx * idx * idx;
        idx_cubed + idx
    }

    /// MiMC round function: (x + k + c)^7
    fn round(&self, x: Fr, k: Fr, c: Fr) -> Fr {
        let t = x + k + c;
        let t2 = t.square();
        let t4 = t2.square();
        let t6 = t4 * t2;
        t6 * t // t^7
    }

    /// MiMC permutation: encrypts x with key k
    fn permute(&self, x: Fr, k: Fr) -> Fr {
        let mut state = x;
        for c in &self.round_constants {
            state = self.round(state, k, *c);
        }
        state + k // Final key addition
    }

    /// Sponge-based hash for arbitrary inputs (matches Noir mimc_sponge_absorb)
    fn sponge_absorb(&self, inputs: &[Fr], capacity: Fr) -> Fr {
        let mut state = capacity;

        for input in inputs {
            state = self.permute(state + *input, Fr::from(0u64));
        }

        state
    }

    /// Hash two field elements (matches Noir hash_2)
    /// Used for Merkle tree pairs and shielded root updates
    pub fn hash_2(&self, left: Fr, right: Fr) -> Fr {
        // Domain separation for 2-input hash (matches Noir)
        let domain = Fr::from(2u64);
        self.sponge_absorb(&[domain, left, right], Fr::from(0u64))
    }

    /// Hash two 32-byte arrays and return 32-byte result
    /// Uses BIG-ENDIAN to match prover-worker's hex_to_decimal_field
    pub fn hash_2_bytes(&self, left: &[u8; 32], right: &[u8; 32]) -> [u8; 32] {
        // Use big-endian to match prover-worker's hex_to_decimal_field
        // which uses BigUint::from_bytes_be when converting hex to field
        let left_f = Fr::from_be_bytes_mod_order(left);
        let right_f = Fr::from_be_bytes_mod_order(right);
        let result = self.hash_2(left_f, right_f);
        field_to_bytes_be(result)
    }
}

/// Convert a field element to 32 bytes (big-endian, to match prover-worker)
fn field_to_bytes_be(f: Fr) -> [u8; 32] {
    let bytes = f.into_bigint().to_bytes_be();
    let mut arr = [0u8; 32];
    let len = bytes.len().min(32);
    arr[..len].copy_from_slice(&bytes[..len]);
    arr
}

/// Convert 32 bytes to a field element (big-endian)
#[allow(dead_code)]
pub fn bytes_to_field_be(bytes: &[u8; 32]) -> Fr {
    Fr::from_be_bytes_mod_order(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mimc_deterministic() {
        let mimc = MiMC::new();
        let a = Fr::from(123u64);
        let b = Fr::from(456u64);

        let h1 = mimc.hash_2(a, b);
        let h2 = mimc.hash_2(a, b);

        assert_eq!(h1, h2, "Hash should be deterministic");
    }

    #[test]
    fn test_mimc_order_matters() {
        let mimc = MiMC::new();
        let a = Fr::from(1u64);
        let b = Fr::from(2u64);

        let h1 = mimc.hash_2(a, b);
        let h2 = mimc.hash_2(b, a);

        assert_ne!(h1, h2, "Hash should be order-sensitive");
    }

    #[test]
    fn test_mimc_bytes() {
        let mimc = MiMC::new();
        let left = [1u8; 32];
        let right = [2u8; 32];

        let result = mimc.hash_2_bytes(&left, &right);

        // Should produce a valid 32-byte output
        assert!(!result.iter().all(|&b| b == 0), "Result should be non-zero");
    }

    #[test]
    fn test_mimc_bytes_deterministic() {
        let mimc = MiMC::new();
        let left = [0xab; 32];
        let right = [0xcd; 32];

        let h1 = mimc.hash_2_bytes(&left, &right);
        let h2 = mimc.hash_2_bytes(&left, &right);

        assert_eq!(h1, h2, "Byte hash should be deterministic");
    }
}
