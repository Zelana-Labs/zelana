use pinocchio::{
    ProgramResult,
    account_info::AccountInfo,
    instruction::{Seed, Signer},
    program_error::ProgramError,
    pubkey::Pubkey,
    sysvars::{Sysvar, rent::Rent},
};
use pinocchio_system::instructions::CreateAccount;

use crate::{
    ID,
    helpers::{
        StateDefinition, check_signer, load_acc_mut_unchecked, load_ix_data,
        utils::{derive_config_pda, derive_vault_pda},
    },
    instruction::InitParams,
    state::{Config, Vault},
};

pub fn process_initialize(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    ix_data: &[u8],
) -> ProgramResult {
    let [payer, config_account, vault_account, _system_program] = accounts else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };

    check_signer(payer)?;

    // decode ix data
    let params = unsafe { load_ix_data::<InitParams>(ix_data)? };

    if params.domain == [0u8; 32] {
        return Err(ProgramError::InvalidInstructionData);
    }

    let (expected_config_pda, config_bump) = derive_config_pda(&ID, &params.domain);

    if config_account.key() != &expected_config_pda {
        return Err(ProgramError::InvalidSeeds);
    }

    let (expected_vault_pda, vault_bump) = derive_vault_pda(&ID, &params.domain);

    if vault_account.key() != &expected_vault_pda {
        return Err(ProgramError::InvalidSeeds);
    }

    if config_account.lamports() > 0 {
        return Err(ProgramError::AccountAlreadyInitialized);
    }

    if !config_account.data_is_empty() {
        return Err(ProgramError::AccountAlreadyInitialized);
    }

    if !vault_account.data_is_empty() {
        return Err(ProgramError::AccountAlreadyInitialized);
    }

    let rent = Rent::get()?;
    let bump_bytes = [config_bump];
    let config_seeds = [
        Seed::from(b"config"),
        Seed::from(params.domain.as_ref()),
        Seed::from(&bump_bytes[..]),
    ];

    let signers = [Signer::from(&config_seeds)];

    CreateAccount {
        from: payer,
        to: config_account,
        space: Config::LEN as u64,
        owner: program_id,
        lamports: rent.minimum_balance(Config::LEN),
    }
    .invoke_signed(&signers)?;

    let bump_bytes = [vault_bump];
    let vault_seeds = [
        Seed::from(b"vault"),
        Seed::from(params.domain.as_ref()),
        Seed::from(&bump_bytes[..]),
    ];

    let signers = [Signer::from(&vault_seeds)];

    CreateAccount {
        from: payer,
        to: vault_account,
        space: Vault::LEN as u64,
        owner: program_id,
        lamports: rent.minimum_balance(Vault::LEN),
    }
    .invoke_signed(&signers)?;

    let config_data = &mut config_account.try_borrow_mut_data()?;
    let config_state = unsafe { load_acc_mut_unchecked::<Config>(config_data)? };

    config_state.new(params.sequencer_authority, params.domain, config_bump)?;

    let vault_account = &mut vault_account.try_borrow_mut_data()?;
    let vault_state = unsafe { load_acc_mut_unchecked::<Vault>(vault_account)? };
    vault_state.new(params.domain, vault_bump);
    Ok(())
}
