use anyhow::Result;
use solana_client::rpc_client::RpcClient;
use solana_commitment_config::CommitmentConfig;
use solana_sdk::{
    message::{AccountMeta, Instruction},
    pubkey::Pubkey,
    signer::Signer,
    transaction::Transaction,
};
use std::str::FromStr;
use tokio::time::Duration;
use zelana_keypair::Keypair;
use zelana_transaction::DepositParams;

const MIN_BALANCE: u64 = 2_000_000_000;
const LAMPORTS_PER_SOL: f64 = 1_000_000_000.0;
const DOMAIN: &[u8] = b"solana";

pub struct BridgeConfig {
    pub rpc_url: String,
    pub bridge_program_id: String,
}

impl Default for BridgeConfig {
    fn default() -> Self {
        Self {
            rpc_url: std::env::var("SOLANA_RPC_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:8899".to_string()),
            bridge_program_id: std::env::var("BRIDGE_PROGRAM_ID")
                .unwrap_or_else(|_| "9HXapBN9otLGnQNGv1HRk91DGqMNvMAvQqohL7gPW1sd".to_string()),
        }
    }
}

/// Requests a Solana airdrop if the balance is below the minimum threshold
pub async fn airdrop_if_needed(rpc: &RpcClient, pubkey: &Pubkey, name: &str) -> Result<()> {
    let balance = rpc.get_balance(pubkey)?;
    println!("{}: {:.2} SOL", name, balance as f64 / LAMPORTS_PER_SOL);

    if balance < MIN_BALANCE {
        println!(
            "💸 Requesting airdrop of {:.2} SOL...",
            MIN_BALANCE as f64 / LAMPORTS_PER_SOL
        );
        let sig = rpc.request_airdrop(pubkey, MIN_BALANCE)?;

        // Wait for confirmation with timeout
        let mut confirmed = false;
        for attempt in 0..30 {
            tokio::time::sleep(Duration::from_millis(500)).await;
            if rpc.confirm_transaction(&sig)? {
                confirmed = true;
                break;
            }
            if attempt % 5 == 0 {
                println!("⏳ Waiting for confirmation... ({}/30)", attempt);
            }
        }

        if !confirmed {
            return Err(anyhow::anyhow!(
                "Airdrop transaction not confirmed after 15 seconds"
            ));
        }

        let new_balance = rpc.get_balance(pubkey)?;
        println!(
            "✅ Airdrop confirmed! New balance: {:.2} SOL",
            new_balance as f64 / LAMPORTS_PER_SOL
        );
    } else {
        println!("✓ Sufficient balance available");
    }

    Ok(())
}

/// Deposits funds to L2 via the bridge smart contract
pub async fn deposit_to_l2(
    rpc: &RpcClient,
    program_id: &Pubkey,
    user_solkey: &solana_sdk::signature::Keypair,
    amount: u64,
    nonce: u64,
) -> Result<String> {
    println!("🌉 Depositing {} lamports to Bridge...", amount);

    let mut domain_padded = [0u8; 32];
    domain_padded[..DOMAIN.len()].copy_from_slice(DOMAIN);

    // Derive PDAs
    let (config_pda, _) = Pubkey::find_program_address(&[b"config", &domain_padded], program_id);
    let (vault_pda, _) = Pubkey::find_program_address(&[b"vault", &domain_padded], program_id);
    let (receipt_pda, _) = Pubkey::find_program_address(
        &[
            b"receipt",
            &domain_padded,
            user_solkey.pubkey().as_ref(),
            &nonce.to_le_bytes(),
        ],
        program_id,
    );

    println!("📍 PDAs:");
    println!("  Config: {}", config_pda);
    println!("  Vault: {}", vault_pda);
    println!("  Receipt: {}", receipt_pda);

    // Serialize deposit parameters
    let params = DepositParams { amount, nonce };
    let mut data = vec![1]; // Instruction discriminator
    data.extend(wincode::serialize(&params)?);

    // Build deposit instruction
    let system_id = Pubkey::from_str("11111111111111111111111111111111")?;
    let deposit_ix = Instruction {
        program_id: *program_id,
        accounts: vec![
            AccountMeta::new(user_solkey.pubkey(), true),
            AccountMeta::new_readonly(config_pda, false),
            AccountMeta::new(vault_pda, false),
            AccountMeta::new(receipt_pda, false),
            AccountMeta::new_readonly(system_id, false),
        ],
        data,
    };

    // Create and send transaction
    let tx = Transaction::new_signed_with_payer(
        &[deposit_ix],
        Some(&user_solkey.pubkey()),
        &[user_solkey],
        rpc.get_latest_blockhash()?,
    );

    println!("📤 Sending bridge transaction...");
    let sig = rpc.send_and_confirm_transaction(&tx)?;
    println!("✅ Bridge transaction confirmed!");

    Ok(sig.to_string())
}

/// Complete airdrop and bridge flow
pub async fn airdrop_and_bridge_flow(
    keypair: &Keypair,
    amount: u64,
    config: &BridgeConfig,
) -> Result<String> {
    let account_id = keypair.account_id();
    let solana_keypair = Keypair::solana_keypair(keypair);

    println!("\n📋 Account Information:");
    println!("  Zelana Account ID: {}", account_id.to_hex());
    println!("  Solana Pubkey: {}", solana_keypair.pubkey());

    // Connect to Solana RPC
    println!("\n🌐 Connecting to Solana RPC: {}", config.rpc_url);
    let rpc = RpcClient::new_with_commitment(&config.rpc_url, CommitmentConfig::confirmed());

    // Step 1: Ensure sufficient balance
    println!("\n💰 Step 1: Checking balance...");
    airdrop_if_needed(&rpc, &solana_keypair.pubkey(), "Account").await?;

    // Step 2: Bridge to L2
    println!("\n🌉 Step 2: Bridging to L2...");
    let program_id = Pubkey::from_str(&config.bridge_program_id)?;
    println!("Bridge Program ID: {}", program_id);

    // Generate nonce based on current timestamp
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs();
    println!("Using nonce: {}", nonce);

    let sig = deposit_to_l2(&rpc, &program_id, &solana_keypair, amount, nonce).await?;

    println!("\n🎉 Success!");
    println!("Transaction Signature: {}", sig);
    println!("L2 Account: {}", account_id.to_hex());
    println!("Amount Bridged: {} lamports", amount);

    Ok(sig)
}
