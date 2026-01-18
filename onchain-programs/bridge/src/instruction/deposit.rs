use pinocchio::{
    ProgramResult,
    account_info::AccountInfo,
    instruction::{Seed, Signer},
    program_error::ProgramError,
    sysvars::{Sysvar, clock::Clock, rent::Rent},
};
use pinocchio_log::log;
use pinocchio_system::instructions::{CreateAccount, Transfer};

use crate::helpers::utils::Initialized;
use crate::{
    ID,
    helpers::{
        StateDefinition, check_signer, derive_deposit_receipt_pda, derive_vault_pda,
        load_acc_mut_unchecked, load_ix_data,
    },
    instruction::DepositParams,
    state::{Config, DepositReceipt},
};

pub fn process_deposit(accounts: &[AccountInfo], ix_data: &[u8]) -> ProgramResult {
    let [
        depositor,
        config_account,
        vault_account,
        receipt_account,
        _system_program,
    ] = accounts
    else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };

    let params = unsafe { load_ix_data::<DepositParams>(ix_data)? };

    check_signer(depositor)?;

    if params.amount == 0 {
        return Err(ProgramError::InvalidInstructionData);
    }

    let mut config_data = config_account.try_borrow_mut_data()?;

    let config = unsafe { load_acc_mut_unchecked::<Config>(&mut config_data)? };

    // load config and domain

    if !config.is_initialized() {
        return Err(ProgramError::UninitializedAccount);
    }

    let domain = config.domain;

    //verify vault pda
    let (vault_pda, _) = derive_vault_pda(&ID, &domain);

    if vault_account.key() != &vault_pda {
        return Err(ProgramError::InvalidSeeds);
    }

    //derive recipet pda
    let (receipt_pda, receipt_bump) =
        derive_deposit_receipt_pda(&ID, &domain, depositor.key(), params.nonce);

    if receipt_account.key() != &receipt_pda {
        return Err(ProgramError::InvalidSeeds);
    }

    if !receipt_account.data_is_empty() {
        return Err(ProgramError::AccountAlreadyInitialized);
    }

    //transfer funds
    Transfer {
        from: depositor,
        to: vault_account,
        lamports: params.amount,
    }
    .invoke()?;
    //creation of deposit receipt PDA
    let rent = Rent::get()?;

    let nonce_le = params.nonce.to_le_bytes();
    let bump_bytes = [receipt_bump];

    let receipt_seeds = [
        Seed::from(b"receipt"),
        Seed::from(domain.as_ref()),
        Seed::from(depositor.key().as_ref()),
        Seed::from(&nonce_le),
        Seed::from(&bump_bytes),
    ];

    CreateAccount {
        from: depositor,
        to: receipt_account,
        space: DepositReceipt::LEN as u64,
        owner: &ID,
        lamports: rent.minimum_balance(DepositReceipt::LEN),
    }
    .invoke_signed(&[Signer::from(&receipt_seeds)])?;

    // Initialize receipt state
    let clock = Clock::get()?;
    let mut receipt_data = receipt_account.try_borrow_mut_data()?;

    let receipt = unsafe { load_acc_mut_unchecked::<DepositReceipt>(&mut receipt_data)? };

    receipt.new(
        *depositor.key(),
        domain,
        params.amount,
        params.nonce,
        clock.unix_timestamp,
        receipt_bump,
    )?;

    log!(
        "ZE_DEPOSIT:{}:{}:{}",
        depositor.key(),
        params.amount,
        params.nonce
    );

    Ok(())
}
