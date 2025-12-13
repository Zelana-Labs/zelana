#![no_std]

#[cfg(not(feature = "no-entrypoint"))]
mod entrypoint;

#[cfg(feature = "std")]
extern crate std;

pub mod helpers;
pub mod instruction;
pub mod state;

pinocchio_pubkey::declare_id!("95sWqtU9fdm19cvQYu94iKijRuYAv3wLqod1pcsSfYth");
