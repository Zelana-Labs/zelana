//! Noir Prover Client
//!
//! HTTP client for communicating with the Noir/Sunspot prover coordinator.
//! Implements BatchProver trait for integration with the sequencer pipeline.
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────────┐
//! │                         Core Sequencer                                   │
//! │                                                                          │
//! │   Pipeline → NoirProverClient::prove_async()                            │
//! │                     │                                                    │
//! │                     │ HTTP POST /v2/batch/prove                         │
//! │                     ▼                                                    │
//! └─────────────────────────────────────────────────────────────────────────┘
//!                       │
//!                       ▼
//! ┌─────────────────────────────────────────────────────────────────────────┐
//! │                    Prover Coordinator                                    │
//! │   (zelana-forge/crates/prover-coordinator)                              │
//! │                                                                          │
//! │   POST /v2/batch/prove → job_id                                         │
//! │   GET  /v2/batch/{job_id}/status → SSE stream                           │
//! │   GET  /v2/batch/{job_id}/proof → CoreProofResult                       │
//! └─────────────────────────────────────────────────────────────────────────┘
//! ```

use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::time::timeout;
use tracing::{debug, info, warn};

use super::prover::{
    AccountStateSnapshot, BatchProof, BatchProver, BatchPublicInputs, BatchWitness,
};
use zelana_transaction::TransactionType;

// ============================================================================
// Configuration
// ============================================================================

/// Configuration for the Noir prover client
#[derive(Debug, Clone)]
pub struct NoirProverConfig {
    /// Base URL of the prover coordinator (e.g., "http://localhost:8080")
    pub coordinator_url: String,
    /// Timeout for proof generation (default: 5 minutes)
    pub proof_timeout: Duration,
    /// Polling interval for status checks (default: 1 second)
    pub poll_interval: Duration,
    /// Maximum retries on transient errors
    pub max_retries: u32,
}

impl Default for NoirProverConfig {
    fn default() -> Self {
        Self {
            coordinator_url: "http://localhost:8080".to_string(),
            proof_timeout: Duration::from_secs(300), // 5 minutes
            poll_interval: Duration::from_secs(1),
            max_retries: 3,
        }
    }
}

// ============================================================================
// API Types (matching prover-coordinator/src/core_api.rs)
// ============================================================================

/// Request to prove a batch
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoreBatchProveRequest {
    pub batch_id: u64,
    pub pre_state_root: String,
    pub post_state_root: String,
    pub pre_shielded_root: String,
    pub post_shielded_root: String,
    #[serde(default)]
    pub transfers: Vec<CoreTransferWitness>,
    #[serde(default)]
    pub withdrawals: Vec<CoreWithdrawalWitness>,
    #[serde(default)]
    pub shielded: Vec<CoreShieldedWitness>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoreTransferWitness {
    pub sender_pubkey: String,
    pub sender_balance: u64,
    pub sender_nonce: u64,
    pub sender_merkle_path: Vec<String>,
    pub sender_path_indices: Vec<u8>,
    pub receiver_pubkey: String,
    pub receiver_balance: u64,
    pub receiver_nonce: u64,
    pub receiver_merkle_path: Vec<String>,
    pub receiver_path_indices: Vec<u8>,
    pub amount: u64,
    pub signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoreWithdrawalWitness {
    pub sender_pubkey: String,
    pub sender_balance: u64,
    pub sender_nonce: u64,
    pub sender_merkle_path: Vec<String>,
    pub sender_path_indices: Vec<u8>,
    pub l1_recipient: String,
    pub amount: u64,
    pub signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoreShieldedWitness {
    pub input_commitment: String,
    pub input_value: u64,
    pub input_blinding: String,
    pub input_position: u64,
    pub input_merkle_path: Vec<String>,
    pub input_path_indices: Vec<u8>,
    pub spending_key: String,
    pub output_owner: String,
    pub output_value: u64,
    pub output_blinding: String,
    pub nullifier: String,
}

/// Response after submitting a batch for proving
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoreBatchProveResponse {
    pub job_id: String,
    pub batch_id: u64,
    pub estimated_time_ms: u64,
    pub status_url: String,
}

/// Proof result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoreProofResult {
    pub job_id: String,
    pub batch_id: u64,
    /// Hex-encoded proof bytes (388 bytes)
    pub proof_bytes: String,
    /// Hex-encoded public witness bytes (236 bytes)
    pub public_witness_bytes: String,
    pub batch_hash: String,
    pub withdrawal_root: String,
    pub proving_time_ms: u64,
}

/// API response wrapper
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "lowercase")]
pub enum ApiResponse<T> {
    Success {
        data: T,
    },
    Error {
        message: String,
        code: Option<String>,
    },
}

/// Job status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProofJobStatus {
    pub job_id: String,
    pub batch_id: u64,
    pub state: ProofJobState,
    pub progress_pct: u8,
    pub message: String,
    pub created_at: u64,
    pub updated_at: u64,
    pub completed_at: Option<u64>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProofJobState {
    Pending,
    Preparing,
    Proving,
    Completed,
    Failed,
    Cancelled,
}

// ============================================================================
// Noir Prover Client
// ============================================================================

/// Client for the Noir/Sunspot prover coordinator
pub struct NoirProverClient {
    config: NoirProverConfig,
    client: reqwest::Client,
    /// Cached verification key hash (from coordinator)
    vk_hash: [u8; 32],
}

impl NoirProverClient {
    /// Create a new client with the given configuration
    pub fn new(config: NoirProverConfig) -> Self {
        let client = reqwest::Client::builder()
            .timeout(config.proof_timeout)
            .build()
            .expect("Failed to create HTTP client");

        // VK hash for the Sunspot verifier on devnet
        // This should match the deployed verifier program
        let vk_hash = *blake3::hash(b"sunspot-zelana-batch-vk").as_bytes();

        Self {
            config,
            client,
            vk_hash,
        }
    }

    /// Create with custom verification key hash
    pub fn with_vk_hash(mut self, vk_hash: [u8; 32]) -> Self {
        self.vk_hash = vk_hash;
        self
    }

    /// Check if the prover coordinator is healthy
    pub async fn health_check(&self) -> Result<bool> {
        let url = format!("{}/v2/health", self.config.coordinator_url);

        match self.client.get(&url).send().await {
            Ok(response) => Ok(response.status().is_success()),
            Err(e) => {
                warn!("Prover coordinator health check failed: {}", e);
                Ok(false)
            }
        }
    }

    /// Submit a batch for proving and wait for the result
    pub async fn prove_async(
        &self,
        inputs: &BatchPublicInputs,
        witness: &BatchWitness,
    ) -> Result<BatchProof> {
        let start = std::time::Instant::now();

        // Convert to API request format
        let request = self.build_request(inputs, witness);

        info!(
            "Submitting batch {} to prover coordinator (transfers={}, withdrawals={}, shielded={})",
            inputs.batch_id,
            request.transfers.len(),
            request.withdrawals.len(),
            request.shielded.len()
        );

        // Submit proof request
        let job_response = self.submit_proof_request(&request).await?;
        info!(
            "Proof job created: {} (estimated {}ms)",
            job_response.job_id, job_response.estimated_time_ms
        );

        // Poll for completion
        let result = self
            .poll_for_completion(&job_response.job_id, inputs.batch_id)
            .await?;

        let elapsed = start.elapsed();
        info!(
            "Proof completed for batch {} in {:?} (proving time: {}ms)",
            inputs.batch_id, elapsed, result.proving_time_ms
        );

        // Convert result to BatchProof
        self.convert_result(inputs, result)
    }

    /// Build the API request from batch inputs and witness
    fn build_request(
        &self,
        inputs: &BatchPublicInputs,
        witness: &BatchWitness,
    ) -> CoreBatchProveRequest {
        let mut transfers = Vec::new();
        let mut withdrawals = Vec::new();
        let mut shielded = Vec::new();

        // Build account lookup for merkle paths
        let account_lookup: std::collections::HashMap<[u8; 32], &AccountStateSnapshot> = witness
            .pre_account_states
            .iter()
            .map(|s| (s.account_id, s))
            .collect();

        for tx in &witness.transactions {
            match tx {
                TransactionType::Transfer(t) => {
                    let sender_state = account_lookup.get(&t.signer_pubkey);
                    let receiver_state = account_lookup.get(&t.data.to.0);

                    transfers.push(CoreTransferWitness {
                        sender_pubkey: hex::encode(t.signer_pubkey),
                        sender_balance: sender_state.map(|s| s.balance).unwrap_or(0),
                        sender_nonce: sender_state.map(|s| s.nonce).unwrap_or(0),
                        sender_merkle_path: sender_state
                            .map(|s| s.merkle_proof.iter().map(|h| hex::encode(h)).collect())
                            .unwrap_or_default(),
                        sender_path_indices: vec![], // TODO: compute from position
                        receiver_pubkey: hex::encode(t.data.to.0),
                        receiver_balance: receiver_state.map(|s| s.balance).unwrap_or(0),
                        receiver_nonce: receiver_state.map(|s| s.nonce).unwrap_or(0),
                        receiver_merkle_path: receiver_state
                            .map(|s| s.merkle_proof.iter().map(|h| hex::encode(h)).collect())
                            .unwrap_or_default(),
                        receiver_path_indices: vec![],
                        amount: t.data.amount,
                        signature: hex::encode(&t.signature),
                    });
                }
                TransactionType::Withdraw(w) => {
                    let sender_state = account_lookup.get(&w.from.0);

                    withdrawals.push(CoreWithdrawalWitness {
                        sender_pubkey: hex::encode(w.from.0),
                        sender_balance: sender_state.map(|s| s.balance).unwrap_or(0),
                        sender_nonce: sender_state.map(|s| s.nonce).unwrap_or(0),
                        sender_merkle_path: sender_state
                            .map(|s| s.merkle_proof.iter().map(|h| hex::encode(h)).collect())
                            .unwrap_or_default(),
                        sender_path_indices: vec![],
                        l1_recipient: hex::encode(w.to_l1_address),
                        amount: w.amount,
                        signature: hex::encode(&w.signature),
                    });
                }
                TransactionType::Shielded(p) => {
                    // Shielded transactions have different structure
                    shielded.push(CoreShieldedWitness {
                        input_commitment: hex::encode(p.nullifier), // Note: simplified mapping
                        input_value: 0,                             // Private
                        input_blinding: "0".to_string(),
                        input_position: 0,
                        input_merkle_path: vec![],
                        input_path_indices: vec![],
                        spending_key: "0".to_string(), // Private
                        output_owner: hex::encode(p.commitment),
                        output_value: 0, // Private
                        output_blinding: "0".to_string(),
                        nullifier: hex::encode(p.nullifier),
                    });
                }
                TransactionType::Deposit(_) => {
                    // Deposits don't need ZK proofs, they're validated on L1
                }
            }
        }

        CoreBatchProveRequest {
            batch_id: inputs.batch_id,
            pre_state_root: hex::encode(inputs.pre_state_root),
            post_state_root: hex::encode(inputs.post_state_root),
            pre_shielded_root: hex::encode(inputs.pre_shielded_root),
            post_shielded_root: hex::encode(inputs.post_shielded_root),
            transfers,
            withdrawals,
            shielded,
        }
    }

    /// Submit the proof request to the coordinator
    async fn submit_proof_request(
        &self,
        request: &CoreBatchProveRequest,
    ) -> Result<CoreBatchProveResponse> {
        let url = format!("{}/v2/batch/prove", self.config.coordinator_url);

        let response = self
            .client
            .post(&url)
            .json(request)
            .send()
            .await
            .context("Failed to connect to prover coordinator")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow!("Prover coordinator returned {}: {}", status, body));
        }

        let api_response: ApiResponse<CoreBatchProveResponse> = response
            .json()
            .await
            .context("Failed to parse prover response")?;

        match api_response {
            ApiResponse::Success { data } => Ok(data),
            ApiResponse::Error { message, code } => Err(anyhow!(
                "Prover error ({}): {}",
                code.unwrap_or_else(|| "unknown".to_string()),
                message
            )),
        }
    }

    /// Poll for proof completion
    async fn poll_for_completion(&self, job_id: &str, batch_id: u64) -> Result<CoreProofResult> {
        let poll_timeout = self.config.proof_timeout;
        let poll_interval = self.config.poll_interval;

        timeout(poll_timeout, async {
            loop {
                // Check job status
                match self.get_job_status(job_id).await {
                    Ok(status) => {
                        debug!(
                            "Job {} status: {:?} ({}%)",
                            job_id, status.state, status.progress_pct
                        );

                        match status.state {
                            ProofJobState::Completed => {
                                // Fetch the proof
                                return self.get_proof(job_id).await;
                            }
                            ProofJobState::Failed => {
                                return Err(anyhow!(
                                    "Proof generation failed: {}",
                                    status.error.unwrap_or_else(|| "unknown error".to_string())
                                ));
                            }
                            ProofJobState::Cancelled => {
                                return Err(anyhow!("Proof job was cancelled"));
                            }
                            _ => {
                                // Still in progress
                            }
                        }
                    }
                    Err(e) => {
                        warn!("Failed to get job status: {}", e);
                        // Continue polling
                    }
                }

                tokio::time::sleep(poll_interval).await;
            }
        })
        .await
        .map_err(|_| anyhow!("Proof generation timed out after {:?}", poll_timeout))?
    }

    /// Get job status from coordinator
    async fn get_job_status(&self, job_id: &str) -> Result<ProofJobStatus> {
        let url = format!("{}/v2/batch/{}/proof", self.config.coordinator_url, job_id);

        let response = self.client.get(&url).send().await?;

        // If proof exists, job is completed
        if response.status().is_success() {
            // Check if this is a "not ready" response
            let api_response: ApiResponse<CoreProofResult> = response.json().await?;
            match api_response {
                ApiResponse::Success { data } => {
                    return Ok(ProofJobStatus {
                        job_id: job_id.to_string(),
                        batch_id: data.batch_id,
                        state: ProofJobState::Completed,
                        progress_pct: 100,
                        message: "Completed".to_string(),
                        created_at: 0,
                        updated_at: 0,
                        completed_at: Some(0),
                        error: None,
                    });
                }
                ApiResponse::Error { message, code } => {
                    // Check error code
                    if code.as_deref() == Some("NOT_READY") {
                        return Ok(ProofJobStatus {
                            job_id: job_id.to_string(),
                            batch_id: 0,
                            state: ProofJobState::Proving,
                            progress_pct: 50,
                            message,
                            created_at: 0,
                            updated_at: 0,
                            completed_at: None,
                            error: None,
                        });
                    }
                    if code.as_deref() == Some("PROOF_FAILED") {
                        return Ok(ProofJobStatus {
                            job_id: job_id.to_string(),
                            batch_id: 0,
                            state: ProofJobState::Failed,
                            progress_pct: 0,
                            message: "Failed".to_string(),
                            created_at: 0,
                            updated_at: 0,
                            completed_at: None,
                            error: Some(message),
                        });
                    }
                }
            }
        }

        // Default to "proving" state
        Ok(ProofJobStatus {
            job_id: job_id.to_string(),
            batch_id: 0,
            state: ProofJobState::Proving,
            progress_pct: 50,
            message: "In progress".to_string(),
            created_at: 0,
            updated_at: 0,
            completed_at: None,
            error: None,
        })
    }

    /// Get completed proof from coordinator
    async fn get_proof(&self, job_id: &str) -> Result<CoreProofResult> {
        let url = format!("{}/v2/batch/{}/proof", self.config.coordinator_url, job_id);

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .context("Failed to fetch proof")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow!("Failed to get proof: {} - {}", status, body));
        }

        let api_response: ApiResponse<CoreProofResult> = response
            .json()
            .await
            .context("Failed to parse proof response")?;

        match api_response {
            ApiResponse::Success { data } => Ok(data),
            ApiResponse::Error { message, .. } => Err(anyhow!("Failed to get proof: {}", message)),
        }
    }

    /// Convert the API result to BatchProof
    fn convert_result(
        &self,
        inputs: &BatchPublicInputs,
        result: CoreProofResult,
    ) -> Result<BatchProof> {
        // Decode hex proof bytes
        let proof_bytes = hex::decode(&result.proof_bytes).context("Invalid proof bytes hex")?;

        // Decode public witness bytes
        let public_witness_bytes = hex::decode(&result.public_witness_bytes)
            .context("Invalid public witness bytes hex")?;

        // Combine proof + public witness for Solana submission
        let mut combined_bytes = proof_bytes;
        combined_bytes.extend(public_witness_bytes);

        Ok(BatchProof {
            public_inputs: inputs.clone(),
            proof_bytes: combined_bytes,
            proving_time_ms: result.proving_time_ms,
        })
    }
}

// ============================================================================
// BatchProver Trait Implementation
// ============================================================================

impl BatchProver for NoirProverClient {
    fn prove(&self, inputs: &BatchPublicInputs, witness: &BatchWitness) -> Result<BatchProof> {
        // Create a new runtime for blocking call
        // This is needed because BatchProver::prove is sync
        let rt = tokio::runtime::Handle::try_current()
            .map_err(|_| anyhow!("No tokio runtime available"))?;

        rt.block_on(self.prove_async(inputs, witness))
    }

    fn verify(&self, proof: &BatchProof) -> Result<bool> {
        // Verification happens on-chain via the Sunspot verifier
        // Here we just check proof is well-formed
        // Expected: 388 bytes proof + 236 bytes public witness = 624 bytes
        let expected_len = 388 + 236;
        if proof.proof_bytes.len() != expected_len {
            warn!(
                "Proof length mismatch: expected {}, got {}",
                expected_len,
                proof.proof_bytes.len()
            );
            return Ok(false);
        }
        Ok(true)
    }

    fn verification_key_hash(&self) -> [u8; 32] {
        self.vk_hash
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = NoirProverConfig::default();
        assert_eq!(config.coordinator_url, "http://localhost:8080");
        assert_eq!(config.proof_timeout, Duration::from_secs(300));
        assert_eq!(config.max_retries, 3);
    }

    #[test]
    fn test_build_request() {
        let config = NoirProverConfig::default();
        let client = NoirProverClient::new(config);

        let inputs = BatchPublicInputs {
            pre_state_root: [1u8; 32],
            post_state_root: [2u8; 32],
            pre_shielded_root: [3u8; 32],
            post_shielded_root: [4u8; 32],
            withdrawal_root: [5u8; 32],
            batch_hash: [6u8; 32],
            batch_id: 42,
        };

        let witness = BatchWitness {
            transactions: vec![],
            results: vec![],
            pre_account_states: vec![],
        };

        let request = client.build_request(&inputs, &witness);

        assert_eq!(request.batch_id, 42);
        assert_eq!(request.pre_state_root, hex::encode([1u8; 32]));
        assert_eq!(request.post_state_root, hex::encode([2u8; 32]));
        assert!(request.transfers.is_empty());
        assert!(request.withdrawals.is_empty());
    }

    #[test]
    fn test_verify_proof_length() {
        let config = NoirProverConfig::default();
        let client = NoirProverClient::new(config);

        let inputs = BatchPublicInputs {
            pre_state_root: [0u8; 32],
            post_state_root: [0u8; 32],
            pre_shielded_root: [0u8; 32],
            post_shielded_root: [0u8; 32],
            withdrawal_root: [0u8; 32],
            batch_hash: [0u8; 32],
            batch_id: 1,
        };

        // Valid proof length
        let valid_proof = BatchProof {
            public_inputs: inputs.clone(),
            proof_bytes: vec![0u8; 388 + 236],
            proving_time_ms: 100,
        };
        assert!(client.verify(&valid_proof).unwrap());

        // Invalid proof length
        let invalid_proof = BatchProof {
            public_inputs: inputs,
            proof_bytes: vec![0u8; 100],
            proving_time_ms: 100,
        };
        assert!(!client.verify(&invalid_proof).unwrap());
    }
}
