use pinocchio::{account_info::AccountInfo, instruction::{Seed, Signer},  program_error::ProgramError, pubkey::{find_program_address}, sysvars::{clock::Clock, rent::Rent, Sysvar}, ProgramResult};
use pinocchio_system::instructions::{CreateAccount, Transfer};
use pinocchio_log::log;

use crate::{helpers::{check_signer, load_acc_mut_unchecked, load_ix_data, StateDefinition}, instruction::DepositParams, state::{DepositReceipt, Vault}, ID};

pub fn process_deposit(accounts:&[AccountInfo],ix_data:&[u8])->ProgramResult{
    let [depositor, config_account,vault_account,deposit_reciept_account,_system_program] = accounts else{
        return Err(ProgramError::NotEnoughAccountKeys)
    };

    let params  = unsafe{
        load_ix_data::<DepositParams>(ix_data)?
    };

    check_signer(depositor)?;

    if params.amount == 0 {
        return Err(ProgramError::InvalidInstructionData)
    }

    if !deposit_reciept_account.data_is_empty(){
        return Err(ProgramError::AccountAlreadyInitialized)
    }

    let (vault_pda,_vault_bump) = find_program_address(&[Vault::SEED.as_bytes(), config_account.key().as_ref()], &ID);
    
    if &vault_pda != vault_account.key(){
        return Err(ProgramError::InvalidSeeds)
    }
    
    let nonce_le = params.nonce.to_le_bytes();

    
    
    let (receipt_pda,receipt_bump) = find_program_address(&[
            DepositReceipt::SEED.as_bytes(),
            config_account.key().as_ref(),
            depositor.key().as_ref(),
            &nonce_le,
        ],
        &ID,);
    
    if &receipt_pda != deposit_reciept_account.key(){
        return Err(ProgramError::InvalidSeeds);
    }


    Transfer{
        from: depositor,
        to:vault_account,
        lamports:params.amount
    }.invoke()?;

    //creation of deposit receipt PDA
    let rent = Rent::get()?;
    let bump_bytes = [receipt_bump];
    let receipt_seeds = [
        Seed::from(DepositReceipt::SEED.as_bytes()),
        Seed::from(config_account.key().as_ref()),
        Seed::from(depositor.key().as_ref()),
        Seed::from(&nonce_le),
        Seed::from(&bump_bytes[..])
    ];

    let signer_seeds = Signer::from(&receipt_seeds);
    CreateAccount{
        from:depositor,
        to:deposit_reciept_account,
        space:DepositReceipt::LEN as u64,
        owner:&ID,
        lamports: rent.minimum_balance(DepositReceipt::LEN)
    }.invoke_signed(&[signer_seeds])?;

    let clock = Clock::get()?;
    let mut receipt_data = deposit_reciept_account.try_borrow_mut_data()?;

    let receipt_state = unsafe {
        load_acc_mut_unchecked::<DepositReceipt>(&mut receipt_data)?
    };

    receipt_state.new(
        *depositor.key(),
        params.amount,
        params.nonce,
        clock.unix_timestamp,
        receipt_bump
    );
    
    log!("depositor:{}",depositor.key());
    log!("vault:{}",vault_account.key());
    log!("config:{}",config_account.key());
    log!("amount:{}",params.amount);
    log!("nonce:{}",params.nonce);
    log!("ts:{}",clock.unix_timestamp);
    Ok(())
}
