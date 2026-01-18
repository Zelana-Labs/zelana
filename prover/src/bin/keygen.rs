//! Key Generation CLI for Zelana L2 ZK Prover
//!
//! Generates the proving and verifying keys for the L2BlockCircuit.
//! These keys are required for real Groth16 proof generation.
//!
//! Usage:
//!   cargo run --package prover --bin keygen -- --pk-out ./proving.key --vk-out ./verifying.key
//!
//! Note: Key generation is a one-time operation. Keys must be regenerated if the circuit changes.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use ark_bn254::Bn254;
use ark_groth16::Groth16;
use ark_serialize::CanonicalSerialize;
use ark_snark::SNARK;
use ark_std::rand::{SeedableRng, rngs::StdRng};

use prover::L2BlockCircuit;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();

    // Parse command line arguments
    let mut pk_path = String::from("./proving.key");
    let mut vk_path = String::from("./verifying.key");
    let mut force = false;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--pk-out" => {
                i += 1;
                if i < args.len() {
                    pk_path = args[i].clone();
                }
            }
            "--vk-out" => {
                i += 1;
                if i < args.len() {
                    vk_path = args[i].clone();
                }
            }
            "--force" | "-f" => {
                force = true;
            }
            "--help" | "-h" => {
                print_help();
                return Ok(());
            }
            _ => {
                eprintln!("Unknown argument: {}", args[i]);
                print_help();
                std::process::exit(1);
            }
        }
        i += 1;
    }

    // Check if keys already exist
    if !force && Path::new(&pk_path).exists() && Path::new(&vk_path).exists() {
        println!("Keys already exist at:");
        println!("  Proving key:   {}", pk_path);
        println!("  Verifying key: {}", vk_path);
        println!("\nUse --force to regenerate keys.");
        return Ok(());
    }

    println!("Zelana L2 ZK Key Generation");
    println!("===========================");
    println!();

    // Create a dummy circuit for setup
    // The circuit structure determines the proving/verifying keys
    println!("Creating dummy L2BlockCircuit for setup...");
    println!("  Public inputs: 7 (pre_state_root, post_state_root, pre_shielded_root,");
    println!("                    post_shielded_root, withdrawal_root, batch_hash, batch_id)");
    println!();

    let dummy_circuit = L2BlockCircuit::dummy();

    // Perform circuit-specific setup (trusted setup)
    println!("Performing Groth16 circuit-specific setup...");
    println!("This may take a few minutes...");

    let mut rng = StdRng::seed_from_u64(0); // Deterministic for reproducibility
    let start = std::time::Instant::now();

    let (pk, vk) = Groth16::<Bn254>::circuit_specific_setup(dummy_circuit, &mut rng)
        .context("Failed to perform circuit setup")?;

    let setup_time = start.elapsed();
    println!("Setup complete in {:?}", setup_time);
    println!();

    // Serialize and save proving key
    println!("Saving proving key to {}...", pk_path);
    let mut pk_bytes = Vec::new();
    pk.serialize_compressed(&mut pk_bytes)
        .context("Failed to serialize proving key")?;

    // Create parent directories if needed
    if let Some(parent) = Path::new(&pk_path).parent() {
        fs::create_dir_all(parent).context("Failed to create proving key directory")?;
    }
    fs::write(&pk_path, &pk_bytes).context("Failed to write proving key")?;
    println!(
        "  Size: {} bytes ({:.2} MB)",
        pk_bytes.len(),
        pk_bytes.len() as f64 / 1024.0 / 1024.0
    );

    // Serialize and save verifying key
    println!("Saving verifying key to {}...", vk_path);
    let mut vk_bytes = Vec::new();
    vk.serialize_compressed(&mut vk_bytes)
        .context("Failed to serialize verifying key")?;

    if let Some(parent) = Path::new(&vk_path).parent() {
        fs::create_dir_all(parent).context("Failed to create verifying key directory")?;
    }
    fs::write(&vk_path, &vk_bytes).context("Failed to write verifying key")?;
    println!("  Size: {} bytes", vk_bytes.len());

    // Compute and display VK hash (for on-chain reference)
    let vk_hash = blake3::hash(&vk_bytes);
    println!();
    println!("Verification key hash (blake3):");
    println!("  {}", hex::encode(vk_hash.as_bytes()));

    println!();
    println!("Key generation complete!");
    println!();
    println!("To use with Zelana sequencer, set environment variables:");
    println!("  export ZELANA_PROVING_KEY={}", pk_path);
    println!("  export ZELANA_VERIFYING_KEY={}", vk_path);
    println!("  export ZELANA_MOCK_PROVER=false");

    Ok(())
}

fn print_help() {
    println!("Zelana L2 ZK Key Generation Tool");
    println!();
    println!("USAGE:");
    println!("    keygen [OPTIONS]");
    println!();
    println!("OPTIONS:");
    println!("    --pk-out <PATH>    Path for proving key output (default: ./proving.key)");
    println!("    --vk-out <PATH>    Path for verifying key output (default: ./verifying.key)");
    println!("    --force, -f        Overwrite existing keys");
    println!("    --help, -h         Show this help message");
    println!();
    println!("EXAMPLES:");
    println!("    keygen --pk-out ./keys/proving.key --vk-out ./keys/verifying.key");
    println!("    keygen -f  # Force regeneration of keys");
}
