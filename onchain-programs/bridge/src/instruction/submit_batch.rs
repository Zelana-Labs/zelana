use pinocchio::{
    account_info::AccountInfo, program_error::ProgramError, pubkey::Pubkey, ProgramResult,
};
use pinocchio_log::log;

use crate::{
    helpers::{check_signer, load_acc_mut},
    instruction::{SubmitBatchHeader,WithdrawalRequest},
    state::Config,
};
use crate::helpers::utils::{Initialized,DataLen};

pub fn process_submit_batch(
    _program_id: &Pubkey,
    accounts: &[AccountInfo],
    ix_data: &[u8],
) -> ProgramResult {
    log!("hi");
    if accounts.len() < 5 {
        return Err(ProgramError::NotEnoughAccountKeys);
    }

    let sequencer = &accounts[0];
    let config_account = &accounts[1];
    let _vault = &accounts[2];
    let _verifier_program = &accounts[3];
    let _system_program = &accounts[4];

    let recipients_iter = &accounts[5..];

    check_signer(sequencer)?;
    let mut config_data = unsafe {
        config_account.borrow_mut_data_unchecked()
    };
    let config = unsafe { load_acc_mut::<Config>(&mut config_data)? };

    if !config.is_initialized() {
        return Err(ProgramError::UninitializedAccount);
    }

    if sequencer.key() != &config.sequencer_authority {
        return Err(ProgramError::IncorrectAuthority);
    }

    let domain = config.domain;


    if ix_data.len() < SubmitBatchHeader::LEN {
        return Err(ProgramError::InvalidInstructionData);
    }
    let ix_data = &ix_data[1..];
    let header: &SubmitBatchHeader = bytemuck::try_from_bytes(
        ix_data.get(..SubmitBatchHeader::LEN)
        .ok_or(ProgramError::InvalidInstructionData)?
    )
    .map_err(|_| ProgramError::InvalidInstructionData)?;

    if header.prev_batch_index != config.batch_index {
        return Err(ProgramError::InvalidInstructionData);
    }

    if header.new_batch_index != config.batch_index + 1 {
        return Err(ProgramError::InvalidInstructionData);
    }

    // Parse proof
    let mut offset = SubmitBatchHeader::LEN;
    let proof_end = offset + header.proof_len as usize;

    if proof_end > ix_data.len() {
        return Err(ProgramError::InvalidInstructionData);
    }
    //proof is opaque ( zk verification pending)
    let _proof = &ix_data[offset..proof_end];
    offset = proof_end;

    // Check withdrawals vs accounts
    if recipients_iter.len() != header.withdrawal_count as usize {
        return Err(ProgramError::InvalidAccountData);
    }

    // Parse each WithdrawalRequest
    for i in 0..header.withdrawal_count as usize {
        let start = offset + i * core::mem::size_of::<WithdrawalRequest>();
        let end = start + core::mem::size_of::<WithdrawalRequest>();

        if end > ix_data.len() {
            return Err(ProgramError::InvalidInstructionData);
        }

        let w: &WithdrawalRequest =
            bytemuck::try_from_bytes(&ix_data[start..end])
                .map_err(|_| ProgramError::InvalidInstructionData)?;

        let recipient_account = &recipients_iter[i];

        // Enforce recipient consistency
        if recipient_account.key() != &w.recipient {
            return Err(ProgramError::InvalidAccountData);
        }

        log!(
            "ZE_WITHDRAW_INTENT:{}:{}",
            recipient_account.key(),
            w.amount
        );
    }

    // Update config
    // Commit new L2 state
    config.state_root = header.new_state_root;
    config.batch_index = header.new_batch_index;

    log!(
        "ZE_BATCH_FINALIZED:{}:{}",
        &domain,
        config.batch_index
    );

    Ok(())
}