//! Send L2 Transfer
//!
//! This script sends a transfer transaction on Zelana L2.
//! The transaction is submitted to the sequencer via HTTP API.
//!
//! Usage:
//!   cargo run --bin transfer -- --to <PUBKEY> --amount 0.1
//!   cargo run --bin transfer -- --to <PUBKEY> --amount 100 --lamports
//!
//! Arguments:
//!   --to <PUBKEY>    Recipient's public key (required)
//!   --amount <NUM>   Amount to send (required)
//!   --lamports       Interpret amount as lamports instead of SOL
//!
//! Environment variables:
//!   PAYER_KEYPAIR  - Path to keypair (default: ~/.config/solana/id.json)
//!   SEQUENCER_URL  - Sequencer API URL (default: http://127.0.0.1:8080)

use zelana_scripts::config::*;
use solana_sdk::{pubkey::Pubkey, signature::Signer};
use std::str::FromStr;

/// Transaction data for L2 transfer
#[derive(serde::Serialize)]
struct TransactionData {
    from: String,
    to: String,
    amount: u64,
    nonce: u64,
    chain_id: u64,
}

/// Signed transaction for submission
#[derive(serde::Serialize)]
struct SignedTransaction {
    data: TransactionData,
    signature: String,
    signer_pubkey: String,
}

/// Response from submit_tx endpoint
#[derive(serde::Deserialize)]
struct SubmitResponse {
    success: bool,
    tx_hash: Option<String>,
    error: Option<String>,
}

/// Account state from sequencer
#[derive(serde::Deserialize)]
struct AccountState {
    balance: u64,
    nonce: u64,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    print_header("Send L2 Transfer");

    // Parse arguments
    let args: Vec<String> = std::env::args().collect();

    let to_str = args
        .iter()
        .position(|a| a == "--to")
        .and_then(|i| args.get(i + 1))
        .unwrap_or_else(|| {
            println!("Usage: transfer --to <PUBKEY> --amount <NUM> [--lamports]");
            println!("  --to <PUBKEY>    Recipient's public key");
            println!("  --amount 0.1     Amount in SOL (or lamports with --lamports)");
            println!("  --lamports       Interpret amount as lamports");
            std::process::exit(1);
        });

    let amount_num: f64 = args
        .iter()
        .position(|a| a == "--amount")
        .and_then(|i| args.get(i + 1))
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| {
            println!("Error: --amount is required");
            std::process::exit(1);
        });

    let use_lamports = args.iter().any(|a| a == "--lamports");

    let amount_lamports = if use_lamports {
        amount_num as u64
    } else {
        (amount_num * 1_000_000_000.0) as u64
    };

    let to_pubkey = Pubkey::from_str(to_str)?;

    // Load configuration
    let payer_path = std::env::var("PAYER_KEYPAIR").unwrap_or_else(|_| default_payer_path());
    let sequencer_url = sequencer_url();

    print_info(&format!("Sequencer URL: {}", sequencer_url));
    print_info(&format!(
        "Amount: {} lamports ({} SOL)",
        amount_lamports,
        amount_lamports as f64 / 1_000_000_000.0

    ));

    // Load keypair
    let payer = load_keypair(&payer_path)?;
    println!("\nFrom: {}", payer.pubkey());
    println!("To: {}", to_pubkey);

    // Create HTTP client
    let client = reqwest::Client::new();

    // Get current nonce
    let account_id = hex::encode(payer.pubkey().to_bytes());
    let nonce = match client
        .get(format!("{}/account/{}", sequencer_url, account_id))
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => resp
            .json::<AccountState>()
            .await
            .map(|s| s.nonce)
            .unwrap_or(0),
        _ => 0,
    };

    println!("Current nonce: {}", nonce);

    // Create transaction data
    let tx_data = TransactionData {
        from: hex::encode(payer.pubkey().to_bytes()),
        to: hex::encode(to_pubkey.to_bytes()),
        amount: amount_lamports,
        nonce,
        chain_id: 1,
    };

    // Serialize for signing (using JSON for simplicity)
    let tx_bytes = serde_json::to_vec(&tx_data)?;

    // Sign the transaction
    let signature = payer.sign_message(&tx_bytes);

    let signed_tx = SignedTransaction {
        data: tx_data,
        signature: bs58::encode(signature.as_ref()).into_string(),
        signer_pubkey: payer.pubkey().to_string(),
    };

    // Submit transaction
    print_waiting("Submitting transaction to sequencer...");

    match client
        .post(format!("{}/submit_tx", sequencer_url))
        .json(&signed_tx)
        .send()
        .await
    {
        Ok(resp) => {
            let status = resp.status();
            if status.is_success() {
                if let Ok(result) = resp.json::<SubmitResponse>().await {
                    if result.success {
                        print_success("Transaction submitted successfully!");
                        if let Some(hash) = result.tx_hash {
                            println!("TX Hash: {}", hash);
                        }
                    } else {
                        print_error(&format!(
                            "Transaction rejected: {}",
                            result.error.unwrap_or_default()
                        ));
                    }
                } else {
                    print_success(&format!("Transaction submitted (status: {})", status));
                }
            } else {
                let body = resp.text().await.unwrap_or_default();
                print_error(&format!("Failed to submit: {} - {}", status, body));
            }
        }
        Err(e) => {
            print_error(&format!("Request failed: {}", e));
            print_info("Make sure the sequencer is running:");
            print_info("  cargo run --package core");
            return Err(e.into());
        }
    }

    println!("\n📋 Transfer Summary:");
    println!("  From: {}", payer.pubkey());
    println!("  To: {}", to_pubkey);
    println!("  Amount: {} SOL", amount_lamports as f64 / 1_000_000_000.0);
    println!("  Nonce: {}", nonce);

    print_info("Use 'check_balance' to verify the transfer was processed.");

    Ok(())
}
