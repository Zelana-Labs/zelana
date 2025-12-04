use bytemuck::{Pod, Zeroable};
use crate::helpers::{Initialized, StateDefinition};

#[derive(Pod, Zeroable, Debug, Clone, Copy, PartialEq,shank::ShankAccount)]
#[repr(C)]
pub struct Vault {
    pub bump: u8,
    pub _padding: [u8; 7], 
}

impl StateDefinition for Vault {
    const LEN: usize = core::mem::size_of::<Vault>();
    const SEED: &'static str = "vault";
}
impl Initialized for Vault {
    /// An account is initialized if its bump seed is non-zero.
    /// The PDA derivation guarantees a non-zero bump on successful creation.
    fn is_initialized(&self) -> bool {
        self.bump != 0
    }
}

impl Vault {
    /// Initializes a new Vault state.
    pub fn new(&mut self, bump: u8) {
        self.bump = bump;
        self._padding = [0; 7];
    }
}
