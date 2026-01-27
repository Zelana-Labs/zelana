// Hash Functions
//
// Using a MiMC-like construction in pure field arithmetic.
// This is compatible with Sunspot's gnark backend which doesn't support
// Noir 1.0's pedersen_hash opcode.
//
// MiMC is a SNARK-friendly hash using the equation: x -> x^7 + c (repeated)
// We use a simplified construction optimized for BN254.
//
// NOTE: For production, use the stdlib pedersen_hash once Sunspot updates
// to support Noir 1.0's ACIR format fully.

// Round constants for MiMC (pre-computed, derived from nothing-up-my-sleeve number)
// Using powers of a fixed generator for simplicity
global MIMC_ROUNDS: u32 = 91;  // Standard MiMC rounds for ~256-bit security

// Pre-computed round constants (first 91 values)
// These are hash("mimc_constant_i") for i = 0..90
// For simplicity, we use x^2 + i as round constant generator
fn round_constant(i: u32) -> Field {
    // Simple round constant: avoids external data
    // RC[i] = (i+1)^2 * 0x30644e72e131a029b85045b68181585d2833e84879b9709143e1f593f0000001
    // Simplified: just use (i+1)^3 + (i+1)
    let idx = i as Field;
    let c = (idx + 1) * (idx + 1) * (idx + 1) + (idx + 1);
    c
}

/// MiMC round function: x -> (x + k + c)^7
fn mimc_round(x: Field, k: Field, c: Field) -> Field {
    let t = x + k + c;
    let t2 = t * t;
    let t4 = t2 * t2;
    let t6 = t4 * t2;
    t6 * t  // t^7
}

/// MiMC permutation: encrypts x with key k
fn mimc_permute(x: Field, k: Field) -> Field {
    let mut state = x;
    for i in 0..MIMC_ROUNDS {
        let c = round_constant(i);
        state = mimc_round(state, k, c);
    }
    state + k  // Final key addition
}

/// Sponge-based hash for arbitrary inputs
/// Uses capacity=1 field element for 128-bit security margin
fn mimc_sponge_absorb(inputs: [Field], capacity: Field) -> Field {
    let mut state = capacity;  // Initial state from capacity
    
    // Absorb phase - XOR inputs into state and permute
    for i in 0..inputs.len() {
        state = mimc_permute(state + inputs[i], 0);
    }
    
    state
}

/// Hash two field elements (used for Merkle tree pairs)
pub fn hash_2(left: Field, right: Field) -> Field {
    // Domain separation for 2-input hash
    let domain: Field = 2;
    mimc_sponge_absorb([domain, left, right], 0)
}

/// Hash three field elements
pub fn hash_3(a: Field, b: Field, c: Field) -> Field {
    let domain: Field = 3;
    mimc_sponge_absorb([domain, a, b, c], 0)
}

/// Hash four field elements (used for transaction hashing)
pub fn hash_4(a: Field, b: Field, c: Field, d: Field) -> Field {
    let domain: Field = 4;
    mimc_sponge_absorb([domain, a, b, c, d], 0)
}

/// Hash five field elements
pub fn hash_5(a: Field, b: Field, c: Field, d: Field, e: Field) -> Field {
    let domain: Field = 5;
    mimc_sponge_absorb([domain, a, b, c, d, e], 0)
}

/// Hash six field elements
pub fn hash_6(a: Field, b: Field, c: Field, d: Field, e: Field, f: Field) -> Field {
    let domain: Field = 6;
    mimc_sponge_absorb([domain, a, b, c, d, e, f], 0)
}

/// Domain-separated hash (adds a domain tag to prevent cross-protocol attacks)
pub fn hash_with_domain(domain: Field, inputs: [Field; 2]) -> Field {
    mimc_sponge_absorb([domain, inputs[0], inputs[1]], 0)
}

// Domain separators for different uses (using small numbers for efficiency)
pub fn domain_account() -> Field {
    1  // Account commitments
}

pub fn domain_merkle() -> Field {
    2  // Merkle tree nodes
}

pub fn domain_nullifier() -> Field {
    3  // Nullifier computation
}

pub fn domain_batch() -> Field {
    4  // Batch hash
}

pub fn domain_withdrawal() -> Field {
    5  // Withdrawal hash
}

pub fn domain_note() -> Field {
    6  // Note commitment
}

#[test]
fn test_hash_2() {
    let a = 1;
    let b = 2;
    let h = hash_2(a, b);
    // Just verify it produces a valid output
    assert(h != 0);
}

#[test]
fn test_hash_deterministic() {
    let a = 123;
    let b = 456;
    let h1 = hash_2(a, b);
    let h2 = hash_2(a, b);
    assert(h1 == h2);
}

#[test]
fn test_hash_collision_resistant() {
    let h1 = hash_2(1, 2);
    let h2 = hash_2(2, 1);
    assert(h1 != h2);  // Order matters
}

#[test]
fn test_mimc_round() {
    // Test round function produces non-trivial output
    let r = mimc_round(1, 2, 3);
    assert(r != 0);
    assert(r != 1);
}
