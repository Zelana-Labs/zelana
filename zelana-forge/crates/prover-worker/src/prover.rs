//! Noir Prover Module
//!
//! Wraps nargo and sunspot CLI tools to generate proofs from circuit inputs.
//! Updated to support the zelana_batch circuit with 8 transfers, 4 withdrawals,
//! and 4 shielded transactions.

use num_bigint::BigUint;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::Stdio;
use thiserror::Error;
use tokio::process::Command;
use tracing::{debug, error, info};

// ============================================================================
// Hex to Decimal Conversion for Noir
// ============================================================================

/// BN254 scalar field modulus (Fr)
/// This is the order of the scalar field used by Groth16 on BN254
const BN254_FR_MODULUS: &str =
    "21888242871839275222246405745257275088548364400416034343698204186575808495617";

/// Convert a hex string (with or without 0x prefix) to a decimal string.
/// Noir/nargo expects Field values as decimal integers, not hex.
/// Values are reduced modulo the BN254 scalar field modulus.
///
/// If the input is already a decimal (or "0"), returns as-is.
/// If the input is 64 hex chars (32 bytes), converts to decimal and reduces mod p.
fn hex_to_decimal_field(s: &str) -> String {
    // Handle empty or zero
    if s.is_empty() || s == "0" {
        return "0".to_string();
    }

    // Strip 0x prefix if present
    let hex_str = s.strip_prefix("0x").unwrap_or(s);

    // Handle 128-char hex strings (64 bytes, e.g., Ed25519 signatures)
    // Take first 32 bytes since Noir field can only hold 32 bytes
    // TODO: Revisit for proper signature verification - currently circuit only checks signature != 0
    if hex_str.len() == 128 && hex_str.chars().all(|c| c.is_ascii_hexdigit()) {
        let first_half = &hex_str[0..64];
        match hex::decode(first_half) {
            Ok(bytes) => {
                let big_int = BigUint::from_bytes_be(&bytes);
                let modulus = BigUint::parse_bytes(BN254_FR_MODULUS.as_bytes(), 10)
                    .expect("Invalid modulus constant");
                let reduced = big_int % modulus;
                return reduced.to_string();
            }
            Err(_) => return s.to_string(),
        }
    }

    // Check if it looks like hex (64 chars = 32 bytes, all hex digits)
    if hex_str.len() == 64 && hex_str.chars().all(|c| c.is_ascii_hexdigit()) {
        // Parse as big-endian hex bytes and convert to decimal
        match hex::decode(hex_str) {
            Ok(bytes) => {
                let big_int = BigUint::from_bytes_be(&bytes);
                // Reduce modulo BN254 scalar field to ensure value is valid for Noir
                let modulus = BigUint::parse_bytes(BN254_FR_MODULUS.as_bytes(), 10)
                    .expect("Invalid modulus constant");
                let reduced = big_int % modulus;
                reduced.to_string()
            }
            Err(_) => s.to_string(), // Fallback: return original
        }
    } else {
        // Already decimal or other format, return as-is
        s.to_string()
    }
}

/// Convert an array of strings, applying hex_to_decimal_field to each element
fn normalize_array(arr: &[String; MERKLE_DEPTH]) -> [String; MERKLE_DEPTH] {
    std::array::from_fn(|i| hex_to_decimal_field(&arr[i]))
}

// ============================================================================
// Errors
// ============================================================================

/// Errors that can occur during proof generation
#[derive(Error, Debug)]
pub enum ProverError {
    #[error("Failed to write Prover.toml: {0}")]
    WriteInputs(#[from] std::io::Error),

    #[error("Failed to serialize inputs: {0}")]
    SerializeInputs(#[from] toml::ser::Error),

    #[error("nargo execution failed: {0}")]
    NargoExecution(String),

    #[error("sunspot proving failed: {0}")]
    SunspotProving(String),

    #[error("Circuit path does not exist: {0}")]
    CircuitNotFound(PathBuf),

    #[error("Invalid proof output: {0}")]
    InvalidProof(String),

    #[error("Proof file not found: {0}")]
    ProofFileNotFound(PathBuf),
}

// ============================================================================
// Circuit Constants
// ============================================================================

/// Maximum transfers per batch (must match circuit)
pub const MAX_TRANSFERS: usize = 8;
/// Maximum withdrawals per batch (must match circuit)
pub const MAX_WITHDRAWALS: usize = 4;
/// Maximum shielded transactions per batch (must match circuit)
pub const MAX_SHIELDED: usize = 4;
/// Merkle tree depth
pub const MERKLE_DEPTH: usize = 32;

// ============================================================================
// Witness Structures (matching zelana_batch circuit)
// ============================================================================

/// Transfer transaction witness
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferWitness {
    pub sender_pubkey: String,
    pub sender_balance: String,
    pub sender_nonce: String,
    pub sender_path: [String; MERKLE_DEPTH],
    pub sender_path_indices: [String; MERKLE_DEPTH],
    pub receiver_pubkey: String,
    pub receiver_balance: String,
    pub receiver_nonce: String,
    pub receiver_path: [String; MERKLE_DEPTH],
    pub receiver_path_indices: [String; MERKLE_DEPTH],
    pub amount: String,
    pub signature: String,
    pub is_valid: bool,
}

impl Default for TransferWitness {
    fn default() -> Self {
        Self {
            sender_pubkey: "0".to_string(),
            sender_balance: "0".to_string(),
            sender_nonce: "0".to_string(),
            sender_path: std::array::from_fn(|_| "0".to_string()),
            sender_path_indices: std::array::from_fn(|_| "0".to_string()),
            receiver_pubkey: "0".to_string(),
            receiver_balance: "0".to_string(),
            receiver_nonce: "0".to_string(),
            receiver_path: std::array::from_fn(|_| "0".to_string()),
            receiver_path_indices: std::array::from_fn(|_| "0".to_string()),
            amount: "0".to_string(),
            signature: "0".to_string(),
            is_valid: false,
        }
    }
}

/// Withdrawal transaction witness
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WithdrawalWitness {
    pub sender_pubkey: String,
    pub sender_balance: String,
    pub sender_nonce: String,
    pub sender_path: [String; MERKLE_DEPTH],
    pub sender_path_indices: [String; MERKLE_DEPTH],
    pub l1_recipient: String,
    pub amount: String,
    pub signature: String,
    pub is_valid: bool,
}

impl Default for WithdrawalWitness {
    fn default() -> Self {
        Self {
            sender_pubkey: "0".to_string(),
            sender_balance: "0".to_string(),
            sender_nonce: "0".to_string(),
            sender_path: std::array::from_fn(|_| "0".to_string()),
            sender_path_indices: std::array::from_fn(|_| "0".to_string()),
            l1_recipient: "0".to_string(),
            amount: "0".to_string(),
            signature: "0".to_string(),
            is_valid: false,
        }
    }
}

/// Shielded transaction witness
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShieldedWitness {
    pub input_owner: String,
    pub input_value: String,
    pub input_blinding: String,
    pub input_position: String,
    pub input_path: [String; MERKLE_DEPTH],
    pub input_path_indices: [String; MERKLE_DEPTH],
    pub spending_key: String,
    pub output_owner: String,
    pub output_value: String,
    pub output_blinding: String,
    pub output_commitment: String,
    pub nullifier: String,
    pub is_valid: bool,
    pub skip_verification: bool,
}

impl Default for ShieldedWitness {
    fn default() -> Self {
        Self {
            input_owner: "0".to_string(),
            input_value: "0".to_string(),
            input_blinding: "0".to_string(),
            input_position: "0".to_string(),
            input_path: std::array::from_fn(|_| "0".to_string()),
            input_path_indices: std::array::from_fn(|_| "0".to_string()),
            spending_key: "0".to_string(),
            output_owner: "0".to_string(),
            output_value: "0".to_string(),
            output_blinding: "0".to_string(),
            output_commitment: "0".to_string(),
            nullifier: "0".to_string(),
            is_valid: false,
            skip_verification: false,
        }
    }
}

// ============================================================================
// Batch Inputs (full zelana_batch circuit inputs)
// ============================================================================

/// Complete batch inputs matching the zelana_batch circuit
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchInputs {
    // === 7 Public Inputs ===
    pub pre_state_root: String,
    pub post_state_root: String,
    pub pre_shielded_root: String,
    pub post_shielded_root: String,
    pub withdrawal_root: String,
    pub batch_hash: String,
    pub batch_id: String,

    // === Private Witness ===
    pub transfers: Vec<TransferWitness>,
    pub withdrawals: Vec<WithdrawalWitness>,
    pub shielded: Vec<ShieldedWitness>,

    // === Transaction counts ===
    pub num_transfers: String,
    pub num_withdrawals: String,
    pub num_shielded: String,
}

impl BatchInputs {
    /// Create an empty batch (all transactions invalid)
    pub fn empty_batch(
        state_root: &str,
        shielded_root: &str,
        batch_id: u64,
        batch_hash: &str,
        withdrawal_root: &str,
    ) -> Self {
        Self {
            pre_state_root: state_root.to_string(),
            post_state_root: state_root.to_string(),
            pre_shielded_root: shielded_root.to_string(),
            post_shielded_root: shielded_root.to_string(),
            withdrawal_root: withdrawal_root.to_string(),
            batch_hash: batch_hash.to_string(),
            batch_id: batch_id.to_string(),
            transfers: (0..MAX_TRANSFERS)
                .map(|_| TransferWitness::default())
                .collect(),
            withdrawals: (0..MAX_WITHDRAWALS)
                .map(|_| WithdrawalWitness::default())
                .collect(),
            shielded: (0..MAX_SHIELDED)
                .map(|_| ShieldedWitness::default())
                .collect(),
            num_transfers: "0".to_string(),
            num_withdrawals: "0".to_string(),
            num_shielded: "0".to_string(),
        }
    }

    /// Pad arrays to the required circuit sizes
    pub fn pad_to_circuit_size(&mut self) {
        while self.transfers.len() < MAX_TRANSFERS {
            self.transfers.push(TransferWitness::default());
        }
        while self.withdrawals.len() < MAX_WITHDRAWALS {
            self.withdrawals.push(WithdrawalWitness::default());
        }
        while self.shielded.len() < MAX_SHIELDED {
            self.shielded.push(ShieldedWitness::default());
        }
    }

    /// Normalize all field values for Noir circuit consumption.
    /// Converts hex strings (32-byte/64-char) to decimal strings.
    /// Noir/nargo expects Field values as decimal integers, not hex.
    pub fn normalize_for_noir(&mut self) {
        // Convert public input roots
        self.pre_state_root = hex_to_decimal_field(&self.pre_state_root);
        self.post_state_root = hex_to_decimal_field(&self.post_state_root);
        self.pre_shielded_root = hex_to_decimal_field(&self.pre_shielded_root);
        self.post_shielded_root = hex_to_decimal_field(&self.post_shielded_root);
        self.withdrawal_root = hex_to_decimal_field(&self.withdrawal_root);
        self.batch_hash = hex_to_decimal_field(&self.batch_hash);
        // batch_id is already numeric

        // Normalize transfer witnesses
        for tx in &mut self.transfers {
            tx.sender_pubkey = hex_to_decimal_field(&tx.sender_pubkey);
            tx.receiver_pubkey = hex_to_decimal_field(&tx.receiver_pubkey);
            tx.signature = hex_to_decimal_field(&tx.signature);
            tx.sender_path = normalize_array(&tx.sender_path);
            tx.receiver_path = normalize_array(&tx.receiver_path);
            // sender_path_indices and receiver_path_indices are 0/1 values, leave as-is
        }

        // Normalize withdrawal witnesses
        for wd in &mut self.withdrawals {
            wd.sender_pubkey = hex_to_decimal_field(&wd.sender_pubkey);
            wd.l1_recipient = hex_to_decimal_field(&wd.l1_recipient);
            wd.signature = hex_to_decimal_field(&wd.signature);
            wd.sender_path = normalize_array(&wd.sender_path);
        }

        // Normalize shielded witnesses
        for sh in &mut self.shielded {
            sh.input_owner = hex_to_decimal_field(&sh.input_owner);
            sh.input_blinding = hex_to_decimal_field(&sh.input_blinding);
            sh.spending_key = hex_to_decimal_field(&sh.spending_key);
            sh.output_owner = hex_to_decimal_field(&sh.output_owner);
            sh.output_blinding = hex_to_decimal_field(&sh.output_blinding);
            sh.nullifier = hex_to_decimal_field(&sh.nullifier);
            sh.output_commitment = hex_to_decimal_field(&sh.output_commitment);
            sh.input_path = normalize_array(&sh.input_path);
        }
    }
}

// ============================================================================
// Legacy ChunkInputs (for backward compatibility with coordinator)
// ============================================================================

/// Legacy inputs for simpler chunk-based proving
/// This is converted to BatchInputs internally
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkInputs {
    pub old_root: String,
    pub new_root: String,
    pub sender_pubkeys: Vec<String>,
    pub receiver_pubkeys: Vec<String>,
    pub amounts: Vec<u64>,
    pub signatures: Vec<String>,
    pub merkle_paths: Vec<Vec<String>>,
}

impl ChunkInputs {
    /// Convert to BatchInputs format for the zelana_batch circuit
    pub fn to_batch_inputs(&self, batch_id: u64) -> BatchInputs {
        let mut batch = BatchInputs::empty_batch(
            &self.old_root,
            &self.old_root, // shielded root same as state for simple transfers
            batch_id,
            "0", // will be computed
            "0", // will be computed
        );

        batch.post_state_root = self.new_root.clone();
        batch.post_shielded_root = self.new_root.clone();

        // Convert legacy format to transfer witnesses
        let num_txs = self.sender_pubkeys.len().min(MAX_TRANSFERS);
        for i in 0..num_txs {
            batch.transfers[i] = TransferWitness {
                sender_pubkey: self
                    .sender_pubkeys
                    .get(i)
                    .cloned()
                    .unwrap_or("0".to_string()),
                sender_balance: "1000000".to_string(), // Placeholder
                sender_nonce: "0".to_string(),
                sender_path: if let Some(path) = self.merkle_paths.get(i) {
                    let mut arr: [String; MERKLE_DEPTH] = std::array::from_fn(|_| "0".to_string());
                    for (j, p) in path.iter().take(MERKLE_DEPTH).enumerate() {
                        arr[j] = p.clone();
                    }
                    arr
                } else {
                    std::array::from_fn(|_| "0".to_string())
                },
                sender_path_indices: std::array::from_fn(|_| "0".to_string()),
                receiver_pubkey: self
                    .receiver_pubkeys
                    .get(i)
                    .cloned()
                    .unwrap_or("0".to_string()),
                receiver_balance: "0".to_string(),
                receiver_nonce: "0".to_string(),
                receiver_path: std::array::from_fn(|_| "0".to_string()),
                receiver_path_indices: std::array::from_fn(|_| "0".to_string()),
                amount: self.amounts.get(i).unwrap_or(&0).to_string(),
                signature: self.signatures.get(i).cloned().unwrap_or("1".to_string()), // Non-zero for validity
                is_valid: true,
            };
        }

        batch.num_transfers = num_txs.to_string();

        batch
    }
}

// ============================================================================
// Proof Result
// ============================================================================

/// Proof generation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProofResult {
    /// Raw proof bytes
    pub proof_bytes: Vec<u8>,
    /// Raw public witness bytes
    pub public_witness_bytes: Vec<u8>,
    /// Hex-encoded proof (for backward compatibility)
    pub proof: String,
    /// Public inputs as hex strings
    pub public_inputs: Vec<String>,
}

impl ProofResult {
    /// Get concatenated proof + public witness for Solana submission
    pub fn to_solana_instruction_data(&self) -> Vec<u8> {
        let mut data = Vec::with_capacity(self.proof_bytes.len() + self.public_witness_bytes.len());
        data.extend_from_slice(&self.proof_bytes);
        data.extend_from_slice(&self.public_witness_bytes);
        data
    }
}

// ============================================================================
// Noir Prover (Real)
// ============================================================================

/// Noir prover wrapper that uses nargo + sunspot
pub struct NoirProver {
    circuit_path: PathBuf,
}

impl NoirProver {
    /// Create a new prover for the given circuit path
    pub fn new(circuit_path: PathBuf) -> Self {
        // Canonicalize to absolute path to avoid working directory issues
        let circuit_path = circuit_path.canonicalize().unwrap_or(circuit_path);
        Self { circuit_path }
    }

    /// Generate a proof for batch inputs
    pub async fn generate_batch_proof(
        &self,
        inputs: BatchInputs,
    ) -> Result<ProofResult, ProverError> {
        // Verify circuit path exists
        if !self.circuit_path.exists() {
            return Err(ProverError::CircuitNotFound(self.circuit_path.clone()));
        }

        info!(
            "Generating batch proof for batch_id={}, transfers={}, withdrawals={}, shielded={}",
            inputs.batch_id, inputs.num_transfers, inputs.num_withdrawals, inputs.num_shielded
        );

        // Step 0: Normalize inputs - convert hex strings to decimal for Noir
        let mut normalized_inputs = inputs;
        normalized_inputs.normalize_for_noir();
        debug!("Normalized inputs for Noir (hex -> decimal conversion applied)");

        // Step 1: Write Prover.toml
        let prover_toml_path = self.circuit_path.join("Prover.toml");
        let toml_content = toml::to_string_pretty(&normalized_inputs)?;

        debug!("Writing Prover.toml ({} bytes)", toml_content.len());
        tokio::fs::write(&prover_toml_path, &toml_content).await?;

        // Step 2: Execute nargo to generate witness
        let witness_name = format!("witness_{}", uuid::Uuid::new_v4().simple());
        info!("Executing nargo execute {}...", witness_name);

        let nargo_output = Command::new("nargo")
            .args(["execute", &witness_name])
            .current_dir(&self.circuit_path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await?;

        if !nargo_output.status.success() {
            let stderr = String::from_utf8_lossy(&nargo_output.stderr);
            error!("nargo failed: {}", stderr);
            return Err(ProverError::NargoExecution(stderr.to_string()));
        }

        debug!(
            "nargo output: {}",
            String::from_utf8_lossy(&nargo_output.stdout)
        );

        // Step 3: Generate proof using sunspot
        // sunspot prove <acir> <witness> <ccs> <pk>
        let target_dir = self.circuit_path.join("target");
        let acir_path = target_dir.join("zelana_batch.json");
        let witness_path = target_dir.join(format!("{}.gz", witness_name));
        let ccs_path = target_dir.join("zelana_batch.ccs");
        let pk_path = target_dir.join("zelana_batch.pk");

        info!("Executing sunspot prove...");
        let sunspot_output = Command::new("sunspot")
            .args([
                "prove",
                acir_path.to_str().unwrap(),
                witness_path.to_str().unwrap(),
                ccs_path.to_str().unwrap(),
                pk_path.to_str().unwrap(),
            ])
            .current_dir(&self.circuit_path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await?;

        if !sunspot_output.status.success() {
            let stderr = String::from_utf8_lossy(&sunspot_output.stderr);
            error!("sunspot prove failed: {}", stderr);
            return Err(ProverError::SunspotProving(stderr.to_string()));
        }

        // Step 4: Read proof and public witness files
        let proof_path = target_dir.join("zelana_batch.proof");
        let pw_path = target_dir.join("zelana_batch.pw");

        if !proof_path.exists() {
            return Err(ProverError::ProofFileNotFound(proof_path));
        }
        if !pw_path.exists() {
            return Err(ProverError::ProofFileNotFound(pw_path));
        }

        let proof_bytes = tokio::fs::read(&proof_path).await?;
        let public_witness_bytes = tokio::fs::read(&pw_path).await?;

        info!(
            "Proof generated: {} bytes proof, {} bytes public witness",
            proof_bytes.len(),
            public_witness_bytes.len()
        );

        // Parse public inputs from witness bytes
        // Format: 4-byte count + 8-byte padding + (32 bytes × N inputs)
        let public_inputs = parse_public_witness(&public_witness_bytes);

        // Clean up witness file
        let _ = tokio::fs::remove_file(&witness_path).await;

        Ok(ProofResult {
            proof: hex::encode(&proof_bytes),
            proof_bytes,
            public_witness_bytes,
            public_inputs,
        })
    }

    /// Generate a proof using legacy ChunkInputs format
    pub async fn generate_proof(&self, inputs: ChunkInputs) -> Result<ProofResult, ProverError> {
        let batch_inputs = inputs.to_batch_inputs(1);
        self.generate_batch_proof(batch_inputs).await
    }
}

/// Parse public inputs from public witness bytes
fn parse_public_witness(bytes: &[u8]) -> Vec<String> {
    if bytes.len() < 12 {
        return vec![];
    }

    // First 4 bytes: number of public inputs (big-endian u32)
    let count = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;

    // Skip 12 bytes header (4 count + 8 padding/metadata)
    let data_start = 12;
    let mut inputs = Vec::with_capacity(count);

    for i in 0..count {
        let offset = data_start + i * 32;
        if offset + 32 <= bytes.len() {
            let input_bytes = &bytes[offset..offset + 32];
            inputs.push(format!("0x{}", hex::encode(input_bytes)));
        }
    }

    inputs
}

// ============================================================================
// Mock Prover (for testing)
// ============================================================================

/// Mock prover for testing (doesn't require nargo/sunspot)
pub struct MockProver {
    delay_ms: u64,
    /// Path to pre-generated proof files (for testing with real proofs)
    proof_files_path: Option<PathBuf>,
}

impl MockProver {
    /// Create a mock prover with simulated delay
    pub fn new(delay_ms: u64) -> Self {
        Self {
            delay_ms,
            proof_files_path: None,
        }
    }

    /// Create a mock prover that returns real proof files
    pub fn with_proof_files(delay_ms: u64, path: PathBuf) -> Self {
        Self {
            delay_ms,
            proof_files_path: Some(path),
        }
    }

    /// Generate a mock proof or return real proof files
    pub async fn generate_proof(&self, inputs: ChunkInputs) -> Result<ProofResult, ProverError> {
        info!(
            "Mock proving chunk with roots: {} -> {}",
            inputs.old_root, inputs.new_root
        );

        // Simulate proving time
        tokio::time::sleep(tokio::time::Duration::from_millis(self.delay_ms)).await;

        // If we have real proof files, use them
        if let Some(path) = &self.proof_files_path {
            let proof_path = path.join("zelana_batch.proof");
            let pw_path = path.join("zelana_batch.pw");

            if proof_path.exists() && pw_path.exists() {
                let proof_bytes = tokio::fs::read(&proof_path).await?;
                let public_witness_bytes = tokio::fs::read(&pw_path).await?;
                let public_inputs = parse_public_witness(&public_witness_bytes);

                return Ok(ProofResult {
                    proof: hex::encode(&proof_bytes),
                    proof_bytes,
                    public_witness_bytes,
                    public_inputs,
                });
            }
        }

        // Generate deterministic mock proof based on inputs
        let mut hasher = sha2::Sha256::new();
        use sha2::Digest;
        hasher.update(inputs.old_root.as_bytes());
        hasher.update(inputs.new_root.as_bytes());
        for amount in &inputs.amounts {
            hasher.update(amount.to_le_bytes());
        }
        let hash = hasher.finalize();

        // Create mock proof (388 bytes to match real proof size)
        let mut proof_bytes = Vec::with_capacity(388);
        for _ in 0..12 {
            proof_bytes.extend_from_slice(&hash);
        }
        proof_bytes.truncate(388);

        // Create mock public witness (236 bytes for 7 inputs)
        let mut public_witness_bytes = Vec::with_capacity(236);
        // Header: count (7) + padding
        public_witness_bytes.extend_from_slice(&[0, 0, 0, 7]); // count
        public_witness_bytes.extend_from_slice(&[0; 8]); // padding
        // 7 public inputs (32 bytes each)
        for _ in 0..7 {
            public_witness_bytes.extend_from_slice(&hash);
        }
        public_witness_bytes.truncate(236);

        Ok(ProofResult {
            proof: hex::encode(&proof_bytes),
            proof_bytes,
            public_witness_bytes,
            public_inputs: vec![inputs.old_root, inputs.new_root],
        })
    }

    /// Generate a mock batch proof
    pub async fn generate_batch_proof(
        &self,
        inputs: BatchInputs,
    ) -> Result<ProofResult, ProverError> {
        let chunk = ChunkInputs {
            old_root: inputs.pre_state_root,
            new_root: inputs.post_state_root,
            sender_pubkeys: vec![],
            receiver_pubkeys: vec![],
            amounts: vec![],
            signatures: vec![],
            merkle_paths: vec![],
        };
        self.generate_proof(chunk).await
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_prover() {
        let prover = MockProver::new(10);

        let inputs = ChunkInputs {
            old_root: "0x1234".to_string(),
            new_root: "0x5678".to_string(),
            sender_pubkeys: vec!["0xabc".to_string()],
            receiver_pubkeys: vec!["0xdef".to_string()],
            amounts: vec![100],
            signatures: vec!["0xsig".to_string()],
            merkle_paths: vec![vec!["0x0".to_string(); 32]],
        };

        let result = prover.generate_proof(inputs).await.unwrap();

        assert_eq!(result.proof_bytes.len(), 388);
        assert_eq!(result.public_witness_bytes.len(), 236);
        assert!(!result.proof.is_empty());
    }

    #[test]
    fn test_chunk_inputs_to_batch_inputs() {
        let inputs = ChunkInputs {
            old_root: "0x1234".to_string(),
            new_root: "0x5678".to_string(),
            sender_pubkeys: vec!["0xabc".to_string()],
            receiver_pubkeys: vec!["0xdef".to_string()],
            amounts: vec![100],
            signatures: vec!["0xsig".to_string()],
            merkle_paths: vec![vec!["0x0".to_string(); 32]],
        };

        let batch = inputs.to_batch_inputs(1);

        assert_eq!(batch.pre_state_root, "0x1234");
        assert_eq!(batch.post_state_root, "0x5678");
        assert_eq!(batch.batch_id, "1");
        assert_eq!(batch.num_transfers, "1");
        assert_eq!(batch.transfers.len(), MAX_TRANSFERS);
        assert!(batch.transfers[0].is_valid);
        assert!(!batch.transfers[1].is_valid);
    }

    #[test]
    fn test_empty_batch() {
        let batch = BatchInputs::empty_batch("0xroot", "0xshielded", 1, "0xhash", "0xwd");

        assert_eq!(batch.pre_state_root, "0xroot");
        assert_eq!(batch.post_state_root, "0xroot");
        assert_eq!(batch.batch_id, "1");
        assert_eq!(batch.transfers.len(), MAX_TRANSFERS);
        assert_eq!(batch.withdrawals.len(), MAX_WITHDRAWALS);
        assert_eq!(batch.shielded.len(), MAX_SHIELDED);

        // All transactions should be invalid
        for t in &batch.transfers {
            assert!(!t.is_valid);
        }
    }

    #[test]
    fn test_parse_public_witness() {
        // Create mock public witness with 7 inputs
        let mut bytes = vec![0, 0, 0, 7]; // count = 7
        bytes.extend_from_slice(&[0; 8]); // padding
        for i in 0..7 {
            let mut input = [0u8; 32];
            input[0] = i as u8;
            bytes.extend_from_slice(&input);
        }

        let inputs = parse_public_witness(&bytes);
        assert_eq!(inputs.len(), 7);
        assert!(inputs[0].starts_with("0x00"));
        assert!(inputs[1].starts_with("0x01"));
    }

    #[test]
    fn test_proof_result_to_solana_data() {
        let result = ProofResult {
            proof_bytes: vec![1, 2, 3, 4],
            public_witness_bytes: vec![5, 6, 7, 8],
            proof: "01020304".to_string(),
            public_inputs: vec![],
        };

        let data = result.to_solana_instruction_data();
        assert_eq!(data, vec![1, 2, 3, 4, 5, 6, 7, 8]);
    }
}
