//! Initialize the Zelana Bridge
//!
//! This script initializes the bridge program with a sequencer authority.
//! Must be run once after deploying the bridge program.
//!
//! Usage:
//!   cargo run --bin init_bridge
//!
//! Environment variables:
//!   SEQUENCER_KEYPAIR - Path to sequencer keypair (default: ~/.config/solana/id.json)
//!   PAYER_KEYPAIR     - Path to payer keypair (default: ~/.config/solana/id.json)

use zelana_scripts::config::*;
use solana_client::rpc_client::RpcClient;
use solana_commitment_config::CommitmentConfig;
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    signature::Signer,
    system_program,
    transaction::Transaction,
};
use zelana_config::SOLANA;

/// Bridge instruction discriminator for Init
const BRIDGE_IX_INIT: u8 = 0;

/// InitParams structure matching the bridge program
#[repr(C, packed)]
#[derive(Clone, Copy)]
struct InitParams {
    sequencer_authority: [u8; 32],
    domain: [u8; 32],
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    print_header("Zelana Bridge Initialization");

    // Load configuration
    let payer_path = std::env::var("PAYER_KEYPAIR").unwrap_or_else(|_| default_payer_path());
    let sequencer_path =
        std::env::var("SEQUENCER_KEYPAIR").unwrap_or_else(|_| default_payer_path());

    print_info(&format!("Bridge Program ID: {}", SOLANA.bridge_program));
    print_info(&format!("Verifier Program ID: {}", SOLANA.verifier_program));
    print_info(&format!("RPC URL: {}", SOLANA.rpc_url));
    print_info(&format!("Payer keypair: {}", payer_path));
    print_info(&format!("Sequencer keypair: {}", sequencer_path));

    // Load keypairs
    let payer = load_keypair(&payer_path)?;
    let sequencer = load_keypair(&sequencer_path)?;

    println!("\nPayer: {}", payer.pubkey());
    println!("Sequencer Authority: {}", sequencer.pubkey());

    // Connect to RPC
    let rpc = RpcClient::new_with_commitment(SOLANA.rpc_url, CommitmentConfig::confirmed());

    // Check payer balance
    let balance = rpc.get_balance(&payer.pubkey())?;
    println!("Payer balance: {} SOL", balance as f64 / 1_000_000_000.0);

    if balance < 10_000_000 {
        print_error("Insufficient balance. Need at least 0.01 SOL for rent.");
        print_info("Run: solana airdrop 2 --url http://127.0.0.1:8899");
        return Err(anyhow::anyhow!("Insufficient balance"));
    }

    // Derive PDAs
    let (config_pda, _config_bump) = derive_config_pda();
    let (vault_pda, _vault_bump) = derive_vault_pda();

    println!("\nConfig PDA: {}", config_pda);
    println!("Vault PDA: {}", vault_pda);

    // Check if already initialized
    match rpc.get_account_with_commitment(&config_pda, CommitmentConfig::confirmed()) {
        Ok(response) if response.value.is_some() => {
            print_info("Bridge already initialized. Config account exists.");
            return Ok(());
        }
        _ => {}
    }

    // Build init instruction
    let domain = domain_bytes();
    let params = InitParams {
        sequencer_authority: sequencer.pubkey().to_bytes(),
        domain,
    };

    let mut instruction_data = vec![BRIDGE_IX_INIT];
    // Safety: InitParams is repr(C, packed) and contains only byte arrays
    instruction_data.extend_from_slice(unsafe {
        std::slice::from_raw_parts(
            &params as *const InitParams as *const u8,
            std::mem::size_of::<InitParams>(),
        )
    });

    let accounts = vec![
        AccountMeta::new(payer.pubkey(), true),
        AccountMeta::new(config_pda, false),
        AccountMeta::new(vault_pda, false),
        AccountMeta::new_readonly(system_program::ID, false),
    ];

    let init_ix = Instruction {
        program_id: bridge_program_id(),
        accounts,
        data: instruction_data,
    };

    // Build and send transaction
    print_waiting("Sending initialization transaction...");

    let recent_blockhash = rpc.get_latest_blockhash()?;
    let tx = Transaction::new_signed_with_payer(
        &[init_ix],
        Some(&payer.pubkey()),
        &[&payer],
        recent_blockhash,
    );

    let sig = rpc.send_and_confirm_transaction(&tx)?;

    print_success(&format!("Bridge initialized successfully!"));
    println!("Transaction signature: {}", sig);
    println!("\nBridge Configuration:");
    println!("  Config PDA: {}", config_pda);
    println!("  Vault PDA: {}", vault_pda);
    println!("  Sequencer: {}", sequencer.pubkey());
    println!("  Domain: {}", hex::encode(&domain[..6]));

    Ok(())
}
