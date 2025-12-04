use bytemuck::{Pod,Zeroable};
use pinocchio::pubkey::Pubkey;

use crate::helpers::{Initialized, StateDefinition};

#[derive(Pod, Zeroable, Debug, Clone, Copy, PartialEq,shank::ShankAccount)]
#[repr(C)]
pub struct UsedNullifier{
    pub nullifier:[u8;32],
    pub used:u8,
    pub bump:u8,
    pub _padding: [u8; 6],
}

impl StateDefinition for UsedNullifier {
    const LEN: usize = core::mem::size_of::<UsedNullifier>();
    const SEED: &'static str = "nullifier";
}

impl Initialized for UsedNullifier {
    /// A nullifier is considered used (and thus initialized) if the `used` flag is 1.
    fn is_initialized(&self) -> bool {
        self.used == 1
    }
}

impl UsedNullifier {
    /// Initializes a new UsedNullifier state.
    pub fn new(&mut self, nullifier: [u8; 32], bump: u8) {
        self.nullifier = nullifier;
        self.used = 1; // Mark as used immediately upon creation
        self.bump = bump;
        self._padding = [0; 6];
    }
}
