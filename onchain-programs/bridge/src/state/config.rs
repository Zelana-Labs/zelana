use bytemuck::{Pod, Zeroable};
use pinocchio::{program_error::ProgramError, pubkey::Pubkey};

use crate::helpers::{Initialized, StateDefinition};

#[derive(Pod, Zeroable, Debug, Clone, Copy, PartialEq, shank::ShankAccount)]
#[repr(C)]
pub struct Config {
    pub sequencer_authority: Pubkey,
    pub domain: [u8; 32],
    /// The Merkle Root of the current L2 State (starts at 0 or genesis root)
    pub state_root: [u8; 32],
    /// The index of the last processed batch
    pub batch_index: u64,
    pub bump: u8,
    pub is_initialized: u8,
    pub _padding: [u8; 6],
}

impl StateDefinition for Config {
    const LEN: usize = core::mem::size_of::<Config>();
}

impl Initialized for Config {
    fn is_initialized(&self) -> bool {
        self.is_initialized == 1
    }
}

impl Config {
    pub fn new(
        &mut self,
        sequencer_authority: Pubkey,
        domain: [u8; 32],
        bump: u8,
    ) -> Result<(), ProgramError> {
        if sequencer_authority == Pubkey::default() {
            return Err(ProgramError::InvalidArgument);
        }
        if domain == [0u8; 32] {
            return Err(ProgramError::InvalidArgument);
        }

        self.sequencer_authority = sequencer_authority;
        self.domain = domain;
        self.state_root = [0u8; 32];
        self.batch_index = 0;
        self.bump = bump;
        self.is_initialized = 1;
        self._padding = [0; 6];

        Ok(())
    }
}
