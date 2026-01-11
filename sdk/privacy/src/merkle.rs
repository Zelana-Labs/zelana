//! Merkle Tree for Note Commitments
//!
//! Implements a sparse Merkle tree for storing note commitments.
//! Used for proving note existence without revealing which note.
//!
//! ```text
//!                    Root
//!                   /    \
//!                 H01    H23
//!                /  \   /   \
//!               H0  H1 H2   H3
//!               |   |   |    |
//!              C0  C1  C2   C3  (Note Commitments)
//! ```

use ark_bls12_381::Fr;
use ark_crypto_primitives::sponge::{
    CryptographicSponge,
    poseidon::{PoseidonConfig, PoseidonSponge, find_poseidon_ark_and_mds},
};
use ark_ff::{BigInteger, PrimeField};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::commitment::Commitment;

/// Tree depth (supports 2^32 notes)
pub const TREE_DEPTH: usize = 32;

/// A Merkle path proving inclusion of a note
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MerklePath {
    /// Sibling hashes from leaf to root
    pub siblings: Vec<[u8; 32]>,
    /// Position bits (0 = left, 1 = right)
    pub path_bits: Vec<bool>,
    /// The leaf position
    pub position: u64,
}

impl MerklePath {
    /// Verify that this path proves inclusion of `leaf` in `root`
    pub fn verify(&self, leaf: &Commitment, root: &[u8; 32]) -> bool {
        let hasher = MerkleHasher::new();
        let computed_root = hasher.compute_root_from_path(&leaf.0, &self.siblings, &self.path_bits);
        &computed_root == root
    }

    /// Get the authentication path as field elements (for ZK circuits)
    pub fn to_field_elements(&self) -> Vec<Fr> {
        self.siblings
            .iter()
            .map(|s| Fr::from_le_bytes_mod_order(s))
            .collect()
    }
}

/// Poseidon-based Merkle hash function
pub struct MerkleHasher {
    config: PoseidonConfig<Fr>,
    /// Precomputed empty subtree roots at each level
    empty_roots: Vec<[u8; 32]>,
}

impl MerkleHasher {
    pub fn new() -> Self {
        let config = Self::poseidon_config();
        let empty_leaf = Self::compute_empty_leaf(&config);
        let empty_roots = Self::compute_empty_roots(&config, &empty_leaf);

        Self {
            config,
            empty_roots,
        }
    }

    /// Hash two children to get parent
    pub fn hash_pair(&self, left: &[u8; 32], right: &[u8; 32]) -> [u8; 32] {
        let mut sponge = PoseidonSponge::new(&self.config);

        let left_f = Fr::from_le_bytes_mod_order(left);
        let right_f = Fr::from_le_bytes_mod_order(right);

        sponge.absorb(&left_f);
        sponge.absorb(&right_f);

        let result: Fr = sponge.squeeze_field_elements(1)[0];
        let bytes = result.into_bigint().to_bytes_le();
        let mut arr = [0u8; 32];
        arr[..bytes.len()].copy_from_slice(&bytes);
        arr
    }

    /// Get the empty root at a given depth
    pub fn empty_root(&self, depth: usize) -> &[u8; 32] {
        &self.empty_roots[depth]
    }

    /// Compute root from leaf and authentication path
    pub fn compute_root_from_path(
        &self,
        leaf: &[u8; 32],
        siblings: &[[u8; 32]],
        path_bits: &[bool],
    ) -> [u8; 32] {
        let mut current = *leaf;

        for (sibling, is_right) in siblings.iter().zip(path_bits.iter()) {
            if *is_right {
                // Current node is on the right
                current = self.hash_pair(sibling, &current);
            } else {
                // Current node is on the left
                current = self.hash_pair(&current, sibling);
            }
        }

        current
    }

    fn poseidon_config() -> PoseidonConfig<Fr> {
        let (ark, mds) = find_poseidon_ark_and_mds::<Fr>(255, 2, 8, 57, 0);
        PoseidonConfig::new(8, 57, 5, mds, ark, 2, 1)
    }

    fn compute_empty_leaf(config: &PoseidonConfig<Fr>) -> [u8; 32] {
        let mut sponge = PoseidonSponge::new(config);
        sponge.absorb(&Fr::from(0u64));
        let result: Fr = sponge.squeeze_field_elements(1)[0];
        let bytes = result.into_bigint().to_bytes_le();
        let mut arr = [0u8; 32];
        arr[..bytes.len()].copy_from_slice(&bytes);
        arr
    }

    fn compute_empty_roots(config: &PoseidonConfig<Fr>, empty_leaf: &[u8; 32]) -> Vec<[u8; 32]> {
        let mut roots = vec![*empty_leaf];

        for _ in 0..TREE_DEPTH {
            let prev = roots.last().unwrap();
            let mut sponge = PoseidonSponge::new(config);
            let prev_f = Fr::from_le_bytes_mod_order(prev);
            sponge.absorb(&prev_f);
            sponge.absorb(&prev_f);
            let result: Fr = sponge.squeeze_field_elements(1)[0];
            let bytes = result.into_bigint().to_bytes_le();
            let mut arr = [0u8; 32];
            arr[..bytes.len()].copy_from_slice(&bytes);
            roots.push(arr);
        }

        roots
    }
}

impl Default for MerkleHasher {
    fn default() -> Self {
        Self::new()
    }
}

/// Sparse Merkle Tree for note commitments
///
/// Uses lazy evaluation - only stores non-empty nodes.
pub struct MerkleTree {
    /// Non-empty nodes: (level, index) -> hash
    nodes: HashMap<(usize, u64), [u8; 32]>,
    /// Next available leaf position
    next_index: u64,
    /// Hasher for computing hashes
    hasher: MerkleHasher,
    /// Current root
    root: [u8; 32],
}

impl MerkleTree {
    /// Create a new empty tree
    pub fn new() -> Self {
        let hasher = MerkleHasher::new();
        let root = *hasher.empty_root(TREE_DEPTH);

        Self {
            nodes: HashMap::new(),
            next_index: 0,
            hasher,
            root,
        }
    }

    /// Get current root
    pub fn root(&self) -> [u8; 32] {
        self.root
    }

    /// Get next available position
    pub fn next_position(&self) -> u64 {
        self.next_index
    }

    /// Insert a commitment and return its position
    pub fn insert(&mut self, commitment: &Commitment) -> u64 {
        let position = self.next_index;
        self.insert_at(position, commitment);
        self.next_index += 1;
        position
    }

    /// Insert at a specific position (for reconstruction)
    pub fn insert_at(&mut self, position: u64, commitment: &Commitment) {
        // Insert leaf
        self.nodes.insert((0, position), commitment.0);

        // Update path to root
        let mut current_index = position;
        let mut current_hash = commitment.0;

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
                .unwrap_or_else(|| *self.hasher.empty_root(level));

            let parent_hash = if is_right {
                self.hasher.hash_pair(&sibling, &current_hash)
            } else {
                self.hasher.hash_pair(&current_hash, &sibling)
            };

            current_index /= 2;
            current_hash = parent_hash;

            self.nodes.insert((level + 1, current_index), parent_hash);
        }

        self.root = current_hash;
    }

    /// Get Merkle path for a position
    pub fn path(&self, position: u64) -> Option<MerklePath> {
        if position >= self.next_index {
            return None;
        }

        let mut siblings = Vec::with_capacity(TREE_DEPTH);
        let mut path_bits = Vec::with_capacity(TREE_DEPTH);
        let mut current_index = position;

        for level in 0..TREE_DEPTH {
            let is_right = current_index & 1 == 1;
            path_bits.push(is_right);

            let sibling_index = if is_right {
                current_index - 1
            } else {
                current_index + 1
            };

            let sibling = self
                .nodes
                .get(&(level, sibling_index))
                .copied()
                .unwrap_or_else(|| *self.hasher.empty_root(level));

            siblings.push(sibling);
            current_index /= 2;
        }

        Some(MerklePath {
            siblings,
            path_bits,
            position,
        })
    }

    /// Check if a commitment exists at a position
    pub fn contains(&self, position: u64, commitment: &Commitment) -> bool {
        self.nodes
            .get(&(0, position))
            .map(|h| h == &commitment.0)
            .unwrap_or(false)
    }

    /// Get commitment at position
    pub fn get(&self, position: u64) -> Option<Commitment> {
        self.nodes.get(&(0, position)).map(|h| Commitment(*h))
    }
}

impl Default for MerkleTree {
    fn default() -> Self {
        Self::new()
    }
}

/// Root history for Merkle tree
///
/// Stores recent roots to allow transactions to reference
/// slightly stale roots (handles race conditions).
#[derive(Default)]
pub struct RootHistory {
    /// Recent roots (most recent first)
    roots: Vec<[u8; 32]>,
    /// Maximum history size
    max_size: usize,
}

impl RootHistory {
    pub fn new(max_size: usize) -> Self {
        Self {
            roots: Vec::new(),
            max_size,
        }
    }

    /// Add a new root
    pub fn push(&mut self, root: [u8; 32]) {
        self.roots.insert(0, root);
        if self.roots.len() > self.max_size {
            self.roots.pop();
        }
    }

    /// Check if a root is valid (current or recent)
    pub fn is_valid(&self, root: &[u8; 32]) -> bool {
        self.roots.contains(root)
    }

    /// Get the most recent root
    pub fn current(&self) -> Option<&[u8; 32]> {
        self.roots.first()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_tree() {
        let tree = MerkleTree::new();
        assert_eq!(tree.next_position(), 0);
        // Root should be the empty root
        let hasher = MerkleHasher::new();
        assert_eq!(tree.root(), *hasher.empty_root(TREE_DEPTH));
    }

    #[test]
    fn test_insert_and_path() {
        let mut tree = MerkleTree::new();

        let c1 = Commitment([1u8; 32]);
        let c2 = Commitment([2u8; 32]);

        let pos1 = tree.insert(&c1);
        let pos2 = tree.insert(&c2);

        assert_eq!(pos1, 0);
        assert_eq!(pos2, 1);

        // Get and verify paths
        let path1 = tree.path(0).unwrap();
        assert!(path1.verify(&c1, &tree.root()));

        let path2 = tree.path(1).unwrap();
        assert!(path2.verify(&c2, &tree.root()));
    }

    #[test]
    fn test_path_invalid_commitment() {
        let mut tree = MerkleTree::new();
        let c1 = Commitment([1u8; 32]);
        tree.insert(&c1);

        let path = tree.path(0).unwrap();
        let wrong_commitment = Commitment([99u8; 32]);

        assert!(!path.verify(&wrong_commitment, &tree.root()));
    }

    #[test]
    fn test_root_changes() {
        let mut tree = MerkleTree::new();
        let root0 = tree.root();

        let c1 = Commitment([1u8; 32]);
        tree.insert(&c1);
        let root1 = tree.root();

        assert_ne!(root0, root1, "root should change after insert");

        let c2 = Commitment([2u8; 32]);
        tree.insert(&c2);
        let root2 = tree.root();

        assert_ne!(root1, root2, "root should change after each insert");
    }

    #[test]
    fn test_root_history() {
        let mut history = RootHistory::new(5);

        let r1 = [1u8; 32];
        let r2 = [2u8; 32];
        let r3 = [3u8; 32];

        history.push(r1);
        history.push(r2);
        history.push(r3);

        assert!(history.is_valid(&r1));
        assert!(history.is_valid(&r2));
        assert!(history.is_valid(&r3));
        assert!(!history.is_valid(&[4u8; 32]));

        assert_eq!(history.current(), Some(&r3));
    }
}
