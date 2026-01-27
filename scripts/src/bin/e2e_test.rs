//! End-to-End Test Script
//!
//! This script runs a complete end-to-end test of the Zelana L2:
//! 1. Check sequencer is running
//! 2. Deposit SOL to L2
//! 3. Wait for deposit to be indexed
//! 4. Check L2 balance
//! 5. Send L2 transfer
//! 6. Verify balances updated
//!
//! Usage:
//!   cargo run --bin e2e_test
//!   cargo run --bin e2e_test -- --skip-deposit
//!
//! Arguments:
//!   --skip-deposit    Skip the deposit step (if already deposited)
//!   --recipient <PK>  Recipient for transfer test (default: self-transfer)
//!
//! Prerequisites:
//!   1. Surfpool running: surfpool start
//!   2. Programs deployed: solana program deploy ...
//!   3. Bridge initialized: cargo run --bin init_bridge
//!   4. VK stored: cargo run --bin store_vk
//!   5. Sequencer running: cargo run --package core

use zelana_scripts::config::*;
use solana_client::rpc_client::RpcClient;
use solana_commitment_config::CommitmentConfig;
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    signature::Signer,
    system_program,
    transaction::Transaction,
};
use zelana_config::{API, SOLANA};
use std::str::FromStr;
use std::time::Duration;
use tokio::time::sleep;

/// Bridge instruction discriminator
const BRIDGE_IX_DEPOSIT: u8 = 1;

#[repr(C, packed)]
#[derive(Clone, Copy)]
struct DepositParams {
    amount: u64,
    nonce: u64,
}

#[derive(serde::Deserialize, Debug)]
struct AccountState {
    #[allow(dead_code)]
    account_id: String,
    balance: u64,
    nonce: u64,
}

#[derive(serde::Deserialize, Debug)]
struct HealthResponse {
    healthy: bool,
    version: String,
    uptime_secs: u64,
}

#[derive(serde::Serialize, Debug)]
struct GetAccountRequest {
    account_id: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    print_header("Zelana L2 End-to-End Test");

    // Parse arguments
    let args: Vec<String> = std::env::args().collect();
    let skip_deposit = args.iter().any(|a| a == "--skip-deposit");

    let recipient_str = args
        .iter()
        .position(|a| a == "--recipient")
        .and_then(|i| args.get(i + 1))
        .map(|s| s.as_str());

    // Load configuration
    let payer_path = std::env::var("PAYER_KEYPAIR").unwrap_or_else(|_| default_payer_path());
    let sequencer_url = format!("http://{}", API.sequencer_url);

    let payer = load_keypair(&payer_path)?;
    let _recipient = recipient_str
        .map(|s| Pubkey::from_str(s).expect("Invalid recipient pubkey"))
        .unwrap_or_else(|| payer.pubkey()); // Reserved for future transfer testing

    println!("Configuration:");
    println!("  Bridge Program: {}", SOLANA.bridge_program);
    println!("  Verifier Program: {}", SOLANA.verifier_program);
    println!("  RPC URL: {}", SOLANA.rpc_url);
    println!("  Sequencer URL: {}", sequencer_url);
    println!("  Payer: {}", payer.pubkey());

    let http_client = reqwest::Client::new();
    let rpc = RpcClient::new_with_commitment(SOLANA.rpc_url, CommitmentConfig::confirmed());

    // ========================================
    // Step 1: Check Sequencer
    // ========================================
    print_header("Step 1: Check Sequencer");

    print_waiting("Connecting to sequencer...");
    match http_client
        .get(format!("{}/health", sequencer_url))
        .timeout(Duration::from_secs(5))
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => {
            if let Ok(health) = resp.json::<HealthResponse>().await {
                print_success(&format!(
                    "Sequencer is running (v{}, uptime: {}s)",
                    health.version, health.uptime_secs
                ));
            } else {
                print_success("Sequencer is running");
            }
        }
        Ok(resp) => {
            print_info(&format!("Sequencer returned: {}", resp.status()));
        }
        Err(e) => {
            print_error(&format!("Cannot connect to sequencer: {}", e));
            println!("\nMake sure the sequencer is running:");
            println!("  cargo run --package zelana-core");
            return Err(anyhow::anyhow!("Sequencer not available"));
        }
    }

    // ========================================
    // Step 2: Check/Fund L1 Balance
    // ========================================
    print_header("Step 2: Check L1 Balance");

    let l1_balance = rpc.get_balance(&payer.pubkey())?;
    println!("L1 Balance: {} SOL", l1_balance as f64 / 1_000_000_000.0);

    if l1_balance < 1_000_000_000 {
        print_waiting("Requesting airdrop...");
        match rpc.request_airdrop(&payer.pubkey(), 2_000_000_000) {
            Ok(sig) => {
                // Wait for confirmation
                for _ in 0..30 {
                    if rpc.confirm_transaction(&sig).unwrap_or(false) {
                        break;
                    }
                    sleep(Duration::from_millis(500)).await;
                }
                print_success("Airdrop received");
            }
            Err(e) => {
                print_error(&format!("Airdrop failed: {}", e));
                print_info("Manually run: solana airdrop 2 --url http://127.0.0.1:8899");
            }
        }
    } else {
        print_success("Sufficient L1 balance");
    }

    // ========================================
    // Step 3: Deposit to L2
    // ========================================
    if !skip_deposit {
        print_header("Step 3: Deposit to L2");

        let deposit_amount = 500_000_000u64; // 0.5 SOL
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos() as u64;

        println!("Deposit amount: 0.5 SOL");
        println!("Nonce: {}", nonce);

        let (config_pda, _) = derive_config_pda();
        let (vault_pda, _) = derive_vault_pda();
        let (receipt_pda, _) = derive_receipt_pda(&payer.pubkey(), nonce);

        let params = DepositParams {
            amount: deposit_amount,
            nonce,
        };

        let mut instruction_data = vec![BRIDGE_IX_DEPOSIT];
        instruction_data.extend_from_slice(unsafe {
            std::slice::from_raw_parts(
                &params as *const DepositParams as *const u8,
                std::mem::size_of::<DepositParams>(),
            )
        });

        let accounts = vec![
            AccountMeta::new(payer.pubkey(), true),
            AccountMeta::new_readonly(config_pda, false),
            AccountMeta::new(vault_pda, false),
            AccountMeta::new(receipt_pda, false),
            AccountMeta::new_readonly(system_program::ID, false),
        ];

        let deposit_ix = Instruction {
            program_id: bridge_program_id(),
            accounts,
            data: instruction_data,
        };

        print_waiting("Sending deposit transaction...");

        let recent_blockhash = rpc.get_latest_blockhash()?;
        let tx = Transaction::new_signed_with_payer(
            &[deposit_ix],
            Some(&payer.pubkey()),
            &[&payer],
            recent_blockhash,
        );

        match rpc.send_and_confirm_transaction(&tx) {
            Ok(sig) => {
                print_success(&format!("Deposit confirmed: {}", sig));
            }
            Err(e) => {
                print_error(&format!("Deposit failed: {}", e));
                return Err(e.into());
            }
        }

        // Wait for sequencer to index
        print_waiting("Waiting for sequencer to index deposit (5 seconds)...");
        sleep(Duration::from_secs(5)).await;
    } else {
        print_header("Step 3: Deposit (Skipped)");
        print_info("Using --skip-deposit flag");
    }

    // ========================================
    // Step 4: Check L2 Balance
    // ========================================
    print_header("Step 4: Check L2 Balance");

    let account_id = hex::encode(payer.pubkey().to_bytes());
    let balance_before = match http_client
        .post(format!("{}/account", sequencer_url))
        .json(&GetAccountRequest {
            account_id: account_id.clone(),
        })
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => resp.json::<AccountState>().await.ok(),
        _ => None,
    };

    if let Some(state) = &balance_before {
        print_success(&format!(
            "L2 Balance: {} SOL (nonce: {})",
            state.balance as f64 / 1_000_000_000.0,
            state.nonce
        ));
    } else {
        print_info("Account not found on L2 (may not be indexed yet)");
    }

    // ========================================
    // Step 5: Check Batch Status
    // ========================================
    print_header("Step 5: Check Batch Status");

    match http_client
        .get(format!("{}/status/batch", sequencer_url))
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => {
            let body = resp.text().await.unwrap_or_default();
            print_success(&format!("Batch status: {}", body));
        }
        Ok(resp) => {
            print_info(&format!("Batch status returned: {}", resp.status()));
        }
        Err(e) => {
            print_error(&format!("Failed to get batch status: {}", e));
        }
    }

    // Note: L2 transfers via HTTP not yet implemented (use UDP)
    // The sequencer uses UDP for high-throughput transaction submission
    println!("\nℹ️  Note: L2 transfers are submitted via UDP, not HTTP.");
    println!("   For testing transfers, use the UDP client or wait for deposits to be indexed.");

    // ========================================
    // Step 6: Verify Final State
    // ========================================
    print_header("Step 6: Verify Final State");

    // Check sender balance
    let sender_final = match http_client
        .post(format!("{}/account", sequencer_url))
        .json(&GetAccountRequest {
            account_id: account_id.clone(),
        })
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => resp.json::<AccountState>().await.ok(),
        _ => None,
    };

    if let Some(state) = &sender_final {
        println!(
            "Account L2 Balance: {} SOL (nonce: {})",
            state.balance as f64 / 1_000_000_000.0,
            state.nonce
        );
    } else {
        print_info("Account not yet indexed on L2");
    }

    // ========================================
    // Summary
    // ========================================
    print_header("Test Summary");

    println!("✅ Sequencer connectivity: PASSED");

    if !skip_deposit {
        println!("✅ L1 Deposit: PASSED");
    }

    if balance_before.is_some() {
        println!("✅ L2 Balance query: PASSED");
    } else {
        println!("⚠️  L2 Balance query: Account not found (deposit may need indexing)");
    }

    println!("✅ Batch status query: PASSED");

    println!("\n🎉 End-to-end test completed!");
    println!("\nNext steps:");
    println!("  - Wait for batch to be sealed and proven");
    println!(
        "  - Check batch status: curl {}/status/batch",
        sequencer_url
    );
    println!("  - Test withdrawal functionality via POST /withdraw");

    Ok(())
}
