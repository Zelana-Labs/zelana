//! L1 Settler
//!
//! Submits batch proofs and state roots to Solana L1.
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                    L1 Settlement Flow                           │
//! │                                                                  │
//! │  ┌────────────┐    ┌────────────┐    ┌────────────────────────┐ │
//! │  │   Batch    │───▶│   Submit   │───▶│    Wait for            │ │
//! │  │   Proved   │    │   to L1    │    │    Confirmation        │ │
//! │  └────────────┘    └────────────┘    └────────────────────────┘ │
//! │                          │                      │               │
//! │                          ▼                      ▼               │
//! │  ┌─────────────────────────────────────────────────────────┐   │
//! │  │              Solana Bridge Program                       │   │
//! │  │  • Verify ZK proof                                       │   │
//! │  │  • Update state root                                     │   │
//! │  │  • Process withdrawals (after challenge period)          │   │
//! │  └─────────────────────────────────────────────────────────┘   │
//! └─────────────────────────────────────────────────────────────────┘
//! ```

use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use solana_client::rpc_client::RpcClient;
use solana_commitment_config::CommitmentConfig;
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    transaction::Transaction,
};
use tokio::sync::Mutex;

use crate::sequencer::prover::{BatchProof, BatchPublicInputs};
use crate::sequencer::withdrawals::TrackedWithdrawal;

// ============================================================================
// Configuration
// ============================================================================

/// Settler configuration
#[derive(Debug, Clone)]
pub struct SettlerConfig {
    /// Solana RPC URL
    pub rpc_url: String,
    /// Bridge program ID
    pub bridge_program_id: String,
    /// Domain for the bridge (e.g., "solana", "testnet")
    pub domain: [u8; 32],
    /// Confirmation commitment level
    pub commitment: CommitmentConfig,
    /// Retry attempts for RPC calls
    pub max_retries: u32,
    /// Delay between retries (ms)
    pub retry_delay_ms: u64,
}

impl Default for SettlerConfig {
    fn default() -> Self {
        let mut domain = [0u8; 32];
        domain[..6].copy_from_slice(b"solana");

        Self {
            rpc_url: "http://127.0.0.1:8899".to_string(),
            bridge_program_id: "9HXapBN9otLGnQNGv1HRk91DGqMNvMAvQqohL7gPW1sd".to_string(),
            domain,
            commitment: CommitmentConfig::confirmed(),
            max_retries: 3,
            retry_delay_ms: 1000,
        }
    }
}

// ============================================================================
// Settlement Types
// ============================================================================

/// Result of a settlement submission
#[derive(Debug, Clone)]
pub struct SettlementResult {
    /// L1 transaction signature
    pub tx_signature: String,
    /// Batch ID that was settled
    pub batch_id: u64,
    /// Whether confirmation is pending
    pub confirmed: bool,
    /// Slot at which tx was included
    pub slot: Option<u64>,
}

/// State update instruction data
#[derive(Debug)]
pub struct StateUpdateParams {
    /// Batch ID
    pub batch_id: u64,
    /// New transparent state root
    pub state_root: [u8; 32],
    /// New shielded state root (commitment tree)
    pub shielded_root: [u8; 32],
    /// Withdrawal merkle root
    pub withdrawal_root: [u8; 32],
    /// ZK proof bytes
    pub proof: Vec<u8>,
}

// ============================================================================
// Settler
// ============================================================================

/// L1 Settler service
pub struct Settler {
    config: SettlerConfig,
    rpc: RpcClient,
    sequencer_keypair: Arc<Keypair>,
    program_id: Pubkey,
}

impl Settler {
    /// Create a new settler
    pub fn new(config: SettlerConfig, sequencer_keypair: Keypair) -> Result<Self> {
        let rpc = RpcClient::new_with_commitment(config.rpc_url.clone(), config.commitment);
        let program_id =
            Pubkey::from_str(&config.bridge_program_id).context("invalid bridge program ID")?;

        Ok(Self {
            config,
            rpc,
            sequencer_keypair: Arc::new(sequencer_keypair),
            program_id,
        })
    }

    /// Get PDAs for the bridge
    fn get_pdas(&self) -> Result<(Pubkey, Pubkey)> {
        let (config_pda, _) =
            Pubkey::find_program_address(&[b"config", &self.config.domain], &self.program_id);
        let (state_pda, _) =
            Pubkey::find_program_address(&[b"state", &self.config.domain], &self.program_id);
        Ok((config_pda, state_pda))
    }

    /// Submit a batch state update to L1
    pub fn submit_state_update(&self, proof: &BatchProof) -> Result<SettlementResult> {
        let (config_pda, state_pda) = self.get_pdas()?;
        let inputs = &proof.public_inputs;

        // Build instruction data
        // Instruction discriminator: 2 = SubmitBatch
        let mut data = vec![2u8];

        // Batch ID (8 bytes)
        data.extend_from_slice(&inputs.batch_id.to_le_bytes());

        // State roots (32 bytes each)
        data.extend_from_slice(&inputs.post_state_root);
        data.extend_from_slice(&inputs.post_shielded_root);
        data.extend_from_slice(&inputs.withdrawal_root);

        // Proof length + proof bytes
        let proof_len = proof.proof_bytes.len() as u32;
        data.extend_from_slice(&proof_len.to_le_bytes());
        data.extend_from_slice(&proof.proof_bytes);

        let instruction = Instruction {
            program_id: self.program_id,
            accounts: vec![
                AccountMeta::new(self.sequencer_keypair.pubkey(), true), // payer/signer
                AccountMeta::new_readonly(config_pda, false),            // config
                AccountMeta::new(state_pda, false),                      // state
            ],
            data,
        };

        // Build and send transaction
        let recent_blockhash = self
            .rpc
            .get_latest_blockhash()
            .context("failed to get blockhash")?;

        let tx = Transaction::new_signed_with_payer(
            &[instruction],
            Some(&self.sequencer_keypair.pubkey()),
            &[&self.sequencer_keypair],
            recent_blockhash,
        );

        let signature = self
            .rpc
            .send_and_confirm_transaction(&tx)
            .context("failed to submit state update")?;

        Ok(SettlementResult {
            tx_signature: signature.to_string(),
            batch_id: inputs.batch_id,
            confirmed: true,
            slot: None, // Could fetch from tx status
        })
    }

    /// Check if a transaction is confirmed
    pub fn check_confirmation(&self, signature: &str) -> Result<bool> {
        use solana_sdk::signature::Signature;

        let sig = Signature::from_str(signature).context("invalid signature")?;

        let status = self
            .rpc
            .get_signature_status(&sig)
            .context("failed to get signature status")?;

        match status {
            Some(result) => Ok(result.is_ok()),
            None => Ok(false),
        }
    }

    /// Process a batch of withdrawals on L1
    pub fn process_withdrawals(
        &self,
        batch_id: u64,
        withdrawals: &[TrackedWithdrawal],
    ) -> Result<Vec<String>> {
        let mut signatures = Vec::new();

        // Process each withdrawal
        // Note: In production, these could be batched into fewer transactions
        for withdrawal in withdrawals {
            let sig = self.process_single_withdrawal(batch_id, withdrawal)?;
            signatures.push(sig);
        }

        Ok(signatures)
    }

    /// Process a single withdrawal
    fn process_single_withdrawal(
        &self,
        batch_id: u64,
        withdrawal: &TrackedWithdrawal,
    ) -> Result<String> {
        let (config_pda, state_pda) = self.get_pdas()?;

        // Derive withdrawal PDA
        let (withdrawal_pda, _) = Pubkey::find_program_address(
            &[b"withdrawal", &self.config.domain, &withdrawal.tx_hash],
            &self.program_id,
        );

        // Recipient pubkey
        let recipient = Pubkey::new_from_array(withdrawal.to_l1_address);

        // Build instruction data
        // Instruction discriminator: 3 = ProcessWithdrawal
        let mut data = vec![3u8];
        data.extend_from_slice(&batch_id.to_le_bytes());
        data.extend_from_slice(&withdrawal.tx_hash);
        data.extend_from_slice(&withdrawal.amount.to_le_bytes());

        let instruction = Instruction {
            program_id: self.program_id,
            accounts: vec![
                AccountMeta::new(self.sequencer_keypair.pubkey(), true), // payer/signer
                AccountMeta::new_readonly(config_pda, false),            // config
                AccountMeta::new_readonly(state_pda, false),             // state
                AccountMeta::new(withdrawal_pda, false),                 // withdrawal record
                AccountMeta::new(recipient, false),                      // recipient
                AccountMeta::new_readonly(
                    Pubkey::from_str("11111111111111111111111111111111")?,
                    false,
                ), // system program
            ],
            data,
        };

        let recent_blockhash = self.rpc.get_latest_blockhash()?;

        let tx = Transaction::new_signed_with_payer(
            &[instruction],
            Some(&self.sequencer_keypair.pubkey()),
            &[&self.sequencer_keypair],
            recent_blockhash,
        );

        let signature = self
            .rpc
            .send_and_confirm_transaction(&tx)
            .context("failed to process withdrawal")?;

        Ok(signature.to_string())
    }

    /// Get the current L1 state
    pub fn get_l1_state(&self) -> Result<L1State> {
        let (_, state_pda) = self.get_pdas()?;

        let account = self
            .rpc
            .get_account(&state_pda)
            .context("failed to get state account")?;

        // Parse state account data
        // Format: batch_id (8) + state_root (32) + shielded_root (32)
        let data = account.data;
        if data.len() < 72 {
            bail!("invalid state account data");
        }

        let batch_id = u64::from_le_bytes(data[0..8].try_into().unwrap());
        let state_root: [u8; 32] = data[8..40].try_into().unwrap();
        let shielded_root: [u8; 32] = data[40..72].try_into().unwrap();

        Ok(L1State {
            batch_id,
            state_root,
            shielded_root,
        })
    }

    /// Get sequencer SOL balance
    pub fn get_balance(&self) -> Result<u64> {
        let balance = self
            .rpc
            .get_balance(&self.sequencer_keypair.pubkey())
            .context("failed to get balance")?;
        Ok(balance)
    }
}

/// Current L1 state
#[derive(Debug, Clone)]
pub struct L1State {
    pub batch_id: u64,
    pub state_root: [u8; 32],
    pub shielded_root: [u8; 32],
}

// ============================================================================
// Async Settler Service
// ============================================================================

/// Async wrapper for settlement operations
pub struct SettlerService {
    settler: Arc<Mutex<Settler>>,
}

impl SettlerService {
    /// Create a new settler service
    pub fn new(config: SettlerConfig, sequencer_keypair: Keypair) -> Result<Self> {
        let settler = Settler::new(config, sequencer_keypair)?;
        Ok(Self {
            settler: Arc::new(Mutex::new(settler)),
        })
    }

    /// Submit a batch for settlement
    pub async fn submit(&self, proof: &BatchProof) -> Result<SettlementResult> {
        let settler = self.settler.lock().await;
        settler.submit_state_update(proof)
    }

    /// Wait for confirmation
    pub async fn wait_confirmation(&self, signature: &str, timeout: Duration) -> Result<bool> {
        let start = std::time::Instant::now();
        let check_interval = Duration::from_millis(500);

        while start.elapsed() < timeout {
            let settler = self.settler.lock().await;
            if settler.check_confirmation(signature)? {
                return Ok(true);
            }
            drop(settler);
            tokio::time::sleep(check_interval).await;
        }

        Ok(false)
    }

    /// Get current L1 state
    pub async fn get_state(&self) -> Result<L1State> {
        let settler = self.settler.lock().await;
        settler.get_l1_state()
    }

    /// Process withdrawals
    pub async fn process_withdrawals(
        &self,
        batch_id: u64,
        withdrawals: &[TrackedWithdrawal],
    ) -> Result<Vec<String>> {
        let settler = self.settler.lock().await;
        settler.process_withdrawals(batch_id, withdrawals)
    }
}

// ============================================================================
// Mock Settler (for testing)
// ============================================================================

/// Mock settler for testing without L1
pub struct MockSettler {
    batch_id: u64,
    state_root: [u8; 32],
    shielded_root: [u8; 32],
}

impl MockSettler {
    pub fn new() -> Self {
        Self {
            batch_id: 0,
            state_root: [0u8; 32],
            shielded_root: [0u8; 32],
        }
    }

    pub fn submit(&mut self, proof: &BatchProof) -> SettlementResult {
        let inputs = &proof.public_inputs;

        // Update mock state
        self.batch_id = inputs.batch_id;
        self.state_root = inputs.post_state_root;
        self.shielded_root = inputs.post_shielded_root;

        // Generate fake signature
        let sig = format!("mock_sig_{}", inputs.batch_id);

        SettlementResult {
            tx_signature: sig,
            batch_id: inputs.batch_id,
            confirmed: true,
            slot: Some(100),
        }
    }

    pub fn get_state(&self) -> L1State {
        L1State {
            batch_id: self.batch_id,
            state_root: self.state_root,
            shielded_root: self.shielded_root,
        }
    }
}

impl Default for MockSettler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_settler_config_default() {
        let config = SettlerConfig::default();
        assert!(config.domain.starts_with(b"solana"));
        assert_eq!(config.max_retries, 3);
    }

    #[test]
    fn test_mock_settler() {
        let mut settler = MockSettler::new();

        let proof = BatchProof {
            public_inputs: BatchPublicInputs {
                pre_state_root: [1u8; 32],
                post_state_root: [2u8; 32],
                pre_shielded_root: [3u8; 32],
                post_shielded_root: [4u8; 32],
                withdrawal_root: [5u8; 32],
                batch_hash: [6u8; 32],
                batch_id: 42,
            },
            proof_bytes: vec![0u8; 256],
            proving_time_ms: 100,
        };

        let result = settler.submit(&proof);
        assert_eq!(result.batch_id, 42);
        assert!(result.confirmed);

        let state = settler.get_state();
        assert_eq!(state.batch_id, 42);
        assert_eq!(state.state_root, [2u8; 32]);
    }
}
