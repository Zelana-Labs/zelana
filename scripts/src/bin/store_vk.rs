//! Store Batch Verifying Key in the Verifier Program (Chunked Upload)
//!
//! This script stores the batch verifying key (VK) required for ZK proof verification.
//! Uses chunked upload to handle large VK data that exceeds transaction size limits.
//!
//! Flow:
//! 1. init_batch_vk - Initialize VK account with base curve points
//! 2. append_ic_points - Add IC points in chunks (2-3 points per tx)
//! 3. finalize_batch_vk - Mark VK as ready for use
//!
//! Usage:
//!   cargo run --bin store_vk
//!   cargo run --bin store_vk -- --vk-file ./keys/batch_vk.json
//!
//! Environment variables:
//!   PAYER_KEYPAIR - Path to payer keypair (default: ~/.config/solana/id.json)
//!   VK_FILE       - Path to VK JSON file (optional, uses mock data if not provided)

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

/// Anchor discriminator for init_batch_vk instruction
/// = sha256("global:init_batch_vk")[0..8]
const INIT_BATCH_VK_DISCRIMINATOR: [u8; 8] = [0x71, 0x53, 0x1b, 0x0e, 0x53, 0xf1, 0x7d, 0x72];

/// Anchor discriminator for append_ic_points instruction
/// = sha256("global:append_ic_points")[0..8]
const APPEND_IC_POINTS_DISCRIMINATOR: [u8; 8] = [0xdd, 0xf3, 0xb9, 0x87, 0x2f, 0x31, 0x5a, 0x57];

/// Anchor discriminator for finalize_batch_vk instruction
/// = sha256("global:finalize_batch_vk")[0..8]
const FINALIZE_BATCH_VK_DISCRIMINATOR: [u8; 8] = [0xd6, 0x5c, 0xdf, 0x94, 0x35, 0x36, 0x08, 0xde];

/// Number of IC points for batch verification (7 public inputs + 1)
const BATCH_IC_POINTS: usize = 8;

/// Max IC points per transaction (to stay under tx size limit)
const IC_POINTS_PER_TX: usize = 4;

/// VK data structure for JSON loading
#[derive(serde::Deserialize)]
struct VkData {
    alpha_g1: Vec<u8>,
    beta_g2: Vec<u8>,
    gamma_g2: Vec<u8>,
    delta_g2: Vec<u8>,
    ic: Vec<Vec<u8>>,
}

/// Generate mock VK data for testing
/// In production, this should come from the actual circuit setup
fn generate_mock_vk() -> ([u8; 64], [u8; 128], [u8; 128], [u8; 128], Vec<[u8; 64]>) {
    // These are placeholder values - real VK must come from trusted setup
    let alpha_g1 = [1u8; 64];
    let beta_g2 = [2u8; 128];
    let gamma_g2 = [3u8; 128];
    let delta_g2 = [4u8; 128];

    // IC points: 8 points for 7 public inputs
    let mut ic = Vec::with_capacity(BATCH_IC_POINTS);
    for i in 0..BATCH_IC_POINTS {
        let mut point = [0u8; 64];
        point[0] = (i + 10) as u8;
        ic.push(point);
    }

    (alpha_g1, beta_g2, gamma_g2, delta_g2, ic)
}

/// Load VK from JSON file
fn load_vk_from_file(
    path: &str,
) -> anyhow::Result<([u8; 64], [u8; 128], [u8; 128], [u8; 128], Vec<[u8; 64]>)> {
    let data = std::fs::read_to_string(path)?;
    let vk: VkData = serde_json::from_str(&data)?;

    let mut alpha_g1 = [0u8; 64];
    let mut beta_g2 = [0u8; 128];
    let mut gamma_g2 = [0u8; 128];
    let mut delta_g2 = [0u8; 128];

    alpha_g1.copy_from_slice(&vk.alpha_g1[..64]);
    beta_g2.copy_from_slice(&vk.beta_g2[..128]);
    gamma_g2.copy_from_slice(&vk.gamma_g2[..128]);
    delta_g2.copy_from_slice(&vk.delta_g2[..128]);

    let ic: Vec<[u8; 64]> = vk
        .ic
        .iter()
        .map(|p| {
            let mut arr = [0u8; 64];
            arr.copy_from_slice(&p[..64]);
            arr
        })
        .collect();

    Ok((alpha_g1, beta_g2, gamma_g2, delta_g2, ic))
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    print_header("Store Batch Verifying Key (Chunked)");

    // Parse arguments
    let args: Vec<String> = std::env::args().collect();
    let vk_file = args
        .iter()
        .position(|a| a == "--vk-file")
        .and_then(|i| args.get(i + 1))
        .map(|s| s.as_str());

    // Load configuration
    let payer_path = std::env::var("PAYER_KEYPAIR").unwrap_or_else(|_| default_payer_path());

    print_info(&format!("Verifier Program ID: {}", SOLANA.verifier_program));
    print_info(&format!("RPC URL: {}", SOLANA.rpc_url));
    print_info(&format!("Payer keypair: {}", payer_path));

    // Load VK data
    let (alpha_g1, beta_g2, gamma_g2, delta_g2, ic) = if let Some(path) = vk_file {
        print_info(&format!("Loading VK from: {}", path));
        load_vk_from_file(path)?
    } else {
        print_info("Using mock VK data (for testing only!)");
        print_info(
            "For production, generate real VK with: cargo run --package prover --bin keygen",
        );
        generate_mock_vk()
    };

    println!("VK IC points: {}", ic.len());

    // Load keypair
    let payer = load_keypair(&payer_path)?;
    println!("\nPayer: {}", payer.pubkey());

    // Connect to RPC
    let rpc = RpcClient::new_with_commitment(SOLANA.rpc_url, CommitmentConfig::confirmed());

    // Check payer balance
    let balance = rpc.get_balance(&payer.pubkey())?;
    println!("Payer balance: {} SOL", balance as f64 / 1_000_000_000.0);

    if balance < 50_000_000 {
        print_error("Insufficient balance. Need at least 0.05 SOL for rent.");
        print_info("Run: solana airdrop 2 --url http://127.0.0.1:8899");
        return Err(anyhow::anyhow!("Insufficient balance"));
    }

    // Derive VK PDA
    let (vk_pda, _) = derive_batch_vk_pda();
    let domain = domain_bytes();

    println!("\nBatch VK PDA: {}", vk_pda);

    // Check if already stored
    match rpc.get_account_with_commitment(&vk_pda, CommitmentConfig::confirmed()) {
        Ok(response) if response.value.is_some() => {
            print_info("VK already stored. VK account exists.");
            print_info("To update, close the account first (not implemented yet).");
            return Ok(());
        }
        _ => {}
    }

    // ========================================================================
    // Step 1: Initialize VK with base curve points
    // ========================================================================
    print_waiting("Step 1/3: Initializing VK account...");

    let mut init_data = Vec::new();
    init_data.extend_from_slice(&INIT_BATCH_VK_DISCRIMINATOR);
    init_data.extend_from_slice(&domain);
    init_data.extend_from_slice(&alpha_g1);
    init_data.extend_from_slice(&beta_g2);
    init_data.extend_from_slice(&gamma_g2);
    init_data.extend_from_slice(&delta_g2);

    println!("  Init instruction data size: {} bytes", init_data.len());

    let init_accounts = vec![
        AccountMeta::new(payer.pubkey(), true),
        AccountMeta::new(vk_pda, false),
        AccountMeta::new_readonly(system_program::ID, false),
    ];

    let init_ix = Instruction {
        program_id: verifier_program_id(),
        accounts: init_accounts,
        data: init_data,
    };

    let recent_blockhash = rpc.get_latest_blockhash()?;
    let init_tx = Transaction::new_signed_with_payer(
        &[init_ix],
        Some(&payer.pubkey()),
        &[&payer],
        recent_blockhash,
    );

    match rpc.send_and_confirm_transaction(&init_tx) {
        Ok(sig) => {
            print_success(&format!("VK account initialized. Sig: {}", sig));
        }
        Err(e) => {
            print_error(&format!("Failed to initialize VK: {}", e));
            return Err(e.into());
        }
    }

    // ========================================================================
    // Step 2: Append IC points in chunks
    // ========================================================================
    let chunks: Vec<&[[u8; 64]]> = ic.chunks(IC_POINTS_PER_TX).collect();
    let total_chunks = chunks.len();

    for (chunk_idx, chunk) in chunks.iter().enumerate() {
        print_waiting(&format!(
            "Step 2/3: Appending IC points chunk {}/{}...",
            chunk_idx + 1,
            total_chunks
        ));

        let mut append_data = Vec::new();
        append_data.extend_from_slice(&APPEND_IC_POINTS_DISCRIMINATOR);

        // Borsh Vec serialization: 4-byte length + elements
        let chunk_len = chunk.len() as u32;
        append_data.extend_from_slice(&chunk_len.to_le_bytes());
        for point in *chunk {
            append_data.extend_from_slice(point);
        }

        println!(
            "  Chunk {} size: {} bytes ({} IC points)",
            chunk_idx + 1,
            append_data.len(),
            chunk.len()
        );

        let append_accounts = vec![
            AccountMeta::new(payer.pubkey(), true),
            AccountMeta::new(vk_pda, false),
        ];

        let append_ix = Instruction {
            program_id: verifier_program_id(),
            accounts: append_accounts,
            data: append_data,
        };

        let recent_blockhash = rpc.get_latest_blockhash()?;
        let append_tx = Transaction::new_signed_with_payer(
            &[append_ix],
            Some(&payer.pubkey()),
            &[&payer],
            recent_blockhash,
        );

        match rpc.send_and_confirm_transaction(&append_tx) {
            Ok(sig) => {
                print_success(&format!(
                    "  Appended {} IC points. Sig: {}",
                    chunk.len(),
                    sig
                ));
            }
            Err(e) => {
                print_error(&format!("Failed to append IC points: {}", e));
                return Err(e.into());
            }
        }
    }

    // ========================================================================
    // Step 3: Finalize VK
    // ========================================================================
    print_waiting("Step 3/3: Finalizing VK...");

    let mut finalize_data = Vec::new();
    finalize_data.extend_from_slice(&FINALIZE_BATCH_VK_DISCRIMINATOR);

    let finalize_accounts = vec![
        AccountMeta::new(payer.pubkey(), true),
        AccountMeta::new(vk_pda, false),
    ];

    let finalize_ix = Instruction {
        program_id: verifier_program_id(),
        accounts: finalize_accounts,
        data: finalize_data,
    };

    let recent_blockhash = rpc.get_latest_blockhash()?;
    let finalize_tx = Transaction::new_signed_with_payer(
        &[finalize_ix],
        Some(&payer.pubkey()),
        &[&payer],
        recent_blockhash,
    );

    match rpc.send_and_confirm_transaction(&finalize_tx) {
        Ok(sig) => {
            print_success("Batch VK stored and finalized successfully!");
            println!("Transaction signature: {}", sig);
        }
        Err(e) => {
            print_error(&format!("Failed to finalize VK: {}", e));
            return Err(e.into());
        }
    }

    println!("\n✅ VK Configuration Complete:");
    println!("  VK PDA: {}", vk_pda);
    println!("  IC points: {}", ic.len());
    println!("  Domain: {}", hex::encode(&domain[..6]));
    println!(
        "  Transactions: {} (init + {} chunks + finalize)",
        2 + total_chunks,
        total_chunks
    );

    Ok(())
}
