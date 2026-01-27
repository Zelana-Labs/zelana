//! Solana Verifier Client
//!
//! Submits ZK proofs to the deployed Sunspot verifier program on Solana.
//!
//! The verifier program expects instruction data in the format:
//! [proof_bytes (388)] + [public_witness_bytes (236)]
//!
//! Where:
//! - proof_bytes: Groth16 proof from sunspot (gnark format)
//! - public_witness_bytes: 4-byte count + 8-byte padding + (32 bytes × N inputs)

use solana_client::rpc_client::RpcClient;
use solana_client::rpc_config::RpcTransactionConfig;
use solana_sdk::{
    commitment_config::CommitmentConfig,
    compute_budget::ComputeBudgetInstruction,
    instruction::Instruction,
    pubkey::Pubkey,
    signature::{Keypair, Signature, Signer},
    transaction::Transaction,
};
use solana_transaction_status::UiTransactionEncoding;
use std::path::Path;
use std::str::FromStr;
use thiserror::Error;
use tracing::{debug, error, info};

// ============================================================================
// Errors
// ============================================================================

#[derive(Error, Debug)]
pub enum SolanaClientError {
    #[error("Failed to parse program ID: {0}")]
    InvalidProgramId(String),

    #[error("Failed to load keypair: {0}")]
    KeypairLoad(String),

    #[error("RPC error: {0}")]
    RpcError(#[from] solana_client::client_error::ClientError),

    #[error("Transaction failed: {0}")]
    TransactionFailed(String),

    #[error("Invalid proof data: {0}")]
    InvalidProofData(String),

    #[error("Insufficient balance for transaction")]
    InsufficientBalance,
}

// ============================================================================
// Configuration
// ============================================================================

/// Configuration for the Solana verifier client
#[derive(Debug, Clone)]
pub struct SolanaVerifierConfig {
    /// RPC URL (e.g., "https://api.devnet.solana.com")
    pub rpc_url: String,
    /// Verifier program ID
    pub program_id: String,
    /// Path to payer keypair file (optional, will use default if None)
    pub keypair_path: Option<String>,
    /// Compute units to request (default: 500,000)
    pub compute_units: u32,
    /// Priority fee in micro-lamports per compute unit
    pub priority_fee_micro_lamports: u64,
}

impl Default for SolanaVerifierConfig {
    fn default() -> Self {
        Self {
            rpc_url: "https://api.devnet.solana.com".to_string(),
            program_id: "EZzyLrTrC4uyU488jVAs4GKeCR1s9GmoFggeiDqwDeNK".to_string(),
            keypair_path: None,
            compute_units: 500_000,
            priority_fee_micro_lamports: 1000,
        }
    }
}

// ============================================================================
// Verification Result
// ============================================================================

/// Result of proof verification on Solana
#[derive(Debug, Clone)]
pub struct VerificationResult {
    /// Transaction signature
    pub signature: Signature,
    /// Whether verification succeeded
    pub verified: bool,
    /// Slot at which transaction was confirmed
    pub slot: u64,
    /// Compute units consumed
    pub compute_units_consumed: Option<u64>,
}

// ============================================================================
// Solana Verifier Client
// ============================================================================

/// Client for submitting proofs to Solana verifier program
pub struct SolanaVerifierClient {
    rpc: RpcClient,
    payer: Keypair,
    program_id: Pubkey,
    config: SolanaVerifierConfig,
}

impl SolanaVerifierClient {
    /// Create a new client with the given configuration
    pub fn new(config: SolanaVerifierConfig) -> Result<Self, SolanaClientError> {
        // Parse program ID
        let program_id = Pubkey::from_str(&config.program_id)
            .map_err(|e| SolanaClientError::InvalidProgramId(e.to_string()))?;

        // Load or generate keypair
        let payer = if let Some(path) = &config.keypair_path {
            load_keypair(path)?
        } else {
            // Try default Solana CLI path
            let default_path = shellexpand::tilde("~/.config/solana/id.json").to_string();
            if Path::new(&default_path).exists() {
                load_keypair(&default_path)?
            } else {
                return Err(SolanaClientError::KeypairLoad(
                    "No keypair path provided and default ~/.config/solana/id.json not found"
                        .to_string(),
                ));
            }
        };

        // Create RPC client
        let rpc =
            RpcClient::new_with_commitment(config.rpc_url.clone(), CommitmentConfig::confirmed());

        info!(
            "Solana verifier client initialized: program={}, payer={}",
            program_id,
            payer.pubkey()
        );

        Ok(Self {
            rpc,
            payer,
            program_id,
            config,
        })
    }

    /// Get the payer's public key
    pub fn payer_pubkey(&self) -> Pubkey {
        self.payer.pubkey()
    }

    /// Get the payer's SOL balance
    pub async fn get_balance(&self) -> Result<u64, SolanaClientError> {
        Ok(self.rpc.get_balance(&self.payer.pubkey())?)
    }

    /// Verify a proof on-chain
    ///
    /// # Arguments
    /// * `proof_bytes` - Raw Groth16 proof bytes (388 bytes from sunspot)
    /// * `public_witness_bytes` - Raw public witness bytes (236 bytes for 7 inputs)
    ///
    /// # Returns
    /// Transaction signature if verification succeeds
    pub async fn verify_proof(
        &self,
        proof_bytes: &[u8],
        public_witness_bytes: &[u8],
    ) -> Result<VerificationResult, SolanaClientError> {
        // Validate input sizes
        if proof_bytes.len() != 388 {
            return Err(SolanaClientError::InvalidProofData(format!(
                "Expected 388 bytes proof, got {}",
                proof_bytes.len()
            )));
        }
        if public_witness_bytes.len() != 236 {
            return Err(SolanaClientError::InvalidProofData(format!(
                "Expected 236 bytes public witness, got {}",
                public_witness_bytes.len()
            )));
        }

        // Build instruction data: proof + public_witness
        let mut instruction_data =
            Vec::with_capacity(proof_bytes.len() + public_witness_bytes.len());
        instruction_data.extend_from_slice(proof_bytes);
        instruction_data.extend_from_slice(public_witness_bytes);

        debug!(
            "Submitting proof to verifier: {} bytes total",
            instruction_data.len()
        );

        // Create verify instruction (no accounts needed for the sunspot verifier)
        let verify_ix = Instruction {
            program_id: self.program_id,
            accounts: vec![],
            data: instruction_data,
        };

        // Create compute budget instructions
        let compute_budget_ix =
            ComputeBudgetInstruction::set_compute_unit_limit(self.config.compute_units);
        let priority_fee_ix = ComputeBudgetInstruction::set_compute_unit_price(
            self.config.priority_fee_micro_lamports,
        );

        // Get recent blockhash
        let recent_blockhash = self.rpc.get_latest_blockhash()?;

        // Build and sign transaction
        let tx = Transaction::new_signed_with_payer(
            &[compute_budget_ix, priority_fee_ix, verify_ix],
            Some(&self.payer.pubkey()),
            &[&self.payer],
            recent_blockhash,
        );

        info!("Submitting verification transaction...");

        // Send and confirm transaction
        let signature = self.rpc.send_and_confirm_transaction(&tx)?;

        info!("Verification transaction confirmed: {}", signature);

        // Get transaction details for compute units
        let tx_config = RpcTransactionConfig {
            encoding: Some(UiTransactionEncoding::Json),
            commitment: Some(CommitmentConfig::confirmed()),
            max_supported_transaction_version: Some(0),
        };

        let tx_result = self.rpc.get_transaction_with_config(&signature, tx_config);

        let (slot, compute_units_consumed) = match tx_result {
            Ok(tx) => {
                let slot = tx.slot;
                let cu = tx
                    .transaction
                    .meta
                    .as_ref()
                    .and_then(|m| m.compute_units_consumed.clone().into());
                (slot, cu)
            }
            Err(_) => (0, None),
        };

        Ok(VerificationResult {
            signature,
            verified: true,
            slot,
            compute_units_consumed,
        })
    }

    /// Verify a proof using pre-read proof and witness files
    pub async fn verify_from_files(
        &self,
        proof_path: &Path,
        pw_path: &Path,
    ) -> Result<VerificationResult, SolanaClientError> {
        let proof_bytes = std::fs::read(proof_path).map_err(|e| {
            SolanaClientError::InvalidProofData(format!("Failed to read proof file: {}", e))
        })?;
        let pw_bytes = std::fs::read(pw_path).map_err(|e| {
            SolanaClientError::InvalidProofData(format!(
                "Failed to read public witness file: {}",
                e
            ))
        })?;

        self.verify_proof(&proof_bytes, &pw_bytes).await
    }

    /// Check if the verifier program exists on-chain
    pub async fn check_program_exists(&self) -> Result<bool, SolanaClientError> {
        match self.rpc.get_account(&self.program_id) {
            Ok(account) => Ok(account.executable),
            Err(e) => {
                if e.to_string().contains("AccountNotFound") {
                    Ok(false)
                } else {
                    Err(SolanaClientError::RpcError(e))
                }
            }
        }
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Load a keypair from a JSON file
fn load_keypair(path: &str) -> Result<Keypair, SolanaClientError> {
    let expanded_path = shellexpand::tilde(path).to_string();
    let file_content = std::fs::read_to_string(&expanded_path)
        .map_err(|e| SolanaClientError::KeypairLoad(format!("Failed to read {}: {}", path, e)))?;

    let bytes: Vec<u8> = serde_json::from_str(&file_content)
        .map_err(|e| SolanaClientError::KeypairLoad(format!("Invalid keypair JSON: {}", e)))?;

    Keypair::try_from(bytes.as_slice())
        .map_err(|e| SolanaClientError::KeypairLoad(format!("Invalid keypair bytes: {}", e)))
}

// ============================================================================
// Proof Data Builder
// ============================================================================

/// Helper to build proof data for submission
pub struct ProofData {
    pub proof_bytes: Vec<u8>,
    pub public_witness_bytes: Vec<u8>,
}

impl ProofData {
    /// Create from raw bytes
    pub fn new(proof_bytes: Vec<u8>, public_witness_bytes: Vec<u8>) -> Self {
        Self {
            proof_bytes,
            public_witness_bytes,
        }
    }

    /// Load from files
    pub fn from_files(proof_path: &Path, pw_path: &Path) -> Result<Self, std::io::Error> {
        let proof_bytes = std::fs::read(proof_path)?;
        let public_witness_bytes = std::fs::read(pw_path)?;
        Ok(Self::new(proof_bytes, public_witness_bytes))
    }

    /// Get concatenated instruction data
    pub fn to_instruction_data(&self) -> Vec<u8> {
        let mut data = Vec::with_capacity(self.proof_bytes.len() + self.public_witness_bytes.len());
        data.extend_from_slice(&self.proof_bytes);
        data.extend_from_slice(&self.public_witness_bytes);
        data
    }

    /// Validate sizes
    pub fn validate(&self) -> Result<(), String> {
        if self.proof_bytes.len() != 388 {
            return Err(format!(
                "Invalid proof size: expected 388, got {}",
                self.proof_bytes.len()
            ));
        }
        if self.public_witness_bytes.len() != 236 {
            return Err(format!(
                "Invalid public witness size: expected 236, got {}",
                self.public_witness_bytes.len()
            ));
        }
        Ok(())
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_proof_data_to_instruction_data() {
        let proof = vec![1u8; 388];
        let pw = vec![2u8; 236];
        let data = ProofData::new(proof.clone(), pw.clone());

        let instruction_data = data.to_instruction_data();
        assert_eq!(instruction_data.len(), 388 + 236);
        assert_eq!(&instruction_data[..388], &proof[..]);
        assert_eq!(&instruction_data[388..], &pw[..]);
    }

    #[test]
    fn test_proof_data_validation() {
        // Valid sizes
        let valid = ProofData::new(vec![0; 388], vec![0; 236]);
        assert!(valid.validate().is_ok());

        // Invalid proof size
        let invalid_proof = ProofData::new(vec![0; 100], vec![0; 236]);
        assert!(invalid_proof.validate().is_err());

        // Invalid witness size
        let invalid_pw = ProofData::new(vec![0; 388], vec![0; 100]);
        assert!(invalid_pw.validate().is_err());
    }

    #[test]
    fn test_default_config() {
        let config = SolanaVerifierConfig::default();
        assert_eq!(config.rpc_url, "https://api.devnet.solana.com");
        assert_eq!(
            config.program_id,
            "EZzyLrTrC4uyU488jVAs4GKeCR1s9GmoFggeiDqwDeNK"
        );
        assert_eq!(config.compute_units, 500_000);
    }
}
