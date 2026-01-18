use pinocchio::{
    ProgramResult,
    account_info::AccountInfo,
    instruction::{AccountMeta, Instruction},
    program::invoke,
    program_error::ProgramError,
    pubkey::Pubkey,
};
use pinocchio_log::log;

use crate::helpers::utils::{DataLen, Initialized};
use crate::{
    helpers::{check_signer, load_acc_mut},
    instruction::{BatchPublicInputs, Groth16Proof, SubmitBatchHeader, WithdrawalRequest},
    state::Config,
};

/// Parse SubmitBatchHeader from unaligned bytes (avoids bytemuck alignment issues)
/// Layout: prev_batch_index (8) + new_batch_index (8) + new_state_root (32) + proof_len (4) + withdrawal_count (4)
fn parse_header_unaligned(data: &[u8]) -> Result<SubmitBatchHeader, ProgramError> {
    if data.len() < SubmitBatchHeader::LEN {
        return Err(ProgramError::InvalidInstructionData);
    }

    let prev_batch_index = u64::from_le_bytes(
        data[0..8]
            .try_into()
            .map_err(|_| ProgramError::InvalidInstructionData)?,
    );
    let new_batch_index = u64::from_le_bytes(
        data[8..16]
            .try_into()
            .map_err(|_| ProgramError::InvalidInstructionData)?,
    );
    let new_state_root: [u8; 32] = data[16..48]
        .try_into()
        .map_err(|_| ProgramError::InvalidInstructionData)?;
    let proof_len = u32::from_le_bytes(
        data[48..52]
            .try_into()
            .map_err(|_| ProgramError::InvalidInstructionData)?,
    );
    let withdrawal_count = u32::from_le_bytes(
        data[52..56]
            .try_into()
            .map_err(|_| ProgramError::InvalidInstructionData)?,
    );

    Ok(SubmitBatchHeader {
        prev_batch_index,
        new_batch_index,
        new_state_root,
        proof_len,
        withdrawal_count,
    })
}

/// Parse Groth16Proof from unaligned bytes
fn parse_proof_unaligned(data: &[u8]) -> Result<Groth16Proof, ProgramError> {
    if data.len() < Groth16Proof::LEN {
        return Err(ProgramError::InvalidInstructionData);
    }

    let pi_a: [u8; 64] = data[0..64]
        .try_into()
        .map_err(|_| ProgramError::InvalidInstructionData)?;
    let pi_b: [u8; 128] = data[64..192]
        .try_into()
        .map_err(|_| ProgramError::InvalidInstructionData)?;
    let pi_c: [u8; 64] = data[192..256]
        .try_into()
        .map_err(|_| ProgramError::InvalidInstructionData)?;

    Ok(Groth16Proof { pi_a, pi_b, pi_c })
}

/// Parse BatchPublicInputs from unaligned bytes
fn parse_public_inputs_unaligned(data: &[u8]) -> Result<BatchPublicInputs, ProgramError> {
    if data.len() < BatchPublicInputs::LEN {
        return Err(ProgramError::InvalidInstructionData);
    }

    let pre_state_root: [u8; 32] = data[0..32]
        .try_into()
        .map_err(|_| ProgramError::InvalidInstructionData)?;
    let post_state_root: [u8; 32] = data[32..64]
        .try_into()
        .map_err(|_| ProgramError::InvalidInstructionData)?;
    let pre_shielded_root: [u8; 32] = data[64..96]
        .try_into()
        .map_err(|_| ProgramError::InvalidInstructionData)?;
    let post_shielded_root: [u8; 32] = data[96..128]
        .try_into()
        .map_err(|_| ProgramError::InvalidInstructionData)?;
    let withdrawal_root: [u8; 32] = data[128..160]
        .try_into()
        .map_err(|_| ProgramError::InvalidInstructionData)?;
    let batch_hash: [u8; 32] = data[160..192]
        .try_into()
        .map_err(|_| ProgramError::InvalidInstructionData)?;
    let batch_id = u64::from_le_bytes(
        data[192..200]
            .try_into()
            .map_err(|_| ProgramError::InvalidInstructionData)?,
    );

    Ok(BatchPublicInputs {
        pre_state_root,
        post_state_root,
        pre_shielded_root,
        post_shielded_root,
        withdrawal_root,
        batch_hash,
        batch_id,
    })
}

/// Parse WithdrawalRequest from unaligned bytes
fn parse_withdrawal_unaligned(data: &[u8]) -> Result<WithdrawalRequest, ProgramError> {
    if data.len() < core::mem::size_of::<WithdrawalRequest>() {
        return Err(ProgramError::InvalidInstructionData);
    }

    let recipient: Pubkey = data[0..32]
        .try_into()
        .map_err(|_| ProgramError::InvalidInstructionData)?;
    let amount = u64::from_le_bytes(
        data[32..40]
            .try_into()
            .map_err(|_| ProgramError::InvalidInstructionData)?,
    );

    Ok(WithdrawalRequest { recipient, amount })
}

/// Anchor instruction discriminator for verify_batch_proof
/// = sha256("global:verify_batch_proof")[0..8]
const VERIFY_BATCH_PROOF_DISCRIMINATOR: [u8; 8] = [0xca, 0xce, 0xf3, 0x17, 0x28, 0x3e, 0x42, 0x37];

/// Build instruction data for verify_batch_proof CPI
/// Format: discriminator (8) + proof (256) + public_inputs (200)
fn build_verify_instruction_data(proof: &Groth16Proof, inputs: &BatchPublicInputs) -> [u8; 464] {
    let mut data = [0u8; 464];

    // Discriminator
    data[0..8].copy_from_slice(&VERIFY_BATCH_PROOF_DISCRIMINATOR);

    // Groth16Proof: pi_a (64) + pi_b (128) + pi_c (64) = 256 bytes
    data[8..72].copy_from_slice(&proof.pi_a);
    data[72..200].copy_from_slice(&proof.pi_b);
    data[200..264].copy_from_slice(&proof.pi_c);

    // BatchPublicInputs: 6 * 32 + 8 = 200 bytes
    data[264..296].copy_from_slice(&inputs.pre_state_root);
    data[296..328].copy_from_slice(&inputs.post_state_root);
    data[328..360].copy_from_slice(&inputs.pre_shielded_root);
    data[360..392].copy_from_slice(&inputs.post_shielded_root);
    data[392..424].copy_from_slice(&inputs.withdrawal_root);
    data[424..456].copy_from_slice(&inputs.batch_hash);
    data[456..464].copy_from_slice(&inputs.batch_id.to_le_bytes());

    data
}

pub fn process_submit_batch(
    _program_id: &Pubkey,
    accounts: &[AccountInfo],
    ix_data: &[u8],
) -> ProgramResult {
    // Minimum accounts: sequencer, config, verifier_program, vk_account
    if accounts.len() < 4 {
        return Err(ProgramError::NotEnoughAccountKeys);
    }

    let sequencer = &accounts[0];
    let config_account = &accounts[1];
    let verifier_program = &accounts[2];
    let vk_account = &accounts[3];

    // Remaining accounts are withdrawal recipients
    let recipients_iter = &accounts[4..];

    check_signer(sequencer)?;
    let mut config_data = unsafe { config_account.borrow_mut_data_unchecked() };
    let config = unsafe { load_acc_mut::<Config>(&mut config_data)? };

    if !config.is_initialized() {
        return Err(ProgramError::UninitializedAccount);
    }

    if sequencer.key() != &config.sequencer_authority {
        return Err(ProgramError::IncorrectAuthority);
    }

    let domain = config.domain;

    // Parse instruction data (skip discriminator byte)
    if ix_data.len() < 1 + SubmitBatchHeader::LEN {
        return Err(ProgramError::InvalidInstructionData);
    }
    let ix_data = &ix_data[1..];

    // Parse header using unaligned parsing (bytemuck requires 8-byte alignment)
    let header = parse_header_unaligned(ix_data)?;

    log!(
        "header.prev={} config.idx={}",
        header.prev_batch_index,
        config.batch_index
    );

    // Validate batch sequence
    if header.prev_batch_index != config.batch_index {
        log!("Invalid prev_batch_index");
        return Err(ProgramError::InvalidInstructionData);
    }

    if header.new_batch_index != config.batch_index + 1 {
        log!("Invalid new_batch_index");
        return Err(ProgramError::InvalidInstructionData);
    }

    // Parse proof (after header)
    let mut offset = SubmitBatchHeader::LEN;

    // Expected proof_len should be Groth16Proof::LEN (256 bytes)
    if header.proof_len as usize != Groth16Proof::LEN {
        log!("Invalid proof length");
        return Err(ProgramError::InvalidInstructionData);
    }

    let proof_end = offset + Groth16Proof::LEN;
    if proof_end > ix_data.len() {
        return Err(ProgramError::InvalidInstructionData);
    }

    // Parse proof using unaligned parsing
    let proof = parse_proof_unaligned(&ix_data[offset..proof_end])?;
    offset = proof_end;

    // Parse BatchPublicInputs (after proof)
    let inputs_end = offset + BatchPublicInputs::LEN;
    if inputs_end > ix_data.len() {
        log!("Missing public inputs");
        return Err(ProgramError::InvalidInstructionData);
    }

    // Parse public inputs using unaligned parsing
    let public_inputs = parse_public_inputs_unaligned(&ix_data[offset..inputs_end])?;
    offset = inputs_end;

    // Validate public inputs match header
    if public_inputs.post_state_root != header.new_state_root {
        log!("Public inputs state root mismatch");
        return Err(ProgramError::InvalidInstructionData);
    }

    if public_inputs.batch_id != header.new_batch_index {
        log!("Public inputs batch_id mismatch");
        return Err(ProgramError::InvalidInstructionData);
    }

    // ==== CPI to Verifier Program ====
    // Build instruction data for verify_batch_proof
    let cpi_data = build_verify_instruction_data(&proof, &public_inputs);

    // Account metas for CPI:
    // 1. caller (signer) - the sequencer
    // 2. vk_account (read-only) - the stored verifying key
    let cpi_accounts = [
        AccountMeta::readonly_signer(sequencer.key()), // caller/signer
        AccountMeta::readonly(vk_account.key()),       // vk_account
    ];

    let cpi_instruction = Instruction {
        program_id: verifier_program.key(),
        accounts: &cpi_accounts,
        data: &cpi_data,
    };

    // Invoke the verifier - this will fail if proof is invalid
    invoke(&cpi_instruction, &[sequencer, vk_account])?;

    log!("ZK proof verified successfully");

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

        // Parse withdrawal using unaligned parsing
        let w = parse_withdrawal_unaligned(&ix_data[start..end])?;

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

    // Update config - only after successful verification
    // Commit new L2 state
    config.state_root = header.new_state_root;
    config.batch_index = header.new_batch_index;

    log!("ZE_BATCH_FINALIZED:{}:{}", &domain, config.batch_index);

    Ok(())
}
