use bytemuck::{Pod, Zeroable};
use pinocchio::pubkey::Pubkey;

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
    pub _padding: [u8; 7],
}

impl StateDefinition for Config {
    const LEN: usize = core::mem::size_of::<Config>();
    const SEED: &'static str = "config";
}

impl Initialized for Config {
    fn is_initialized(&self) -> bool {
        self.sequencer_authority != Pubkey::default()
    }
}

impl Config {
    pub fn new(&mut self, sequencer_authority: Pubkey, domain: [u8; 32], bump: u8) {
        self.sequencer_authority = sequencer_authority;
        self.domain = domain;
        self.state_root = [0u8; 32];
        self.batch_index = 0;
        self.bump = bump;
        self._padding = [0; 7];
    }
}