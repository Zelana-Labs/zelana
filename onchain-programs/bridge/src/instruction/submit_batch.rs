use pinocchio::{
    account_info::AccountInfo, program_error::ProgramError, pubkey::Pubkey, ProgramResult,
};
use pinocchio_log::log;

use crate::{
    helpers::{check_signer, load_acc_mut, load_ix_data},
    instruction::SubmitBatchParams,
    state::Config,
};

pub fn process_submit_batch(
    _program_id: &Pubkey,
    accounts: &[AccountInfo],
    ix_data: &[u8],
) -> ProgramResult {
    //[Sequencer, Config, (Optional) Verifier, System]
    let (sequencer, config_account, _verifier_opt, _system_program) = match accounts {
        [seq, cfg, verifier, sys] => (seq, cfg, Some(verifier), sys),
        [seq, cfg, sys] => (seq, cfg, None, sys),
        _ => return Err(ProgramError::NotEnoughAccountKeys),
    };

    // implement zk proof verification
    // currently, ix relies only on the sequencer's signature
    // we can use 'verifier_opt'  to perform cpi to verifier program to validate the batch proof before updaing the root.
    check_signer(sequencer)?;

    let mut config_data = config_account.try_borrow_mut_data()?;
    let config = unsafe { load_acc_mut::<Config>(&mut config_data)? };

    if *sequencer.key() != config.sequencer_authority {
        return Err(ProgramError::IncorrectAuthority);
    }
    let params = unsafe { load_ix_data::<SubmitBatchParams>(ix_data)? };
    log!("Old Root: {}", &config.state_root);
    log!("New Root: {}", &params.new_state_root);

    config.state_root = params.new_state_root;
    config.batch_index += 1;

    log!("ZE_BATCH_FINALIZED:{}:0", config.batch_index);
    Ok(())
}