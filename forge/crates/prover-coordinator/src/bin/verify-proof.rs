//! Standalone proof verification CLI
//!
//! Usage: verify-proof --proof <path> --public-witness <path> [--program-id <id>] [--rpc-url <url>]

use clap::Parser;
use std::path::PathBuf;

use prover_coordinator::{ProofData, SolanaVerifierClient, SolanaVerifierConfig};

#[derive(Parser, Debug)]
#[command(name = "verify-proof")]
#[command(about = "Verify a ZK proof on Solana devnet")]
struct Args {
    /// Path to proof file (388 bytes)
    #[arg(long)]
    proof: PathBuf,

    /// Path to public witness file (236 bytes)
    #[arg(long)]
    public_witness: PathBuf,

    /// Verifier program ID
    #[arg(long, default_value = "EZzyLrTrC4uyU488jVAs4GKeCR1s9GmoFggeiDqwDeNK")]
    program_id: String,

    /// Solana RPC URL
    #[arg(long, default_value = "https://api.devnet.solana.com")]
    rpc_url: String,

    /// Path to keypair file
    #[arg(long)]
    keypair: Option<String>,

    /// Compute units to request
    #[arg(long, default_value = "500000")]
    compute_units: u32,

    /// Dry run (don't submit transaction)
    #[arg(long)]
    dry_run: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let args = Args::parse();

    println!("=== Zelana Proof Verifier ===\n");
    println!("Configuration:");
    println!("  Proof file: {:?}", args.proof);
    println!("  Public witness: {:?}", args.public_witness);
    println!("  Program ID: {}", args.program_id);
    println!("  RPC URL: {}", args.rpc_url);
    println!("  Compute units: {}", args.compute_units);
    println!();

    // Load proof data
    let proof_data = ProofData::from_files(&args.proof, &args.public_witness)?;

    // Validate
    proof_data.validate().map_err(|e| anyhow::anyhow!(e))?;

    println!("Proof data loaded:");
    println!("  Proof: {} bytes", proof_data.proof_bytes.len());
    println!(
        "  Public witness: {} bytes",
        proof_data.public_witness_bytes.len()
    );
    println!(
        "  Total instruction data: {} bytes",
        proof_data.to_instruction_data().len()
    );
    println!();

    // Parse first few public inputs from witness
    if proof_data.public_witness_bytes.len() >= 12 {
        let count = u32::from_be_bytes([
            proof_data.public_witness_bytes[0],
            proof_data.public_witness_bytes[1],
            proof_data.public_witness_bytes[2],
            proof_data.public_witness_bytes[3],
        ]);
        println!("Public inputs count: {}", count);

        // Show first input
        if proof_data.public_witness_bytes.len() >= 44 {
            let first_input = &proof_data.public_witness_bytes[12..44];
            println!("First public input: 0x{}", hex::encode(first_input));
        }
        println!();
    }

    if args.dry_run {
        println!("Dry run mode - not submitting transaction");
        println!("\nInstruction data (first 64 bytes):");
        let data = proof_data.to_instruction_data();
        println!("{}", hex::encode(&data[..64.min(data.len())]));
        return Ok(());
    }

    // Create client
    let config = SolanaVerifierConfig {
        rpc_url: args.rpc_url,
        program_id: args.program_id,
        keypair_path: args.keypair,
        compute_units: args.compute_units,
        priority_fee_micro_lamports: 1000,
    };

    let client = SolanaVerifierClient::new(config)?;

    println!("Payer: {}", client.payer_pubkey());

    // Check balance
    let balance = client.get_balance().await?;
    println!(
        "Balance: {} lamports ({} SOL)",
        balance,
        balance as f64 / 1e9
    );

    if balance < 10_000_000 {
        println!("\nWARNING: Low balance. You may need to airdrop SOL.");
        println!("Run: solana airdrop 1 --url <rpc_url>");
    }
    println!();

    // Check program exists
    println!("Checking verifier program...");
    if !client.check_program_exists().await? {
        return Err(anyhow::anyhow!("Verifier program not found on chain"));
    }
    println!("Verifier program found\n");

    // Submit verification
    println!("Submitting verification transaction...");
    let result = client
        .verify_proof(&proof_data.proof_bytes, &proof_data.public_witness_bytes)
        .await?;

    println!("\n=== Verification Result ===");
    println!("Transaction: {}", result.signature);
    println!("Verified: {}", result.verified);
    println!("Slot: {}", result.slot);
    if let Some(cu) = result.compute_units_consumed {
        println!("Compute units: {}", cu);
    }

    println!("\nView on Solana Explorer:");
    println!(
        "https://explorer.solana.com/tx/{}?cluster=devnet",
        result.signature
    );

    Ok(())
}
