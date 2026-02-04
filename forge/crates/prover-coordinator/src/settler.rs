//! Solana Settlement Module
//!
//! Submits batch proofs to Solana for on-chain verification.
//!
//! This module supports two modes:
//! 1. Mock mode - For testing without Solana
//! 2. Real mode - Uses the SolanaVerifierClient to submit proofs on-chain

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{error, info, warn};

use crate::dispatcher::{BatchProofs, ChunkProof};
use crate::solana_client::{
    ProofData, SolanaClientError, SolanaVerifierClient, SolanaVerifierConfig,
};

// Types

/// Settlement configuration
#[derive(Debug, Clone)]
pub struct SettlerConfig {
    /// Solana RPC URL
    pub rpc_url: String,
    /// Verifier program ID
    pub program_id: String,
    /// Path to keypair file (payer)
    pub keypair_path: Option<String>,
    /// Path to circuit target directory (for proof files)
    pub circuit_target_path: Option<PathBuf>,
    /// Compute units to request
    pub compute_units: u32,
}

impl Default for SettlerConfig {
    fn default() -> Self {
        Self {
            rpc_url: "https://api.devnet.solana.com".to_string(),
            program_id: "EZzyLrTrC4uyU488jVAs4GKeCR1s9GmoFggeiDqwDeNK".to_string(),
            keypair_path: None,
            circuit_target_path: None,
            compute_units: 500_000,
        }
    }
}

/// Settlement result for a single proof
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProofSettlement {
    pub chunk_id: u32,
    pub tx_signature: String,
    pub verified: bool,
    pub compute_units_consumed: Option<u64>,
}

/// Settlement result for the entire batch
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchSettlement {
    pub batch_id: String,
    /// Individual proof settlements
    pub settlements: Vec<ProofSettlement>,
    /// Single batched transaction signature (if using batched mode)
    pub batched_tx_signature: Option<String>,
    /// Total settlement time in ms
    pub settlement_time_ms: u64,
    /// All proofs verified successfully
    pub all_verified: bool,
}

/// Settlement mode
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SettlementMode {
    /// Submit each proof as a separate transaction
    Sequential,
    /// Submit all proofs in a single transaction (batched verification)
    Batched,
}

// Real Settler (using Solana)

/// Solana proof settler using real on-chain verification
pub struct Settler {
    config: SettlerConfig,
    mode: SettlementMode,
    client: Option<Arc<SolanaVerifierClient>>,
}

impl Settler {
    /// Create a new settler with lazy client initialization
    pub fn new(config: SettlerConfig, mode: SettlementMode) -> Self {
        Self {
            config,
            mode,
            client: None,
        }
    }

    /// Initialize the Solana client (lazy initialization)
    fn get_or_init_client(&mut self) -> Result<&SolanaVerifierClient, String> {
        if self.client.is_none() {
            let solana_config = SolanaVerifierConfig {
                rpc_url: self.config.rpc_url.clone(),
                program_id: self.config.program_id.clone(),
                keypair_path: self.config.keypair_path.clone(),
                compute_units: self.config.compute_units,
                priority_fee_micro_lamports: 1000,
            };

            match SolanaVerifierClient::new(solana_config) {
                Ok(client) => {
                    self.client = Some(Arc::new(client));
                }
                Err(e) => {
                    return Err(format!("Failed to initialize Solana client: {}", e));
                }
            }
        }

        Ok(self.client.as_ref().unwrap())
    }

    /// Submit a single proof to Solana
    async fn submit_proof(&mut self, proof: &ChunkProof) -> Result<ProofSettlement, String> {
        info!(
            "Submitting proof for chunk {} to Solana (program: {})",
            proof.chunk_id,
            &self.config.program_id[..16.min(self.config.program_id.len())]
        );

        // Parse proof data from the hex-encoded proof
        // The proof in ChunkProof is hex-encoded, but we need raw bytes
        let proof_bytes =
            hex::decode(&proof.proof).map_err(|e| format!("Invalid proof hex: {}", e))?;

        // For real verification, we need the public witness file
        // The proof bytes should be 388 bytes (proof) + 236 bytes (public witness)
        // or we need to get the public witness separately

        // If the proof contains both (624 bytes hex = 312 bytes), split them
        // Otherwise, try to load from circuit target directory
        let (proof_data, pw_data) = if proof_bytes.len() >= 624 {
            // Combined proof + public witness
            (proof_bytes[..388].to_vec(), proof_bytes[388..624].to_vec())
        } else if proof_bytes.len() == 388 {
            // Just the proof - need to load public witness from file
            if let Some(target_path) = &self.config.circuit_target_path {
                let pw_path = target_path.join("zelana_batch.pw");
                let pw_data = std::fs::read(&pw_path)
                    .map_err(|e| format!("Failed to read public witness: {}", e))?;
                (proof_bytes, pw_data)
            } else {
                return Err(
                    "Proof is 388 bytes but no circuit_target_path configured for public witness"
                        .to_string(),
                );
            }
        } else {
            return Err(format!(
                "Invalid proof size: expected 388 or 624 bytes, got {}",
                proof_bytes.len()
            ));
        };

        // Validate sizes
        let proof_data_obj = ProofData::new(proof_data.clone(), pw_data.clone());
        proof_data_obj.validate().map_err(|e| e)?;

        // Get client
        let client = self.get_or_init_client()?;

        // Submit to Solana
        match client.verify_proof(&proof_data, &pw_data).await {
            Ok(result) => {
                info!(
                    "Chunk {} verified on Solana: {} (CU: {:?})",
                    proof.chunk_id, result.signature, result.compute_units_consumed
                );
                Ok(ProofSettlement {
                    chunk_id: proof.chunk_id,
                    tx_signature: result.signature.to_string(),
                    verified: result.verified,
                    compute_units_consumed: result.compute_units_consumed,
                })
            }
            Err(e) => {
                error!("Failed to verify chunk {}: {}", proof.chunk_id, e);
                Err(format!("Verification failed: {}", e))
            }
        }
    }

    /// Submit all proofs in a single batched transaction
    /// Note: Current Solana verifier only supports one proof per transaction
    /// This submits them sequentially but aggregates results
    async fn submit_batched(&mut self, proofs: &[ChunkProof]) -> Result<BatchSettlement, String> {
        let start = std::time::Instant::now();

        info!(
            "Submitting {} proofs in batched mode to Solana",
            proofs.len()
        );

        // For now, we submit each proof separately since the verifier
        // doesn't support multiple proofs per instruction
        // In a future version, we could use a batch verifier program
        let mut settlements = Vec::new();
        let mut first_signature: Option<String> = None;

        for proof in proofs {
            match self.submit_proof(proof).await {
                Ok(settlement) => {
                    if first_signature.is_none() {
                        first_signature = Some(settlement.tx_signature.clone());
                    }
                    settlements.push(settlement);
                }
                Err(e) => {
                    error!("Failed to settle chunk {}: {}", proof.chunk_id, e);
                    settlements.push(ProofSettlement {
                        chunk_id: proof.chunk_id,
                        tx_signature: String::new(),
                        verified: false,
                        compute_units_consumed: None,
                    });
                }
            }
        }

        let settlement_time_ms = start.elapsed().as_millis() as u64;
        let all_verified = settlements.iter().all(|s| s.verified);

        info!(
            "Batched settlement complete: {} proofs in {}ms, all_verified: {}",
            proofs.len(),
            settlement_time_ms,
            all_verified
        );

        Ok(BatchSettlement {
            batch_id: String::new(), // Will be filled by caller
            settlements,
            batched_tx_signature: first_signature,
            settlement_time_ms,
            all_verified,
        })
    }

    /// Submit all proofs sequentially
    async fn submit_sequential(
        &mut self,
        proofs: &[ChunkProof],
    ) -> Result<BatchSettlement, String> {
        let start = std::time::Instant::now();
        let mut settlements = Vec::new();

        for proof in proofs {
            match self.submit_proof(proof).await {
                Ok(settlement) => settlements.push(settlement),
                Err(e) => {
                    error!("Failed to settle chunk {}: {}", proof.chunk_id, e);
                    settlements.push(ProofSettlement {
                        chunk_id: proof.chunk_id,
                        tx_signature: String::new(),
                        verified: false,
                        compute_units_consumed: None,
                    });
                }
            }
        }

        let settlement_time_ms = start.elapsed().as_millis() as u64;
        let all_verified = settlements.iter().all(|s| s.verified);

        Ok(BatchSettlement {
            batch_id: String::new(),
            settlements,
            batched_tx_signature: None,
            settlement_time_ms,
            all_verified,
        })
    }

    /// Settle a batch of proofs
    pub async fn settle_batch(
        &mut self,
        batch_proofs: &BatchProofs,
    ) -> Result<BatchSettlement, String> {
        info!(
            "Settling batch {} ({} proofs, mode: {:?})",
            batch_proofs.batch_id,
            batch_proofs.proofs.len(),
            self.mode
        );

        let mut result = match self.mode {
            SettlementMode::Batched => self.submit_batched(&batch_proofs.proofs).await?,
            SettlementMode::Sequential => self.submit_sequential(&batch_proofs.proofs).await?,
        };

        result.batch_id = batch_proofs.batch_id.clone();

        if result.all_verified {
            info!(
                "Batch {} fully settled on Solana in {}ms",
                result.batch_id, result.settlement_time_ms
            );
        } else {
            warn!(
                "Batch {} settlement incomplete: {}/{} verified",
                result.batch_id,
                result.settlements.iter().filter(|s| s.verified).count(),
                result.settlements.len()
            );
        }

        Ok(result)
    }
}

// File-based Settler (uses pre-generated proof files)

/// Settler that uses pre-generated proof files from circuit target directory
pub struct FileBasedSettler {
    config: SettlerConfig,
    client: Option<Arc<SolanaVerifierClient>>,
}

impl FileBasedSettler {
    pub fn new(config: SettlerConfig) -> Self {
        Self {
            config,
            client: None,
        }
    }

    /// Initialize the Solana client
    fn init_client(&mut self) -> Result<(), String> {
        if self.client.is_none() {
            let solana_config = SolanaVerifierConfig {
                rpc_url: self.config.rpc_url.clone(),
                program_id: self.config.program_id.clone(),
                keypair_path: self.config.keypair_path.clone(),
                compute_units: self.config.compute_units,
                priority_fee_micro_lamports: 1000,
            };

            match SolanaVerifierClient::new(solana_config) {
                Ok(client) => {
                    self.client = Some(Arc::new(client));
                }
                Err(e) => {
                    return Err(format!("Failed to initialize Solana client: {}", e));
                }
            }
        }
        Ok(())
    }

    /// Verify using proof files from the circuit target directory
    pub async fn verify_from_circuit_output(&mut self) -> Result<ProofSettlement, String> {
        self.init_client()?;

        let target_path = self
            .config
            .circuit_target_path
            .as_ref()
            .ok_or("No circuit_target_path configured")?;

        let proof_path = target_path.join("zelana_batch.proof");
        let pw_path = target_path.join("zelana_batch.pw");

        if !proof_path.exists() {
            return Err(format!("Proof file not found: {:?}", proof_path));
        }
        if !pw_path.exists() {
            return Err(format!("Public witness file not found: {:?}", pw_path));
        }

        info!("Loading proof from {:?}", proof_path);
        let proof_bytes =
            std::fs::read(&proof_path).map_err(|e| format!("Failed to read proof: {}", e))?;
        let pw_bytes =
            std::fs::read(&pw_path).map_err(|e| format!("Failed to read public witness: {}", e))?;

        let client = self.client.as_ref().unwrap();
        match client.verify_proof(&proof_bytes, &pw_bytes).await {
            Ok(result) => {
                info!(
                    "Proof verified on Solana: {} (CU: {:?})",
                    result.signature, result.compute_units_consumed
                );
                Ok(ProofSettlement {
                    chunk_id: 0,
                    tx_signature: result.signature.to_string(),
                    verified: result.verified,
                    compute_units_consumed: result.compute_units_consumed,
                })
            }
            Err(e) => {
                error!("Verification failed: {}", e);
                Err(format!("Verification failed: {}", e))
            }
        }
    }
}

// Mock Settler for Testing

/// Mock settler that simulates Solana settlement
pub struct MockSettler {
    delay_ms: u64,
    /// Optional: path to real proof files (for hybrid testing)
    proof_files_path: Option<PathBuf>,
}

impl MockSettler {
    pub fn new(delay_ms: u64) -> Self {
        Self {
            delay_ms,
            proof_files_path: None,
        }
    }

    /// Create mock settler that returns real proof signatures (for demo)
    pub fn with_proof_files(delay_ms: u64, path: PathBuf) -> Self {
        Self {
            delay_ms,
            proof_files_path: Some(path),
        }
    }

    pub async fn settle_batch(
        &self,
        batch_proofs: &BatchProofs,
    ) -> Result<BatchSettlement, String> {
        let start = std::time::Instant::now();

        info!(
            "Mock settling batch {} ({} proofs)",
            batch_proofs.batch_id,
            batch_proofs.proofs.len()
        );

        // Simulate settlement delay
        tokio::time::sleep(tokio::time::Duration::from_millis(self.delay_ms)).await;

        let tx_signature = format!("mock_batch_{:016x}", rand::random::<u64>());
        let settlements: Vec<_> = batch_proofs
            .proofs
            .iter()
            .map(|p| ProofSettlement {
                chunk_id: p.chunk_id,
                tx_signature: tx_signature.clone(),
                verified: true,
                compute_units_consumed: Some(200_000), // Mock value
            })
            .collect();

        Ok(BatchSettlement {
            batch_id: batch_proofs.batch_id.clone(),
            settlements,
            batched_tx_signature: Some(tx_signature),
            settlement_time_ms: start.elapsed().as_millis() as u64,
            all_verified: true,
        })
    }
}

// Tests

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_settler() {
        let settler = MockSettler::new(10);

        let batch_proofs = BatchProofs {
            batch_id: "test-batch".to_string(),
            proofs: vec![
                ChunkProof {
                    chunk_id: 0,
                    worker_id: 1,
                    proof: "deadbeef".to_string(),
                    public_inputs: vec!["0x1".to_string(), "0x2".to_string()],
                    proving_time_ms: 100,
                },
                ChunkProof {
                    chunk_id: 1,
                    worker_id: 2,
                    proof: "cafebabe".to_string(),
                    public_inputs: vec!["0x2".to_string(), "0x3".to_string()],
                    proving_time_ms: 150,
                },
            ],
            total_time_ms: 200,
            workers_used: 2,
        };

        let result = settler.settle_batch(&batch_proofs).await.unwrap();

        assert_eq!(result.batch_id, "test-batch");
        assert_eq!(result.settlements.len(), 2);
        assert!(result.all_verified);
        assert!(result.batched_tx_signature.is_some());
    }

    #[test]
    fn test_default_config() {
        let config = SettlerConfig::default();
        assert_eq!(config.rpc_url, "https://api.devnet.solana.com");
        assert_eq!(
            config.program_id,
            "EZzyLrTrC4uyU488jVAs4GKeCR1s9GmoFggeiDqwDeNK"
        );
    }
}
