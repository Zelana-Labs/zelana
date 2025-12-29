mod common;

use common::TestFixture;
use bridge_z::{
    instruction::{BridgeIx, SubmitBatchHeader},
    state::Config,
};
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    signer::Signer,
    system_program,
    pubkey::Pubkey
};

#[test]
fn test_submit_batch_success() {
    let mut fixture = TestFixture::new();
    fixture.initialize_bridge().expect("bridge init failed");

    // Load initial config
    let config_before = fixture
        .svm
        .get_account(&fixture.config_pda)
        .expect("config missing");
    let config_state_before: &Config =
        bytemuck::from_bytes(&config_before.data);

    assert_eq!(config_state_before.batch_index, 0);

    // Build batch header
    let header = SubmitBatchHeader {
        prev_batch_index: 0,
        new_batch_index: 1,
        new_state_root: [9u8; 32],
        proof_len: 0,
        withdrawal_count: 0,
    };

    let mut data = vec![BridgeIx::SubmitBatch as u8];
    data.extend_from_slice(bytemuck::bytes_of(&header));

    let dummy_verifier = Pubkey::new_unique();
    fixture.svm.add_program_from_file(
        dummy_verifier,
        "./target/deploy/bridge_z.so", // any valid ELF works
    ).unwrap();

    let accounts = vec![
        AccountMeta::new(fixture.sequencer.pubkey(), true),
        AccountMeta::new(fixture.config_pda, false),
        AccountMeta::new_readonly(fixture.vault_pda, false),
        AccountMeta::new_readonly(dummy_verifier, false), // verifier (unused)
        AccountMeta::new_readonly(system_program::ID, false),
    ];

    let ix = Instruction {
        program_id: fixture.program_id,
        accounts,
        data,
    };

    let sequencer = fixture.sequencer.insecure_clone();
    let result =
        fixture.build_and_send_transaction(&[&sequencer], vec![ix]);

    assert!(result.is_ok(), "submit_batch should succeed");

    // Verify config updated
    let config_after = fixture
        .svm
        .get_account(&fixture.config_pda)
        .expect("config missing after submit");

    let config_state_after: &Config =
        bytemuck::from_bytes(&config_after.data);

    assert_eq!(config_state_after.batch_index, 1);
    assert_eq!(config_state_after.state_root, [9u8; 32]);
}

#[test]
fn test_submit_batch_wrong_sequencer_fails() {
    let mut fixture = TestFixture::new();
    fixture.initialize_bridge().expect("bridge init failed");

    let header = SubmitBatchHeader {
        prev_batch_index: 0,
        new_batch_index: 1,
        new_state_root: [1u8; 32],
        proof_len: 0,
        withdrawal_count: 0,
    };

    let mut data = vec![BridgeIx::SubmitBatch as u8];
    data.extend_from_slice(bytemuck::bytes_of(&header));

    let unauthorized = solana_sdk::signature::Keypair::new();

    let accounts = vec![
        AccountMeta::new(unauthorized.pubkey(), true),
        AccountMeta::new(fixture.config_pda, false),
        AccountMeta::new_readonly(fixture.vault_pda, false),
        AccountMeta::new_readonly(system_program::ID, false),
        AccountMeta::new_readonly(system_program::ID, false),
    ];

    let ix = Instruction {
        program_id: fixture.program_id,
        accounts,
        data,
    };

    let result =
        fixture.build_and_send_transaction(&[&unauthorized], vec![ix]);

    assert!(result.is_err(), "unauthorized submit must fail");
}

#[test]
fn test_submit_batch_replay_fails() {
    let mut fixture = TestFixture::new();
    fixture.initialize_bridge().expect("bridge init failed");

    let header = SubmitBatchHeader {
        prev_batch_index: 0,
        new_batch_index: 1,
        new_state_root: [2u8; 32],
        proof_len: 0,
        withdrawal_count: 0,
    };

    let mut data = vec![BridgeIx::SubmitBatch as u8];
    data.extend_from_slice(bytemuck::bytes_of(&header));

    let accounts = vec![
        AccountMeta::new(fixture.sequencer.pubkey(), true),
        AccountMeta::new(fixture.config_pda, false),
        AccountMeta::new_readonly(fixture.vault_pda, false),
        AccountMeta::new_readonly(system_program::ID, false),
        AccountMeta::new_readonly(system_program::ID, false),
    ];

    let ix = Instruction {
        program_id: fixture.program_id,
        accounts,
        data,
    };

    let sequencer = fixture.sequencer.insecure_clone();

    // First submit succeeds
    assert!(
        fixture
            .build_and_send_transaction(&[&sequencer], vec![ix.clone()])
            .is_ok()
    );

    // Replay with same batch index must fail
    let replay =
        fixture.build_and_send_transaction(&[&sequencer], vec![ix]);

    assert!(replay.is_err(), "batch replay must fail");
}
