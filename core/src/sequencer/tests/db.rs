use tempfile::TempDir;

use crate::sequencer::RocksDbStore;
use crate::storage::state::StateStore;
use zelana_account::{AccountId, AccountState};
use zelana_block::{BlockHeader, HEADER_MAGIC, HEADER_VERSION};

/// Create a temporary DB for each test
fn temp_db() -> RocksDbStore {
    let dir = TempDir::new().unwrap();
    RocksDbStore::open(dir.path()).unwrap()
}

fn account(id: u8) -> AccountId {
    let mut b = [0u8; 32];
    b[0] = id;
    AccountId(b)
}

#[test]
fn set_and_get_account_state() {
    let mut db = temp_db();
    let id = account(1);

    let state = AccountState {
        balance: 100,
        nonce: 2,
    };

    db.set_account_state(id, state.clone()).unwrap();

    let loaded = db.get_account_state(&id).unwrap();
    assert_eq!(loaded, state);
}

#[test]
fn missing_account_returns_default() {
    let db = temp_db();
    let id = account(42);

    let state = db.get_account_state(&id).unwrap();
    assert_eq!(state.balance, 0);
    assert_eq!(state.nonce, 0);
}

#[test]
fn account_state_overwrite_works() {
    let mut db = temp_db();
    let id = account(1);

    db.set_account_state(
        id,
        AccountState {
            balance: 10,
            nonce: 0,
        },
    )
    .unwrap();

    db.set_account_state(
        id,
        AccountState {
            balance: 50,
            nonce: 1,
        },
    )
    .unwrap();

    let st = db.get_account_state(&id).unwrap();
    assert_eq!(st.balance, 50);
    assert_eq!(st.nonce, 1);
}

#[test]
fn add_encrypted_tx_stores_blob() {
    let db = temp_db();

    let tx_hash = [9u8; 32];
    let blob = vec![1, 2, 3, 4, 5];

    let res = db.add_encrypted_tx(tx_hash, blob.clone());
    assert!(res.is_ok());

    // We cannot read blobs back directly (by design),
    // but this test ensures no panic / CF misconfiguration.
}

#[test]
fn latest_state_root_is_genesis_when_no_blocks() {
    let db = temp_db();

    let root = db.get_latest_state_root().unwrap();
    assert_eq!(root, [0u8; 32]);
}

#[test]
fn store_block_header_and_get_latest_root() {
    let db = temp_db();

    let header1 = BlockHeader {
        magic: HEADER_MAGIC,
        hdr_version: HEADER_VERSION,
        batch_id: 1,
        prev_root: [0u8; 32],
        new_root: [1u8; 32],
        tx_count: 1,
        open_at: 123,
        flags: 0,
    };

    let header2 = BlockHeader {
        magic: HEADER_MAGIC,
        hdr_version: HEADER_VERSION,
        batch_id: 2,
        prev_root: [1u8; 32],
        new_root: [2u8; 32],
        tx_count: 2,
        open_at: 456,
        flags: 0,
    };

    db.store_block_header(header1).unwrap();
    db.store_block_header(header2).unwrap();

    let latest_root = db.get_latest_state_root().unwrap();
    assert_eq!(latest_root, [2u8; 32]);
}

#[test]
fn nullifier_mark_and_check() {
    let db = temp_db();
    let nullifier = [7u8; 32];

    let exists_before = db.nullifier_exists(&nullifier).unwrap();
    assert!(!exists_before);

    db.mark_nullifier(&nullifier).unwrap();

    let exists_after = db.nullifier_exists(&nullifier).unwrap();
    assert!(exists_after);
}
