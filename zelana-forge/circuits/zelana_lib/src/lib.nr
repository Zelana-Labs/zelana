// Zelana Library - Core cryptographic primitives for ZK circuits
//
// This library provides:
// - Poseidon hash functions (SNARK-friendly)
// - Merkle tree operations  
// - Schnorr signature verification (for transfers)
// - Nullifier computation (for shielded transactions)
// - Account state management

pub mod poseidon;
pub mod merkle;
pub mod signature;
pub mod account;
pub mod nullifier;
