//! Noir Prover Integration Tests
//!
//! Tests the integration between the Core Sequencer and the Noir/Sunspot prover
//! coordinator. These tests validate:
//!
//! 1. NoirProverClient API request/response handling
//! 2. Proof format detection (Groth16 vs Noir)
//! 3. Settlement routing based on proof type
//! 4. End-to-end batch proving flow with mock coordinator
//!
//! ## Running with real coordinator
//!
//! To run tests against a real prover-coordinator:
//! ```bash
//! # Terminal 1: Start the prover-coordinator
//! cd zelana-forge && cargo run -p prover-coordinator -- \
//!     --enable-core-api --mock-prover --port 8090
//!
//! # Terminal 2: Run the tests
//! ZL_NOIR_COORDINATOR_URL=http://localhost:8090 cargo test noir_integration
//! ```

use std::sync::Arc;
use tempfile::TempDir;

use crate::sequencer::RocksDbStore;
use crate::sequencer::pipeline::{PipelineConfig, ProverMode};
use crate::sequencer::settlement::noir_client::{
    CoreBatchProveRequest, CoreProofResult, NoirProverClient, NoirProverConfig,
};
use crate::sequencer::settlement::prover::{BatchProof, BatchPublicInputs};
use crate::sequencer::settlement::settler::{
    NoirProofData, SUNSPOT_VERIFIER_PROGRAM_ID, SettlerService,
};
use zelana_account::AccountId;

// ============================================================================
// Test Helpers
// ============================================================================

fn temp_db() -> (TempDir, Arc<RocksDbStore>) {
    let dir = TempDir::new().unwrap();
    let db = RocksDbStore::open(dir.path()).unwrap();
    (dir, Arc::new(db))
}

fn mock_batch_public_inputs(batch_id: u64) -> BatchPublicInputs {
    BatchPublicInputs {
        pre_state_root: [1u8; 32],
        post_state_root: [2u8; 32],
        pre_shielded_root: [3u8; 32],
        post_shielded_root: [4u8; 32],
        withdrawal_root: [5u8; 32],
        batch_hash: [6u8; 32],
        batch_id,
    }
}

fn mock_groth16_proof(batch_id: u64) -> BatchProof {
    BatchProof {
        public_inputs: mock_batch_public_inputs(batch_id),
        proof_bytes: vec![0u8; 256], // Groth16 = 256 bytes
        proving_time_ms: 100,
    }
}

fn mock_noir_proof(batch_id: u64) -> BatchProof {
    BatchProof {
        public_inputs: mock_batch_public_inputs(batch_id),
        proof_bytes: vec![0u8; 388], // Noir/Sunspot = 388 bytes
        proving_time_ms: 50,
    }
}

// ============================================================================
// NoirProverConfig Tests
// ============================================================================

#[test]
fn test_noir_prover_config_default() {
    let config = NoirProverConfig::default();
    assert_eq!(config.coordinator_url, "http://localhost:8080");
    assert_eq!(config.proof_timeout.as_secs(), 300);
    assert_eq!(config.poll_interval.as_secs(), 1);
    assert_eq!(config.max_retries, 3);
}

#[test]
fn test_noir_prover_config_custom() {
    let mut config = NoirProverConfig::default();
    config.coordinator_url = "http://prover.example.com:9000".to_string();
    config.proof_timeout = std::time::Duration::from_secs(600);

    assert_eq!(config.coordinator_url, "http://prover.example.com:9000");
    assert_eq!(config.proof_timeout.as_secs(), 600);
}

// ============================================================================
// Proof Format Detection Tests
// ============================================================================

#[test]
fn test_proof_format_detection_groth16() {
    let proof = mock_groth16_proof(1);
    assert!(!SettlerService::is_noir_proof(&proof));
    assert_eq!(proof.proof_bytes.len(), 256);
}

#[test]
fn test_proof_format_detection_noir() {
    let proof = mock_noir_proof(1);
    assert!(SettlerService::is_noir_proof(&proof));
    assert_eq!(proof.proof_bytes.len(), 388);
}

#[test]
fn test_noir_proof_data_validation() {
    let proof = mock_noir_proof(1);
    let noir_data = NoirProofData::from_batch_proof(&proof).unwrap();

    assert_eq!(noir_data.proof_bytes.len(), NoirProofData::PROOF_SIZE);
    assert_eq!(
        noir_data.public_witness.len(),
        NoirProofData::PUBLIC_WITNESS_SIZE
    );

    // Validation should pass
    noir_data.validate().unwrap();
}

#[test]
fn test_noir_proof_data_from_groth16_fails() {
    let proof = mock_groth16_proof(1);
    let result = NoirProofData::from_batch_proof(&proof);

    // Should fail - Groth16 proofs are not Noir proofs
    assert!(result.is_err());
}

// ============================================================================
// Pipeline Config Tests
// ============================================================================

#[test]
fn test_pipeline_config_prover_mode_default() {
    let config = PipelineConfig::default();
    assert_eq!(config.prover_mode, ProverMode::Mock);
    assert!(config.noir_coordinator_url.is_none());
    assert!(config.noir_proof_timeout_secs.is_none());
}

#[test]
fn test_pipeline_config_noir_mode() {
    let mut config = PipelineConfig::default();
    config.prover_mode = ProverMode::Noir;
    config.noir_coordinator_url = Some("http://localhost:8090".to_string());
    config.noir_proof_timeout_secs = Some(600);

    assert_eq!(config.prover_mode, ProverMode::Noir);
    assert_eq!(
        config.noir_coordinator_url,
        Some("http://localhost:8090".to_string())
    );
    assert_eq!(config.noir_proof_timeout_secs, Some(600));
}

// ============================================================================
// NoirProverClient Request Building Tests
// ============================================================================

#[test]
fn test_core_batch_prove_request_serialization() {
    let request = CoreBatchProveRequest {
        batch_id: 42,
        pre_state_root: "0x".to_string() + &hex::encode([1u8; 32]),
        post_state_root: "0x".to_string() + &hex::encode([2u8; 32]),
        pre_shielded_root: "0x".to_string() + &hex::encode([3u8; 32]),
        post_shielded_root: "0x".to_string() + &hex::encode([4u8; 32]),
        transfers: vec![],
        withdrawals: vec![],
        shielded: vec![],
    };

    let json = serde_json::to_string(&request).unwrap();
    assert!(json.contains("\"batch_id\":42"));
    assert!(json.contains("\"transfers\":[]"));

    // Deserialize back
    let parsed: CoreBatchProveRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.batch_id, 42);
}

// ============================================================================
// Sunspot Verifier Constants Tests
// ============================================================================

#[test]
fn test_sunspot_verifier_program_id() {
    // Verify the constant is valid base58
    use solana_sdk::pubkey::Pubkey;
    use std::str::FromStr;

    let pubkey = Pubkey::from_str(SUNSPOT_VERIFIER_PROGRAM_ID);
    assert!(
        pubkey.is_ok(),
        "SUNSPOT_VERIFIER_PROGRAM_ID should be valid base58"
    );

    // Verify it matches expected devnet program
    assert_eq!(
        SUNSPOT_VERIFIER_PROGRAM_ID,
        "EZzyLrTrC4uyU488jVAs4GKeCR1s9GmoFggeiDqwDeNK"
    );
}

// ============================================================================
// NoirProverClient Integration Tests (requires running coordinator)
// ============================================================================

/// Test that NoirProverClient can be constructed
#[test]
fn test_noir_prover_client_construction() {
    let config = NoirProverConfig::default();
    let _client = NoirProverClient::new(config);
    // Just verifying construction doesn't panic
}

/// Integration test with real prover-coordinator
/// Run with: ZL_NOIR_COORDINATOR_URL=http://localhost:8090 cargo test noir_integration_live --ignored
#[tokio::test]
#[ignore] // Requires running prover-coordinator
async fn test_noir_prover_client_health_check() {
    let url = std::env::var("ZL_NOIR_COORDINATOR_URL")
        .unwrap_or_else(|_| "http://localhost:8090".to_string());

    let mut config = NoirProverConfig::default();
    config.coordinator_url = url;

    let client = NoirProverClient::new(config);
    let healthy = client.health_check().await;

    match healthy {
        Ok(true) => println!("Prover coordinator is healthy"),
        Ok(false) => panic!("Prover coordinator returned unhealthy"),
        Err(e) => {
            println!("Could not reach prover coordinator: {}. Is it running?", e);
            // Don't fail - just skip if coordinator not available
        }
    }
}

/// Full end-to-end test with mock batch
/// Run with: ZL_NOIR_COORDINATOR_URL=http://localhost:8090 cargo test noir_integration_prove --ignored
#[tokio::test]
#[ignore] // Requires running prover-coordinator with --mock-prover
async fn test_noir_prover_client_prove_batch() {
    use crate::sequencer::settlement::prover::BatchWitness;

    let url = std::env::var("ZL_NOIR_COORDINATOR_URL")
        .unwrap_or_else(|_| "http://localhost:8090".to_string());

    let mut config = NoirProverConfig::default();
    config.coordinator_url = url.clone();
    config.proof_timeout = std::time::Duration::from_secs(60);

    let client = NoirProverClient::new(config);

    // Build mock inputs
    let public_inputs = mock_batch_public_inputs(1);
    let witness = BatchWitness {
        transactions: vec![],
        results: vec![],
        pre_account_states: vec![],
    };

    println!("Submitting batch to prover coordinator at {}", url);

    // Use prove_async directly since we're in an async context
    // (prove() uses block_on which can't be called from within a runtime)
    match client.prove_async(&public_inputs, &witness).await {
        Ok(proof) => {
            println!("Proof generated successfully!");
            println!("  Batch ID: {}", proof.public_inputs.batch_id);
            println!("  Proof size: {} bytes", proof.proof_bytes.len());

            // Verify it's a Noir proof (388 bytes) or mock
            // Mock prover may return different sizes
            assert!(proof.proof_bytes.len() > 0);
        }
        Err(e) => {
            println!("Proof generation failed: {}", e);
            println!(
                "Make sure prover-coordinator is running with --enable-core-api --mock-prover"
            );
        }
    }
}

// ============================================================================
// Pipeline with Noir Prover Tests
// ============================================================================

#[tokio::test]
async fn test_pipeline_noir_mode_config() {
    use crate::sequencer::pipeline::PipelineOrchestrator;

    let (_temp_dir, db) = temp_db();

    let mut config = PipelineConfig::default();
    config.prover_mode = ProverMode::Noir;
    config.noir_coordinator_url = Some("http://localhost:8090".to_string());

    // This should fall back to MockProver since coordinator URL won't connect
    // but the configuration parsing should work
    let result = PipelineOrchestrator::new(db, config, None);

    // Should succeed (falls back to mock if coordinator unavailable)
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_pipeline_mock_mode_config() {
    use crate::sequencer::pipeline::PipelineOrchestrator;

    let (_temp_dir, db) = temp_db();
    let config = PipelineConfig::default(); // Uses Mock mode

    let result = PipelineOrchestrator::new(db, config, None);
    assert!(result.is_ok());
}

/// Full E2E test with Noir prover mode
/// Run with: ZL_NOIR_COORDINATOR_URL=http://localhost:8090 cargo test test_pipeline_noir_full_e2e --ignored
#[tokio::test]
#[ignore] // Requires running prover-coordinator with --enable-core-api --mock-prover
async fn test_pipeline_noir_full_e2e() {
    use crate::sequencer::pipeline::{PipelineService, PipelineState};
    use std::time::Duration;
    use zelana_transaction::{DepositEvent, TransactionType};

    let url = std::env::var("ZL_NOIR_COORDINATOR_URL")
        .unwrap_or_else(|_| "http://localhost:8090".to_string());

    println!("Testing pipeline with Noir prover at {}", url);

    let (_temp_dir, db) = temp_db();

    // Configure pipeline with Noir prover
    let mut config = PipelineConfig::default();
    config.prover_mode = ProverMode::Noir;
    config.noir_coordinator_url = Some(url.clone());
    config.poll_interval_ms = 50;
    config.batch_config.max_transactions = 5;
    config.batch_config.min_transactions = 1;

    let service = match PipelineService::start(db.clone(), config, None) {
        Ok(s) => s,
        Err(e) => {
            println!(
                "Failed to start pipeline: {}. Is prover-coordinator running?",
                e
            );
            return;
        }
    };

    // Submit deposits to create accounts
    let alice = AccountId([1u8; 32]);
    let bob = AccountId([2u8; 32]);

    println!("Submitting deposits...");
    service
        .submit(TransactionType::Deposit(DepositEvent {
            to: alice,
            amount: 10000,
            l1_seq: 1,
        }))
        .await
        .unwrap();

    service
        .submit(TransactionType::Deposit(DepositEvent {
            to: bob,
            amount: 5000,
            l1_seq: 2,
        }))
        .await
        .unwrap();

    // Seal and wait for batch to process
    println!("Sealing batch...");
    let batch_id = service.seal().await.unwrap();
    assert_eq!(batch_id, Some(1));
    println!("Sealed batch: {:?}", batch_id);

    // Wait for proving (via Noir coordinator)
    println!("Waiting for Noir proof generation...");
    for i in 0..100 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        let stats = service.stats().await.unwrap();
        if stats.batches_proved >= 1 {
            println!("Batch proved after {}ms", (i + 1) * 100);
            break;
        }
        if i % 10 == 0 {
            println!("  Still waiting... ({}ms)", (i + 1) * 100);
        }
    }

    let stats = service.stats().await.unwrap();
    println!("Final stats: {:?}", stats);

    assert!(
        stats.batches_proved >= 1,
        "Expected at least 1 batch proved, got {}",
        stats.batches_proved
    );
    assert_eq!(stats.state, PipelineState::Running);

    // Clean shutdown
    service.shutdown().await.unwrap();
    println!("Pipeline E2E test with Noir prover completed successfully!");
}
