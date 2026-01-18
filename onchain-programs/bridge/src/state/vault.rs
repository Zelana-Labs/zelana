use crate::helpers::{Initialized, StateDefinition};
use bytemuck::{Pod, Zeroable};

#[derive(Pod, Zeroable, Debug, Clone, Copy, PartialEq, shank::ShankAccount)]
#[repr(C)]
pub struct Vault {
    pub domain: [u8; 32],
    pub bump: u8,
    pub _padding: [u8; 7],
}

impl Initialized for Vault {
    /// An account is initialized if its bump seed is non-zero.
    /// The PDA derivation guarantees a non-zero bump on successful creation.
    fn is_initialized(&self) -> bool {
        self.bump != 0
    }
}

impl StateDefinition for Vault {
    const LEN: usize = core::mem::size_of::<Vault>();
}

impl Vault {
    pub fn new(&mut self, domain: [u8; 32], bump: u8) {
        self.domain = domain;
        self.bump = bump;
        self._padding = [0; 7];
    }
}
