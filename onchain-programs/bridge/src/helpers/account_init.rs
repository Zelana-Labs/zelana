use pinocchio::{
    account_info::AccountInfo,
    instruction::{Seed, Signer},
    program_error::ProgramError,
    pubkey::Pubkey,
    sysvars::rent::Rent,
};
use pinocchio_system::instructions::CreateAccount;

pub trait HasOwner {
    fn owner(&self) -> &Pubkey;
}

pub trait StateDefinition {
    const LEN: usize;
}

#[inline(always)]
pub fn create_pda_account(
    payer: &AccountInfo,
    account: &AccountInfo,
    signer_seeds: &[Seed],
    rent: &Rent,
) -> Result<(), ProgramError> {
    let signers = [Signer::from(signer_seeds)];

    CreateAccount {
        from: payer,
        to: account,
        space: account.data_len() as u64,
        owner: &crate::ID,
        lamports: rent.minimum_balance(account.data_len()),
    }
    .invoke_signed(&signers)?;

    Ok(())
}
