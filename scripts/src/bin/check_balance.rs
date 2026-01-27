//! Check L2 Balance
//!
//! This script queries the sequencer API to check an account's L2 balance.
//!
//! Usage:
//!   cargo run --bin check_balance
//!   cargo run --bin check_balance -- --account <PUBKEY>
//!
//! Arguments:
//!   --account <PUBKEY>  Account to check (default: payer's pubkey)
//!
//! Environment variables:
//!   PAYER_KEYPAIR  - Path to keypair (default: ~/.config/solana/id.json)
//!   SEQUENCER_URL  - Sequencer API URL (default: http://127.0.0.1:8080)

use solana_sdk::{pubkey::Pubkey, signature::Signer};
use std::str::FromStr;
use zelana_config::API;
use zelana_scripts::config::*;

#[derive(serde::Deserialize, Debug)]
struct AccountState {
    balance: u64,
    nonce: u64,
}

#[derive(serde::Deserialize, Debug)]
struct BatchInfo {
    batch_index: u64,
    state_root: String,
    tx_count: u64,
}

#[derive(serde::Deserialize, Debug)]
struct SequencerStatus {
    status: String,
    batch_index: Option<u64>,
    pending_txs: Option<u64>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    print_header("Check Zelana L2 Balance");

    // Parse arguments
    let args: Vec<String> = std::env::args().collect();

    let account_str = args
        .iter()
        .position(|a| a == "--account")
        .and_then(|i| args.get(i + 1))
        .map(|s| s.as_str());

    let sequencer_url = API.sequencer_url;

    print_info(&format!("Sequencer URL: {}", sequencer_url));

    // Determine account to check
    let account_pubkey = if let Some(acc) = account_str {
        Pubkey::from_str(acc)?
    } else {
        let payer_path = std::env::var("PAYER_KEYPAIR").unwrap_or_else(|_| default_payer_path());
        let payer = load_keypair(&payer_path)?;
        payer.pubkey()
    };

    println!("Account: {}", account_pubkey);

    // Create HTTP client
    let client = reqwest::Client::new();

    // Check sequencer status
    print_waiting("Checking sequencer status...");

    match client.get(format!("{}/health", sequencer_url)).send().await {
        Ok(resp) => {
            if resp.status().is_success() {
                if let Ok(status) = resp.json::<SequencerStatus>().await {
                    print_success("Sequencer is running");
                    println!("  Status: {}", status.status);
                    if let Some(idx) = status.batch_index {
                        println!("  Current batch: {}", idx);
                    }
                    if let Some(pending) = status.pending_txs {
                        println!("  Pending txs: {}", pending);
                    }
                }
            } else {
                print_info(&format!("Sequencer returned: {}", resp.status()));
            }
        }
        Err(e) => {
            print_error(&format!("Cannot connect to sequencer: {}", e));
            print_info("Make sure the sequencer is running:");
            print_info("  cargo run --package core");
            return Err(anyhow::anyhow!("Sequencer not available"));
        }
    }

    // Get account balance
    print_waiting("Fetching account state...");

    let account_id = hex::encode(account_pubkey.to_bytes());
    let balance_url = format!("{}/account/{}", sequencer_url, account_id);

    match client.get(&balance_url).send().await {
        Ok(resp) => {
            if resp.status().is_success() {
                if let Ok(state) = resp.json::<AccountState>().await {
                    println!("\n📊 Account State:");
                    println!(
                        "  Balance: {} lamports ({} SOL)",
                        state.balance,
                        state.balance as f64 / 1_000_000_000.0
                    );
                    println!("  Nonce: {}", state.nonce);
                } else {
                    print_info("Account not found or empty response");
                }
            } else if resp.status().as_u16() == 404 {
                println!("\n📊 Account State:");
                println!("  Balance: 0 lamports (0 SOL)");
                println!("  Nonce: 0");
                print_info("Account not yet registered. Deposit to create.");
            } else {
                print_error(&format!("Failed to get balance: {}", resp.status()));
            }
        }
        Err(e) => {
            print_error(&format!("Request failed: {}", e));
        }
    }

    // Get latest batch info
    print_waiting("Fetching latest batch info...");

    match client
        .get(format!("{}/batch/latest", sequencer_url))
        .send()
        .await
    {
        Ok(resp) => {
            if resp.status().is_success() {
                if let Ok(batch) = resp.json::<BatchInfo>().await {
                    println!("\n📦 Latest Batch:");
                    println!("  Batch Index: {}", batch.batch_index);
                    println!("  State Root: {}", batch.state_root);
                    println!("  TX Count: {}", batch.tx_count);
                }
            }
        }
        Err(_) => {
            // Batch endpoint might not exist yet
        }
    }

    Ok(())
}
