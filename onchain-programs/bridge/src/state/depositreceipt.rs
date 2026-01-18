use bytemuck::{Pod, Zeroable};
use pinocchio::{program_error::ProgramError, pubkey::Pubkey};

use crate::helpers::{Initialized, StateDefinition};

#[derive(Pod, Zeroable, Debug, Clone, Copy, PartialEq, shank::ShankAccount)]
#[repr(C)]
pub struct DepositReceipt {
    pub depositor: Pubkey,
    domain: Pubkey,
    pub amount: u64,
    pub nonce: u64,
    pub ts: i64,
    pub bump: u8,
    pub is_initialized: u8,
    pub _padding: [u8; 6],
}

impl StateDefinition for DepositReceipt {
    const LEN: usize = core::mem::size_of::<DepositReceipt>();
}

impl Initialized for DepositReceipt {
    /// A receipt is initialized if the depositor key is not the default Pubkey.
    fn is_initialized(&self) -> bool {
        self.is_initialized == 1
    }
}

impl DepositReceipt {
    pub fn new(
        &mut self,
        depositor: Pubkey,
        domain: [u8; 32],
        amount: u64,
        nonce: u64,
        timestamp: i64,
        bump: u8,
    ) -> Result<(), ProgramError> {
        // Defensive checks — NEVER trust instruction input
        if depositor == Pubkey::default() {
            return Err(ProgramError::InvalidArgument);
        }
        if domain == [0u8; 32] {
            return Err(ProgramError::InvalidArgument);
        }
        if amount == 0 {
            return Err(ProgramError::InvalidArgument);
        }

        self.depositor = depositor;
        self.domain = domain;
        self.amount = amount;
        self.nonce = nonce;
        self.ts = timestamp;
        self.bump = bump;
        self.is_initialized = 1;
        self._padding = [0; 6];

        Ok(())
    }
}
