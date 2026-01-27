// Merkle Tree Operations
//
// Implements a sparse Merkle tree with 32 levels (supporting 2^32 leaves).
// Uses Poseidon hash for all internal nodes.
//
// Tree structure:
//                    Root (level 32)
//                   /              \
//              H(0,1)              H(2,3)
//             /      \            /      \
//          H(0)     H(1)       H(2)     H(3)
//           |        |          |        |
//        Leaf0    Leaf1      Leaf2    Leaf3

use crate::poseidon::hash_2;

// Tree depth - supports 2^32 leaves
global TREE_DEPTH: u32 = 32;

/// Compute Merkle root from a leaf and authentication path
///
/// # Arguments
/// * `leaf` - The leaf value (already hashed)
/// * `path` - Array of sibling hashes from leaf to root
/// * `path_indices` - Array of bits indicating left(0) or right(1) position
///
/// # Returns
/// The computed Merkle root
pub fn compute_merkle_root(
    leaf: Field,
    path: [Field; 32],
    path_indices: [Field; 32]  // 0 = left, 1 = right
) -> Field {
    let mut current = leaf;
    
    for i in 0..TREE_DEPTH {
        let sibling = path[i];
        let is_right = path_indices[i];
        
        // If is_right == 1, current is on right, so hash(sibling, current)
        // If is_right == 0, current is on left, so hash(current, sibling)
        let (left, right) = if is_right == 1 {
            (sibling, current)
        } else {
            (current, sibling)
        };
        
        current = hash_2(left, right);
    }
    
    current
}

/// Verify that a leaf is included in a Merkle tree with given root
///
/// # Arguments
/// * `leaf` - The leaf value to verify
/// * `root` - The expected Merkle root
/// * `path` - Authentication path (sibling hashes)
/// * `path_indices` - Position bits for the path
///
/// # Returns
/// true if the leaf is included, false otherwise
pub fn verify_merkle_path(
    leaf: Field,
    root: Field,
    path: [Field; 32],
    path_indices: [Field; 32]
) -> bool {
    let computed_root = compute_merkle_root(leaf, path, path_indices);
    computed_root == root
}

/// Compute the new root after updating a leaf
///
/// # Arguments
/// * `old_leaf` - The old leaf value
/// * `new_leaf` - The new leaf value
/// * `path` - Authentication path for the leaf position
/// * `path_indices` - Position bits
/// * `old_root` - The current root (for verification)
///
/// # Returns
/// The new Merkle root after the update
pub fn update_merkle_root(
    old_leaf: Field,
    new_leaf: Field,
    path: [Field; 32],
    path_indices: [Field; 32],
    old_root: Field
) -> Field {
    // First verify old_leaf was in the tree
    let computed_old_root = compute_merkle_root(old_leaf, path, path_indices);
    assert(computed_old_root == old_root, "Old leaf not in tree");
    
    // Compute new root with updated leaf
    compute_merkle_root(new_leaf, path, path_indices)
}

/// Compute leaf index from path indices
pub fn path_to_index(path_indices: [Field; 32]) -> Field {
    let mut index: Field = 0;
    let mut power: Field = 1;
    
    for i in 0..TREE_DEPTH {
        index = index + path_indices[i] * power;
        power = power * 2;
    }
    
    index
}

/// Empty leaf hash (Poseidon of zero)
pub fn empty_leaf() -> Field {
    hash_2(0, 0)
}

#[test]
fn test_merkle_path_consistency() {
    // Create a simple path verification
    let leaf = 12345;
    let path = [0; 32];  // All zeros (empty siblings)
    let indices = [0; 32];  // All left positions
    
    let root = compute_merkle_root(leaf, path, indices);
    
    // Verify it's consistent
    assert(verify_merkle_path(leaf, root, path, indices));
}

#[test]
fn test_merkle_update() {
    let old_leaf = 100;
    let new_leaf = 200;
    let path = [0; 32];
    let indices = [0; 32];
    
    let old_root = compute_merkle_root(old_leaf, path, indices);
    let new_root = update_merkle_root(old_leaf, new_leaf, path, indices, old_root);
    
    // Verify new leaf is now in tree
    assert(verify_merkle_path(new_leaf, new_root, path, indices));
    
    // Old leaf should NOT be in new tree
    assert(!verify_merkle_path(old_leaf, new_root, path, indices));
}
