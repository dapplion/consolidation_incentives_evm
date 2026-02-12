//! Sparse Merkle Proof Generation
//!
//! Generates SSZ Merkle proofs without allocating full trees for large lists.
//! This avoids the 140TB memory allocation that ssz_rs's `Prove` trait would
//! require for `List<Validator, 2^40>`.
//!
//! # Approach
//! Instead of using ssz_rs's `Prove` trait (which calls `compute_merkle_tree`
//! allocating the full tree in memory), we:
//! 1. Hash individual elements using `hash_tree_root()`
//! 2. Build proofs through the tree using SHA-256, layer by layer
//! 3. Use precomputed "zero hashes" for empty subtrees
//!
//! This is the same approach used by Ethereum consensus clients like Lighthouse.

use sha2::{Digest, Sha256};
use ssz_rs::prelude::*;

/// Maximum supported tree depth
const MAX_DEPTH: usize = 64;

/// Precomputed zero hashes for each depth level.
/// `ZERO_HASHES[0]` = all-zeros (the zero leaf).
/// `ZERO_HASHES[i]` = hash(ZERO_HASHES[i-1], ZERO_HASHES[i-1])
fn zero_hashes() -> Vec<[u8; 32]> {
    let mut hashes = vec![[0u8; 32]; MAX_DEPTH + 1];
    let mut hasher = Sha256::new();
    for i in 1..=MAX_DEPTH {
        hasher.update(hashes[i - 1]);
        hasher.update(hashes[i - 1]);
        hashes[i] = hasher.finalize_reset().into();
    }
    hashes
}

/// SHA-256 hash of two 32-byte nodes
fn hash_pair(left: &[u8; 32], right: &[u8; 32]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(left);
    hasher.update(right);
    hasher.finalize().into()
}

/// Generate a Merkle proof for `leaf_chunks[index]` in a tree of depth `depth`.
///
/// `leaf_chunks` contains the actual (non-zero) leaves, and `depth` is the
/// total tree depth (i.e., the tree has `2^depth` leaf slots).
///
/// For leaves beyond `leaf_chunks.len()`, zero hashes are used.
///
/// Returns `(proof, root)` where `proof` is the list of sibling hashes
/// from leaf to root (length = depth).
pub fn prove_against_leaf_chunks(
    leaf_chunks: &[[u8; 32]],
    index: usize,
    depth: u32,
) -> (Vec<[u8; 32]>, [u8; 32]) {
    let zh = zero_hashes();
    let leaf_count = 1usize << depth;
    assert!(index < leaf_count, "index {index} out of range for depth {depth}");

    // Build the tree layer by layer from bottom up.
    // We only need to keep track of the current layer and compute parents.
    // But for correctness and simplicity, we'll use a sparse approach:
    // at each level, compute the sibling of the target node's ancestor.

    let mut proof = Vec::with_capacity(depth as usize);

    // Current position in the tree (0-indexed within the layer)
    let mut pos = index;

    // Current layer: start with leaf chunks padded with zeros
    // But we don't want to allocate 2^40 entries. Instead, compute
    // the sibling at each level by only computing what's needed.

    // We use a recursive/iterative approach:
    // At depth d (bottom), the node at position `pos` is leaf_chunks[pos] (or zero).
    // Its sibling is at position pos^1.
    // The sibling might be a single leaf, or might be the root of a subtree that needs computation.

    // For efficiency, we compute sibling subtree roots on-the-fly.
    // The sibling at each level is the hash of a subtree. If all leaves in that
    // subtree are zero, it's just zero_hashes[level]. Otherwise we need to compute it.

    // Strategy: at each level (from 0 = leaves to depth-1):
    //   1. Compute the sibling node's value
    //   2. Add it to the proof
    //   3. Compute the parent and move up

    // To compute a subtree root of arbitrary leaves, we use `compute_subtree_root`.

    for level in 0..depth {
        let sibling_pos = pos ^ 1;
        let subtree_depth = level; // at level 0, siblings are individual leaves; at level 1, siblings are roots of depth-1 subtrees, etc.
        // Wait, let me reconsider. At level 0 (bottom), we're looking at individual leaf positions.
        // The sibling is just leaf_chunks[sibling_pos] or zero.
        // At level 1, after hashing pairs, the sibling is a node that's the hash of 2 leaves.
        // But we don't store intermediate layers.

        // Better approach: compute the sibling hash by computing the root of the subtree
        // that the sibling covers at this level.

        // At level `level`, each node covers 2^level leaves.
        // The sibling at this level covers leaves [sibling_pos * 2^level .. (sibling_pos+1) * 2^level)
        let subtree_size = 1usize << level;
        let start = sibling_pos * subtree_size;

        let sibling_hash = compute_subtree_root(leaf_chunks, start, level as usize, &zh);
        proof.push(sibling_hash);

        pos /= 2;
    }

    // Compute the root
    let root = {
        let mut current = get_leaf(leaf_chunks, index);
        for (level, sibling) in proof.iter().enumerate() {
            let bit = (index >> level) & 1;
            if bit == 0 {
                current = hash_pair(&current, sibling);
            } else {
                current = hash_pair(sibling, &current);
            }
        }
        current
    };

    (proof, root)
}

/// Get a leaf value, returning zero hash if out of bounds
fn get_leaf(leaf_chunks: &[[u8; 32]], index: usize) -> [u8; 32] {
    if index < leaf_chunks.len() {
        leaf_chunks[index]
    } else {
        [0u8; 32]
    }
}

/// Compute the root of a subtree starting at leaf index `start` with depth `depth`.
/// Uses zero hashes for missing leaves.
fn compute_subtree_root(
    leaf_chunks: &[[u8; 32]],
    start: usize,
    depth: usize,
    zh: &[[u8; 32]],
) -> [u8; 32] {
    if depth == 0 {
        return get_leaf(leaf_chunks, start);
    }

    let subtree_size = 1usize << depth;

    // If all leaves in this subtree are beyond our data, use precomputed zero hash
    if start >= leaf_chunks.len() {
        return zh[depth];
    }

    // If this entire subtree is within our data, compute normally
    let half = subtree_size / 2;
    let left = compute_subtree_root(leaf_chunks, start, depth - 1, zh);
    let right = compute_subtree_root(leaf_chunks, start + half, depth - 1, zh);
    hash_pair(&left, &right)
}

/// Mix in the length for a List's Merkle root.
/// `list_root = hash(data_root, length_as_le_bytes32)`
pub fn mix_in_length(data_root: [u8; 32], length: usize) -> [u8; 32] {
    let mut length_bytes = [0u8; 32];
    length_bytes[..8].copy_from_slice(&(length as u64).to_le_bytes());
    hash_pair(&data_root, &length_bytes)
}

/// Generate a Merkle proof for an element within a List<T, N>.
///
/// Returns `(proof_from_leaf_to_list_root, list_root)`.
///
/// The proof includes:
/// 1. Sibling hashes through the data tree (depth = log2(N * item_size / 32))
/// 2. The length mix-in sibling (the length node)
///
/// Total proof length = data_tree_depth + 1
pub fn prove_list_element(
    element_hashes: &[[u8; 32]],
    element_index: usize,
    list_limit_depth: u32,
    list_length: usize,
) -> (Vec<[u8; 32]>, [u8; 32]) {
    // Generate proof through data tree
    let (mut proof, data_root) =
        prove_against_leaf_chunks(element_hashes, element_index, list_limit_depth);

    // Add length mix-in: the sibling at the mix-in level is the length chunk
    let mut length_bytes = [0u8; 32];
    length_bytes[..8].copy_from_slice(&(list_length as u64).to_le_bytes());
    proof.push(length_bytes);

    // Compute list root
    let list_root = hash_pair(&data_root, &length_bytes);

    (proof, list_root)
}

/// Generate a Merkle proof for a field within a container.
///
/// `field_hashes` contains the hash of each field in the container.
/// `field_index` is the index of the target field.
/// `num_fields` is the total number of fields in the container.
///
/// Returns `(proof, container_root)`.
pub fn prove_container_field(
    field_hashes: &[[u8; 32]],
    field_index: usize,
    num_fields: usize,
) -> (Vec<[u8; 32]>, [u8; 32]) {
    let depth = if num_fields <= 1 {
        0
    } else {
        (num_fields as u64).next_power_of_two().trailing_zeros()
    };
    prove_against_leaf_chunks(field_hashes, field_index, depth)
}

/// Generate a proof for a field within a fixed-size SSZ container (like Validator).
///
/// Uses ssz_rs's `prove` for small types where it's efficient.
pub fn prove_small_container_field<T: SimpleSerialize>(
    container: &T,
    path: &[PathElement],
) -> Result<(Vec<[u8; 32]>, [u8; 32], [u8; 32]), MerkleizationError> {
    let (proof, witness) = container.prove(path)?;
    let branch: Vec<[u8; 32]> = proof.branch.into_iter().map(|n| n.into()).collect();
    let leaf: [u8; 32] = proof.leaf.into();
    let root: [u8; 32] = witness.into();
    Ok((branch, leaf, root))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zero_hashes() {
        let zh = zero_hashes();
        assert_eq!(zh[0], [0u8; 32]);
        // zh[1] = hash(zero, zero)
        let expected = hash_pair(&[0u8; 32], &[0u8; 32]);
        assert_eq!(zh[1], expected);
        // zh[2] = hash(zh[1], zh[1])
        let expected2 = hash_pair(&zh[1], &zh[1]);
        assert_eq!(zh[2], expected2);
    }

    #[test]
    fn test_prove_single_leaf() {
        let leaves = vec![[1u8; 32]];
        let (proof, root) = prove_against_leaf_chunks(&leaves, 0, 1);
        // depth 1: 2 leaves. leaf[0] = [1;32], leaf[1] = [0;32]
        assert_eq!(proof.len(), 1);
        assert_eq!(proof[0], [0u8; 32]); // sibling is zero
        assert_eq!(root, hash_pair(&[1u8; 32], &[0u8; 32]));
    }

    #[test]
    fn test_prove_depth_0() {
        let leaves = vec![[42u8; 32]];
        let (proof, root) = prove_against_leaf_chunks(&leaves, 0, 0);
        assert_eq!(proof.len(), 0);
        assert_eq!(root, [42u8; 32]);
    }

    #[test]
    fn test_prove_two_leaves() {
        let leaves = vec![[1u8; 32], [2u8; 32]];
        let (proof, root) = prove_against_leaf_chunks(&leaves, 0, 1);
        assert_eq!(proof.len(), 1);
        assert_eq!(proof[0], [2u8; 32]);
        assert_eq!(root, hash_pair(&[1u8; 32], &[2u8; 32]));

        // Prove index 1
        let (proof, root2) = prove_against_leaf_chunks(&leaves, 1, 1);
        assert_eq!(proof.len(), 1);
        assert_eq!(proof[0], [1u8; 32]);
        assert_eq!(root2, root); // Same root
    }

    #[test]
    fn test_prove_with_virtual_padding() {
        // 3 actual leaves in a depth-2 tree (4 leaf slots)
        let leaves = vec![[1u8; 32], [2u8; 32], [3u8; 32]];
        let zh = zero_hashes();

        // Prove leaf at index 0
        let (proof, root) = prove_against_leaf_chunks(&leaves, 0, 2);
        assert_eq!(proof.len(), 2);
        assert_eq!(proof[0], [2u8; 32]); // sibling at depth 0
        // sibling at depth 1 is hash(leaf[2], leaf[3]=zero)
        let right_subtree = hash_pair(&[3u8; 32], &zh[0]);
        assert_eq!(proof[1], right_subtree);

        // Verify: recompute root from proof
        let mut current = [1u8; 32];
        current = hash_pair(&current, &proof[0]); // hash(leaf[0], leaf[1])
        current = hash_pair(&current, &proof[1]); // hash(left_subtree, right_subtree)
        assert_eq!(current, root);
    }

    #[test]
    fn test_prove_large_depth_sparse() {
        // Only 2 leaves, but depth 20 (simulating a large list)
        let leaves = vec![[0xAA; 32], [0xBB; 32]];
        let (proof, _root) = prove_against_leaf_chunks(&leaves, 0, 20);
        assert_eq!(proof.len(), 20);

        // All siblings above depth 1 should be zero hashes
        // (since only indices 0 and 1 have data)
        let zh = zero_hashes();
        assert_eq!(proof[0], [0xBB; 32]); // sibling leaf
        for i in 1..20 {
            assert_eq!(proof[i], zh[i], "sibling at level {i} should be zero hash");
        }
    }

    #[test]
    fn test_prove_list_element_simple() {
        // List with 2 elements, limit depth 2 (limit = 4)
        let elements = vec![[0xAA; 32], [0xBB; 32]];
        let (proof, list_root) = prove_list_element(&elements, 0, 2, 2);

        // Proof should have depth 2 (data tree) + 1 (length mixin) = 3 elements
        assert_eq!(proof.len(), 3);

        // Last element is the length chunk
        let mut length_bytes = [0u8; 32];
        length_bytes[..8].copy_from_slice(&2u64.to_le_bytes());
        assert_eq!(proof[2], length_bytes);

        // Verify root
        let (_, data_root) = prove_against_leaf_chunks(&elements, 0, 2);
        assert_eq!(list_root, hash_pair(&data_root, &length_bytes));
    }

    #[test]
    fn test_verify_proof_with_ssz_rs() {
        // Generate proof with our code, verify with ssz_rs
        let leaves = vec![[1u8; 32], [2u8; 32], [3u8; 32], [4u8; 32]];
        let (proof, root) = prove_against_leaf_chunks(&leaves, 2, 2);

        let root_node = Node::try_from(root.as_slice()).unwrap();
        let leaf_node = Node::try_from(leaves[2].as_slice()).unwrap();
        let branch: Vec<Node> = proof
            .iter()
            .map(|b| Node::try_from(b.as_slice()).unwrap())
            .collect();

        // gindex for index 2 at depth 2 = 4 + 2 = 6
        ssz_rs::proofs::is_valid_merkle_branch_for_generalized_index(
            leaf_node, &branch, 6, root_node,
        )
        .expect("proof should be valid");
    }

    #[test]
    fn test_prove_container_field_simple() {
        // 4-field container
        let fields = vec![[1u8; 32], [2u8; 32], [3u8; 32], [4u8; 32]];
        let (proof, root) = prove_container_field(&fields, 1, 4);

        // depth = 2 for 4 fields
        assert_eq!(proof.len(), 2);
        assert_eq!(proof[0], [1u8; 32]); // sibling of field 1 is field 0

        // Verify
        let mut current = [2u8; 32]; // field 1
        // index 1 -> bit 0 is 1 (right child), bit 1 is 0 (left child)
        current = hash_pair(&proof[0], &current); // hash(field[0], field[1])
        current = hash_pair(&current, &proof[1]); // hash(left_pair, right_pair)
        assert_eq!(current, root);
    }
}
