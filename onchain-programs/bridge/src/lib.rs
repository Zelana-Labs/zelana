#![no_std]

#[cfg(not(feature = "no-entrypoint"))]
mod entrypoint;

#[cfg(feature = "std")]
extern crate std;

pub mod helpers;
pub mod instruction;
pub mod state;

pinocchio_pubkey::declare_id!("8SE6gCijcFQixvDQqWu29mCm9AydN8hcwWh2e2Q6RQgE");
