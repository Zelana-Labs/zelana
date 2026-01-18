use pinocchio::{
    account_info::AccountInfo,
    program_error::ProgramError,
    pubkey::{Pubkey, find_program_address},
};

use crate::helpers::StateDefinition;

pub trait DataLen {
    const LEN: usize;
}

pub trait Initialized {
    fn is_initialized(&self) -> bool;
}

#[inline(always)]
pub unsafe fn load_acc<T: StateDefinition + Initialized>(bytes: &[u8]) -> Result<&T, ProgramError> {
    unsafe {
        load_acc_unchecked::<T>(bytes).and_then(|acc| {
            if acc.is_initialized() {
                Ok(acc)
            } else {
                Err(ProgramError::UninitializedAccount)
            }
        })
    }
}

#[inline(always)]
pub unsafe fn load_acc_unchecked<T: StateDefinition>(bytes: &[u8]) -> Result<&T, ProgramError> {
    if bytes.len() != T::LEN {
        return Err(ProgramError::InvalidAccountData);
    }
    unsafe { Ok(&*(bytes.as_ptr() as *const T)) }
}

#[inline(always)]
pub unsafe fn load_acc_mut<T: StateDefinition + Initialized>(
    bytes: &mut [u8],
) -> Result<&mut T, ProgramError> {
    unsafe {
        load_acc_mut_unchecked::<T>(bytes).and_then(|acc| {
            if acc.is_initialized() {
                Ok(acc)
            } else {
                Err(ProgramError::UninitializedAccount)
            }
        })
    }
}

#[inline(always)]
pub unsafe fn load_acc_mut_unchecked<T: StateDefinition>(
    bytes: &mut [u8],
) -> Result<&mut T, ProgramError> {
    if bytes.len() != T::LEN {
        return Err(ProgramError::InvalidAccountData);
    }
    unsafe { Ok(&mut *(bytes.as_mut_ptr() as *mut T)) }
}

#[inline(always)]
pub unsafe fn load_ix_data<T: DataLen>(bytes: &[u8]) -> Result<&T, ProgramError> {
    if bytes.len() != T::LEN {
        return Err(ProgramError::InvalidInstructionData.into());
    }
    unsafe { Ok(&*(bytes.as_ptr() as *const T)) }
}

pub unsafe fn to_bytes<T: DataLen>(data: &T) -> &[u8] {
    unsafe { core::slice::from_raw_parts(data as *const T as *const u8, T::LEN) }
}

pub unsafe fn to_mut_bytes<T: DataLen>(data: &mut T) -> &mut [u8] {
    unsafe { core::slice::from_raw_parts_mut(data as *mut T as *mut u8, T::LEN) }
}

pub unsafe fn try_from_account_info<T: DataLen>(acc: &AccountInfo) -> Result<&T, ProgramError> {
    if acc.owner() != &crate::ID {
        return Err(ProgramError::IllegalOwner);
    }
    let bytes = acc.try_borrow_data()?;

    if bytes.len() != T::LEN {
        return Err(ProgramError::InvalidAccountData);
    }
    unsafe { Ok(&*(bytes.as_ptr() as *const T)) }
}

pub unsafe fn try_from_account_info_mut<T: DataLen>(
    acc: &AccountInfo,
) -> Result<&mut T, ProgramError> {
    if acc.owner() != &crate::ID {
        return Err(ProgramError::IllegalOwner);
    }

    let mut bytes = acc.try_borrow_mut_data()?;

    if bytes.len() != T::LEN {
        return Err(ProgramError::InvalidAccountData);
    }

    unsafe { Ok(&mut *(bytes.as_mut_ptr() as *mut T)) }
}

#[inline(always)]
pub fn derive_config_pda(program_id: &Pubkey, domain: &[u8; 32]) -> (Pubkey, u8) {
    find_program_address(&[b"config", domain.as_ref()], program_id)
}

#[inline(always)]
pub fn derive_deposit_receipt_pda(
    program_id: &Pubkey,
    domain: &[u8; 32],
    depositor: &Pubkey,
    nonce: u64,
) -> (Pubkey, u8) {
    find_program_address(
        &[
            b"receipt",
            domain.as_ref(),
            depositor.as_ref(),
            &nonce.to_le_bytes(),
        ],
        program_id,
    )
}

#[inline(always)]
pub fn derive_vault_pda(program_id: &Pubkey, domain: &[u8; 32]) -> (Pubkey, u8) {
    find_program_address(&[b"vault", domain.as_ref()], program_id)
}

#[inline(always)]
pub fn derive_nullifier_pda(
    program_id: &Pubkey,
    domain: &[u8; 32],
    nullifier: &[u8; 32],
) -> (Pubkey, u8) {
    find_program_address(&[b"nullifier", domain.as_ref(), nullifier], program_id)
}
