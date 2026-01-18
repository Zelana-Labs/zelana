#![allow(unexpected_cfgs)]

use pinocchio::{
    ProgramResult, account_info::AccountInfo, default_panic_handler, no_allocator,
    program_entrypoint, program_error::ProgramError, pubkey::Pubkey,
};

use super::ID;
use crate::instruction::{self, BridgeIx};

program_entrypoint!(process_instruction);

//Do not allocate memory.
no_allocator!();
// Use the no_std panic handler.
default_panic_handler!();

fn process_instruction(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    instruction_data: &[u8],
) -> ProgramResult {
    assert_eq!(program_id, &ID);

    let (discriminator, data) = instruction_data
        .split_first()
        .ok_or(ProgramError::InvalidInstructionData)?;

    match BridgeIx::try_from(discriminator)? {
        BridgeIx::INIT => {
            instruction::init::process_initialize(program_id, accounts, data)?;
            Ok(())
        }
        BridgeIx::DEPOSIT => {
            instruction::deposit::process_deposit(accounts, data)?;
            Ok(())
        }
        BridgeIx::WITHDRAWATTESTED => {
            instruction::withdraw::process_withdraw_attested(program_id, accounts, data)?;
            Ok(())
        }
        BridgeIx::SubmitBatch => {
            instruction::submit_batch::process_submit_batch(program_id, accounts, data)?;
            Ok(())
        }
    }
}
