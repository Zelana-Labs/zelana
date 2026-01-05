use std::collections::HashMap;

use zelana_account::{AccountId, AccountState};
use zelana_block::{HEADER_MAGIC, HEADER_VERSION};

use crate::sequencer::session::{
    Session,
    compute_state_root,
};
use crate::sequencer::executor::{ExecutionResult, StateDiff};

fn account(id: u8) -> AccountId {
    let mut b = [0u8; 32];
    b[0] = id;
    AccountId(b)
}

fn exec_result(
    tx_hash: u8,
    updates: Vec<(AccountId, AccountState)>,
) -> ExecutionResult {
    let mut map = HashMap::new();
    for (id, st) in updates {
        map.insert(id, st);
    }
    ExecutionResult {
        tx_hash: [tx_hash; 32],
        state_diff: StateDiff { updates: map },
    }
}

#[test]
fn push_merges_state_diffs() {
    let mut session = Session::new(1);

    let a = account(1);
    let b = account(2);

    session.push_execution(exec_result(
        1,
        vec![(a, AccountState { balance: 50, nonce: 0 })],
    ));

    session.push_execution(exec_result(
        2,
        vec![(b, AccountState { balance: 30, nonce: 0 })],
    ));

    assert_eq!(session.tx_count(), 2);
    assert_eq!(session.merged_state.len(), 2);
    assert_eq!(session.merged_state[&a].balance, 50);
    assert_eq!(session.merged_state[&b].balance, 30);
}

#[test]
fn later_state_overwrites_earlier_state() {
    let mut session = Session::new(1);
    let a = account(1);

    session.push_execution(exec_result(
        1,
        vec![(a, AccountState { balance: 100, nonce: 0 })],
    ));

    session.push_execution(exec_result(
        2,
        vec![(a, AccountState { balance: 80, nonce: 1 })],
    ));

    assert_eq!(session.tx_count(), 2);
    assert_eq!(session.merged_state[&a].balance, 80);
    assert_eq!(session.merged_state[&a].nonce, 1);
}

#[test]
fn close_produces_correct_block_header() {
    let mut session = Session::new(42);
    let a = account(1);

    session.push_execution(exec_result(
        1,
        vec![(a, AccountState { balance: 10, nonce: 0 })],
    ));

    let prev_root = [9u8; 32];
    let closed = session.close(prev_root);

    let header = closed.header;

    assert_eq!(header.magic, HEADER_MAGIC);
    assert_eq!(header.hdr_version, HEADER_VERSION);
    assert_eq!(header.batch_id, 42);
    assert_eq!(header.prev_root, prev_root);
    assert_eq!(header.tx_count, 1);
}

#[test]
fn compute_state_root_is_deterministic() {
    let a = account(1);
    let b = account(2);

    let mut map1 = HashMap::new();
    map1.insert(a, AccountState { balance: 5, nonce: 0 });
    map1.insert(b, AccountState { balance: 7, nonce: 1 });

    let mut map2 = HashMap::new();
    map2.insert(b, AccountState { balance: 7, nonce: 1 });
    map2.insert(a, AccountState { balance: 5, nonce: 0 });

    let r1 = compute_state_root(&map1);
    let r2 = compute_state_root(&map2);

    assert_eq!(r1, r2);
}
