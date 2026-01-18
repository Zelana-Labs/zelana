//! Zelana L2 Prover (Standalone Service)
//!
//! NOTE: This standalone prover is deprecated for MVP.
//! Use the integrated prover in zelana-core instead.
//!
//! For key generation, use:
//!   cargo run --package prover --bin keygen
//!
//! The main sequencer (zelana-core) includes an integrated Groth16 prover
//! that generates real ZK proofs when ZL_MOCK_PROVER=false.

fn main() {
    println!("Zelana L2 Prover");
    println!("================");
    println!();
    println!("This standalone prover service is deprecated.");
    println!();
    println!("For MVP, use the integrated prover in zelana-core:");
    println!("  export ZL_MOCK_PROVER=false");
    println!("  export ZL_PROVING_KEY=./keys/proving.key");
    println!("  export ZL_VERIFYING_KEY=./keys/verifying.key");
    println!("  cargo run --package zelana-core");
    println!();
    println!("To generate proving/verifying keys:");
    println!("  cargo run --package prover --bin keygen -- \\");
    println!("    --pk-out ./keys/proving.key \\");
    println!("    --vk-out ./keys/verifying.key");
}
