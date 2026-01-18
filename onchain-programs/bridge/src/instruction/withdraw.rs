use pinocchio::{
    ProgramResult,
    account_info::AccountInfo,
    instruction::{Seed, Signer},
    program_error::ProgramError,
    pubkey::Pubkey,
    sysvars::{Sysvar, clock::Clock, rent::Rent},
};
use pinocchio_log::log;
use pinocchio_system::instructions::CreateAccount;

use crate::helpers::utils::Initialized;
use crate::{
    ID,
    helpers::{
        StateDefinition, check_signer, derive_nullifier_pda, derive_vault_pda, load_acc,
        load_acc_mut_unchecked, load_ix_data,
    },
    instruction::WithdrawAttestedParams,
    state::{Config, UsedNullifier},
};

pub fn process_withdraw_attested(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    ix_data: &[u8],
) -> ProgramResult {
    let [
        sequencer,
        config_account,
        vault_account,
        recipient,
        nullifier_account,
        _system_program,
    ] = accounts
    else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };

    check_signer(sequencer)?;

    let config_data = config_account.try_borrow_data()?;
    let config = unsafe { load_acc::<Config>(&config_data)? };

    if !config.is_initialized() {
        return Err(ProgramError::UninitializedAccount);
    }

    if sequencer.key() != &config.sequencer_authority {
        return Err(ProgramError::IncorrectAuthority);
    }

    let domain = config.domain;

    let params = unsafe { load_ix_data::<WithdrawAttestedParams>(ix_data)? };

    if params.amount == 0 {
        return Err(ProgramError::InvalidInstructionData);
    }

    let (expected_vault_pda, _) = derive_vault_pda(&ID, &domain);

    if vault_account.key() != &expected_vault_pda {
        return Err(ProgramError::InvalidSeeds);
    }

    let (nullifier_pda, nullifier_bump) = derive_nullifier_pda(&ID, &domain, &params.nullifier);

    if nullifier_account.key() != &nullifier_pda {
        return Err(ProgramError::InvalidSeeds);
    }

    if !nullifier_account.data_is_empty() {
        return Err(ProgramError::InvalidInstructionData); // replay attempt
    }

    let rent = Rent::get()?;
    let bump_bytes = [nullifier_bump];

    let nullifier_seeds = [
        Seed::from(b"nullifier"),
        Seed::from(domain.as_ref()),
        Seed::from(&params.nullifier),
        Seed::from(&bump_bytes),
    ];

    CreateAccount {
        from: sequencer,
        to: nullifier_account,
        space: UsedNullifier::LEN as u64,
        owner: program_id,
        lamports: rent.minimum_balance(UsedNullifier::LEN),
    }
    .invoke_signed(&[Signer::from(&nullifier_seeds)])?;

    let mut nullifier_data = nullifier_account.try_borrow_mut_data()?;
    let nullifier_state = unsafe { load_acc_mut_unchecked::<UsedNullifier>(&mut nullifier_data)? };
    nullifier_state.new(
        domain,
        params.nullifier,
        *recipient.key(),
        params.amount,
        nullifier_bump,
    )?;

    *vault_account.try_borrow_mut_lamports()? -= params.amount;
    *recipient.try_borrow_mut_lamports()? += params.amount;

    let clock = Clock::get()?;

    log!("withdraw:{}", params.amount);
    log!("ts:{}", clock.unix_timestamp);

    Ok(())
}
