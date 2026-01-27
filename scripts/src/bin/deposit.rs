//! Deposit SOL to Zelana L2
//!
//! This script deposits SOL from L1 (Solana) to L2 (Zelana) via the bridge.
//! The sequencer will index the deposit and credit the L2 account.
//!
//! Usage:
//!   cargo run --bin deposit -- --amount 1.0
//!   cargo run --bin deposit -- --amount 0.5 --nonce 123
//!
//! Arguments:
//!   --amount <SOL>   Amount to deposit in SOL (required)
//!   --nonce <u64>    Unique nonce for this deposit (default: random)
//!
//! Environment variables:
//!   PAYER_KEYPAIR - Path to keypair (default: ~/.config/solana/id.json)

use zelana_scripts::config::*;
use solana_client::rpc_client::RpcClient;
use solana_commitment_config::CommitmentConfig;
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    signature::Signer,
    system_program,
    transaction::Transaction,
};
use zelana_config::{SOLANA};


/// Bridge instruction discriminator for Deposit
const BRIDGE_IX_DEPOSIT: u8 = 1;

/// DepositParams structure matching the bridge program
#[repr(C, packed)]
#[derive(Clone, Copy)]
struct DepositParams {
    amount: u64,
    nonce: u64,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    print_header("Deposit to Zelana L2");

    // Parse arguments
    let args: Vec<String> = std::env::args().collect();

    let amount_sol: f64 = args
        .iter()
        .position(|a| a == "--amount")
        .and_then(|i| args.get(i + 1))
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| {
            println!("Usage: deposit --amount <SOL> [--nonce <u64>]");
            println!("  --amount 1.0     Deposit 1 SOL");
            println!("  --nonce 123      Use specific nonce (optional)");
            std::process::exit(1);
        });

    let nonce: u64 = args
        .iter()
        .position(|a| a == "--nonce")
        .and_then(|i| args.get(i + 1))
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| {
            // Generate random nonce
            use std::time::{SystemTime, UNIX_EPOCH};
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos() as u64
        });

    let amount_lamports = (amount_sol * 1_000_000_000.0) as u64;

    // Load configuration
    let payer_path = std::env::var("PAYER_KEYPAIR").unwrap_or_else(|_| default_payer_path());

    print_info(&format!("Bridge Program ID: {}", SOLANA.bridge_program));
    print_info(&format!("RPC URL: {}", SOLANA.rpc_url));
    print_info(&format!(
        "Amount: {} SOL ({} lamports)",
        amount_sol, amount_lamports
    ));
    print_info(&format!("Nonce: {}", nonce));

    // Load keypair
    let payer = load_keypair(&payer_path)?;
    println!("\nDepositor: {}", payer.pubkey());

    // Connect to RPC
    let rpc = RpcClient::new_with_commitment(SOLANA.rpc_url, CommitmentConfig::confirmed());

    // Check balance
    let balance = rpc.get_balance(&payer.pubkey())?;
    println!("Current balance: {} SOL", balance as f64 / 1_000_000_000.0);

    if balance < amount_lamports + 10_000_000 {
        print_error(&format!(
            "Insufficient balance. Need {} SOL + fees",
            amount_sol
        ));
        print_info("Run: solana airdrop 2 --url http://127.0.0.1:8899");
        return Err(anyhow::anyhow!("Insufficient balance"));
    }

    // Derive PDAs
    let (config_pda, _) = derive_config_pda();
    let (vault_pda, _) = derive_vault_pda();
    let (receipt_pda, _) = derive_receipt_pda(&payer.pubkey(), nonce);

    println!("\nConfig PDA: {}", config_pda);
    println!("Vault PDA: {}", vault_pda);
    println!("Receipt PDA: {}", receipt_pda);

    // Check if receipt already exists (prevent replay)
    match rpc.get_account_with_commitment(&receipt_pda, CommitmentConfig::confirmed()) {
        Ok(response) if response.value.is_some() => {
            print_error("Deposit with this nonce already exists!");
            print_info("Use a different --nonce value.");
            return Err(anyhow::anyhow!("Duplicate nonce"));
        }
        _ => {}
    }

    // Build deposit instruction
    let params = DepositParams {
        amount: amount_lamports,
        nonce,
    };

    let mut instruction_data = vec![BRIDGE_IX_DEPOSIT];
    // Safety: DepositParams is repr(C, packed) and contains only primitives
    instruction_data.extend_from_slice(unsafe {
        std::slice::from_raw_parts(
            &params as *const DepositParams as *const u8,
            std::mem::size_of::<DepositParams>(),
        )
    });

    let accounts = vec![
        AccountMeta::new(payer.pubkey(), true), // depositor (signer, payer)
        AccountMeta::new_readonly(config_pda, false), // config
        AccountMeta::new(vault_pda, false),     // vault (receives SOL)
        AccountMeta::new(receipt_pda, false),   // receipt (created)
        AccountMeta::new_readonly(system_program::ID, false), // system_program
    ];

    let deposit_ix = Instruction {
        program_id: bridge_program_id(),
        accounts,
        data: instruction_data,
    };

    // Build and send transaction
    print_waiting("Sending deposit transaction...");

    let recent_blockhash = rpc.get_latest_blockhash()?;
    let tx = Transaction::new_signed_with_payer(
        &[deposit_ix],
        Some(&payer.pubkey()),
        &[&payer],
        recent_blockhash,
    );

    let sig = rpc.send_and_confirm_transaction(&tx)?;

    print_success("Deposit successful!");
    println!("Transaction signature: {}", sig);

    // Show new balance
    let new_balance = rpc.get_balance(&payer.pubkey())?;
    println!(
        "\nNew L1 balance: {} SOL",
        new_balance as f64 / 1_000_000_000.0
    );

    println!("\n📋 Deposit Summary:");
    println!("  Amount: {} SOL", amount_sol);
    println!("  Nonce: {}", nonce);
    println!("  Receipt PDA: {}", receipt_pda);
    println!("  L2 Account ID: {}", payer.pubkey());

    print_info("The sequencer will index this deposit and credit your L2 account.");
    print_info("Use 'check_balance' to verify your L2 balance after a few seconds.");

    Ok(())
}
