use pinocchio::{
    account_info::AccountInfo,
    instruction::{Seed, Signer},
    program_error::ProgramError,
    pubkey::{find_program_address, Pubkey},
    sysvars::{clock::Clock, rent::Rent, Sysvar},
    ProgramResult,
};
use pinocchio_log::log;
use pinocchio_system::instructions::CreateAccount;

use crate::{
    helpers::{check_signer, load_acc, load_acc_mut_unchecked, load_ix_data, StateDefinition},
    instruction::WithdrawAttestedParams,
    state::{Config, UsedNullifier, Vault},
    ID,
};

pub fn process_withdraw_attested(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    ix_data: &[u8],
) -> ProgramResult {
    let [sequencer, config_account, vault_account, recipient, user_nillifier_account, _system_program] =
        accounts
    else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };

    check_signer(sequencer)?;

    let config_data = config_account.try_borrow_data()?;
    let config_state = unsafe { load_acc::<Config>(&config_data)? };

    if *sequencer.key() != config_state.sequencer_authority {
        return Err(ProgramError::IncorrectAuthority);
    }

    let params = unsafe { load_ix_data::<WithdrawAttestedParams>(ix_data)? };

    if params.amount == 0 {
        return Err(ProgramError::InvalidInstructionData);
    }

    let (expected_vault_pda, _) = find_program_address(
        &[Vault::SEED.as_bytes(), config_account.key().as_ref()],
        &ID,
    );

    if vault_account.key() != &expected_vault_pda {
        return Err(ProgramError::InvalidSeeds);
    }

    if user_nillifier_account.lamports() > 0 {
        return Err(ProgramError::AccountAlreadyInitialized);
    }
    let nullifier_seeds: &[&[u8]] = &[
        UsedNullifier::SEED.as_bytes(),
        config_account.key().as_ref(),
        &params.nullifier,
    ];
    let (nullfier_pda, nullifier_bump) = find_program_address(nullifier_seeds, &ID);

    if &nullfier_pda != user_nillifier_account.key() {
        return Err(ProgramError::InvalidSeeds);
    }

    let rent = Rent::get()?;
    let bump_bytes = [nullifier_bump];
    let nullifier_signer_seeds = &[
        Seed::from(UsedNullifier::SEED.as_bytes()),
        Seed::from(config_account.key().as_ref()),
        Seed::from(&params.nullifier),
        Seed::from(&bump_bytes[..]),
    ];

    let signer_seeds = [Signer::from(nullifier_signer_seeds)];
    CreateAccount {
        from: sequencer,
        to: user_nillifier_account,
        space: UsedNullifier::LEN as u64,
        owner: program_id,
        lamports: rent.minimum_balance(UsedNullifier::LEN),
    }
    .invoke_signed(&signer_seeds)?;

    let mut nullifier_data = user_nillifier_account.try_borrow_mut_data()?;
    let nullifier_state = unsafe { load_acc_mut_unchecked::<UsedNullifier>(&mut nullifier_data)? };
    nullifier_state.new(
        params.nullifier,
        *recipient.key(),
        params.amount,
        nullifier_bump,
    );

    *vault_account.try_borrow_mut_lamports()? -= params.amount;
    *recipient.try_borrow_mut_lamports()? += params.amount;

    let clock = Clock::get()?;

    log!("withdraw:{}", params.amount);
    log!("ts:{}", clock.unix_timestamp);

    Ok(())
}