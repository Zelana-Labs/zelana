mod common;
use bridge_z::{ID, helpers::{Initialized, StateDefinition}, state::Config};
use common::TestFixture;
use solana_sdk::{signer::Signer};
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
    assert_eq!(config_state.domain, fixture.domain);
    assert!(
        config_state.is_initialized(),
        "config should be initialized"
    );
    let vault_account = fixture.svm.get_account(&fixture.vault_pda).expect("Vault account not found");
    assert_eq!(vault_account.owner, ID.into());
}

#[test]
fn test_initialize_bridge_twice_fails(){
    let mut fixture = TestFixture::new();
    fixture.initialize_bridge().expect("first init");

    let original_config_account = fixture
        .svm
        .get_account(&fixture.config_pda)
        .expect("Config account missing");

    let second_result = fixture.initialize_bridge();

    assert!(second_result.is_err(),"second init should fail");
    // Config must be unchanged
    let config_account_after = fixture
        .svm
        .get_account(&fixture.config_pda)
        .expect("Config account missing after second init");

    assert_eq!(
        original_config_account.data,
        config_account_after.data,
        "config state must not change on re-init attempt"
    );
}
