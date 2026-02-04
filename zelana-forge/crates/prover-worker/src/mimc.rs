//! MiMC Hash Implementation
//!
//! Rust implementation of MiMC hash matching the Noir circuit in zelana_lib/poseidon.nr.
//! Uses the same MiMC-like construction with x^7 S-box and 91 rounds.
//!
//! This is used to compute public inputs (batch_hash, withdrawal_root) that
//! will match what the circuit computes internally.

use ark_bn254::Fr;
use ark_ff::{BigInteger, Field, PrimeField};

/// Number of MiMC rounds (matches circuit)
const MIMC_ROUNDS: usize = 91;

/// Domain separators matching Noir circuit
pub mod domain {
    use ark_bn254::Fr;
    use ark_ff::PrimeField;

    pub fn account() -> Fr {
        Fr::from(1u64)
    }
    pub fn merkle() -> Fr {
        Fr::from(2u64)
    }
    pub fn nullifier() -> Fr {
        Fr::from(3u64)
    }
    pub fn batch() -> Fr {
        Fr::from(4u64)
    }
    pub fn withdrawal() -> Fr {
        Fr::from(5u64)
    }
    pub fn note() -> Fr {
        Fr::from(6u64)
    }
}

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
    /// Used for Merkle tree pairs
    pub fn hash_2(&self, left: Fr, right: Fr) -> Fr {
        let domain = Fr::from(2u64);
        self.sponge_absorb(&[domain, left, right], Fr::from(0u64))
    }

    /// Hash three field elements (matches Noir hash_3)
    pub fn hash_3(&self, a: Fr, b: Fr, c: Fr) -> Fr {
        let domain = Fr::from(3u64);
        self.sponge_absorb(&[domain, a, b, c], Fr::from(0u64))
    }

    /// Hash four field elements (matches Noir hash_4)
    /// Used for transaction hashing
    pub fn hash_4(&self, a: Fr, b: Fr, c: Fr, d: Fr) -> Fr {
        let domain = Fr::from(4u64);
        self.sponge_absorb(&[domain, a, b, c, d], Fr::from(0u64))
    }

    /// Hash five field elements (matches Noir hash_5)
    pub fn hash_5(&self, a: Fr, b: Fr, c: Fr, d: Fr, e: Fr) -> Fr {
        let domain = Fr::from(5u64);
        self.sponge_absorb(&[domain, a, b, c, d, e], Fr::from(0u64))
    }

    /// Hash six field elements (matches Noir hash_6)
    pub fn hash_6(&self, a: Fr, b: Fr, c: Fr, d: Fr, e: Fr, f: Fr) -> Fr {
        let domain = Fr::from(6u64);
        self.sponge_absorb(&[domain, a, b, c, d, e, f], Fr::from(0u64))
    }

    /// Compute account leaf: hash_4(domain_account, pubkey, balance, nonce)
    /// Matches circuit's compute_account_leaf: hash_4(domain_account(), pubkey, balance, nonce)
    pub fn compute_account_leaf(&self, pubkey: Fr, balance: Fr, nonce: Fr) -> Fr {
        self.hash_4(domain::account(), pubkey, balance, nonce)
    }

    /// Compute nullifier: hash_3(spending_key, commitment, position)
    pub fn compute_nullifier(&self, spending_key: Fr, commitment: Fr, position: Fr) -> Fr {
        // Match circuit: hash_3(domain_nullifier(), spending_key, hash_2(commitment, position))
        let inner = self.hash_2(commitment, position);
        self.hash_3(domain::nullifier(), spending_key, inner)
    }
}

// ============================================================================
// Batch Hash Computation (matches circuit logic)
// ============================================================================

/// Transfer data for batch hash computation
#[derive(Debug, Clone)]
pub struct TransferData {
    pub sender_pubkey: Fr,
    pub receiver_pubkey: Fr,
    pub amount: Fr,
    pub sender_nonce: Fr,
}

/// Withdrawal data for batch hash computation
#[derive(Debug, Clone)]
pub struct WithdrawalData {
    pub sender_pubkey: Fr,
    pub l1_recipient: Fr,
    pub amount: Fr,
}

/// Shielded transaction data for batch hash computation
#[derive(Debug, Clone)]
pub struct ShieldedData {
    pub nullifier: Fr,
    pub output_commitment: Fr,
}

/// Compute batch hash matching the Noir circuit
///
/// Matches main.nr:
/// ```noir
/// let mut batch_accumulator = hash_2(domain_batch(), batch_id);
/// // For each transfer: batch_accumulator = hash_3(batch_accumulator, tx_hash, amount)
/// // For each withdrawal: batch_accumulator = hash_3(batch_accumulator, wd_hash, amount)
/// // For each shielded: batch_accumulator = hash_3(batch_accumulator, nullifier, commitment)
/// let final_batch_hash = hash_4(batch_accumulator, num_transfers, num_withdrawals, num_shielded);
/// ```
pub fn compute_batch_hash(
    mimc: &MiMC,
    batch_id: Fr,
    transfers: &[TransferData],
    withdrawals: &[WithdrawalData],
    shielded: &[ShieldedData],
) -> Fr {
    // Initial accumulator
    let mut batch_acc = mimc.hash_2(domain::batch(), batch_id);

    // Process transfers
    for tx in transfers {
        // tx_hash = hash_4(sender_pubkey, receiver_pubkey, amount, sender_nonce)
        let tx_hash = mimc.hash_4(
            tx.sender_pubkey,
            tx.receiver_pubkey,
            tx.amount,
            tx.sender_nonce,
        );
        batch_acc = mimc.hash_3(batch_acc, tx_hash, tx.amount);
    }

    // Process withdrawals
    for wd in withdrawals {
        // wd_hash = hash_3(l1_recipient, amount, sender_pubkey)
        let wd_hash = mimc.hash_3(wd.l1_recipient, wd.amount, wd.sender_pubkey);
        batch_acc = mimc.hash_3(batch_acc, wd_hash, wd.amount);
    }

    // Process shielded
    for sh in shielded {
        batch_acc = mimc.hash_3(batch_acc, sh.nullifier, sh.output_commitment);
    }

    // Finalize with counts
    let num_transfers = Fr::from(transfers.len() as u64);
    let num_withdrawals = Fr::from(withdrawals.len() as u64);
    let num_shielded = Fr::from(shielded.len() as u64);

    mimc.hash_4(batch_acc, num_transfers, num_withdrawals, num_shielded)
}

/// Compute withdrawal root matching the Noir circuit
///
/// Matches main.nr:
/// ```noir
/// let mut withdrawal_accumulator = hash_2(domain_withdrawal(), batch_id);
/// // For each withdrawal: withdrawal_accumulator = hash_2(withdrawal_accumulator, wd_hash)
/// let final_withdrawal_root = hash_2(withdrawal_accumulator, num_withdrawals);
/// ```
pub fn compute_withdrawal_root(mimc: &MiMC, batch_id: Fr, withdrawals: &[WithdrawalData]) -> Fr {
    let mut wd_acc = mimc.hash_2(domain::withdrawal(), batch_id);

    for wd in withdrawals {
        // wd_hash = hash_3(l1_recipient, amount, sender_pubkey)
        let wd_hash = mimc.hash_3(wd.l1_recipient, wd.amount, wd.sender_pubkey);
        wd_acc = mimc.hash_2(wd_acc, wd_hash);
    }

    // Finalize with count
    let num_withdrawals = Fr::from(withdrawals.len() as u64);
    mimc.hash_2(wd_acc, num_withdrawals)
}

// ============================================================================
// Utility Functions
// ============================================================================

/// Convert 32-byte array to field element
pub fn bytes_to_field(bytes: &[u8; 32]) -> Fr {
    Fr::from_le_bytes_mod_order(bytes)
}

/// Convert field element to 32-byte array (big-endian for circuit compatibility)
pub fn field_to_bytes(f: Fr) -> [u8; 32] {
    let bigint = f.into_bigint();
    let mut bytes = [0u8; 32];
    // ark-ff uses little-endian internally
    let le_bytes = bigint.to_bytes_le();
    bytes[..le_bytes.len()].copy_from_slice(&le_bytes);
    bytes
}

/// Convert hex string to field element
pub fn hex_to_field(hex: &str) -> Result<Fr, &'static str> {
    let hex = hex.strip_prefix("0x").unwrap_or(hex);
    let bytes = hex::decode(hex).map_err(|_| "Invalid hex string")?;

    if bytes.len() > 32 {
        return Err("Hex string too long for field element");
    }

    let mut padded = [0u8; 32];
    // Left-pad with zeros, bytes are big-endian from hex
    padded[32 - bytes.len()..].copy_from_slice(&bytes);
    // Reverse to little-endian for ark-ff
    padded.reverse();

    Ok(Fr::from_le_bytes_mod_order(&padded))
}

/// Convert field element to hex string (with 0x prefix)
pub fn field_to_hex(f: Fr) -> String {
    let bytes = field_to_bytes(f);
    // Reverse to big-endian for display
    let mut be_bytes = bytes;
    be_bytes.reverse();
    format!("0x{}", hex::encode(be_bytes))
}

// ============================================================================
// Tests
// ============================================================================

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

        assert_eq!(h1, h2);
    }

    #[test]
    fn test_mimc_collision_resistant() {
        let mimc = MiMC::new();
        let a = Fr::from(1u64);
        let b = Fr::from(2u64);

        let h1 = mimc.hash_2(a, b);
        let h2 = mimc.hash_2(b, a);

        assert_ne!(h1, h2, "Order should matter");
    }

    #[test]
    fn test_round_constant() {
        // RC[0] = (0+1)^3 + (0+1) = 1 + 1 = 2
        let rc0 = MiMC::compute_round_constant(0);
        assert_eq!(rc0, Fr::from(2u64));

        // RC[1] = (1+1)^3 + (1+1) = 8 + 2 = 10
        let rc1 = MiMC::compute_round_constant(1);
        assert_eq!(rc1, Fr::from(10u64));

        // RC[2] = (2+1)^3 + (2+1) = 27 + 3 = 30
        let rc2 = MiMC::compute_round_constant(2);
        assert_eq!(rc2, Fr::from(30u64));
    }

    #[test]
    fn test_empty_batch_hash() {
        let mimc = MiMC::new();
        let batch_id = Fr::from(1u64);

        let hash = compute_batch_hash(&mimc, batch_id, &[], &[], &[]);

        // Should be deterministic for empty batch
        let hash2 = compute_batch_hash(&mimc, batch_id, &[], &[], &[]);
        assert_eq!(hash, hash2);

        // Different batch_id should give different hash
        let hash3 = compute_batch_hash(&mimc, Fr::from(2u64), &[], &[], &[]);
        assert_ne!(hash, hash3);
    }

    #[test]
    fn test_withdrawal_root() {
        let mimc = MiMC::new();
        let batch_id = Fr::from(1u64);

        // Empty withdrawals
        let root1 = compute_withdrawal_root(&mimc, batch_id, &[]);

        // One withdrawal
        let wd = WithdrawalData {
            sender_pubkey: Fr::from(100u64),
            l1_recipient: Fr::from(200u64),
            amount: Fr::from(1000u64),
        };
        let root2 = compute_withdrawal_root(&mimc, batch_id, &[wd]);

        assert_ne!(root1, root2);
    }

    #[test]
    fn test_hex_conversion() {
        let f = Fr::from(12345u64);
        let hex = field_to_hex(f);
        let f2 = hex_to_field(&hex).unwrap();
        assert_eq!(f, f2);
    }

    #[test]
    fn test_account_leaf() {
        let mimc = MiMC::new();

        let pubkey = Fr::from(1u64);
        let balance = Fr::from(1000u64);
        let nonce = Fr::from(5u64);

        let leaf = mimc.compute_account_leaf(pubkey, balance, nonce);

        // Should be hash_4(domain_account, pubkey, balance, nonce)
        // Matches circuit's compute_account_leaf
        let expected = mimc.hash_4(domain::account(), pubkey, balance, nonce);
        assert_eq!(leaf, expected);
    }

    #[test]
    fn test_batch_58_hash() {
        // Test case from actual failed batch 58:
        // - 0 transfers, 0 withdrawals, 1 shielded transaction
        // - Values from Prover.toml
        use ark_ff::BigInteger;
        use num_bigint::BigUint;

        let mimc = MiMC::new();
        let batch_id = Fr::from(58u64);

        // Shielded transaction from Prover.toml
        let nullifier_dec =
            "7616971353247117454465635208226161158442151985157735778832845157632758123933";
        let output_commitment_dec =
            "9742579207011299985260428178793458874858518230054558356243537317566210478598";

        // Parse as BigUint and convert to Fr
        let nullifier_big: BigUint = nullifier_dec.parse().unwrap();
        let output_commitment_big: BigUint = output_commitment_dec.parse().unwrap();

        let nullifier = Fr::from_be_bytes_mod_order(&{
            let bytes = nullifier_big.to_bytes_be();
            let mut arr = [0u8; 32];
            let start = 32 - bytes.len();
            arr[start..].copy_from_slice(&bytes);
            arr
        });
        let output_commitment = Fr::from_be_bytes_mod_order(&{
            let bytes = output_commitment_big.to_bytes_be();
            let mut arr = [0u8; 32];
            let start = 32 - bytes.len();
            arr[start..].copy_from_slice(&bytes);
            arr
        });

        let shielded = vec![ShieldedData {
            nullifier,
            output_commitment,
        }];

        // Compute batch hash
        let batch_hash = compute_batch_hash(&mimc, batch_id, &[], &[], &shielded);

        // Convert to decimal string for comparison
        let hash_big = BigUint::from_bytes_be(&batch_hash.into_bigint().to_bytes_be());
        let hash_str = hash_big.to_string();

        // Expected from public witness file
        let expected =
            "1763393191922739858634693308814702990929063366376880176226696705996392451429";

        println!("Computed batch_hash: {}", hash_str);
        println!("Expected batch_hash: {}", expected);

        assert_eq!(
            hash_str, expected,
            "Batch hash mismatch! Circuit and Rust MiMC must produce same value"
        );
    }
}
