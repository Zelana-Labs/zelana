//! Quick test for ZelanaConfig loading
//!
//! # Config Access
//!
//! ```ignore
//! use zelana_config::{SOLANA, API, DATABASE, PIPELINE, BATCH, FEATURES};
//!
//! SOLANA.bridge_program      // Pubkey
//! API.sequencer_url          // &str
//! DATABASE.path              // &str
//! PIPELINE.prover_mode       // ProverModeToml
//! BATCH.max_transactions     // usize
//! FEATURES.dev_mode          // bool
//! ```

use zelana_config::{ZelanaConfig, API, BATCH, DATABASE, FEATURES, PIPELINE, SOLANA};

fn main() {
    println!("=== ZelanaConfig Test ===\n");

    // -------------------------------------------------------------------------
    // Explicit load - for startup validation with error handling
    // -------------------------------------------------------------------------
    println!("1. ZelanaConfig::load() - explicit with error handling\n");

    match ZelanaConfig::load() {
        Ok(config) => {
            println!("   rpc_url   = {}", config.solana.rpc_url);
            println!("   bridge_id = {}", config.solana.bridge_program_id);
            println!("   dev_mode  = {}", config.features.dev_mode);
        }
        Err(e) => {
            println!("   Failed: {}", e);
            return;
        }
    }

    // -------------------------------------------------------------------------
    // Lazy constants - all config sections as constants
    // -------------------------------------------------------------------------
    println!("\n2. Config constants (lazy-loaded)\n");

    println!("   SOLANA:");
    println!("     bridge_program   = {}", SOLANA.bridge_program);
    println!("     verifier_program = {}", SOLANA.verifier_program);
    println!("     rpc_url          = {}", SOLANA.rpc_url);
    println!("     ws_url           = {}", SOLANA.ws_url);
    println!("     domain           = {:?}", SOLANA.domain);

    println!("\n   API:");
    println!("     sequencer_url    = {}", API.sequencer_url);
    println!("     udp_port         = {:?}", API.udp_port);

    println!("\n   DATABASE:");
    println!("     path             = {}", DATABASE.path);

    println!("\n   PIPELINE:");
    println!("     prover_mode      = {:?}", PIPELINE.prover_mode);
    println!("     settlement_enabled = {}", PIPELINE.settlement_enabled);
    println!("     max_retries      = {}", PIPELINE.max_settlement_retries);

    println!("\n   BATCH:");
    println!("     max_transactions = {}", BATCH.max_transactions);
    println!("     max_batch_age    = {}s", BATCH.max_batch_age_secs);
    println!("     min_transactions = {}", BATCH.min_transactions);

    println!("\n   FEATURES:");
    println!("     dev_mode         = {}", FEATURES.dev_mode);
    println!("     fast_withdrawals = {}", FEATURES.fast_withdrawals);
    println!("     threshold_encryption = {}", FEATURES.threshold_encryption);
}
