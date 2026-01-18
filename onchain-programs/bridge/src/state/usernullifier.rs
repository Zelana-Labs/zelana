use bytemuck::{Pod, Zeroable};
use pinocchio::{program_error::ProgramError, pubkey::Pubkey};

use crate::helpers::{Initialized, StateDefinition};

#[derive(Pod, Zeroable, Debug, Clone, Copy, PartialEq, shank::ShankAccount)]
#[repr(C)]
pub struct UsedNullifier {
    pub domain: [u8; 32],
    pub nullifier: [u8; 32],
    pub recipient: Pubkey,
    pub amount: u64,
    pub used: u8,
    pub bump: u8,
    pub _padding: [u8; 6],
}
impl StateDefinition for UsedNullifier {
    const LEN: usize = core::mem::size_of::<UsedNullifier>();
}

impl Initialized for UsedNullifier {
    /// A nullifier is considered used (and thus initialized) if the `used` flag is 1.
    fn is_initialized(&self) -> bool {
        self.used == 1
    }
}

impl UsedNullifier {
    /// Initializes a new UsedNullifier state.
    pub fn new(
        &mut self,
        domain: [u8; 32],
        nullifier: [u8; 32],
        recipient: Pubkey,
        amount: u64,
        bump: u8,
    ) -> Result<(), ProgramError> {
        if domain == [0u8; 32] {
            return Err(ProgramError::InvalidArgument);
        }
        if nullifier == [0u8; 32] {
            return Err(ProgramError::InvalidArgument);
        }

        self.domain = domain;
        self.nullifier = nullifier;
        self.recipient = recipient;
        self.amount = amount;
        self.used = 1;
        self.bump = bump;
        self._padding = [0; 6];

        Ok(())
    }
}
