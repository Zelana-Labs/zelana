use pinocchio::{program_error::ProgramError, pubkey::Pubkey};
use bytemuck::{Pod,Zeroable};
use shank::*;

use crate::helpers::DataLen;

pub mod deposit;
pub mod init;
pub mod withdraw;

#[repr(u8)]
pub enum BridgeIx{
    INIT = 0,
    DEPOSIT = 1,
    WITHDRAWATTESTED= 2
}

impl TryFrom<&u8> for BridgeIx  {
    type Error = ProgramError;
     fn try_from(value: &u8) -> Result<Self, Self::Error> {
        match *value {
            0 => Ok(BridgeIx::INIT),
            1=>Ok(BridgeIx::DEPOSIT),
            2=>Ok(BridgeIx::WITHDRAWATTESTED),
            _=>Err(ProgramError::InvalidInstructionData)
        }
    }
}
#[derive(Pod,Zeroable,Clone, Copy, Debug, PartialEq, shank::ShankType)]
#[repr(C)]
pub struct InitParams{
    pub sequencer_authority:Pubkey,
    pub domain:[u8;32]
}

impl DataLen for InitParams {
    const LEN: usize = core::mem::size_of::<InitParams>();
}

#[derive(Pod,Zeroable,Clone, Copy,shank::ShankType)]
#[repr(C)]
pub struct DepositParams{
    pub amount:u64,
    pub nonce:u64
}

impl DataLen for DepositParams{
    const LEN: usize = core::mem::size_of::<DepositParams>();
}


#[derive(Pod, Zeroable, Clone, Copy,shank::ShankType)]
#[repr(C)]
pub struct WithdrawAttestedParams {
    pub recipient: Pubkey,
    pub amount: u64,
    pub nullifier: [u8; 32],
}

impl DataLen for WithdrawAttestedParams{
    const LEN: usize = core::mem::size_of::<WithdrawAttestedParams>();
}


mod idl_gen{
    use super::{DepositParams,InitParams,WithdrawAttestedParams};
    #[derive(shank::ShankInstruction)]
    #[rustfmt::skip]
    enum _BridgeInstruction{
        #[account(0, writable, signer, name="payer", desc="Fee payer")]
        #[account(1, writable, name="config", desc="The Config PDA to be created. Seeds: ['config']")]
        #[account(2, writable, name="vault", desc="The Vault PDA to be created. Seeds: ['vault', config_pda]")]
        #[account(3, name="system_program", desc="System Program")]
        Initialize(InitParams),

        #[account(0, writable, signer, name="depositor", desc="The user depositing SOL")]
        #[account(1, name="config", desc="The bridge's config account")]
        #[account(2, writable, name="vault", desc="The bridge's vault account")]
        #[account(3, writable, name="deposit_receipt", desc="The unique PDA receipt for this deposit")]
        #[account(4, name="system_program", desc="System Program")]
        Deposit(DepositParams),

        #[account(0, signer, name="sequencer", desc="The authorized sequencer signing the withdrawal")]
        #[account(1, name="config", desc="The bridge's config account")]
        #[account(2, writable, name="vault", desc="The bridge's vault account")]
        #[account(3, writable, name="recipient", desc="The account receiving the withdrawn SOL")]
        #[account(4, writable, name="used_nullifier", desc="The nullifier PDA to prevent replay attacks")]
        #[account(5, name="system_program", desc="System Program")]
        WithdrawAttested(WithdrawAttestedParams),
    }
}