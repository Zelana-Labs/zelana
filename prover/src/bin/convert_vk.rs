//! Convert Arkworks Verifying Key to Solana Format
//!
//! This binary converts an arkworks BN254 verifying key to the format
//! needed for on-chain storage in the Zelana verifier program.
//!
//! Usage:
//!   cargo run --package prover --bin convert_vk -- \
//!     --vk-in ./keys/verifying.key \
//!     --vk-out ./keys/batch_vk.json
//!
//! The output JSON can be used with the store_vk script.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use ark_bn254::Bn254;
use ark_ec::AffineRepr;
use ark_ff::{BigInteger, PrimeField};
use ark_groth16::VerifyingKey;
use ark_serialize::CanonicalDeserialize;
use serde::Serialize;

/// JSON output format for VK
#[derive(Serialize)]
struct VkJson {
    /// Alpha G1 point (64 bytes, little-endian)
    alpha_g1: Vec<u8>,
    /// Beta G2 point (128 bytes, little-endian)
    beta_g2: Vec<u8>,
    /// Gamma G2 point (128 bytes, little-endian)
    gamma_g2: Vec<u8>,
    /// Delta G2 point (128 bytes, little-endian)  
    delta_g2: Vec<u8>,
    /// IC points (each 64 bytes, little-endian)
    ic: Vec<Vec<u8>>,
    /// Number of public inputs (IC count - 1)
    num_public_inputs: usize,
    /// VK hash for reference
    vk_hash: String,
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();

    // Parse arguments
    let mut vk_in_path = String::from("./keys/verifying.key");
    let mut vk_out_path = String::from("./keys/batch_vk.json");

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--vk-in" => {
                i += 1;
                if i < args.len() {
                    vk_in_path = args[i].clone();
                }
            }
            "--vk-out" => {
                i += 1;
                if i < args.len() {
                    vk_out_path = args[i].clone();
                }
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

    println!("Zelana VK Conversion Tool");
    println!("=========================");
    println!();

    // Check input exists
    if !Path::new(&vk_in_path).exists() {
        eprintln!("Error: Verifying key not found at {}", vk_in_path);
        eprintln!();
        eprintln!("Generate keys first with:");
        eprintln!("  cargo run --package prover --bin keygen");
        std::process::exit(1);
    }

    // Load verifying key
    println!("Loading verifying key from {}...", vk_in_path);
    let vk_bytes = fs::read(&vk_in_path).context("Failed to read verifying key")?;
    let vk = VerifyingKey::<Bn254>::deserialize_compressed(&*vk_bytes)
        .context("Failed to deserialize verifying key")?;

    println!(
        "  IC points: {} (for {} public inputs)",
        vk.gamma_abc_g1.len(),
        vk.gamma_abc_g1.len() - 1
    );

    // Convert to Solana format
    println!("Converting to Solana format...");

    // Alpha G1 (64 bytes)
    let alpha_g1 = g1_to_bytes(&vk.alpha_g1);
    println!("  alpha_g1: {} bytes", alpha_g1.len());

    // Beta G2 (128 bytes)
    let beta_g2 = g2_to_bytes(&vk.beta_g2);
    println!("  beta_g2: {} bytes", beta_g2.len());

    // Gamma G2 (128 bytes)
    let gamma_g2 = g2_to_bytes(&vk.gamma_g2);
    println!("  gamma_g2: {} bytes", gamma_g2.len());

    // Delta G2 (128 bytes)
    let delta_g2 = g2_to_bytes(&vk.delta_g2);
    println!("  delta_g2: {} bytes", delta_g2.len());

    // IC points (each 64 bytes)
    let ic: Vec<Vec<u8>> = vk.gamma_abc_g1.iter().map(|p| g1_to_bytes(p)).collect();
    println!("  ic: {} points x 64 bytes", ic.len());

    // Compute VK hash
    let vk_hash = blake3::hash(&vk_bytes);

    // Build JSON
    let vk_json = VkJson {
        alpha_g1,
        beta_g2,
        gamma_g2,
        delta_g2,
        ic,
        num_public_inputs: vk.gamma_abc_g1.len() - 1,
        vk_hash: hex::encode(vk_hash.as_bytes()),
    };

    // Write output
    println!("Writing output to {}...", vk_out_path);

    if let Some(parent) = Path::new(&vk_out_path).parent() {
        fs::create_dir_all(parent).context("Failed to create output directory")?;
    }

    let json = serde_json::to_string_pretty(&vk_json)?;
    fs::write(&vk_out_path, &json).context("Failed to write output")?;

    println!();
    println!("Conversion complete!");
    println!("  Output: {}", vk_out_path);
    println!("  VK hash: {}", hex::encode(vk_hash.as_bytes()));
    println!();
    println!("To store on-chain:");
    println!(
        "  cargo run --package zelana-scripts --bin store_vk -- --vk-file {}",
        vk_out_path
    );

    Ok(())
}

/// Convert G1 affine point to 64 bytes (little-endian)
fn g1_to_bytes(p: &ark_bn254::G1Affine) -> Vec<u8> {
    let mut bytes = vec![0u8; 64];
    if !p.is_zero() {
        let x_bytes = p.x.into_bigint().to_bytes_le();
        let y_bytes = p.y.into_bigint().to_bytes_le();
        bytes[..32].copy_from_slice(&x_bytes[..32.min(x_bytes.len())]);
        bytes[32..64].copy_from_slice(&y_bytes[..32.min(y_bytes.len())]);
    }
    bytes
}

/// Convert G2 affine point to 128 bytes (little-endian)
fn g2_to_bytes(p: &ark_bn254::G2Affine) -> Vec<u8> {
    let mut bytes = vec![0u8; 128];
    if !p.is_zero() {
        // G2 point has Fq2 coordinates: (x.c0, x.c1, y.c0, y.c1)
        let x_c0_bytes = p.x.c0.into_bigint().to_bytes_le();
        let x_c1_bytes = p.x.c1.into_bigint().to_bytes_le();
        let y_c0_bytes = p.y.c0.into_bigint().to_bytes_le();
        let y_c1_bytes = p.y.c1.into_bigint().to_bytes_le();

        bytes[0..32].copy_from_slice(&x_c0_bytes[..32.min(x_c0_bytes.len())]);
        bytes[32..64].copy_from_slice(&x_c1_bytes[..32.min(x_c1_bytes.len())]);
        bytes[64..96].copy_from_slice(&y_c0_bytes[..32.min(y_c0_bytes.len())]);
        bytes[96..128].copy_from_slice(&y_c1_bytes[..32.min(y_c1_bytes.len())]);
    }
    bytes
}

fn print_help() {
    println!("Zelana VK Conversion Tool");
    println!();
    println!("Converts an arkworks BN254 verifying key to the format needed");
    println!("for on-chain storage in the Zelana verifier program.");
    println!();
    println!("USAGE:");
    println!("    convert_vk [OPTIONS]");
    println!();
    println!("OPTIONS:");
    println!(
        "    --vk-in <PATH>     Path to arkworks verifying key (default: ./keys/verifying.key)"
    );
    println!("    --vk-out <PATH>    Path for JSON output (default: ./keys/batch_vk.json)");
    println!("    --help, -h         Show this help message");
    println!();
    println!("EXAMPLE:");
    println!("    convert_vk --vk-in ./verifying.key --vk-out ./batch_vk.json");
}
