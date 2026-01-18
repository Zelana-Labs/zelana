use std::sync::Arc;
use tempfile::TempDir;

use crate::sequencer::RocksDbStore;
use crate::sequencer::execution::executor::{ExecutionError, Executor};
use crate::storage::state::StateStore;
use zelana_account::{AccountId, AccountState};
use zelana_transaction::{SignedTransaction, TransactionData};

/// Create a temp RocksDB
fn temp_db() -> RocksDbStore {
    let dir = TempDir::new().unwrap();
    RocksDbStore::open(dir.path()).unwrap()
}

fn account(id: u8) -> AccountId {
    let mut bytes = [0u8; 32];
    bytes[0] = id;
    AccountId(bytes)
}

fn signed_transfer(from: AccountId, to: AccountId, amount: u64, nonce: u64) -> SignedTransaction {
    SignedTransaction {
        data: TransactionData {
            from,
            to,
            amount,
            nonce,
            chain_id: 1,
        },
        signature: vec![9u8; 64],
        signer_pubkey: from.0,
    }
}

#[test]
fn valid_transfer_updates_state() {
    let mut db = temp_db();

    let from = account(1);
    let to = account(2);

    // Seed DB
    db.set_account_state(
        from,
        AccountState {
            balance: 100,
            nonce: 0,
        },
    )
    .unwrap();

    db.set_account_state(
        to,
        AccountState {
            balance: 10,
            nonce: 0,
        },
    )
    .unwrap();

    let mut executor = Executor::new(db.clone().into());

    let tx = signed_transfer(from, to, 25, 0);
    let tx_hash = [1u8; 32];

    let result = executor
        .execute_signed_tx(tx, tx_hash)
        .expect("execution should succeed");

    let diff = result.state_diff.updates;

    assert_eq!(diff[&from].balance, 75);
    assert_eq!(diff[&from].nonce, 1);

    assert_eq!(diff[&to].balance, 35);
    assert_eq!(diff[&to].nonce, 0);
}

#[test]
fn invalid_nonce_fails() {
    let mut db = temp_db();
    let from = account(1);
    let to = account(2);

    db.set_account_state(
        from,
        AccountState {
            balance: 50,
            nonce: 1,
        },
    )
    .unwrap();

    let mut executor = Executor::new(db.clone().into());

    let tx = signed_transfer(from, to, 10, 0); // WRONG nonce

    let err = executor.execute_signed_tx(tx, [0u8; 32]).unwrap_err();

    matches!(err, ExecutionError::InvalidNonce);
}

#[test]
fn insufficient_balance_fails() {
    let mut db = temp_db();
    let from = account(1);
    let to = account(2);

    db.set_account_state(
        from,
        AccountState {
            balance: 5,
            nonce: 0,
        },
    )
    .unwrap();

    let mut executor = Executor::new(db.clone().into());

    let tx = signed_transfer(from, to, 10, 0); // too much

    let err = executor.execute_signed_tx(tx, [0u8; 32]).unwrap_err();

    matches!(err, ExecutionError::InsufficientBalance);
}

#[test]
fn executor_does_not_mutate_db() {
    let db = temp_db();
    let from = account(1);
    let to = account(2);

    db.set_account_state(
        from,
        AccountState {
            balance: 100,
            nonce: 0,
        },
    )
    .unwrap();

    let mut executor = Executor::new(db.clone().into());

    let tx = signed_transfer(from, to, 20, 0);
    executor.execute_signed_tx(tx, [0u8; 32]).unwrap();

    // DB must be unchanged
    let st = db.get_account_state(&from).unwrap();
    assert_eq!(st.balance, 100);
    assert_eq!(st.nonce, 0);
}
