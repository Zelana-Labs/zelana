//! Account State Merkle Tree
//!
//! Implements a sparse Merkle tree for transparent account states.
//! Uses MiMC hash function compatible with the Noir circuit.
//!
//! ```text
//!                    Root (level 32)
//!                   /              \
//!              H(0,1)              H(2,3)
//!             /      \            /      \
//!          H(0)     H(1)       H(2)     H(3)
//!           |        |          |        |
//!        Leaf0    Leaf1      Leaf2    Leaf3
//! ```
//!
//! Each leaf is: MiMC(domain_account, pubkey, balance, nonce)

use num_bigint::BigUint;
use num_traits::{One, Zero};
use std::collections::HashMap;

use zelana_account::{AccountId, AccountState};

/// Tree depth (supports 2^32 accounts)
pub const TREE_DEPTH: usize = 32;

/// Number of MiMC rounds (matches circuit)
const MIMC_ROUNDS: u32 = 91;

/// BN254 scalar field modulus (Fr)
/// q = 21888242871839275222246405745257275088548364400416034343698204186575808495617
fn bn254_modulus() -> BigUint {
    BigUint::parse_bytes(
        b"21888242871839275222246405745257275088548364400416034343698204186575808495617",
        10,
    )
    .expect("Invalid modulus")
}

/// Domain separator for account commitments (matches circuit)
fn domain_account() -> BigUint {
    BigUint::one()
}

// MiMC Hash Implementation (matches Noir circuit exactly)

/// Compute round constant: (i+1)^3 + (i+1)
fn round_constant(i: u32) -> BigUint {
    let idx = BigUint::from(i + 1);
    let modulus = bn254_modulus();
    let idx_cubed = (&idx * &idx * &idx) % &modulus;
    (idx_cubed + &idx) % &modulus
}

/// MiMC round function: x -> (x + k + c)^7 mod p
fn mimc_round(x: &BigUint, k: &BigUint, c: &BigUint, modulus: &BigUint) -> BigUint {
    let t = (x + k + c) % modulus;
    let t2 = (&t * &t) % modulus;
    let t4 = (&t2 * &t2) % modulus;
    let t6 = (&t4 * &t2) % modulus;
    (&t6 * &t) % modulus
}

/// MiMC permutation: encrypts x with key k
fn mimc_permute(x: &BigUint, k: &BigUint) -> BigUint {
    let modulus = bn254_modulus();
    let mut state = x.clone();

    for i in 0..MIMC_ROUNDS {
        let c = round_constant(i);
        state = mimc_round(&state, k, &c, &modulus);
    }

    // Final key addition
    (&state + k) % &modulus
}

/// MiMC sponge absorption (matches Noir's mimc_sponge_absorb)
fn mimc_sponge_absorb(inputs: &[BigUint], capacity: &BigUint) -> BigUint {
    let modulus = bn254_modulus();
    let mut state = capacity.clone();
    let zero = BigUint::zero();

    for input in inputs {
        let sum = (&state + input) % &modulus;
        state = mimc_permute(&sum, &zero);
    }

    state
}

/// Hash 2 field elements (for Merkle tree pairs)
pub fn mimc_hash_2(left: &BigUint, right: &BigUint) -> BigUint {
    let domain = BigUint::from(2u64); // Domain separator for 2-input hash
    mimc_sponge_absorb(&[domain, left.clone(), right.clone()], &BigUint::zero())
}

/// Hash 4 field elements (for account leaf)
pub fn mimc_hash_4(a: &BigUint, b: &BigUint, c: &BigUint, d: &BigUint) -> BigUint {
    let domain = BigUint::from(4u64); // Domain separator for 4-input hash
    mimc_sponge_absorb(
        &[domain, a.clone(), b.clone(), c.clone(), d.clone()],
        &BigUint::zero(),
    )
}

/// Compute account leaf: MiMC(domain_account, pubkey, balance, nonce)
/// This matches compute_account_leaf in the Noir circuit
pub fn compute_account_leaf(pubkey: &BigUint, balance: u64, nonce: u64) -> BigUint {
    let domain = domain_account();
    let balance_field = BigUint::from(balance);
    let nonce_field = BigUint::from(nonce);

    mimc_sponge_absorb(
        &[
            BigUint::from(4u64), // 4-input domain
            domain,
            pubkey.clone(),
            balance_field,
            nonce_field,
        ],
        &BigUint::zero(),
    )
}

/// Domain separator for withdrawal hashes (matches circuit's domain_withdrawal())
fn domain_withdrawal() -> BigUint {
    BigUint::from(5u64)
}

/// Domain separator for batch hashes (matches circuit's domain_batch())
fn domain_batch() -> BigUint {
    BigUint::from(4u64)
}

/// Compute withdrawal root using MiMC (matches circuit exactly)
///
/// Circuit computation:
///   withdrawal_accumulator = hash_2(domain_withdrawal(), batch_id)
///   for each withdrawal: wd_acc = hash_2(wd_acc, hash_3(l1_recipient, amount, sender))
///   final_withdrawal_root = hash_2(withdrawal_accumulator, num_withdrawals)
pub fn compute_withdrawal_root_mimc(batch_id: u64, num_withdrawals: u64) -> [u8; 32] {
    // Initial accumulator: hash_2(5, batch_id)
    let batch_id_field = BigUint::from(batch_id);
    let wd_acc = mimc_hash_2(&domain_withdrawal(), &batch_id_field);

    // For now, we only support empty batches (deposits only)
    // TODO: Add withdrawal hashing when we have withdrawal transactions

    // Final: hash_2(wd_acc, num_withdrawals)
    let num_wd_field = BigUint::from(num_withdrawals);
    let final_root = mimc_hash_2(&wd_acc, &num_wd_field);

    field_to_bytes(&final_root)
}

/// Compute batch hash using MiMC (matches circuit exactly)
///
/// Circuit computation:
///   batch_accumulator = hash_2(domain_batch(), batch_id)
///   for each transfer: batch_acc = hash_3(batch_acc, tx_hash, amount)
///   for each withdrawal: batch_acc = hash_3(batch_acc, wd_hash, amount)
///   for each shielded: batch_acc = hash_3(batch_acc, nullifier, commitment)
///   final_batch_hash = hash_4(batch_acc, num_transfers, num_withdrawals, num_shielded)
pub fn compute_batch_hash_mimc(
    batch_id: u64,
    num_transfers: u64,
    num_withdrawals: u64,
    num_shielded: u64,
) -> [u8; 32] {
    // Initial accumulator: hash_2(4, batch_id)
    let batch_id_field = BigUint::from(batch_id);
    let batch_acc = mimc_hash_2(&domain_batch(), &batch_id_field);

    // For now, we only support empty batches (deposits only)
    // TODO: Add transaction hashing when we have transfers

    // Final: hash_4(batch_acc, num_transfers, num_withdrawals, num_shielded)
    let num_t = BigUint::from(num_transfers);
    let num_w = BigUint::from(num_withdrawals);
    let num_s = BigUint::from(num_shielded);
    let final_hash = mimc_hash_4(&batch_acc, &num_t, &num_w, &num_s);

    field_to_bytes(&final_hash)
}

/// Convert 32-byte array to BigUint (big-endian)
fn bytes_to_field(bytes: &[u8; 32]) -> BigUint {
    let modulus = bn254_modulus();
    let big_int = BigUint::from_bytes_be(bytes);
    big_int % modulus
}

/// Convert BigUint to 32-byte array (big-endian)
fn field_to_bytes(field: &BigUint) -> [u8; 32] {
    let bytes = field.to_bytes_be();
    let mut result = [0u8; 32];
    let start = 32_usize.saturating_sub(bytes.len());
    result[start..].copy_from_slice(&bytes);
    result
}

// Merkle Path

/// A Merkle proof for an account
#[derive(Debug, Clone)]
pub struct AccountMerklePath {
    /// Sibling hashes from leaf to root (32 elements)
    pub siblings: [[u8; 32]; TREE_DEPTH],
    /// Position bits (0 = left, 1 = right) - indicates which side the current node is on
    pub path_indices: [u8; TREE_DEPTH],
    /// The leaf position (account index)
    pub position: u64,
}

impl AccountMerklePath {
    /// Verify that this path proves inclusion of a leaf in the given root
    pub fn verify(&self, leaf: &[u8; 32], root: &[u8; 32]) -> bool {
        let computed_root = self.compute_root(leaf);
        &computed_root == root
    }

    /// Compute root from leaf using this path
    pub fn compute_root(&self, leaf: &[u8; 32]) -> [u8; 32] {
        let mut current = bytes_to_field(leaf);

        for i in 0..TREE_DEPTH {
            let sibling = bytes_to_field(&self.siblings[i]);
            let is_right = self.path_indices[i] == 1;

            current = if is_right {
                mimc_hash_2(&sibling, &current)
            } else {
                mimc_hash_2(&current, &sibling)
            };
        }

        field_to_bytes(&current)
    }

    /// Get siblings as hex strings for prover API
    pub fn siblings_hex(&self) -> Vec<String> {
        self.siblings.iter().map(hex::encode).collect()
    }

    /// Get path indices as Vec<u8>
    pub fn path_indices_vec(&self) -> Vec<u8> {
        self.path_indices.to_vec()
    }
}

impl Default for AccountMerklePath {
    fn default() -> Self {
        Self {
            siblings: [[0u8; 32]; TREE_DEPTH],
            path_indices: [0u8; TREE_DEPTH],
            position: 0,
        }
    }
}

// Account Sparse Merkle Tree

/// Sparse Merkle tree for account states.
///
/// Uses lazy evaluation - only stores non-empty nodes.
/// Account positions are derived from the account ID hash.
#[derive(Clone)]
pub struct AccountTree {
    /// Non-empty nodes: (level, index) -> hash
    nodes: HashMap<(usize, u64), [u8; 32]>,
    /// Account ID -> position mapping
    positions: HashMap<AccountId, u64>,
    /// Current root
    root: [u8; 32],
    /// Precomputed empty roots at each level
    empty_roots: Vec<[u8; 32]>,
}

impl AccountTree {
    /// Create a new empty tree
    pub fn new() -> Self {
        let empty_roots = Self::compute_empty_roots();
        let root = empty_roots[TREE_DEPTH];

        Self {
            nodes: HashMap::new(),
            positions: HashMap::new(),
            root,
            empty_roots,
        }
    }

    /// Compute empty subtree roots at each level
    fn compute_empty_roots() -> Vec<[u8; 32]> {
        let mut roots = vec![[0u8; 32]]; // Empty leaf is all zeros

        // For an empty tree, each level's hash is H(empty, empty)
        for _ in 0..TREE_DEPTH {
            let prev = roots.last().unwrap();
            let prev_field = bytes_to_field(prev);
            let parent = mimc_hash_2(&prev_field, &prev_field);
            roots.push(field_to_bytes(&parent));
        }

        roots
    }

    /// Get current root
    pub fn root(&self) -> [u8; 32] {
        self.root
    }

    /// Get position for an account ID (deterministic from ID)
    fn get_or_create_position(&mut self, account_id: &AccountId) -> u64 {
        if let Some(&pos) = self.positions.get(account_id) {
            return pos;
        }

        // Derive position from account ID using first 4 bytes
        // This gives us up to 2^32 unique positions
        let pos = u32::from_be_bytes([
            account_id.0[0],
            account_id.0[1],
            account_id.0[2],
            account_id.0[3],
        ]) as u64;

        self.positions.insert(*account_id, pos);
        pos
    }

    /// Get position for an account if it exists
    pub fn get_position(&self, account_id: &AccountId) -> Option<u64> {
        self.positions.get(account_id).copied()
    }

    /// Insert or update an account state
    pub fn insert(&mut self, account_id: &AccountId, state: &AccountState) -> u64 {
        let position = self.get_or_create_position(account_id);

        // Compute account leaf
        let pubkey_field = bytes_to_field(&account_id.0);
        let leaf_field = compute_account_leaf(&pubkey_field, state.balance, state.nonce);
        let leaf = field_to_bytes(&leaf_field);

        // Insert leaf and update path to root
        self.insert_leaf_at(position, leaf);

        position
    }

    /// Insert a leaf at a specific position and update the tree
    fn insert_leaf_at(&mut self, position: u64, leaf: [u8; 32]) {
        // Insert leaf at level 0
        self.nodes.insert((0, position), leaf);

        // Update path to root
        let mut current_index = position;
        let mut current_hash = leaf;

        for level in 0..TREE_DEPTH {
            let is_right = current_index & 1 == 1;
            let sibling_index = if is_right {
                current_index - 1
            } else {
                current_index + 1
            };

            let sibling = self
                .nodes
                .get(&(level, sibling_index))
                .copied()
                .unwrap_or(self.empty_roots[level]);

            let current_field = bytes_to_field(&current_hash);
            let sibling_field = bytes_to_field(&sibling);

            let parent_field = if is_right {
                mimc_hash_2(&sibling_field, &current_field)
            } else {
                mimc_hash_2(&current_field, &sibling_field)
            };

            current_index /= 2;
            current_hash = field_to_bytes(&parent_field);

            self.nodes.insert((level + 1, current_index), current_hash);
        }

        self.root = current_hash;
    }

    /// Get Merkle path for an account
    pub fn path(&self, account_id: &AccountId) -> Option<AccountMerklePath> {
        let position = self.positions.get(account_id)?;
        self.path_at_position(*position)
    }

    /// Get Merkle path at a specific position
    pub fn path_at_position(&self, position: u64) -> Option<AccountMerklePath> {
        let mut siblings = [[0u8; 32]; TREE_DEPTH];
        let mut path_indices = [0u8; TREE_DEPTH];
        let mut current_index = position;

        for level in 0..TREE_DEPTH {
            let is_right = current_index & 1 == 1;
            path_indices[level] = if is_right { 1 } else { 0 };

            let sibling_index = if is_right {
                current_index - 1
            } else {
                current_index + 1
            };

            siblings[level] = self
                .nodes
                .get(&(level, sibling_index))
                .copied()
                .unwrap_or(self.empty_roots[level]);

            current_index /= 2;
        }

        Some(AccountMerklePath {
            siblings,
            path_indices,
            position,
        })
    }

    /// Get the leaf hash for an account
    pub fn leaf(&self, account_id: &AccountId) -> Option<[u8; 32]> {
        let position = self.positions.get(account_id)?;
        self.nodes.get(&(0, *position)).copied()
    }

    /// Check if an account exists in the tree
    pub fn contains(&self, account_id: &AccountId) -> bool {
        self.positions.contains_key(account_id)
    }

    /// Get the number of accounts in the tree
    pub fn len(&self) -> usize {
        self.positions.len()
    }

    /// Check if the tree is empty
    pub fn is_empty(&self) -> bool {
        self.positions.is_empty()
    }
}

impl Default for AccountTree {
    fn default() -> Self {
        Self::new()
    }
}

// Tests

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mimc_round_constant() {
        // Test that round constants are computed correctly
        // RC[0] = 2^3 + 2 = 10 (since i=0 -> idx=1 -> 1^3 + 1 = 2)
        let rc0 = round_constant(0);
        assert_eq!(rc0, BigUint::from(2u64));

        // RC[1] = (2)^3 + 2 = 10
        let rc1 = round_constant(1);
        assert_eq!(rc1, BigUint::from(10u64));
    }

    #[test]
    fn test_mimc_hash_deterministic() {
        let a = BigUint::from(123u64);
        let b = BigUint::from(456u64);

        let h1 = mimc_hash_2(&a, &b);
        let h2 = mimc_hash_2(&a, &b);

        assert_eq!(h1, h2, "Hash should be deterministic");
    }

    #[test]
    fn test_mimc_hash_order_matters() {
        let a = BigUint::from(123u64);
        let b = BigUint::from(456u64);

        let h1 = mimc_hash_2(&a, &b);
        let h2 = mimc_hash_2(&b, &a);

        assert_ne!(h1, h2, "Hash order should matter");
    }

    #[test]
    fn test_account_leaf() {
        let pubkey = BigUint::from(12345u64);
        let balance = 1000u64;
        let nonce = 5u64;

        let leaf1 = compute_account_leaf(&pubkey, balance, nonce);
        let leaf2 = compute_account_leaf(&pubkey, balance, nonce);

        assert_eq!(leaf1, leaf2, "Account leaf should be deterministic");

        // Different balance should give different leaf
        let leaf3 = compute_account_leaf(&pubkey, balance + 1, nonce);
        assert_ne!(leaf1, leaf3);
    }

    #[test]
    fn test_empty_tree() {
        let tree = AccountTree::new();
        assert!(tree.is_empty());

        // Empty root should be consistent
        let tree2 = AccountTree::new();
        assert_eq!(tree.root(), tree2.root());
    }

    #[test]
    fn test_insert_and_path() {
        let mut tree = AccountTree::new();

        let account_id = AccountId([1u8; 32]);
        let state = AccountState {
            balance: 1000,
            nonce: 0,
        };

        let position = tree.insert(&account_id, &state);

        // Should be able to get path
        let path = tree.path(&account_id).expect("Should have path");
        assert_eq!(path.position, position);

        // Path should verify
        let leaf = tree.leaf(&account_id).expect("Should have leaf");
        assert!(path.verify(&leaf, &tree.root()));
    }

    #[test]
    fn test_root_changes_on_update() {
        let mut tree = AccountTree::new();
        let root0 = tree.root();

        let account_id = AccountId([1u8; 32]);
        let state = AccountState {
            balance: 1000,
            nonce: 0,
        };

        tree.insert(&account_id, &state);
        let root1 = tree.root();

        assert_ne!(root0, root1, "Root should change after insert");

        // Update same account
        let state2 = AccountState {
            balance: 2000,
            nonce: 1,
        };
        tree.insert(&account_id, &state2);
        let root2 = tree.root();

        assert_ne!(root1, root2, "Root should change after update");
    }

    #[test]
    fn test_multiple_accounts() {
        let mut tree = AccountTree::new();

        let accounts = [
            (AccountId([1u8; 32]), 1000u64),
            (AccountId([2u8; 32]), 2000u64),
            (AccountId([3u8; 32]), 3000u64),
        ];

        for (id, balance) in &accounts {
            tree.insert(
                id,
                &AccountState {
                    balance: *balance,
                    nonce: 0,
                },
            );
        }

        assert_eq!(tree.len(), 3);

        // Each account should have a valid path
        for (id, _) in &accounts {
            let path = tree.path(id).expect("Should have path");
            let leaf = tree.leaf(id).expect("Should have leaf");
            assert!(path.verify(&leaf, &tree.root()));
        }
    }

    #[test]
    fn test_path_at_position() {
        let mut tree = AccountTree::new();

        let account_id = AccountId([42u8; 32]);
        let state = AccountState {
            balance: 5000,
            nonce: 3,
        };

        let position = tree.insert(&account_id, &state);

        // Get path by position
        let path = tree.path_at_position(position).expect("Should have path");
        let leaf = tree.leaf(&account_id).expect("Should have leaf");

        assert!(path.verify(&leaf, &tree.root()));
    }
}
