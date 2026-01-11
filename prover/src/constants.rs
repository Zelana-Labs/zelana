/// Maximum number of transactions per L2 batch.
/// Actual batch size ≤ MAX_TXS, remaining slots are dummy txs.
pub const MAX_TXS: usize = 64;

/// Maximum Merkle tree depth for accounts
pub const MERKLE_DEPTH: usize = 32;
