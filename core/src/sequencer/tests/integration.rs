use std::sync::Arc;
use tempfile::TempDir;

use x25519_dalek::{StaticSecret, PublicKey};

use zelana_account::{AccountId, AccountState};
use zelana_transaction::{SignedTransaction, TransactionData};
use zelana_block::HEADER_MAGIC;
use crate::storage::state::StateStore;
use txblob::{encrypt_signed_tx, decrypt_signed_tx};

use crate::sequencer::{
    db::RocksDbStore,
    executor::Executor,
    session::Session,
};

/// Helpers

fn account(id: u8) -> AccountId {
    let mut b = [0u8; 32];
    b[0] = id;
    AccountId(b)
}

fn signed_transfer(from: AccountId, to: AccountId) -> SignedTransaction {
    SignedTransaction {
        data: TransactionData {
            from,
            to,
            amount: 25,
            nonce: 0,
            chain_id: 1,
        },
        signature: vec![1u8; 64],
        signer_pubkey: from.0,
    }
}

fn temp_db() -> RocksDbStore {
    let dir = TempDir::new().unwrap();
    RocksDbStore::open(dir.path()).unwrap()
}

#[test]
fn encrypted_tx_executes_and_updates_state() {
    // --- Setup DB ---
    let mut db = temp_db();

    let from = account(1);
    let to = account(2);

    db.set_account_state(
        from,
        AccountState { balance: 100, nonce: 0 },
    )
    .unwrap();

    db.set_account_state(
        to,
        AccountState { balance: 0, nonce: 0 },
    )
    .unwrap();

    // --- Sequencer keys ---
    let sequencer_secret = StaticSecret::random();
    let sequencer_pub = PublicKey::from(&sequencer_secret);

    let client_secret = StaticSecret::random();
    let client_pub = PublicKey::from(&client_secret);

    // --- Create signed tx ---
    let signed_tx = signed_transfer(from, to);

    // --- Encrypt tx ---
    let blob = encrypt_signed_tx(
        &signed_tx,
        &signed_tx.signer_pubkey,
        &client_secret,
        &sequencer_pub,
        0,
    )
    .unwrap();

    let tx_hash = [7u8; 32];

    // --- Decrypt in sequencer ---
    let decrypted = decrypt_signed_tx(
        &blob,
        &sequencer_secret,
        &client_pub,
    )
    .unwrap();

    // --- Execute ---
    let mut executor = Executor::new(db.clone());
    let exec_result = executor
        .execute_signed_tx(decrypted, tx_hash)
        .unwrap();

    // --- Batch in session ---
    let mut session = Session::new(1);
    session.push_execution(exec_result);

    let prev_root = [0u8; 32];
    let closed = session.close(prev_root);

    // --- Persist state ---
   // --- Persist final state ---
    for (id, state) in &closed.merged_state {
        db.set_account_state(*id, state.clone()).unwrap();
    }

    db.store_block_header(closed.header.clone()).unwrap();


    // --- Assertions ---
    let from_state = db.get_account_state(&from).unwrap();
    let to_state = db.get_account_state(&to).unwrap();

    assert_eq!(from_state.balance, 75);
    assert_eq!(from_state.nonce, 1);

    assert_eq!(to_state.balance, 25);
    assert_eq!(to_state.nonce, 0);

    assert_eq!(closed.header.magic, HEADER_MAGIC);
}
