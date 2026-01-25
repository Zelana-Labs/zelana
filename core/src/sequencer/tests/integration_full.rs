//! Full Integration Tests
//!
//! Tests the complete L2 cycle including shielded transactions,
//! fast withdrawals, and threshold encryption.

use crate::sequencer::{
    EncryptedMempoolConfig, PendingWithdrawal, RocksDbStore, ShieldedState,
    ThresholdMempoolManager, WithdrawalQueue, WithdrawalState, create_test_committee,
};
use crate::storage::state::StateStore;
use std::sync::Arc;
use tempfile::TempDir;

use zelana_account::{AccountId, AccountState};
use zelana_privacy::{Commitment, Nullifier};
use zelana_threshold::encrypt_for_committee;

// ============================================================================
// Test Helpers
// ============================================================================

fn temp_db() -> RocksDbStore {
    let dir = TempDir::new().unwrap();
    RocksDbStore::open(dir.path()).unwrap()
}

fn account(id: u8) -> AccountId {
    let mut b = [0u8; 32];
    b[0] = id;
    AccountId(b)
}

// ============================================================================
// Shielded State Tests
// ============================================================================

#[test]
fn test_shielded_commitment_lifecycle() {
    let mut state = ShieldedState::new();

    // Add some commitments
    let commitment1 = Commitment([1u8; 32]);
    let commitment2 = Commitment([2u8; 32]);
    let commitment3 = Commitment([3u8; 32]);

    let pos1 = state.insert_commitment(commitment1);
    let pos2 = state.insert_commitment(commitment2);
    let pos3 = state.insert_commitment(commitment3);

    assert_eq!(pos1, 0);
    assert_eq!(pos2, 1);
    assert_eq!(pos3, 2);
    assert_eq!(state.commitment_count(), 3);

    // Check root changed
    let root = state.root();
    assert_ne!(root, [0u8; 32]);

    // Get merkle paths
    let path1 = state.get_path(pos1);
    assert!(path1.is_some());
    let path1 = path1.unwrap();
    assert!(!path1.siblings.is_empty());

    // Add nullifiers
    let nullifier1 = Nullifier([10u8; 32]);
    let nullifier2 = Nullifier([20u8; 32]);

    assert!(!state.nullifier_exists(&nullifier1));
    state.spend_nullifier(nullifier1).unwrap();
    assert!(state.nullifier_exists(&nullifier1));
    assert!(!state.nullifier_exists(&nullifier2));
    assert_eq!(state.nullifier_count(), 1);
}

#[test]
fn test_shielded_merkle_root_consistency() {
    let mut state1 = ShieldedState::new();
    let mut state2 = ShieldedState::new();

    // Add same commitments in same order
    let commitments: Vec<Commitment> = (0..5)
        .map(|i| {
            let mut bytes = [0u8; 32];
            bytes[0] = i;
            Commitment(bytes)
        })
        .collect();

    for cm in &commitments {
        state1.insert_commitment(*cm);
        state2.insert_commitment(*cm);
    }

    // Roots should be identical
    assert_eq!(state1.root(), state2.root());
}

// ============================================================================
// Withdrawal Queue Tests
// ============================================================================

#[test]
fn test_withdrawal_queue_lifecycle() {
    let db = Arc::new(temp_db());
    let mut queue = WithdrawalQueue::new(db);

    let tx_hash: [u8; 32] = [42u8; 32];
    let from = account(1);
    let to_l1: [u8; 32] = [99u8; 32];

    // Create a pending withdrawal
    let pending = PendingWithdrawal {
        tx_hash,
        from,
        to_l1_address: to_l1,
        amount: 1000,
        l2_nonce: 0,
    };

    // Add the withdrawal
    queue.add(pending).unwrap();

    // Check it's pending
    let withdrawal = queue.get(&tx_hash);
    assert!(withdrawal.is_some());
    let w = withdrawal.unwrap();
    assert_eq!(w.amount, 1000);
    assert!(matches!(w.state, WithdrawalState::Pending));

    // Get pending
    let pending_list = queue.get_pending();
    assert_eq!(pending_list.len(), 1);

    // Mark as in batch
    queue.mark_in_batch(&[tx_hash], 1).unwrap();
    let w = queue.get(&tx_hash).unwrap();
    assert!(matches!(w.state, WithdrawalState::InBatch { batch_id: 1 }));

    // Mark submitted
    queue.mark_submitted(1, "Abc123TxSig".to_string()).unwrap();
    let w = queue.get(&tx_hash).unwrap();
    assert!(matches!(w.state, WithdrawalState::Submitted { .. }));

    // Finalize
    queue.finalize(&tx_hash).unwrap();
    let w = queue.get(&tx_hash).unwrap();
    assert!(matches!(w.state, WithdrawalState::Finalized));
}

// ============================================================================
// Threshold Mempool Tests
// ============================================================================

#[tokio::test]
async fn test_threshold_mempool_basic() {
    let config = EncryptedMempoolConfig {
        enabled: true,
        threshold: 2,
        total_members: 3,
        max_pending: 100,
    };

    let manager = ThresholdMempoolManager::new(config);
    let (committee, local_members) = create_test_committee(2, 3);

    manager.initialize_committee(committee.clone()).await;
    manager.set_local_member(local_members[0].clone()).await;

    assert!(manager.is_active().await);
    assert_eq!(manager.pending_count().await, 0);

    // Submit an encrypted transaction
    let plaintext = b"test transaction data for batch processing";
    let encrypted = encrypt_for_committee(plaintext, &committee, None).expect("encryption failed");

    manager
        .add_encrypted_tx(encrypted.clone())
        .await
        .expect("should add tx");

    assert_eq!(manager.pending_count().await, 1);

    // Order for batch
    let ordered = manager.order_for_batch(1).await;
    assert_eq!(ordered.len(), 1);
    assert_eq!(ordered[0].batch_id, 1);
    assert_eq!(ordered[0].sequence, 0);
    assert_eq!(manager.pending_count().await, 0);

    // Submit shares for decryption
    for local_member in local_members.iter().take(2) {
        let our_share = encrypted
            .encrypted_shares
            .iter()
            .find(|s| s.member_id == local_member.id)
            .expect("share not found");

        let share = local_member
            .decrypt_share(our_share)
            .expect("share decrypt failed");
        manager.submit_share(encrypted.tx_id, share).await;
    }

    // Decrypt
    let decrypted = manager
        .try_decrypt(&encrypted)
        .await
        .expect("decryption failed");
    assert_eq!(decrypted, plaintext);
}

#[tokio::test]
async fn test_threshold_mempool_ordering() {
    let config = EncryptedMempoolConfig {
        enabled: true,
        threshold: 2,
        total_members: 3,
        max_pending: 100,
    };

    let manager = ThresholdMempoolManager::new(config);
    let (committee, _) = create_test_committee(2, 3);
    manager.initialize_committee(committee.clone()).await;

    // Submit multiple transactions
    for i in 0..5 {
        let plaintext = format!("transaction {}", i);
        let encrypted =
            encrypt_for_committee(plaintext.as_bytes(), &committee, None).expect("encryption");
        manager.add_encrypted_tx(encrypted).await.expect("add");
    }

    assert_eq!(manager.pending_count().await, 5);

    // Order for batch
    let ordered = manager.order_for_batch(42).await;
    assert_eq!(ordered.len(), 5);
    assert_eq!(manager.pending_count().await, 0);

    // Check ordering
    for (i, otx) in ordered.iter().enumerate() {
        assert_eq!(otx.sequence, i as u64);
        assert_eq!(otx.batch_id, 42);
    }
}

#[tokio::test]
async fn test_threshold_mempool_max_pending() {
    let config = EncryptedMempoolConfig {
        enabled: true,
        threshold: 2,
        total_members: 3,
        max_pending: 5, // Small limit
    };

    let manager = ThresholdMempoolManager::new(config);
    let (committee, _) = create_test_committee(2, 3);
    manager.initialize_committee(committee.clone()).await;

    // Fill up the mempool
    for i in 0..5 {
        let plaintext = format!("tx {}", i);
        let encrypted =
            encrypt_for_committee(plaintext.as_bytes(), &committee, None).expect("encryption");
        manager.add_encrypted_tx(encrypted).await.expect("add");
    }

    // Next one should fail
    let plaintext = b"overflow tx";
    let encrypted = encrypt_for_committee(plaintext, &committee, None).expect("encryption");
    let result = manager.add_encrypted_tx(encrypted).await;
    assert!(result.is_err());
}

// ============================================================================
// Full L2 Cycle Test
// ============================================================================

#[test]
fn test_full_l2_cycle_deposit_transfer_withdraw() {
    let mut db = temp_db();

    // 1. Simulate deposit (credit account)
    let user = account(1);
    let recipient = account(2);

    db.set_account_state(
        user,
        AccountState {
            balance: 1000,
            nonce: 0,
        },
    )
    .unwrap();

    db.set_account_state(
        recipient,
        AccountState {
            balance: 0,
            nonce: 0,
        },
    )
    .unwrap();

    // 2. Execute transfer (simulated)
    let user_state = db.get_account_state(&user).unwrap();
    assert_eq!(user_state.balance, 1000);

    let transfer_amount = 300;
    db.set_account_state(
        user,
        AccountState {
            balance: user_state.balance - transfer_amount,
            nonce: user_state.nonce + 1,
        },
    )
    .unwrap();

    let recipient_state = db.get_account_state(&recipient).unwrap();
    db.set_account_state(
        recipient,
        AccountState {
            balance: recipient_state.balance + transfer_amount,
            nonce: recipient_state.nonce,
        },
    )
    .unwrap();

    // Verify transfer
    let user_state = db.get_account_state(&user).unwrap();
    let recipient_state = db.get_account_state(&recipient).unwrap();
    assert_eq!(user_state.balance, 700);
    assert_eq!(recipient_state.balance, 300);

    // 3. Setup withdrawal queue
    let db_arc = Arc::new(db);
    let mut queue = WithdrawalQueue::new(db_arc.clone());

    // Create pending withdrawal for recipient
    let withdrawal_hash: [u8; 32] = [77u8; 32];
    let l1_address: [u8; 32] = [88u8; 32];
    let pending = PendingWithdrawal {
        tx_hash: withdrawal_hash,
        from: recipient,
        to_l1_address: l1_address,
        amount: 200,
        l2_nonce: 0,
    };
    queue.add(pending).unwrap();

    // 4. Process in batch
    let pending_list = queue.get_pending();
    assert_eq!(pending_list.len(), 1);
    assert_eq!(pending_list[0].amount, 200);

    queue.mark_in_batch(&[withdrawal_hash], 1).unwrap();

    // 5. Settlement (simulated)
    queue.mark_submitted(1, "Abc123TxSig".to_string()).unwrap();
    queue.finalize(&withdrawal_hash).unwrap();

    let w = queue.get(&withdrawal_hash).unwrap();
    assert!(matches!(w.state, WithdrawalState::Finalized));
}

// ============================================================================
// Database Persistence Tests
// ============================================================================

#[test]
fn test_account_state_persistence() {
    let mut db = temp_db();

    let acc = account(42);
    db.set_account_state(
        acc,
        AccountState {
            balance: 12345,
            nonce: 7,
        },
    )
    .unwrap();

    let loaded = db.get_account_state(&acc).unwrap();
    assert_eq!(loaded.balance, 12345);
    assert_eq!(loaded.nonce, 7);
}

#[test]
fn test_nullifier_persistence() {
    let db = temp_db();

    let nullifier: [u8; 32] = [55u8; 32];

    assert!(!db.nullifier_exists(&nullifier).unwrap());
    db.mark_nullifier(&nullifier).unwrap();
    assert!(db.nullifier_exists(&nullifier).unwrap());
}

#[test]
fn test_commitment_persistence() {
    let db = temp_db();

    let commitment: [u8; 32] = [66u8; 32];
    let position = 42u32;

    db.insert_commitment(position, commitment).unwrap();

    let commitments = db.get_all_commitments().unwrap();
    assert!(
        commitments
            .iter()
            .any(|(pos, cm)| *pos == position && *cm == commitment)
    );
}

// ============================================================================
// Pipeline Integration Tests
// ============================================================================

#[tokio::test]
async fn test_pipeline_end_to_end_with_deposits_and_transfers() {
    use crate::sequencer::pipeline::{PipelineConfig, PipelineService, PipelineState};
    use std::time::Duration;
    use zelana_transaction::{DepositEvent, SignedTransaction, TransactionData, TransactionType};

    let temp_dir = tempfile::TempDir::new().unwrap();
    let db = Arc::new(RocksDbStore::open(temp_dir.path().to_str().unwrap()).unwrap());

    // Configure fast pipeline for testing
    let mut config = PipelineConfig::default();
    config.poll_interval_ms = 10;
    config.batch_config.max_transactions = 5;
    config.batch_config.min_transactions = 1;

    let service = PipelineService::start(db.clone(), config, None).unwrap();

    // Submit deposits to create accounts
    let alice = AccountId([1u8; 32]);
    let bob = AccountId([2u8; 32]);

    service
        .submit(TransactionType::Deposit(DepositEvent {
            to: alice,
            amount: 10000,
            l1_seq: 1,
        }))
        .await
        .unwrap();

    service
        .submit(TransactionType::Deposit(DepositEvent {
            to: bob,
            amount: 5000,
            l1_seq: 2,
        }))
        .await
        .unwrap();

    // Seal and wait for batch to process
    let batch_id = service.seal().await.unwrap();
    assert_eq!(batch_id, Some(1));

    // Wait for proving and settlement
    for _ in 0..50 {
        tokio::time::sleep(Duration::from_millis(20)).await;
        let stats = service.stats().await.unwrap();
        if stats.batches_settled >= 1 {
            break;
        }
    }

    let stats = service.stats().await.unwrap();
    assert_eq!(stats.state, PipelineState::Running);
    assert_eq!(stats.batches_proved, 1);
    assert_eq!(stats.batches_settled, 1);

    // Submit transfers in a new batch
    let transfer = SignedTransaction {
        data: TransactionData {
            from: alice,
            to: bob,
            amount: 1000,
            nonce: 0,
            chain_id: 1,
        },
        signature: vec![0u8; 64], // Mock signature for test
        signer_pubkey: [0u8; 32],
    };

    service
        .submit(TransactionType::Transfer(transfer))
        .await
        .unwrap();

    // Seal second batch
    let batch_id = service.seal().await.unwrap();
    assert_eq!(batch_id, Some(2));

    // Wait for second batch
    for _ in 0..50 {
        tokio::time::sleep(Duration::from_millis(20)).await;
        let stats = service.stats().await.unwrap();
        if stats.batches_settled >= 2 {
            break;
        }
    }

    let final_stats = service.stats().await.unwrap();
    assert_eq!(final_stats.batches_proved, 2);
    assert_eq!(final_stats.batches_settled, 2);

    service.shutdown().await.unwrap();
}

#[tokio::test]
async fn test_pipeline_multiple_batches_in_sequence() {
    use crate::sequencer::pipeline::{PipelineConfig, PipelineService};
    use std::time::Duration;
    use zelana_transaction::{DepositEvent, TransactionType};

    let temp_dir = tempfile::TempDir::new().unwrap();
    let db = Arc::new(RocksDbStore::open(temp_dir.path().to_str().unwrap()).unwrap());

    let mut config = PipelineConfig::default();
    config.poll_interval_ms = 10;
    config.batch_config.min_transactions = 1;

    let service = PipelineService::start(db, config, None).unwrap();

    // Submit and seal 3 batches
    for batch_num in 1..=3 {
        for tx_num in 0..3 {
            service
                .submit(TransactionType::Deposit(DepositEvent {
                    to: AccountId([(batch_num * 10 + tx_num) as u8; 32]),
                    amount: 1000,
                    l1_seq: (batch_num * 10 + tx_num) as u64,
                }))
                .await
                .unwrap();
        }
        service.seal().await.unwrap();
    }

    // Wait for all batches to complete
    for _ in 0..100 {
        tokio::time::sleep(Duration::from_millis(20)).await;
        let stats = service.stats().await.unwrap();
        if stats.batches_settled >= 3 {
            break;
        }
    }

    let stats = service.stats().await.unwrap();
    assert_eq!(stats.batches_proved, 3);
    assert_eq!(stats.batches_settled, 3);
    assert_eq!(stats.last_settled_batch, Some(3));

    service.shutdown().await.unwrap();
}

#[tokio::test]
async fn test_pipeline_pause_and_resume() {
    use crate::sequencer::pipeline::{PipelineConfig, PipelineService, PipelineState};
    use std::time::Duration;
    use zelana_transaction::{DepositEvent, TransactionType};

    let temp_dir = tempfile::TempDir::new().unwrap();
    let db = Arc::new(RocksDbStore::open(temp_dir.path().to_str().unwrap()).unwrap());

    let mut config = PipelineConfig::default();
    config.poll_interval_ms = 10;

    let service = PipelineService::start(db, config, None).unwrap();

    // Submit a transaction
    service
        .submit(TransactionType::Deposit(DepositEvent {
            to: AccountId([1u8; 32]),
            amount: 1000,
            l1_seq: 1,
        }))
        .await
        .unwrap();

    // Pause the pipeline
    service.pause("test pause".to_string()).await.unwrap();
    let stats = service.stats().await.unwrap();
    assert!(matches!(stats.state, PipelineState::Paused { .. }));

    // Seal batch while paused
    service.seal().await.unwrap();

    // Give time for pipeline tick (should not process while paused)
    tokio::time::sleep(Duration::from_millis(50)).await;
    let stats = service.stats().await.unwrap();
    assert_eq!(stats.batches_proved, 0, "should not prove while paused");

    // Resume pipeline
    service.resume().await.unwrap();
    let stats = service.stats().await.unwrap();
    assert_eq!(stats.state, PipelineState::Running);

    // Wait for batch to be processed
    for _ in 0..50 {
        tokio::time::sleep(Duration::from_millis(20)).await;
        let stats = service.stats().await.unwrap();
        if stats.batches_settled >= 1 {
            break;
        }
    }

    let final_stats = service.stats().await.unwrap();
    assert_eq!(final_stats.batches_proved, 1);
    assert_eq!(final_stats.batches_settled, 1);

    service.shutdown().await.unwrap();
}

// ============================================================================
// Settlement with Withdrawals Tests
// ============================================================================

#[tokio::test]
async fn test_pipeline_with_withdrawals() {
    use crate::sequencer::pipeline::{PipelineConfig, PipelineService, PipelineState};
    use std::time::Duration;
    use zelana_transaction::{DepositEvent, TransactionType, WithdrawRequest};

    let temp_dir = tempfile::TempDir::new().unwrap();
    let db = Arc::new(RocksDbStore::open(temp_dir.path().to_str().unwrap()).unwrap());

    // Configure fast pipeline for testing
    let mut config = PipelineConfig::default();
    config.poll_interval_ms = 10;
    config.batch_config.max_transactions = 10;
    config.batch_config.min_transactions = 1;

    let service = PipelineService::start(db.clone(), config, None).unwrap();

    // 1. Submit deposit to fund an account
    let alice = AccountId([1u8; 32]);
    let l1_destination: [u8; 32] = [0xAA; 32];

    service
        .submit(TransactionType::Deposit(DepositEvent {
            to: alice,
            amount: 10000,
            l1_seq: 1,
        }))
        .await
        .unwrap();

    // Seal deposit batch
    let batch_id = service.seal().await.unwrap();
    assert_eq!(batch_id, Some(1));

    // Wait for deposit batch to be proved and settled
    for _ in 0..50 {
        tokio::time::sleep(Duration::from_millis(20)).await;
        let stats = service.stats().await.unwrap();
        if stats.batches_settled >= 1 {
            break;
        }
    }

    let stats = service.stats().await.unwrap();
    assert_eq!(stats.batches_proved, 1);
    assert_eq!(stats.batches_settled, 1);

    // 2. Submit a withdrawal transaction
    let withdraw_amount = 3000u64;
    let withdraw = WithdrawRequest {
        from: alice,
        to_l1_address: l1_destination,
        amount: withdraw_amount,
        nonce: 0,
        signature: vec![0u8; 64], // Mock signature for test
        signer_pubkey: [0u8; 32],
    };

    service
        .submit(TransactionType::Withdraw(withdraw))
        .await
        .unwrap();

    // Seal withdrawal batch
    let batch_id = service.seal().await.unwrap();
    assert_eq!(batch_id, Some(2));

    // Wait for withdrawal batch to be proved and settled
    for _ in 0..50 {
        tokio::time::sleep(Duration::from_millis(20)).await;
        let stats = service.stats().await.unwrap();
        if stats.batches_settled >= 2 {
            break;
        }
    }

    // Verify final state
    let final_stats = service.stats().await.unwrap();
    assert_eq!(final_stats.state, PipelineState::Running);
    assert_eq!(final_stats.batches_proved, 2);
    assert_eq!(final_stats.batches_settled, 2);

    service.shutdown().await.unwrap();
}

#[tokio::test]
async fn test_pipeline_multiple_withdrawals_in_batch() {
    use crate::sequencer::pipeline::{PipelineConfig, PipelineService};
    use std::time::Duration;
    use zelana_transaction::{DepositEvent, TransactionType, WithdrawRequest};

    let temp_dir = tempfile::TempDir::new().unwrap();
    let db = Arc::new(RocksDbStore::open(temp_dir.path().to_str().unwrap()).unwrap());

    let mut config = PipelineConfig::default();
    config.poll_interval_ms = 10;
    config.batch_config.max_transactions = 20;
    config.batch_config.min_transactions = 1;

    let service = PipelineService::start(db.clone(), config, None).unwrap();

    // Fund multiple accounts
    let accounts: Vec<AccountId> = (0..3).map(|i| AccountId([(i + 1) as u8; 32])).collect();

    for (i, acc) in accounts.iter().enumerate() {
        service
            .submit(TransactionType::Deposit(DepositEvent {
                to: *acc,
                amount: 5000,
                l1_seq: i as u64 + 1,
            }))
            .await
            .unwrap();
    }

    // Seal deposit batch
    service.seal().await.unwrap();

    // Wait for deposits
    for _ in 0..50 {
        tokio::time::sleep(Duration::from_millis(20)).await;
        let stats = service.stats().await.unwrap();
        if stats.batches_settled >= 1 {
            break;
        }
    }

    // Submit multiple withdrawals
    for (i, acc) in accounts.iter().enumerate() {
        let l1_dest: [u8; 32] = [(0xB0 + i) as u8; 32];
        let withdraw = WithdrawRequest {
            from: *acc,
            to_l1_address: l1_dest,
            amount: 1000 + (i as u64 * 500),
            nonce: 0,
            signature: vec![0u8; 64],
            signer_pubkey: [0u8; 32],
        };
        service
            .submit(TransactionType::Withdraw(withdraw))
            .await
            .unwrap();
    }

    // Seal withdrawal batch
    let batch_id = service.seal().await.unwrap();
    assert_eq!(batch_id, Some(2));

    // Wait for withdrawal batch
    for _ in 0..50 {
        tokio::time::sleep(Duration::from_millis(20)).await;
        let stats = service.stats().await.unwrap();
        if stats.batches_settled >= 2 {
            break;
        }
    }

    let stats = service.stats().await.unwrap();
    assert_eq!(stats.batches_proved, 2);
    assert_eq!(stats.batches_settled, 2);

    service.shutdown().await.unwrap();
}

#[test]
fn test_withdrawal_merkle_root_computation() {
    use crate::sequencer::{TrackedWithdrawal, WithdrawalState, build_withdrawal_merkle_root};

    // Test that multiple withdrawals produce a non-zero merkle root
    let withdrawals: Vec<TrackedWithdrawal> = (0..3)
        .map(|i| TrackedWithdrawal {
            tx_hash: [i as u8; 32],
            from: account(i + 1),
            to_l1_address: [(0xA0 + i) as u8; 32],
            amount: 1000 * (i as u64 + 1),
            l2_nonce: i as u64,
            state: WithdrawalState::InBatch { batch_id: 1 },
            created_at: 0,
            batch_id: Some(1),
        })
        .collect();

    let root = build_withdrawal_merkle_root(&withdrawals);
    assert_ne!(root, [0u8; 32], "withdrawal root should not be zero");

    // Same withdrawals should produce same root
    let root2 = build_withdrawal_merkle_root(&withdrawals);
    assert_eq!(root, root2, "withdrawal root should be deterministic");

    // Different withdrawals should produce different root
    let mut different = withdrawals.clone();
    different[0].amount = 9999;
    let root3 = build_withdrawal_merkle_root(&different);
    assert_ne!(
        root, root3,
        "different withdrawals should produce different root"
    );
}

// ============================================================================
// Batch & Transaction Recording Tests
// ============================================================================

#[tokio::test]
async fn test_pipeline_stores_batch_and_tx_summaries() {
    use crate::api::types::{BatchStatus, TxStatus, TxType};
    use crate::sequencer::pipeline::{PipelineConfig, PipelineService, PipelineState};
    use std::time::Duration;
    use zelana_transaction::{DepositEvent, TransactionType};

    let temp_dir = tempfile::TempDir::new().unwrap();
    let db = Arc::new(RocksDbStore::open(temp_dir.path().to_str().unwrap()).unwrap());

    // Configure fast pipeline for testing
    let mut config = PipelineConfig::default();
    config.poll_interval_ms = 10;
    config.batch_config.max_transactions = 10;
    config.batch_config.min_transactions = 1;

    let service = PipelineService::start(db.clone(), config, None).unwrap();

    // Submit deposits
    let alice = AccountId([1u8; 32]);
    let bob = AccountId([2u8; 32]);

    service
        .submit(TransactionType::Deposit(DepositEvent {
            to: alice,
            amount: 10000,
            l1_seq: 1,
        }))
        .await
        .unwrap();

    service
        .submit(TransactionType::Deposit(DepositEvent {
            to: bob,
            amount: 5000,
            l1_seq: 2,
        }))
        .await
        .unwrap();

    // Seal and wait for batch to process
    let batch_id = service.seal().await.unwrap();
    assert_eq!(batch_id, Some(1));

    // Wait for proving and settlement
    for _ in 0..50 {
        tokio::time::sleep(Duration::from_millis(20)).await;
        let stats = service.stats().await.unwrap();
        if stats.batches_settled >= 1 {
            break;
        }
    }

    let stats = service.stats().await.unwrap();
    assert_eq!(stats.state, PipelineState::Running);
    assert_eq!(stats.batches_proved, 1);
    assert_eq!(stats.batches_settled, 1);

    // Verify batch summary was stored
    let batch_summary = db.get_batch_summary(1).unwrap();
    assert!(batch_summary.is_some(), "batch summary should be stored");
    let batch_summary = batch_summary.unwrap();
    assert_eq!(batch_summary.batch_id, 1);
    assert_eq!(batch_summary.tx_count, 2);
    assert_eq!(batch_summary.status, BatchStatus::Settled);
    assert!(batch_summary.l1_tx_sig.is_some());
    assert!(batch_summary.settled_at.is_some());

    // Verify transaction summaries were stored
    let (txs, total) = db.list_transactions(0, 10, None, None, None).unwrap();
    assert_eq!(total, 2);
    assert_eq!(txs.len(), 2);

    // Check tx properties
    for tx in &txs {
        assert_eq!(tx.batch_id, Some(1));
        assert_eq!(tx.status, TxStatus::Settled);
        assert_eq!(tx.tx_type, TxType::Deposit);
        assert!(tx.amount.is_some());
        assert!(tx.executed_at.is_some());
    }

    // Verify we can filter by status
    let (settled_txs, _) = db
        .list_transactions(0, 10, None, None, Some(TxStatus::Settled))
        .unwrap();
    assert_eq!(settled_txs.len(), 2);

    // Verify we can filter by type
    let (deposit_txs, _) = db
        .list_transactions(0, 10, None, Some(TxType::Deposit), None)
        .unwrap();
    assert_eq!(deposit_txs.len(), 2);

    // Verify we can filter by batch_id
    let (batch1_txs, _) = db.list_transactions(0, 10, Some(1), None, None).unwrap();
    assert_eq!(batch1_txs.len(), 2);

    // Submit a second batch to verify multiple batches work
    service
        .submit(TransactionType::Deposit(DepositEvent {
            to: AccountId([3u8; 32]),
            amount: 3000,
            l1_seq: 3,
        }))
        .await
        .unwrap();

    service.seal().await.unwrap();

    // Wait for second batch
    for _ in 0..50 {
        tokio::time::sleep(Duration::from_millis(20)).await;
        let stats = service.stats().await.unwrap();
        if stats.batches_settled >= 2 {
            break;
        }
    }

    // Verify batch listing
    let (batches, batch_total) = db.list_batches(0, 10).unwrap();
    assert_eq!(batch_total, 2);
    assert_eq!(batches.len(), 2);
    // Should be newest first
    assert_eq!(batches[0].batch_id, 2);
    assert_eq!(batches[1].batch_id, 1);

    // Verify total transactions
    let (all_txs, tx_total) = db.list_transactions(0, 10, None, None, None).unwrap();
    assert_eq!(tx_total, 3);
    assert_eq!(all_txs.len(), 3);

    service.shutdown().await.unwrap();
}

// ============================================================================
// End-to-End Shielded Transaction Tests
// ============================================================================

/// Tests shielded transaction through the full pipeline with MockProver
#[tokio::test]
async fn test_pipeline_shielded_transaction_end_to_end() {
    use crate::sequencer::pipeline::{PipelineConfig, PipelineService, PipelineState};
    use std::time::Duration;
    use zelana_transaction::{PrivateTransaction, TransactionType};

    let temp_dir = tempfile::TempDir::new().unwrap();
    let db = Arc::new(RocksDbStore::open(temp_dir.path().to_str().unwrap()).unwrap());

    // Configure fast pipeline for testing
    let mut config = PipelineConfig::default();
    config.poll_interval_ms = 10;
    config.batch_config.max_transactions = 10;
    config.batch_config.min_transactions = 1;

    let service = PipelineService::start(db.clone(), config, None).unwrap();

    // Submit a shielded transaction with mock proof
    let nullifier = [0xAA; 32];
    let commitment = [0xBB; 32];
    let private_tx = PrivateTransaction {
        proof: vec![1, 2, 3, 4], // Non-empty mock proof
        nullifier,
        commitment,
        ciphertext: vec![0xCC; 64],
        ephemeral_key: [0xDD; 32],
    };

    service
        .submit(TransactionType::Shielded(private_tx))
        .await
        .unwrap();

    // Seal the batch
    let batch_id = service.seal().await.unwrap();
    assert_eq!(batch_id, Some(1));

    // Wait for batch to be proved and settled
    for _ in 0..100 {
        tokio::time::sleep(Duration::from_millis(20)).await;
        let stats = service.stats().await.unwrap();
        if stats.batches_settled >= 1 {
            break;
        }
    }

    let stats = service.stats().await.unwrap();
    assert_eq!(stats.state, PipelineState::Running);
    assert_eq!(stats.batches_proved, 1);
    assert_eq!(stats.batches_settled, 1);

    // Verify the shielded transaction was recorded
    let (txs, total) = db.list_transactions(0, 10, None, None, None).unwrap();
    assert_eq!(total, 1);
    assert_eq!(txs.len(), 1);

    // Verify it's a shielded transaction
    use crate::api::types::{TxStatus, TxType};
    assert_eq!(txs[0].tx_type, TxType::Shielded);
    assert_ne!(
        txs[0].status,
        TxStatus::Failed,
        "Shielded tx should not fail"
    );

    service.shutdown().await.unwrap();
}

/// Tests that double-spend prevention works across batches
#[tokio::test]
async fn test_pipeline_shielded_double_spend_prevention() {
    use crate::sequencer::pipeline::{PipelineConfig, PipelineService};
    use std::time::Duration;
    use zelana_transaction::{PrivateTransaction, TransactionType};

    let temp_dir = tempfile::TempDir::new().unwrap();
    let db = Arc::new(RocksDbStore::open(temp_dir.path().to_str().unwrap()).unwrap());

    let mut config = PipelineConfig::default();
    config.poll_interval_ms = 10;
    config.batch_config.max_transactions = 10;
    config.batch_config.min_transactions = 1;

    let service = PipelineService::start(db.clone(), config, None).unwrap();

    // Submit first shielded transaction
    let nullifier = [0x11; 32];
    let private_tx1 = PrivateTransaction {
        proof: vec![1, 2, 3, 4],
        nullifier, // This nullifier will be spent
        commitment: [0x22; 32],
        ciphertext: vec![0x33; 64],
        ephemeral_key: [0x44; 32],
    };

    service
        .submit(TransactionType::Shielded(private_tx1))
        .await
        .unwrap();

    // Seal and wait for first batch
    service.seal().await.unwrap();

    for _ in 0..100 {
        tokio::time::sleep(Duration::from_millis(20)).await;
        let stats = service.stats().await.unwrap();
        if stats.batches_settled >= 1 {
            break;
        }
    }

    // Submit second shielded transaction with SAME nullifier (double-spend attempt)
    let private_tx2 = PrivateTransaction {
        proof: vec![5, 6, 7, 8],
        nullifier, // Same nullifier - should be rejected
        commitment: [0x55; 32],
        ciphertext: vec![0x66; 64],
        ephemeral_key: [0x77; 32],
    };

    service
        .submit(TransactionType::Shielded(private_tx2))
        .await
        .unwrap();

    // Seal and wait for second batch
    service.seal().await.unwrap();

    for _ in 0..100 {
        tokio::time::sleep(Duration::from_millis(20)).await;
        let stats = service.stats().await.unwrap();
        if stats.batches_settled >= 2 {
            break;
        }
    }

    // Verify transactions
    use crate::api::types::TxStatus;
    let (txs, total) = db.list_transactions(0, 10, None, None, None).unwrap();
    assert_eq!(total, 2);

    // First tx should succeed, second should fail (double-spend)
    let tx1 = txs.iter().find(|t| t.batch_id == Some(1)).unwrap();
    let tx2 = txs.iter().find(|t| t.batch_id == Some(2)).unwrap();

    assert_ne!(
        tx1.status,
        TxStatus::Failed,
        "First shielded tx should succeed"
    );
    assert_eq!(
        tx2.status,
        TxStatus::Failed,
        "Double-spend attempt should fail: nullifier already spent"
    );

    service.shutdown().await.unwrap();
}

/// Tests the full L2 lifecycle: Deposit → Transfer → Shielded → Withdraw
#[tokio::test]
async fn test_pipeline_full_l2_cycle_with_shielded() {
    use crate::api::types::{TxStatus, TxType};
    use crate::sequencer::pipeline::{PipelineConfig, PipelineService, PipelineState};
    use std::time::Duration;
    use zelana_keypair::Keypair;
    use zelana_transaction::{DepositEvent, PrivateTransaction, TransactionData, TransactionType};

    let temp_dir = tempfile::TempDir::new().unwrap();
    let db = Arc::new(RocksDbStore::open(temp_dir.path().to_str().unwrap()).unwrap());

    let mut config = PipelineConfig::default();
    config.poll_interval_ms = 10;
    config.batch_config.max_transactions = 10;
    config.batch_config.min_transactions = 1;

    let service = PipelineService::start(db.clone(), config, None).unwrap();

    // Create real keypairs for Alice and Bob
    let alice_kp = Keypair::new_random();
    let bob_kp = Keypair::new_random();
    let alice = alice_kp.account_id();
    let bob = bob_kp.account_id();

    // ========================================================================
    // Step 1: Deposit funds to Alice
    // ========================================================================
    service
        .submit(TransactionType::Deposit(DepositEvent {
            to: alice,
            amount: 10_000,
            l1_seq: 1,
        }))
        .await
        .unwrap();

    service.seal().await.unwrap();

    for _ in 0..100 {
        tokio::time::sleep(Duration::from_millis(20)).await;
        let stats = service.stats().await.unwrap();
        if stats.batches_settled >= 1 {
            break;
        }
    }

    // ========================================================================
    // Step 2: Transfer from Alice to Bob (with valid signature)
    // ========================================================================
    let transfer_data = TransactionData {
        from: alice,
        to: bob,
        amount: 3_000,
        nonce: 0,
        chain_id: 1,
    };
    let signed_transfer = alice_kp.sign_transaction(transfer_data);

    service
        .submit(TransactionType::Transfer(signed_transfer))
        .await
        .unwrap();

    service.seal().await.unwrap();

    for _ in 0..100 {
        tokio::time::sleep(Duration::from_millis(20)).await;
        let stats = service.stats().await.unwrap();
        if stats.batches_settled >= 2 {
            break;
        }
    }

    // ========================================================================
    // Step 3: Shielded transaction (privacy layer)
    // ========================================================================
    let private_tx = PrivateTransaction {
        proof: vec![1, 2, 3, 4], // Mock proof
        nullifier: [0xDE; 32],
        commitment: [0xAD; 32],
        ciphertext: vec![0xBE; 64],
        ephemeral_key: [0xEF; 32],
    };

    service
        .submit(TransactionType::Shielded(private_tx))
        .await
        .unwrap();

    service.seal().await.unwrap();

    for _ in 0..100 {
        tokio::time::sleep(Duration::from_millis(20)).await;
        let stats = service.stats().await.unwrap();
        if stats.batches_settled >= 3 {
            break;
        }
    }

    // ========================================================================
    // Step 4: Withdraw from Bob to L1 (with valid signature)
    // ========================================================================
    let l1_dest: [u8; 32] = [0xFF; 32];
    let withdraw = bob_kp.sign_withdrawal(l1_dest, 1_000, 0);

    service
        .submit(TransactionType::Withdraw(withdraw))
        .await
        .unwrap();

    service.seal().await.unwrap();

    for _ in 0..100 {
        tokio::time::sleep(Duration::from_millis(20)).await;
        let stats = service.stats().await.unwrap();
        if stats.batches_settled >= 4 {
            break;
        }
    }

    // ========================================================================
    // Verify final state
    // ========================================================================
    let stats = service.stats().await.unwrap();
    assert_eq!(stats.state, PipelineState::Running);
    assert_eq!(stats.batches_proved, 4);
    assert_eq!(stats.batches_settled, 4);

    // Verify all transaction types were recorded
    let (txs, total) = db.list_transactions(0, 10, None, None, None).unwrap();
    assert_eq!(total, 4);

    // Count transaction types
    let deposit_count = txs.iter().filter(|t| t.tx_type == TxType::Deposit).count();
    let transfer_count = txs.iter().filter(|t| t.tx_type == TxType::Transfer).count();
    let shielded_count = txs.iter().filter(|t| t.tx_type == TxType::Shielded).count();
    let withdraw_count = txs
        .iter()
        .filter(|t| t.tx_type == TxType::Withdrawal)
        .count();

    assert_eq!(deposit_count, 1, "Should have 1 deposit");
    assert_eq!(transfer_count, 1, "Should have 1 transfer");
    assert_eq!(shielded_count, 1, "Should have 1 shielded tx");
    assert_eq!(withdraw_count, 1, "Should have 1 withdrawal");

    // All transactions should succeed
    for tx in &txs {
        assert_ne!(
            tx.status,
            TxStatus::Failed,
            "Transaction {:?} in batch {:?} should succeed",
            tx.tx_type,
            tx.batch_id
        );
    }

    // Verify batch listing
    let (batches, batch_total) = db.list_batches(0, 10).unwrap();
    assert_eq!(batch_total, 4);
    assert_eq!(batches.len(), 4);

    service.shutdown().await.unwrap();
}
