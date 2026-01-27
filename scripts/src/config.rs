//! Shared configuration and utilities for Zelana test scripts
//!
//! Contains program IDs, RPC URLs, and helper functions used across all test scripts.

use solana_sdk::pubkey::Pubkey;
use std::{str::FromStr, sync::OnceLock};
use zelana_config::{ZelanaConfig};

static CONFIG: OnceLock<ZelanaConfig> = OnceLock::new();

/// Solana RPC URL (Surfpool default)
pub fn rpc_url() -> &'static str {
    &get_config().solana.rpc_url
}

/// Sequencer HTTP API URL
pub fn sequencer_url() -> &'static str {
    &get_config().api.sequencer
}

/// Default domain for the L2 (padded to 32 bytes)
pub const DOMAIN: &[u8] = b"zelana";

pub fn get_config() -> &'static ZelanaConfig {
    CONFIG.get_or_init(|| ZelanaConfig::load().expect("Failed to load ZelanaConfig"))
}

/// Get the bridge program ID as Pubkey
pub fn bridge_program_id() -> Pubkey {
    Pubkey::from_str(&get_config().solana.bridge_program_id).expect("Invalid bridge program ID")
}

/// Get the verifier program ID as Pubkey
pub fn verifier_program_id() -> Pubkey {
    Pubkey::from_str(&get_config().solana.verifier_program_id).expect("Invalid verifier program ID")
}

/// Get the domain as a 32-byte array
pub fn domain_bytes() -> [u8; 32] {
    let mut domain = [0u8; 32];
    domain[..DOMAIN.len()].copy_from_slice(DOMAIN);
    domain
}

/// Derive the config PDA for the bridge
pub fn derive_config_pda() -> (Pubkey, u8) {
    let program_id = bridge_program_id();
    let domain = domain_bytes();
    Pubkey::find_program_address(&[b"config", &domain], &program_id)
}

/// Derive the vault PDA for the bridge
pub fn derive_vault_pda() -> (Pubkey, u8) {
    let program_id = bridge_program_id();
    let domain = domain_bytes();
    Pubkey::find_program_address(&[b"vault", &domain], &program_id)
}

/// Derive the batch VK PDA in the verifier program
pub fn derive_batch_vk_pda() -> (Pubkey, u8) {
    let verifier_id = verifier_program_id();
    let domain = domain_bytes();
    Pubkey::find_program_address(&[b"batch_vk", &domain], &verifier_id)
}

/// Derive the deposit receipt PDA
pub fn derive_receipt_pda(depositor: &Pubkey, nonce: u64) -> (Pubkey, u8) {
    let program_id = bridge_program_id();
    let domain = domain_bytes();
    Pubkey::find_program_address(
        &[
            b"receipt",
            &domain,
            depositor.as_ref(),
            &nonce.to_le_bytes(),
        ],
        &program_id,
    )
}

/// Load keypair from file path
pub fn load_keypair(path: &str) -> anyhow::Result<solana_sdk::signature::Keypair> {
    let data = std::fs::read_to_string(path)?;
    let bytes: Vec<u8> = serde_json::from_str(&data)?;
    Ok(solana_sdk::signature::Keypair::from_bytes(&bytes)?)
}

/// Get the default payer keypair path
pub fn default_payer_path() -> String {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    format!("{}/.config/solana/id.json", home)
}

/// Print a section header
pub fn print_header(title: &str) {
    println!("\n{}", "=".repeat(60));
    println!("  {}", title);
    println!("{}\n", "=".repeat(60));
}

/// Print success message
pub fn print_success(msg: &str) {
    println!("✅ {}", msg);
}

/// Print error message
pub fn print_error(msg: &str) {
    println!("❌ {}", msg);
}

/// Print info message
pub fn print_info(msg: &str) {
    println!("ℹ️  {}", msg);
}

/// Print waiting message
pub fn print_waiting(msg: &str) {
    println!("⏳ {}", msg);
}
