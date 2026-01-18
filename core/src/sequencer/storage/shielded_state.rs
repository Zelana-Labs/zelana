#![allow(dead_code)] // Some methods reserved for shielded transaction feature
//! Shielded State Management
//!
//! Manages the shielded pool state including:
//! - Note commitment Merkle tree (with frontier-based persistence)
//! - Nullifier set for double-spend prevention
//! - Encrypted notes for viewing key scanning
//!
//! Uses frontier-based persistence: only stores commitments + frontier nodes,
//! not the full tree. This enables fast startup with O(depth) storage overhead.

use anyhow::Result;
use log::info;
use std::collections::HashSet;

use zelana_privacy::{
    Commitment, EncryptedNote, MerkleHasher, MerklePath, MerkleTree, Nullifier, RootHistory,
    TREE_DEPTH,
};

use super::db::RocksDbStore;

/// Maximum number of recent roots to keep for race condition tolerance
const ROOT_HISTORY_SIZE: usize = 100;

/// Frontier node for incremental tree persistence
/// Stores the rightmost nodes at each level needed to compute the root
#[derive(Debug, Clone)]
pub struct TreeFrontier {
    /// Frontier nodes at each level (index 0 = leaf level)
    pub frontier: Vec<Option<[u8; 32]>>,
    /// Next insertion position
    pub next_position: u64,
}

impl TreeFrontier {
    /// Create empty frontier
    pub fn new() -> Self {
        Self {
            frontier: vec![None; TREE_DEPTH],
            next_position: 0,
        }
    }

    /// Update frontier after inserting a leaf
    pub fn insert(&mut self, leaf: [u8; 32], hasher: &MerkleHasher) -> [u8; 32] {
        let position = self.next_position;
        self.next_position += 1;

        let mut current = leaf;
        let mut current_position = position;

        for level in 0..TREE_DEPTH {
            let is_right = current_position & 1 == 1;

            if is_right {
                // We're on the right, so there's a left sibling in frontier
                let left = self.frontier[level].unwrap_or(*hasher.empty_root(level));
                current = hasher.hash_pair(&left, &current);
                // Clear this level's frontier (we've moved past it)
                self.frontier[level] = None;
            } else {
                // We're on the left, store in frontier and use empty for right
                self.frontier[level] = Some(current);
                current = hasher.hash_pair(&current, hasher.empty_root(level));
            }

            current_position /= 2;
        }

        current // This is the new root
    }
}

impl Default for TreeFrontier {
    fn default() -> Self {
        Self::new()
    }
}

/// Shielded pool state
pub struct ShieldedState {
    /// Sparse Merkle tree of note commitments
    tree: MerkleTree,

    /// Tree frontier for incremental persistence
    frontier: TreeFrontier,

    /// Set of spent nullifiers
    nullifiers: HashSet<Nullifier>,

    /// Recent valid roots (for race condition tolerance)
    root_history: RootHistory,

    /// Hasher instance
    hasher: MerkleHasher,
}

impl ShieldedState {
    /// Create a new empty shielded state
    pub fn new() -> Self {
        let hasher = MerkleHasher::new();
        let tree = MerkleTree::new();
        let mut root_history = RootHistory::new(ROOT_HISTORY_SIZE);
        root_history.push(tree.root());

        Self {
            tree,
            frontier: TreeFrontier::new(),
            nullifiers: HashSet::new(),
            root_history,
            hasher,
        }
    }

    /// Load shielded state from database
    ///
    /// Reconstructs tree from persisted commitments and frontier
    pub fn load(db: &RocksDbStore) -> Result<Self> {
        let mut state = Self::new();

        // Load nullifiers
        let nullifiers = db.get_all_nullifiers()?;
        state.nullifiers = nullifiers.into_iter().collect();

        // Load commitments and rebuild tree
        let commitments = db.get_all_commitments()?;
        for (position, commitment) in commitments {
            let c = Commitment(commitment);
            state.tree.insert_at(position as u64, &c);

            // Update frontier
            state.frontier.next_position = (position + 1) as u64;
        }

        // Update root history with current root
        state.root_history.push(state.tree.root());

        info!(
            "Loaded shielded state: {} commitments, {} nullifiers",
            state.tree.next_position(),
            state.nullifiers.len()
        );

        Ok(state)
    }

    /// Insert a new note commitment
    ///
    /// Returns the position where the commitment was inserted
    pub fn insert_commitment(&mut self, commitment: Commitment) -> u32 {
        let position = self.tree.insert(&commitment) as u32;

        // Update frontier
        self.frontier.insert(commitment.0, &self.hasher);

        // Update root history
        self.root_history.push(self.tree.root());

        position
    }

    /// Check if a nullifier has been spent
    pub fn nullifier_exists(&self, nullifier: &Nullifier) -> bool {
        self.nullifiers.contains(nullifier)
    }

    /// Mark a nullifier as spent
    pub fn spend_nullifier(&mut self, nullifier: Nullifier) -> Result<()> {
        if self.nullifiers.contains(&nullifier) {
            anyhow::bail!("Nullifier already spent (double-spend attempt)");
        }
        self.nullifiers.insert(nullifier);
        Ok(())
    }

    /// Verify that a commitment is included in the tree
    ///
    /// Accepts current root or any recent root (for race condition tolerance)
    pub fn verify_inclusion(
        &self,
        commitment: &Commitment,
        path: &MerklePath,
        claimed_root: &[u8; 32],
    ) -> bool {
        // First check if the root is valid
        if !self.root_history.is_valid(claimed_root) {
            return false;
        }

        // Verify the path
        path.verify(commitment, claimed_root)
    }

    /// Get the current merkle root
    pub fn root(&self) -> [u8; 32] {
        self.tree.root()
    }

    /// Get the next available commitment position
    pub fn next_position(&self) -> u32 {
        self.tree.next_position() as u32
    }

    /// Get a merkle path for a commitment at a given position
    pub fn get_path(&self, position: u32) -> Option<MerklePath> {
        self.tree.path(position as u64)
    }

    /// Get commitment at a position
    pub fn get_commitment(&self, position: u32) -> Option<Commitment> {
        self.tree.get(position as u64)
    }

    /// Check if a root is valid (current or recent)
    pub fn is_valid_root(&self, root: &[u8; 32]) -> bool {
        self.root_history.is_valid(root)
    }

    /// Get the number of nullifiers
    pub fn nullifier_count(&self) -> usize {
        self.nullifiers.len()
    }

    /// Get the number of commitments
    pub fn commitment_count(&self) -> u64 {
        self.tree.next_position()
    }

    /// Persist new state to database
    ///
    /// Only persists the diff since last persist:
    /// - New commitments
    /// - New nullifiers
    /// - Updated frontier
    pub fn persist_diff(
        &self,
        db: &RocksDbStore,
        new_commitments: &[(u32, Commitment, EncryptedNote)],
        new_nullifiers: &[Nullifier],
    ) -> Result<()> {
        // Persist new commitments
        for (position, commitment, encrypted_note) in new_commitments {
            db.insert_commitment(*position, commitment.0)?;
            db.store_encrypted_note(commitment.0, encrypted_note)?;
        }

        // Persist new nullifiers
        for nullifier in new_nullifiers {
            db.mark_nullifier(&nullifier.0)?;
        }

        // Update frontier (for fast restart)
        db.store_tree_frontier(&self.frontier)?;

        Ok(())
    }
}

impl Default for ShieldedState {
    fn default() -> Self {
        Self::new()
    }
}

/// State diff for shielded operations in a batch
#[derive(Debug, Clone, Default)]
pub struct ShieldedStateDiff {
    /// New commitments added: (position, commitment, encrypted_note)
    pub new_commitments: Vec<(u32, Commitment, EncryptedNote)>,

    /// Nullifiers spent
    pub spent_nullifiers: Vec<Nullifier>,
}

impl ShieldedStateDiff {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_commitment(&mut self, position: u32, commitment: Commitment, note: EncryptedNote) {
        self.new_commitments.push((position, commitment, note));
    }

    pub fn add_nullifier(&mut self, nullifier: Nullifier) {
        self.spent_nullifiers.push(nullifier);
    }

    pub fn is_empty(&self) -> bool {
        self.new_commitments.is_empty() && self.spent_nullifiers.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shielded_state_new() {
        let state = ShieldedState::new();
        assert_eq!(state.commitment_count(), 0);
        assert_eq!(state.nullifier_count(), 0);
    }

    #[test]
    fn test_insert_commitment() {
        let mut state = ShieldedState::new();

        let c1 = Commitment([1u8; 32]);
        let c2 = Commitment([2u8; 32]);

        let pos1 = state.insert_commitment(c1);
        let pos2 = state.insert_commitment(c2);

        assert_eq!(pos1, 0);
        assert_eq!(pos2, 1);
        assert_eq!(state.commitment_count(), 2);
    }

    #[test]
    fn test_nullifier_double_spend() {
        let mut state = ShieldedState::new();

        let nullifier = Nullifier([42u8; 32]);

        // First spend should succeed
        assert!(state.spend_nullifier(nullifier).is_ok());

        // Second spend should fail
        assert!(state.spend_nullifier(nullifier).is_err());
    }

    #[test]
    fn test_merkle_path_verification() {
        let mut state = ShieldedState::new();

        let commitment = Commitment([1u8; 32]);
        let pos = state.insert_commitment(commitment);

        let path = state.get_path(pos).unwrap();
        let root = state.root();

        assert!(state.verify_inclusion(&commitment, &path, &root));
    }

    #[test]
    fn test_root_history() {
        let mut state = ShieldedState::new();

        let root0 = state.root();

        let c1 = Commitment([1u8; 32]);
        state.insert_commitment(c1);
        let root1 = state.root();

        // Both roots should be valid
        assert!(state.is_valid_root(&root0));
        assert!(state.is_valid_root(&root1));

        // Unknown root should be invalid
        assert!(!state.is_valid_root(&[99u8; 32]));
    }
}
