mod common;
use bridge_z::{helpers::StateDefinition, state::Config, ID};
use common::TestFixture;
use pinocchio::{pubkey::Pubkey};
use solana_sdk::{program_error::ProgramError, signer::Signer};
#[test]
fn test_initialize_bridge_success(){
    let mut fixture = TestFixture::new();

    let result = fixture.initialize_bridge();

    assert!(result.is_ok(),"intialize tx failed {:?}",result.unwrap_err());

    let config_account = fixture.svm.get_account(&fixture.config_pda).expect("Config account not found");
    assert_eq!(config_account.owner, ID.into());
    assert_eq!(config_account.data.len(), Config::LEN);
    let config_state: &Config= bytemuck::from_bytes(&config_account.data);
    let pubkey = fixture.sequencer.pubkey();
    assert_eq!(config_state.sequencer_authority, *pubkey.as_array());
    
    let vault_account = fixture.svm.get_account(&fixture.vault_pda).expect("Vault account not found");
    assert_eq!(vault_account.owner, ID.into());
}

#[test]
fn test_initialize_bridge_twice_fails(){
    let mut fixture = TestFixture::new();
    fixture.initialize_bridge().expect("first init");

    let second_result = fixture.initialize_bridge();

    assert!(second_result.is_err(),"second init should fail");
    let tx_error = second_result.unwrap_err().err;
    // assert_eq!(tx_error,ProgramError::AccountAlreadyInitialized);
}
