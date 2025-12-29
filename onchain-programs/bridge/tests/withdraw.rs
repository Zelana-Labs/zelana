mod common;
use common::TestFixture;
use bridge_z::{
     ID, helpers::{Initialized, derive_nullifier_pda}, instruction::{BridgeIx, WithdrawAttestedParams}, state::UsedNullifier
};
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    signature::Keypair,
    signer::Signer,
    system_program,
};

#[test]
fn test_withdraw_attested_success() {
     let mut fixture = TestFixture::new();
    fixture.initialize_bridge().expect("Bridge initialization failed");

    let vault_initial_balance = 2_000_000_000;
    fixture.fund_vault(vault_initial_balance).expect("Funding vault failed");

    let recipient = Keypair::new();
    fixture.svm.airdrop(&recipient.pubkey(), 500_000_000).unwrap();

    let withdraw_amount = 500_000_000;
    let nullifier = [1u8; 32];

    let vault_balance_before = fixture.svm.get_balance(&fixture.vault_pda).unwrap();
    let recipient_balance_before = fixture.svm.get_balance(&recipient.pubkey()).unwrap();

    
    let ( nullifier_pda,_) = Pubkey::find_program_address(
             &[b"nullifier", fixture.domain.as_ref(), &nullifier],
        &fixture.program_id,
    );

    let ix_data = WithdrawAttestedParams {
        recipient: recipient.pubkey().to_bytes(),
        amount: withdraw_amount,
        nullifier,
    };

    let mut instruction_data = vec![BridgeIx::WITHDRAWATTESTED as u8];
    instruction_data.extend_from_slice(bytemuck::bytes_of(&ix_data));

    let accounts = vec![
        AccountMeta::new(fixture.sequencer.pubkey(), true),
        AccountMeta::new_readonly(fixture.config_pda, false),
        AccountMeta::new(fixture.vault_pda, false),
        AccountMeta::new(recipient.pubkey(), false),
        AccountMeta::new(nullifier_pda.into(), false),
        AccountMeta::new_readonly(system_program::ID, false),
    ];

    let ix = Instruction {
        program_id: fixture.program_id,
        accounts,
        data: instruction_data,
    };

    let sequencer = fixture.sequencer.insecure_clone();
    let result = fixture.build_and_send_transaction(&[&sequencer], vec![ix]);
    assert!(result.is_ok(), "withdraw should succeed");

    // Balance checks
    let vault_balance_after = fixture.svm.get_balance(&fixture.vault_pda).unwrap();
    assert_eq!(vault_balance_after, vault_balance_before - withdraw_amount);

    let recipient_balance_after = fixture.svm.get_balance(&recipient.pubkey()).unwrap();
    assert_eq!(recipient_balance_after, recipient_balance_before + withdraw_amount);

    // Nullifier checks
    let nullifier_account = fixture
        .svm
        .get_account(&nullifier_pda.into())
        .expect("Nullifier account not found");

    let nullifier_state: &UsedNullifier =
        bytemuck::from_bytes(&nullifier_account.data);

    assert_eq!(nullifier_state.domain, fixture.domain);
    assert_eq!(nullifier_state.nullifier, nullifier);
    assert!(nullifier_state.is_initialized());
}


#[test]
fn test_withdraw_unauthorized_sequencer_fails() {
    let mut fixture = TestFixture::new();
    fixture.initialize_bridge().expect("Bridge initialization failed");
    fixture.fund_vault(1_000_000_000).expect("Funding vault failed");

    let recipient = Keypair::new();
    let unauthorized = Keypair::new();
    let nullifier = [2u8; 32];

    let ( nullifier_pda,_) = Pubkey::find_program_address(
             &[b"nullifier", fixture.domain.as_ref(), &nullifier],
        &fixture.program_id,
    );
    let ix_data = WithdrawAttestedParams {
        recipient: recipient.pubkey().to_bytes(),
        amount: 500_000_000,
        nullifier,
    };

    let mut data = vec![BridgeIx::WITHDRAWATTESTED as u8];
    data.extend_from_slice(bytemuck::bytes_of(&ix_data));

    let accounts = vec![
        AccountMeta::new(unauthorized.pubkey(), true),
        AccountMeta::new_readonly(fixture.config_pda, false),
        AccountMeta::new(fixture.vault_pda, false),
        AccountMeta::new(recipient.pubkey(), false),
        AccountMeta::new(nullifier_pda.into(), false),
        AccountMeta::new_readonly(system_program::ID, false),
    ];

    let ix = Instruction {
        program_id: fixture.program_id,
        accounts,
        data,
    };

    let result = fixture.build_and_send_transaction(&[&unauthorized], vec![ix]);
    assert!(result.is_err(), "unauthorized sequencer must fail");
}

#[test]
fn test_withdraw_replay_fails() {
    let mut fixture = TestFixture::new();
    fixture.initialize_bridge().expect("Bridge initialization failed");
    fixture.fund_vault(2_000_000_000).expect("Funding vault failed");

    let recipient = Keypair::new();
    let withdraw_amount = 500_000_000;
    let nullifier = [4u8; 32];

    let ( nullifier_pda,_) = Pubkey::find_program_address(
             &[b"nullifier", fixture.domain.as_ref(), &nullifier],
        &fixture.program_id,
    );
    let ix_data = WithdrawAttestedParams {
        recipient: recipient.pubkey().to_bytes(),
        amount: withdraw_amount,
        nullifier,
    };

    let mut data = vec![BridgeIx::WITHDRAWATTESTED as u8];
    data.extend_from_slice(bytemuck::bytes_of(&ix_data));

    let accounts = vec![
        AccountMeta::new(fixture.sequencer.pubkey(), true),
        AccountMeta::new_readonly(fixture.config_pda, false),
        AccountMeta::new(fixture.vault_pda, false),
        AccountMeta::new(recipient.pubkey(), false),
        AccountMeta::new(nullifier_pda.into(), false),
        AccountMeta::new_readonly(system_program::ID, false),
    ];

    let ix = Instruction {
        program_id: fixture.program_id,
        accounts,
        data,
    };

    let sequencer = fixture.sequencer.insecure_clone();
    // First withdraw succeeds
    assert!(
        fixture
            .build_and_send_transaction(&[&sequencer], vec![ix.clone()])
            .is_ok()
    );

    // Replay must fail
    let replay =
        fixture.build_and_send_transaction(&[&sequencer], vec![ix]);

    assert!(replay.is_err(), "replay withdraw must fail");
}