use bytemuck::{Pod,Zeroable};
use pinocchio::{account_info::AccountInfo, program_error::ProgramError, pubkey::Pubkey};

use crate::helpers::{Initialized, StateDefinition};

#[derive(Pod, Zeroable, Debug, Clone, Copy, PartialEq,shank::ShankAccount)]
#[repr(C)]
pub struct DepositReceipt{
    pub depositor: Pubkey,
    pub amount: u64,
    pub nonce: u64,
    pub ts: i64,
    pub bump: u8,
    pub _padding: [u8; 7],
}


impl StateDefinition for DepositReceipt {
    const LEN: usize = core::mem::size_of::<DepositReceipt>();
    const SEED: &'static str = "receipt";
}

impl Initialized for DepositReceipt {
    /// A receipt is initialized if the depositor key is not the default Pubkey.
    fn is_initialized(&self) -> bool {
        self.depositor != Pubkey::default()
    }
}

impl DepositReceipt {
    /// Initializes a new DepositReceipt state.
    pub fn new(&mut self, depositor: Pubkey, amount: u64, nonce: u64, timestamp: i64, bump: u8) {
        self.depositor = depositor;
        self.amount = amount;
        self.nonce = nonce;
        self.ts = timestamp;
        self.bump = bump;
        self._padding = [0; 7];
    }
}