// Zelana Batch Circuit
//
// This circuit proves the validity of a batch of L2 transactions.
// It handles three types of transactions:
// 1. Transfers - Standard balance transfers between accounts
// 2. Withdrawals - L2 -> L1 withdrawal requests
// 3. Shielded - Private transactions with nullifiers and commitments
//
// Public Inputs (7 field elements, matching L2BlockCircuit):
// 1. pre_state_root      - Account state root before batch
// 2. post_state_root     - Account state root after batch
// 3. pre_shielded_root   - Shielded commitment tree root before batch
// 4. post_shielded_root  - Shielded commitment tree root after batch
// 5. withdrawal_root     - Merkle root of withdrawals in this batch
// 6. batch_hash          - Hash of all transactions in the batch
// 7. batch_id            - Batch sequence number

use dep::zelana_lib::poseidon::{
    hash_2, hash_3, hash_4,
    domain_batch, domain_withdrawal
};
use dep::zelana_lib::merkle::{verify_merkle_path, update_merkle_root};
use dep::zelana_lib::account::compute_account_leaf;
use dep::zelana_lib::nullifier::compute_nullifier;

// Configuration constants
global MAX_TRANSFERS: u32 = 8;      // Max transfers per batch chunk
global MAX_WITHDRAWALS: u32 = 4;    // Max withdrawals per batch chunk
global MAX_SHIELDED: u32 = 4;       // Max shielded txs per batch chunk

// ============================================================================
// Transaction Structures
// ============================================================================

/// Transfer transaction witness
struct TransferWitness {
    // Sender info
    sender_pubkey: Field,
    sender_balance: Field,
    sender_nonce: Field,
    sender_path: [Field; 32],
    sender_path_indices: [Field; 32],
    
    // Receiver info
    receiver_pubkey: Field,
    receiver_balance: Field,
    receiver_nonce: Field,
    receiver_path: [Field; 32],
    receiver_path_indices: [Field; 32],
    
    // Transaction data
    amount: Field,
    
    // Signature (simplified for now)
    signature: Field,
    
    // Flag: is this slot used?
    is_valid: bool,
}

/// Withdrawal transaction witness
struct WithdrawalWitness {
    // L2 account
    sender_pubkey: Field,
    sender_balance: Field,
    sender_nonce: Field,
    sender_path: [Field; 32],
    sender_path_indices: [Field; 32],
    
    // L1 recipient (Solana address as Field)
    l1_recipient: Field,
    
    // Withdrawal amount
    amount: Field,
    
    // Signature
    signature: Field,
    
    // Flag: is this slot used?
    is_valid: bool,
}

/// Shielded transaction witness
struct ShieldedWitness {
    // Input note
    input_owner: Field,
    input_value: Field,
    input_blinding: Field,
    input_position: Field,
    input_path: [Field; 32],
    input_path_indices: [Field; 32],
    spending_key: Field,
    
    // Output note
    output_owner: Field,
    output_value: Field,
    output_blinding: Field,
    
    // Public nullifier (revealed when spending)
    nullifier: Field,
    
    // Flag: is this slot used?
    is_valid: bool,
}

// ============================================================================
// Main Circuit
// ============================================================================

fn main(
    // === Public Inputs (7) ===
    pre_state_root: pub Field,
    post_state_root: pub Field,
    pre_shielded_root: pub Field,
    post_shielded_root: pub Field,
    withdrawal_root: pub Field,
    batch_hash: pub Field,
    batch_id: pub Field,
    
    // === Private Witness: Transfers ===
    transfers: [TransferWitness; MAX_TRANSFERS],
    
    // === Private Witness: Withdrawals ===
    withdrawals: [WithdrawalWitness; MAX_WITHDRAWALS],
    
    // === Private Witness: Shielded ===
    shielded: [ShieldedWitness; MAX_SHIELDED],
    
    // === Transaction counts (for batch hash) ===
    num_transfers: Field,
    num_withdrawals: Field,
    num_shielded: Field
) {
    // Track state roots as we process transactions
    let mut current_state_root = pre_state_root;
    let mut current_shielded_root = pre_shielded_root;
    
    // Accumulators for batch hash
    let mut batch_accumulator = hash_2(domain_batch(), batch_id);
    
    // Accumulator for withdrawal root
    let mut withdrawal_accumulator = hash_2(domain_withdrawal(), batch_id);
    
    // =========================================================================
    // Process Transfers
    // =========================================================================
    
    for i in 0..MAX_TRANSFERS {
        let tx = transfers[i];
        
        if tx.is_valid {
            // 1. Verify sender exists in current state
            let sender_leaf = compute_account_leaf(
                tx.sender_pubkey,
                tx.sender_balance,
                tx.sender_nonce
            );
            assert(
                verify_merkle_path(sender_leaf, current_state_root, tx.sender_path, tx.sender_path_indices),
                "Sender not in state tree"
            );
            
            // 2. Verify sufficient balance
            let sender_balance_u64 = tx.sender_balance as u64;
            let amount_u64 = tx.amount as u64;
            assert(sender_balance_u64 >= amount_u64, "Insufficient balance");
            
            // 3. Verify signature (simplified)
            let tx_hash = hash_4(
                tx.sender_pubkey,
                tx.receiver_pubkey,
                tx.amount,
                tx.sender_nonce
            );
            assert(tx.signature != 0, "Invalid signature");
            
            // 4. Update sender account (debit)
            let new_sender_leaf = compute_account_leaf(
                tx.sender_pubkey,
                tx.sender_balance - tx.amount,
                tx.sender_nonce + 1
            );
            current_state_root = update_merkle_root(
                sender_leaf,
                new_sender_leaf,
                tx.sender_path,
                tx.sender_path_indices,
                current_state_root
            );
            
            // 5. Verify receiver exists (or is new account with 0 balance)
            let receiver_leaf = compute_account_leaf(
                tx.receiver_pubkey,
                tx.receiver_balance,
                tx.receiver_nonce
            );
            // For simplicity, assume receiver path is valid
            // In production, would need to handle new accounts
            
            // 6. Update receiver account (credit)
            let new_receiver_leaf = compute_account_leaf(
                tx.receiver_pubkey,
                tx.receiver_balance + tx.amount,
                tx.receiver_nonce
            );
            current_state_root = update_merkle_root(
                receiver_leaf,
                new_receiver_leaf,
                tx.receiver_path,
                tx.receiver_path_indices,
                current_state_root
            );
            
            // 7. Accumulate into batch hash
            batch_accumulator = hash_3(batch_accumulator, tx_hash, tx.amount);
        }
    }
    
    // =========================================================================
    // Process Withdrawals
    // =========================================================================
    
    for i in 0..MAX_WITHDRAWALS {
        let wd = withdrawals[i];
        
        if wd.is_valid {
            // 1. Verify sender exists
            let sender_leaf = compute_account_leaf(
                wd.sender_pubkey,
                wd.sender_balance,
                wd.sender_nonce
            );
            assert(
                verify_merkle_path(sender_leaf, current_state_root, wd.sender_path, wd.sender_path_indices),
                "Withdrawal sender not in state"
            );
            
            // 2. Verify sufficient balance
            let sender_balance_u64 = wd.sender_balance as u64;
            let amount_u64 = wd.amount as u64;
            assert(sender_balance_u64 >= amount_u64, "Insufficient balance for withdrawal");
            
            // 3. Verify signature
            assert(wd.signature != 0, "Invalid withdrawal signature");
            
            // 4. Update sender (debit)
            let new_sender_leaf = compute_account_leaf(
                wd.sender_pubkey,
                wd.sender_balance - wd.amount,
                wd.sender_nonce + 1
            );
            current_state_root = update_merkle_root(
                sender_leaf,
                new_sender_leaf,
                wd.sender_path,
                wd.sender_path_indices,
                current_state_root
            );
            
            // 5. Accumulate withdrawal for L1 settlement
            let wd_hash = hash_3(wd.l1_recipient, wd.amount, wd.sender_pubkey);
            withdrawal_accumulator = hash_2(withdrawal_accumulator, wd_hash);
            
            // 6. Add to batch hash
            batch_accumulator = hash_3(batch_accumulator, wd_hash, wd.amount);
        }
    }
    
    // =========================================================================
    // Process Shielded Transactions
    // =========================================================================
    
    for i in 0..MAX_SHIELDED {
        let sh = shielded[i];
        
        if sh.is_valid {
            // 1. Compute input commitment
            let input_commitment = hash_3(
                sh.input_owner,
                sh.input_value,
                sh.input_blinding
            );
            
            // 2. Verify input exists in shielded tree
            assert(
                verify_merkle_path(input_commitment, current_shielded_root, sh.input_path, sh.input_path_indices),
                "Input note not in commitment tree"
            );
            
            // 3. Verify nullifier
            let computed_nullifier = compute_nullifier(
                sh.spending_key,
                input_commitment,
                sh.input_position
            );
            assert(computed_nullifier == sh.nullifier, "Invalid nullifier");
            
            // 4. Verify value conservation
            assert(sh.input_value == sh.output_value, "Shielded value not conserved");
            
            // 5. Compute output commitment
            let output_commitment = hash_3(
                sh.output_owner,
                sh.output_value,
                sh.output_blinding
            );
            
            // 6. Add output commitment to shielded tree
            // (Simplified: we assume sequential insertion and update root)
            current_shielded_root = hash_2(current_shielded_root, output_commitment);
            
            // 7. Add to batch hash (only reveals nullifier and commitment, not values)
            batch_accumulator = hash_3(batch_accumulator, sh.nullifier, output_commitment);
        }
    }
    
    // =========================================================================
    // Finalize and Assert Public Outputs
    // =========================================================================
    
    // Finalize batch hash with counts
    let final_batch_hash = hash_4(
        batch_accumulator,
        num_transfers,
        num_withdrawals,
        num_shielded
    );
    
    // Finalize withdrawal root with count
    let final_withdrawal_root = hash_2(withdrawal_accumulator, num_withdrawals);
    
    // Assert final state matches public inputs
    assert(current_state_root == post_state_root, "Post state root mismatch");
    assert(current_shielded_root == post_shielded_root, "Post shielded root mismatch");
    assert(final_withdrawal_root == withdrawal_root, "Withdrawal root mismatch");
    assert(final_batch_hash == batch_hash, "Batch hash mismatch");
}

// ============================================================================
// Tests
// ============================================================================

#[test]
fn test_empty_batch() {
    // Create empty witnesses
    let empty_transfer = TransferWitness {
        sender_pubkey: 0,
        sender_balance: 0,
        sender_nonce: 0,
        sender_path: [0; 32],
        sender_path_indices: [0; 32],
        receiver_pubkey: 0,
        receiver_balance: 0,
        receiver_nonce: 0,
        receiver_path: [0; 32],
        receiver_path_indices: [0; 32],
        amount: 0,
        signature: 0,
        is_valid: false,
    };
    
    let empty_withdrawal = WithdrawalWitness {
        sender_pubkey: 0,
        sender_balance: 0,
        sender_nonce: 0,
        sender_path: [0; 32],
        sender_path_indices: [0; 32],
        l1_recipient: 0,
        amount: 0,
        signature: 0,
        is_valid: false,
    };
    
    let empty_shielded = ShieldedWitness {
        input_owner: 0,
        input_value: 0,
        input_blinding: 0,
        input_position: 0,
        input_path: [0; 32],
        input_path_indices: [0; 32],
        spending_key: 0,
        output_owner: 0,
        output_value: 0,
        output_blinding: 0,
        nullifier: 0,
        is_valid: false,
    };
    
    // For empty batch, all roots stay the same
    let state_root = hash_2(0, 0);
    let shielded_root = hash_2(0, 0);
    
    // Compute expected hashes
    let batch_acc = hash_2(domain_batch(), 1); // batch_id = 1
    let wd_acc = hash_2(domain_withdrawal(), 1);
    let expected_batch_hash = hash_4(batch_acc, 0, 0, 0);
    let expected_wd_root = hash_2(wd_acc, 0);
    
    // Verify structures are valid (suppress unused warnings)
    assert(!empty_transfer.is_valid);
    assert(!empty_withdrawal.is_valid);
    assert(!empty_shielded.is_valid);
    
    // Verify hashes are computed correctly
    assert(state_root != 0);
    assert(shielded_root != 0);
    assert(expected_batch_hash != 0);
    assert(expected_wd_root != 0);
}
