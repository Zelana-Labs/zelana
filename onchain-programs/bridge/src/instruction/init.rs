use pinocchio::{account_info::AccountInfo, instruction::{Seed, Signer}, program_error::ProgramError, pubkey::{find_program_address, Pubkey}, sysvars::{rent::Rent, Sysvar}, ProgramResult};
use pinocchio_system::instructions::CreateAccount;

use crate::{
    helpers::{ check_signer, load_acc_mut_unchecked, load_ix_data, StateDefinition},
    instruction::InitParams,
    state::{Config, Vault,},
    ID,
};


pub fn process_initialize(program_id:&Pubkey,accounts:&[AccountInfo],ix_data:&[u8])->ProgramResult{
    let [payer,config_account,vault_account,_system_program] = accounts else{
        return  Err(ProgramError::NotEnoughAccountKeys);
    };

    check_signer(payer)?;

    if config_account.lamports()>0{
        return Err(ProgramError::AccountAlreadyInitialized);
    }

    let (config_pda,config_bump) = find_program_address(&[Config::SEED.as_bytes()], &ID);

    if config_pda != *config_account.key(){
        return Err(ProgramError::InvalidSeeds);
    }

    let (vault_pda , vault_bump) = find_program_address(&[Vault::SEED.as_bytes(), config_account.key().as_ref()], &ID);

    if vault_pda!=*vault_account.key(){
        return Err(ProgramError::InvalidSeeds);
    }

    let rent = Rent::get()?;
    let bump_bytes = [config_bump];
    let config_signer_seeds= [
        Seed::from(Config::SEED.as_bytes()),
        Seed::from(&bump_bytes[..])
    ];
    
    let signers = [Signer::from(&config_signer_seeds)];


    CreateAccount{
        from: payer,
        to:config_account,
        space:Config::LEN as u64,
        owner:program_id,
        lamports:rent.minimum_balance(Config::LEN),
    }.invoke_signed(&signers)?;

    let bump_bytes = [vault_bump];
    let vault_signer_seeds = [
        Seed::from(Vault::SEED.as_bytes()),
        Seed::from(config_account.key().as_ref()),
        Seed::from(&bump_bytes[..])
    ];

    let signers = [Signer::from(&vault_signer_seeds)];

    CreateAccount{
        from:payer,
        to:vault_account,
        space:Vault::LEN as u64,
        owner:program_id,
        lamports:rent.minimum_balance(Vault::LEN)
    }.invoke_signed(&signers)?;

    let ix_data = unsafe {
        load_ix_data::<InitParams>(&ix_data)?
    };

    let  config_data = &mut config_account.try_borrow_mut_data()?;
    let config_state = unsafe {
        load_acc_mut_unchecked::<Config>(config_data)?
    };

    config_state.new(ix_data.sequencer_authority, ix_data.domain, config_bump);

    let  vault_account = &mut vault_account.try_borrow_mut_data()?;
    let vault_state = unsafe{
        load_acc_mut_unchecked::<Vault>( vault_account)?
    };
    vault_state.new(vault_bump);
    Ok(())

}