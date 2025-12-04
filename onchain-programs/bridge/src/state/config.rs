use bytemuck::{Pod,Zeroable};
use pinocchio::pubkey::Pubkey;

use crate::helpers::{StateDefinition,Initialized};

#[derive(Pod, Zeroable, Debug, Clone, Copy, PartialEq,shank::ShankAccount)]
#[repr(C)]
pub struct Config{
    pub sequencer_authority: Pubkey,
    pub domain: [u8;32],
    pub bump: u8,
    _padding: [u8; 7],
}


impl StateDefinition for Config{
    const LEN: usize = core::mem::size_of::<Config>();
    const SEED: &'static str = "config";
}

impl Initialized for Config{
    fn is_initialized(&self) -> bool {
        self.sequencer_authority != Pubkey::default()
    }
}

impl Config{
    pub fn new(&mut self, sequencer_authority: Pubkey, domain: [u8; 32], bump: u8) {
        self.sequencer_authority = sequencer_authority;
        self.domain = domain;
        self.bump = bump;
        self._padding = [0; 7];
    }
}