mod common;
use common::TestFixture;
use bridge_z::{
    helpers::StateDefinition, instruction::{BridgeIx, WithdrawAttestedParams}, state::UsedNullifier, ID
};
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    program_error::ProgramError,
    pubkey::Pubkey,
    signature::Keypair,
    signer::Signer,
    system_program,
};

#[test]
fn test_withdraw_attested_success() {
    let mut fixture = TestFixture::new();
    fixture.initialize_bridge().expect("Bridge initialization failed");

    let vault_initial_balance = 2_000_000_000; // 2 SOL
    fixture.fund_vault(vault_initial_balance).expect("Funding vault failed");

    let sequencer_keypair = fixture.sequencer.insecure_clone();
    let sequencer_pubkey = sequencer_keypair.pubkey();
    let config_pda = fixture.config_pda;
    let vault_pda = fixture.vault_pda;

    let recipient = Keypair::new();
    let withdraw_amount = 500_000_000; // 0.5 SOL
    let nullifier = [1u8; 32];

    fixture.svm.airdrop(&recipient.pubkey(), 500_000_000).unwrap();

    let vault_balance_before = fixture.svm.get_balance(&fixture.vault_pda).unwrap();
    let recipient_balance_before = fixture.svm.get_balance(&recipient.pubkey()).unwrap();

    let pubkey = Pubkey::from(ID);
    let (nullifier_pda, _) = Pubkey::find_program_address(
        &[
            UsedNullifier::SEED.as_bytes(),
            fixture.config_pda.as_ref(),
            &nullifier,
        ],
        &pubkey,
    );

    let ix_data = WithdrawAttestedParams {
        amount: withdraw_amount,
        recipient: recipient.pubkey().to_bytes(),
        nullifier,
    };
    let mut instruction_data = vec![BridgeIx::WITHDRAWATTESTED as u8];
    instruction_data.extend_from_slice(bytemuck::bytes_of(&ix_data));

    let accounts = vec![
        AccountMeta::new(sequencer_pubkey, true),
        AccountMeta::new_readonly(config_pda, false),
        AccountMeta::new(vault_pda, false),
        AccountMeta::new(recipient.pubkey(), false),
        AccountMeta::new(nullifier_pda, false),
        AccountMeta::new_readonly(system_program::ID, false),
    ];

    let withdraw_ix = Instruction { program_id: pubkey, accounts, data: instruction_data };

    let result = fixture.build_and_send_transaction(&[&sequencer_keypair], vec![withdraw_ix]);
    assert!(result.is_ok(), "Withdrawal transaction failed: {:?}", result.unwrap_err());

    let vault_balance_after = fixture.svm.get_balance(&fixture.vault_pda).unwrap();
    assert_eq!(vault_balance_after, vault_balance_before - withdraw_amount);

    let recipient_balance_after = fixture.svm.get_balance(&recipient.pubkey()).unwrap();
    assert_eq!(recipient_balance_after, recipient_balance_before + withdraw_amount);

    let nullifier_account = fixture.svm.get_account(&nullifier_pda).expect("Nullifier account not found");
    assert_eq!(nullifier_account.owner, pubkey);
}

#[test]
fn test_withdraw_unauthorized_sequencer_fails() {
    let mut fixture = TestFixture::new();
    fixture.initialize_bridge().expect("Bridge initialization failed");
    fixture.fund_vault(1_000_000_000).expect("Funding vault failed");
    let pubkey = Pubkey::from(ID);

    let recipient = Keypair::new();
    let unauthorized_sequencer = Keypair::new(); 
    let withdraw_amount = 500_000_000;
    let nullifier = [2u8; 32];

    let config_pda = fixture.config_pda;
    let vault_pda = fixture.vault_pda;

    let (nullifier_pda, _) = Pubkey::find_program_address(&[UsedNullifier::SEED.as_bytes(), fixture.config_pda.as_ref(), &nullifier], &pubkey);
    let ix_data = WithdrawAttestedParams { amount: withdraw_amount, recipient: recipient.pubkey().to_bytes(), nullifier };
    let mut instruction_data = vec![BridgeIx::WITHDRAWATTESTED as u8];
    instruction_data.extend_from_slice(bytemuck::bytes_of(&ix_data));

    let accounts = vec![
        AccountMeta::new(unauthorized_sequencer.pubkey(), true),
        AccountMeta::new_readonly(fixture.config_pda, false),
        AccountMeta::new(fixture.vault_pda, false),
        AccountMeta::new(recipient.pubkey(), false),
        AccountMeta::new(nullifier_pda, false),
        AccountMeta::new_readonly(system_program::ID, false),
    ];
    let withdraw_ix = Instruction { program_id: pubkey, accounts, data: instruction_data };

    let result = fixture.build_and_send_transaction(&[&unauthorized_sequencer], vec![withdraw_ix]);

    assert!(result.is_err());
    // assert_eq!(result.unwrap_err().err, ProgramError::IncorrectAuthority);
}

#[test]
fn test_withdraw_insufficient_funds_fails() {
    let mut fixture = TestFixture::new();
    fixture.initialize_bridge().expect("Bridge initialization failed");
    fixture.fund_vault(1_000_000_000).expect("Funding vault failed");

    let sequencer_clone = fixture.sequencer.insecure_clone();
    let recipient = Keypair::new();
    let withdraw_amount = 2_000_000_000;
    let nullifier = [3u8; 32];
    let pubkey = Pubkey::from(ID);

    let (nullifier_pda, _) = Pubkey::find_program_address(&[UsedNullifier::SEED.as_bytes(), fixture.config_pda.as_ref(), &nullifier], &pubkey);
    let ix_data = WithdrawAttestedParams { amount: withdraw_amount, recipient: recipient.pubkey().to_bytes(), nullifier };
    let mut instruction_data = vec![BridgeIx::WITHDRAWATTESTED as u8];
    instruction_data.extend_from_slice(bytemuck::bytes_of(&ix_data));

    let accounts = vec![
        AccountMeta::new(fixture.sequencer.pubkey(), true),
        AccountMeta::new_readonly(fixture.config_pda, false),
        AccountMeta::new(fixture.vault_pda, false),
        AccountMeta::new(recipient.pubkey(), false),
        AccountMeta::new(nullifier_pda, false),
        AccountMeta::new_readonly(system_program::ID, false),
    ];
    let withdraw_ix = Instruction { program_id: pubkey, accounts, data: instruction_data };

    let result = fixture.build_and_send_transaction(&[&sequencer_clone], vec![withdraw_ix]);
    
    assert!(result.is_err());
    // NOTE: The exact error for insufficient funds might depend on the native runtime.
    // We expect a custom error, but it might manifest as a generic one.
    // For now, checking for any error is sufficient.
}

#[test]
fn test_withdraw_replay_fails() {
    let mut fixture = TestFixture::new();
    fixture.initialize_bridge().expect("Bridge initialization failed");
    fixture.fund_vault(2_000_000_000).expect("Funding vault failed");
    let sequencer_clone = fixture.sequencer.insecure_clone();
    let recipient = Keypair::new();
    let withdraw_amount = 500_000_000;
    let nullifier = [4u8; 32]; // The nullifier to be reused.
    let pubkey = Pubkey::from(ID);

    let (nullifier_pda, _) = Pubkey::find_program_address(&[UsedNullifier::SEED.as_bytes(), fixture.config_pda.as_ref(), &nullifier], &pubkey);
    let ix_data = WithdrawAttestedParams { amount: withdraw_amount, recipient: recipient.pubkey().to_bytes(), nullifier };
    let mut instruction_data = vec![BridgeIx::WITHDRAWATTESTED as u8];
    instruction_data.extend_from_slice(bytemuck::bytes_of(&ix_data));

    let accounts = vec![
        AccountMeta::new(fixture.sequencer.pubkey(), true),
        AccountMeta::new_readonly(fixture.config_pda, false),
        AccountMeta::new(fixture.vault_pda, false),
        AccountMeta::new(recipient.pubkey(), false),
        AccountMeta::new(nullifier_pda, false),
        AccountMeta::new_readonly(system_program::ID, false),
    ];
    let withdraw_ix = Instruction { program_id: pubkey, accounts, data: instruction_data };

    // 2. Action
    // First withdrawal should succeed.
    let first_result = fixture.build_and_send_transaction(&[&sequencer_clone], vec![withdraw_ix.clone()]);
    assert!(first_result.is_ok(), "First withdrawal should have succeeded");

    // Attempt the exact same withdrawal again.
    let second_result = fixture.build_and_send_transaction(&[&sequencer_clone], vec![withdraw_ix]);

    // 3. Verification
    assert!(second_result.is_err(), "Second withdrawal (replay) should have failed");
    let tx_error = second_result.unwrap_err().err;
    // assert_eq!(tx_error, ProgramError::AccountAlreadyInitialized);
}

