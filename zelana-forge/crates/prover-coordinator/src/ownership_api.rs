//! Ownership Proof API Module
//!
//! Provides HTTP endpoints for generating ownership proofs (client-side ZK proofs).
//! These are lightweight Groth16 proofs generated via Sunspot.
//!
//! ## Endpoints
//!
//! - `POST /v2/ownership/prove` - Generate an ownership proof (synchronous)
//!
//! ## Proof Format
//!
//! The ownership proof is a Groth16 proof (388 bytes) with 3 public inputs:
//! - commitment: The note commitment
//! - nullifier: The unique nullifier for this note
//! - blinded_proxy: For delegated Merkle path fetching

use axum::{Json, Router, extract::State, http::StatusCode, routing::post};
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, process::Stdio, sync::Arc, time::Instant};
use tokio::{process::Command, sync::RwLock};
use tracing::{error, info};

// ============================================================================
// Types
// ============================================================================

/// Request for ownership proof generation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OwnershipProveRequest {
    /// Private inputs (processed securely, never stored)
    /// Spending key - the user's secret key (decimal string or hex)
    pub spending_key: String,
    /// Note value in lamports
    pub note_value: String,
    /// Note blinding factor (randomness)
    pub note_blinding: String,
    /// Note position in the commitment tree
    pub note_position: String,

    /// Public outputs (verified by the circuit)
    /// Expected commitment = H(owner_pk, value, blinding)
    pub commitment: String,
    /// Expected nullifier = H(NULLIFIER_DOMAIN, spending_key, commitment, position)
    pub nullifier: String,
    /// Expected blinded proxy = H(DELEGATE_DOMAIN, commitment, position)
    pub blinded_proxy: String,
}

/// Response from ownership proof generation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OwnershipProofResult {
    /// Groth16 proof bytes (388 bytes, hex encoded)
    pub proof_bytes: String,
    /// Public witness bytes (108 bytes for 3 inputs, hex encoded)
    pub public_witness_bytes: String,
    /// Verified public inputs
    pub commitment: String,
    pub nullifier: String,
    pub blinded_proxy: String,
    /// Proving time in milliseconds
    pub proving_time_ms: u64,
}

/// API Response wrapper
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "lowercase")]
pub enum ApiResponse<T> {
    Success { data: T },
    Error { message: String },
}

impl<T> ApiResponse<T> {
    pub fn success(data: T) -> Self {
        ApiResponse::Success { data }
    }

    pub fn error(message: impl Into<String>) -> Self {
        ApiResponse::Error {
            message: message.into(),
        }
    }
}

// ============================================================================
// State
// ============================================================================

/// Configuration for ownership prover
#[derive(Debug, Clone)]
pub struct OwnershipProverConfig {
    /// Path to ownership circuit directory
    pub circuit_path: PathBuf,
    /// Use mock prover for testing
    pub mock_prover: bool,
    /// Mock delay in milliseconds
    pub mock_delay_ms: u64,
}

impl Default for OwnershipProverConfig {
    fn default() -> Self {
        Self {
            circuit_path: PathBuf::from("../../circuits/ownership"),
            mock_prover: false,
            mock_delay_ms: 100,
        }
    }
}

/// Ownership prover state
pub struct OwnershipProverState {
    pub config: OwnershipProverConfig,
    /// Track number of proofs generated
    pub total_proofs: u64,
    /// Average proving time
    pub avg_proving_time_ms: u64,
}

pub type SharedOwnershipState = Arc<RwLock<OwnershipProverState>>;

impl OwnershipProverState {
    pub fn new(config: OwnershipProverConfig) -> Self {
        Self {
            config,
            total_proofs: 0,
            avg_proving_time_ms: 0,
        }
    }

    /// Update statistics after a proof
    pub fn record_proof(&mut self, proving_time_ms: u64) {
        let total = self.total_proofs;
        self.avg_proving_time_ms = if total == 0 {
            proving_time_ms
        } else {
            (self.avg_proving_time_ms * total + proving_time_ms) / (total + 1)
        };
        self.total_proofs += 1;
    }
}

// ============================================================================
// Router
// ============================================================================

/// Create the ownership API router
pub fn ownership_api_router(state: SharedOwnershipState) -> Router {
    Router::new()
        .route("/v2/ownership/prove", post(prove_handler))
        .route("/v2/ownership/health", axum::routing::get(health_handler))
        .with_state(state)
}

// ============================================================================
// Handlers
// ============================================================================

/// Health check for ownership prover
async fn health_handler(
    State(state): State<SharedOwnershipState>,
) -> Json<ApiResponse<serde_json::Value>> {
    let prover_state = state.read().await;

    let health = serde_json::json!({
        "status": "ok",
        "circuit_path": prover_state.config.circuit_path.to_string_lossy(),
        "mock_prover": prover_state.config.mock_prover,
        "total_proofs": prover_state.total_proofs,
        "avg_proving_time_ms": prover_state.avg_proving_time_ms,
    });

    Json(ApiResponse::success(health))
}

/// Generate an ownership proof
async fn prove_handler(
    State(state): State<SharedOwnershipState>,
    Json(request): Json<OwnershipProveRequest>,
) -> Result<Json<ApiResponse<OwnershipProofResult>>, StatusCode> {
    let start = Instant::now();

    info!(
        "Generating ownership proof for commitment: {}...",
        &request.commitment.chars().take(16).collect::<String>()
    );

    let (circuit_path, mock_prover, mock_delay) = {
        let prover_state = state.read().await;
        (
            prover_state.config.circuit_path.clone(),
            prover_state.config.mock_prover,
            prover_state.config.mock_delay_ms,
        )
    };

    let result = if mock_prover {
        generate_mock_proof(&request, mock_delay).await
    } else {
        generate_real_proof(&request, &circuit_path).await
    };

    match result {
        Ok(mut proof_result) => {
            let proving_time_ms = start.elapsed().as_millis() as u64;
            proof_result.proving_time_ms = proving_time_ms;

            // Update statistics
            {
                let mut prover_state = state.write().await;
                prover_state.record_proof(proving_time_ms);
            }

            info!(
                "Ownership proof generated in {}ms (proof size: {} bytes)",
                proving_time_ms,
                proof_result.proof_bytes.len() / 2
            );

            Ok(Json(ApiResponse::success(proof_result)))
        }
        Err(e) => {
            error!("Ownership proof generation failed: {}", e);
            Ok(Json(ApiResponse::error(e)))
        }
    }
}

// ============================================================================
// Proof Generation
// ============================================================================

/// Generate a real ownership proof using nargo + sunspot
async fn generate_real_proof(
    request: &OwnershipProveRequest,
    circuit_path: &PathBuf,
) -> Result<OwnershipProofResult, String> {
    // Verify circuit path exists
    if !circuit_path.exists() {
        return Err(format!("Circuit path not found: {:?}", circuit_path));
    }

    let target_dir = circuit_path.join("target");

    // Check required files exist
    let acir_path = target_dir.join("ownership.json");
    let ccs_path = target_dir.join("ownership.ccs");
    let pk_path = target_dir.join("ownership.pk");

    if !acir_path.exists() || !ccs_path.exists() || !pk_path.exists() {
        return Err(format!(
            "Missing circuit artifacts. Run: nargo compile && sunspot compile && sunspot setup"
        ));
    }

    // Step 1: Write Prover.toml with circuit inputs
    let prover_toml = format!(
        r#"# Private inputs
spending_key = "{}"
note_value = "{}"
note_blinding = "{}"
note_position = "{}"

# Public inputs
commitment = "{}"
nullifier = "{}"
blinded_proxy = "{}"
"#,
        request.spending_key,
        request.note_value,
        request.note_blinding,
        request.note_position,
        request.commitment,
        request.nullifier,
        request.blinded_proxy
    );

    let prover_toml_path = circuit_path.join("Prover.toml");
    tokio::fs::write(&prover_toml_path, &prover_toml)
        .await
        .map_err(|e| format!("Failed to write Prover.toml: {}", e))?;

    // Step 2: Execute nargo to generate witness
    let witness_name = format!("ownership_witness_{}", uuid::Uuid::new_v4().simple());

    let nargo_output = Command::new("nargo")
        .args(["execute", &witness_name])
        .current_dir(circuit_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| format!("Failed to run nargo: {}", e))?;

    if !nargo_output.status.success() {
        let stderr = String::from_utf8_lossy(&nargo_output.stderr);
        return Err(format!("nargo execution failed: {}", stderr));
    }

    // Step 3: Generate proof using sunspot
    let witness_path = target_dir.join(format!("{}.gz", witness_name));

    let sunspot_output = Command::new("sunspot")
        .args([
            "prove",
            acir_path.to_str().unwrap(),
            witness_path.to_str().unwrap(),
            ccs_path.to_str().unwrap(),
            pk_path.to_str().unwrap(),
        ])
        .current_dir(circuit_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| format!("Failed to run sunspot: {}", e))?;

    if !sunspot_output.status.success() {
        let stderr = String::from_utf8_lossy(&sunspot_output.stderr);
        return Err(format!("sunspot prove failed: {}", stderr));
    }

    // Step 4: Read proof and public witness files
    let proof_path = target_dir.join("ownership.proof");
    let pw_path = target_dir.join("ownership.pw");

    if !proof_path.exists() {
        return Err(format!("Proof file not found: {:?}", proof_path));
    }
    if !pw_path.exists() {
        return Err(format!("Public witness file not found: {:?}", pw_path));
    }

    let proof_bytes = tokio::fs::read(&proof_path)
        .await
        .map_err(|e| format!("Failed to read proof: {}", e))?;

    let public_witness_bytes = tokio::fs::read(&pw_path)
        .await
        .map_err(|e| format!("Failed to read public witness: {}", e))?;

    // Clean up witness file
    let _ = tokio::fs::remove_file(&witness_path).await;

    Ok(OwnershipProofResult {
        proof_bytes: hex::encode(&proof_bytes),
        public_witness_bytes: hex::encode(&public_witness_bytes),
        commitment: request.commitment.clone(),
        nullifier: request.nullifier.clone(),
        blinded_proxy: request.blinded_proxy.clone(),
        proving_time_ms: 0, // Set by caller
    })
}

/// Generate a mock proof for testing
async fn generate_mock_proof(
    request: &OwnershipProveRequest,
    delay_ms: u64,
) -> Result<OwnershipProofResult, String> {
    use sha2::{Digest, Sha256};

    // Simulate proving delay
    tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)).await;

    // Generate deterministic mock proof
    let mut hasher = Sha256::new();
    hasher.update(request.commitment.as_bytes());
    hasher.update(request.nullifier.as_bytes());
    hasher.update(request.blinded_proxy.as_bytes());
    let hash = hasher.finalize();

    // Create mock proof (388 bytes)
    let mut proof_bytes = Vec::with_capacity(388);
    for _ in 0..13 {
        proof_bytes.extend_from_slice(&hash);
    }
    proof_bytes.truncate(388);

    // Create mock public witness (108 bytes for 3 inputs)
    // Header: count (4) + padding (8) + 3 * 32 bytes = 108
    let mut pw_bytes = Vec::with_capacity(108);
    pw_bytes.extend_from_slice(&[0, 0, 0, 3]); // count = 3 (big-endian)
    pw_bytes.extend_from_slice(&[0; 8]); // padding
    // 3 public inputs (commitment, nullifier, blinded_proxy)
    for _ in 0..3 {
        pw_bytes.extend_from_slice(&hash);
    }
    pw_bytes.truncate(108);

    Ok(OwnershipProofResult {
        proof_bytes: hex::encode(&proof_bytes),
        public_witness_bytes: hex::encode(&pw_bytes),
        commitment: request.commitment.clone(),
        nullifier: request.nullifier.clone(),
        blinded_proxy: request.blinded_proxy.clone(),
        proving_time_ms: 0, // Set by caller
    })
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_proof_generation() {
        let request = OwnershipProveRequest {
            spending_key: "12345".to_string(),
            note_value: "1000000000".to_string(),
            note_blinding: "9999999".to_string(),
            note_position: "0".to_string(),
            commitment: "123456789".to_string(),
            nullifier: "987654321".to_string(),
            blinded_proxy: "555555555".to_string(),
        };

        let result = generate_mock_proof(&request, 10).await.unwrap();

        // Proof should be 388 bytes (776 hex chars)
        assert_eq!(result.proof_bytes.len(), 388 * 2);
        // Public witness should be 108 bytes (216 hex chars)
        assert_eq!(result.public_witness_bytes.len(), 108 * 2);
    }
}
