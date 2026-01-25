#![allow(dead_code)] // Pending: L1 settlement phase
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
use solana_compute_budget_interface::ComputeBudgetInstruction;
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    transaction::Transaction,
};
use tokio::sync::Mutex;

use crate::sequencer::BatchProof;
use crate::sequencer::TrackedWithdrawal;
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
    /// Verifier program ID
    pub verifier_program_id: String,
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
            bridge_program_id: "8SE6gCijcFQixvDQqWu29mCm9AydN8hcwWh2e2Q6RQgE".to_string(),
            verifier_program_id: "8TveT3mvH59qLzZNwrTT6hBqDHEobW2XnCPb7xZLBYHd".to_string(),
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
    verifier_program_id: Pubkey,
}

impl Settler {
    /// Create a new settler
    pub fn new(config: SettlerConfig, sequencer_keypair: Keypair) -> Result<Self> {
        let rpc = RpcClient::new_with_commitment(config.rpc_url.clone(), config.commitment);
        let program_id =
            Pubkey::from_str(&config.bridge_program_id).context("invalid bridge program ID")?;
        let verifier_program_id =
            Pubkey::from_str(&config.verifier_program_id).context("invalid verifier program ID")?;

        Ok(Self {
            config,
            rpc,
            sequencer_keypair: Arc::new(sequencer_keypair),
            program_id,
            verifier_program_id,
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

    /// Get VK PDA for the verifier
    fn get_vk_pda(&self) -> Pubkey {
        let (vk_pda, _) = Pubkey::find_program_address(
            &[b"batch_vk", &self.config.domain],
            &self.verifier_program_id,
        );
        vk_pda
    }

    /// Submit a batch state update to L1
    pub fn submit_state_update(
        &self,
        proof: &BatchProof,
        prev_batch_id: u64,
    ) -> Result<SettlementResult> {
        self.submit_state_update_with_withdrawals(proof, prev_batch_id, &[])
    }

    /// Submit a batch state update to L1 with withdrawals
    pub fn submit_state_update_with_withdrawals(
        &self,
        proof: &BatchProof,
        prev_batch_id: u64,
        withdrawals: &[TrackedWithdrawal],
    ) -> Result<SettlementResult> {
        let (config_pda, _state_pda) = self.get_pdas()?;
        let vk_pda = self.get_vk_pda();
        let inputs = &proof.public_inputs;

        // Build instruction data
        // Instruction discriminator: 3 = SubmitBatch
        let mut data = vec![3u8];

        // SubmitBatchHeader (packed, C repr): 56 bytes total
        // prev_batch_index: u64 (8 bytes)
        data.extend_from_slice(&prev_batch_id.to_le_bytes());
        // new_batch_index: u64 (8 bytes)
        data.extend_from_slice(&inputs.batch_id.to_le_bytes());
        // new_state_root: [u8; 32] (32 bytes)
        data.extend_from_slice(&inputs.post_state_root);
        // proof_len: u32 (4 bytes) - Groth16Proof = 256 bytes
        let proof_len = proof.proof_bytes.len() as u32;
        data.extend_from_slice(&proof_len.to_le_bytes());
        // withdrawal_count: u32 (4 bytes)
        let withdrawal_count = withdrawals.len() as u32;
        data.extend_from_slice(&withdrawal_count.to_le_bytes());

        let header_end = data.len();
        tracing::info!(
            "SubmitBatchHeader: {} bytes (expected 57 = 1 discriminator + 56 header)",
            header_end
        );

        // Proof bytes (Groth16Proof: pi_a (64) + pi_b (128) + pi_c (64) = 256 bytes)
        if proof.proof_bytes.len() != 256 {
            tracing::error!(
                "Invalid proof length: {} bytes (expected 256)",
                proof.proof_bytes.len()
            );
            bail!(
                "proof must be exactly 256 bytes, got {}",
                proof.proof_bytes.len()
            );
        }
        data.extend_from_slice(&proof.proof_bytes);

        let proof_end = data.len();
        tracing::info!("After proof: {} bytes (expected 313 = 57 + 256)", proof_end);

        // BatchPublicInputs (for CPI to verifier): 200 bytes total
        // pre_state_root: [u8; 32]
        data.extend_from_slice(&inputs.pre_state_root);
        // post_state_root: [u8; 32]
        data.extend_from_slice(&inputs.post_state_root);
        // pre_shielded_root: [u8; 32]
        data.extend_from_slice(&inputs.pre_shielded_root);
        // post_shielded_root: [u8; 32]
        data.extend_from_slice(&inputs.post_shielded_root);
        // withdrawal_root: [u8; 32]
        data.extend_from_slice(&inputs.withdrawal_root);
        // batch_hash: [u8; 32]
        data.extend_from_slice(&inputs.batch_hash);
        // batch_id: u64
        data.extend_from_slice(&inputs.batch_id.to_le_bytes());

        let inputs_end = data.len();
        tracing::info!(
            "After public inputs: {} bytes (expected 513 = 313 + 200)",
            inputs_end
        );

        // Append WithdrawalRequest structs (recipient: Pubkey (32) + amount: u64 (8) = 40 bytes each)
        for withdrawal in withdrawals {
            // recipient: [u8; 32]
            data.extend_from_slice(&withdrawal.to_l1_address);
            // amount: u64
            data.extend_from_slice(&withdrawal.amount.to_le_bytes());
        }

        tracing::info!(
            "Total instruction data: {} bytes (with {} withdrawals)",
            data.len(),
            withdrawals.len()
        );

        // Derive vault PDA
        let (_vault_pda, _) =
            Pubkey::find_program_address(&[b"vault", &self.config.domain], &self.program_id);

        // Build accounts list
        // Accounts order per updated Bridge IDL:
        // 0. sequencer (signer)
        // 1. config (writable)
        // 2. verifier_program
        // 3. vk_account
        // 4+. recipient accounts for each withdrawal
        let mut accounts = vec![
            AccountMeta::new(self.sequencer_keypair.pubkey(), true), // sequencer (signer)
            AccountMeta::new(config_pda, false),                     // config (writable)
            AccountMeta::new_readonly(self.verifier_program_id, false), // verifier program
            AccountMeta::new_readonly(vk_pda, false),                // vk_account
        ];

        // Add recipient accounts for each withdrawal
        for withdrawal in withdrawals {
            let recipient = Pubkey::new_from_array(withdrawal.to_l1_address);
            accounts.push(AccountMeta::new(recipient, false)); // recipient (writable for SOL transfer)
        }

        let instruction = Instruction {
            program_id: self.program_id,
            accounts,
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

        let signature = self.rpc.send_and_confirm_transaction(&tx).map_err(|e| {
            tracing::error!("Settlement transaction failed: {:?}", e);
            anyhow::anyhow!("failed to submit state update: {}", e)
        })?;

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

    /// Submit a Noir/Sunspot proof for verification on L1
    ///
    /// This method submits the proof directly to the Sunspot verifier program
    /// for on-chain verification. Unlike Groth16, Sunspot uses a different
    /// instruction format with 388-byte proofs and 236-byte public witnesses.
    ///
    /// # Arguments
    /// * `proof` - The BatchProof containing Noir proof data (388 bytes)
    /// * `prev_batch_id` - Previous batch ID for state continuity
    ///
    /// # Returns
    /// * `SettlementResult` with L1 transaction signature
    ///
    /// # Note
    /// This method calls through the Bridge program. If Bridge is not deployed,
    /// use `verify_sunspot_direct` instead for standalone verification.
    pub fn submit_sunspot_proof(
        &self,
        proof: &BatchProof,
        prev_batch_id: u64,
    ) -> Result<SettlementResult> {
        // Validate this is a Noir proof
        let noir_proof = NoirProofData::from_batch_proof(proof)?;
        noir_proof.validate()?;

        let (config_pda, _state_pda) = self.get_pdas()?;
        let inputs = &proof.public_inputs;

        // Get Sunspot verifier program ID
        let sunspot_verifier = Pubkey::from_str(SUNSPOT_VERIFIER_PROGRAM_ID)
            .context("invalid Sunspot verifier program ID")?;

        // Derive VK PDA for Sunspot verifier (uses same pattern as arkworks verifier)
        let (vk_pda, _) =
            Pubkey::find_program_address(&[b"batch_vk", &self.config.domain], &sunspot_verifier);

        // Build instruction data for Sunspot verification
        // Instruction discriminator: 3 = SubmitBatch (same as Groth16 flow)
        let mut data = vec![3u8];

        // SubmitBatchHeader: 56 bytes
        // prev_batch_index: u64 (8 bytes)
        data.extend_from_slice(&prev_batch_id.to_le_bytes());
        // new_batch_index: u64 (8 bytes)
        data.extend_from_slice(&inputs.batch_id.to_le_bytes());
        // new_state_root: [u8; 32] (32 bytes)
        data.extend_from_slice(&inputs.post_state_root);
        // proof_len: u32 (4 bytes) - Sunspot = 388 bytes
        let proof_len = noir_proof.proof_bytes.len() as u32;
        data.extend_from_slice(&proof_len.to_le_bytes());
        // withdrawal_count: u32 (4 bytes) - 0 for now
        let withdrawal_count: u32 = 0;
        data.extend_from_slice(&withdrawal_count.to_le_bytes());

        let header_end = data.len();
        tracing::info!(
            "Sunspot SubmitBatchHeader: {} bytes (expected 57)",
            header_end
        );

        // Proof bytes (388 bytes for Sunspot)
        data.extend_from_slice(&noir_proof.proof_bytes);

        let proof_end = data.len();
        tracing::info!(
            "After Sunspot proof: {} bytes (expected {})",
            proof_end,
            57 + NoirProofData::PROOF_SIZE
        );

        // Public witness (236 bytes for Sunspot)
        data.extend_from_slice(&noir_proof.public_witness);

        tracing::info!("Total Sunspot instruction data: {} bytes", data.len());

        // Build accounts list for Sunspot verification
        // Accounts order matches Bridge IDL:
        // 0. sequencer (signer)
        // 1. config (writable)
        // 2. verifier_program (Sunspot)
        // 3. vk_account
        let accounts = vec![
            AccountMeta::new(self.sequencer_keypair.pubkey(), true), // sequencer (signer)
            AccountMeta::new(config_pda, false),                     // config (writable)
            AccountMeta::new_readonly(sunspot_verifier, false),      // Sunspot verifier program
            AccountMeta::new_readonly(vk_pda, false),                // vk_account
        ];

        let instruction = Instruction {
            program_id: self.program_id,
            accounts,
            data,
        };

        // Compute budget: Sunspot verification uses ~500k CU
        let compute_budget_ix = ComputeBudgetInstruction::set_compute_unit_limit(500_000);
        let priority_fee_ix = ComputeBudgetInstruction::set_compute_unit_price(1000);

        // Build and send transaction
        let recent_blockhash = self
            .rpc
            .get_latest_blockhash()
            .context("failed to get blockhash")?;

        let tx = Transaction::new_signed_with_payer(
            &[compute_budget_ix, priority_fee_ix, instruction],
            Some(&self.sequencer_keypair.pubkey()),
            &[&self.sequencer_keypair],
            recent_blockhash,
        );

        let signature = self.rpc.send_and_confirm_transaction(&tx).map_err(|e| {
            tracing::error!("Sunspot settlement transaction failed: {:?}", e);
            anyhow::anyhow!("failed to submit Sunspot proof: {}", e)
        })?;

        tracing::info!(
            "Sunspot proof verified on L1: batch={}, tx={}",
            inputs.batch_id,
            signature
        );

        Ok(SettlementResult {
            tx_signature: signature.to_string(),
            batch_id: inputs.batch_id,
            confirmed: true,
            slot: None,
        })
    }

    /// Verify a Noir/Sunspot proof directly against the Sunspot verifier program.
    ///
    /// This bypasses the Bridge program and calls the Sunspot verifier directly.
    /// Use this for testing or when Bridge is not deployed.
    ///
    /// The Sunspot verifier expects:
    /// - No accounts (VK is embedded in the program)
    /// - Instruction data: proof_bytes (388) + public_witness_bytes (236)
    ///
    /// # Arguments
    /// * `proof` - The batch proof with 388-byte Noir proof
    ///
    /// # Returns
    /// * `SettlementResult` with L1 transaction signature
    pub fn verify_sunspot_direct(&self, proof: &BatchProof) -> Result<SettlementResult> {
        // Validate this is a Noir proof
        let noir_proof = NoirProofData::from_batch_proof(proof)?;
        noir_proof.validate()?;

        let inputs = &proof.public_inputs;

        // Get Sunspot verifier program ID
        let sunspot_verifier = Pubkey::from_str(SUNSPOT_VERIFIER_PROGRAM_ID)
            .context("invalid Sunspot verifier program ID")?;

        // Build instruction data: proof + public_witness (no header, no accounts)
        let mut instruction_data =
            Vec::with_capacity(NoirProofData::PROOF_SIZE + NoirProofData::PUBLIC_WITNESS_SIZE);
        instruction_data.extend_from_slice(&noir_proof.proof_bytes);
        instruction_data.extend_from_slice(&noir_proof.public_witness);

        tracing::info!(
            "Sunspot direct verification: {} bytes (proof={}, pw={})",
            instruction_data.len(),
            noir_proof.proof_bytes.len(),
            noir_proof.public_witness.len()
        );

        // Create verify instruction (no accounts needed for Sunspot verifier)
        let verify_ix = Instruction {
            program_id: sunspot_verifier,
            accounts: vec![],
            data: instruction_data,
        };

        // Compute budget: Sunspot verification uses ~500k CU
        let compute_budget_ix = ComputeBudgetInstruction::set_compute_unit_limit(500_000);
        let priority_fee_ix = ComputeBudgetInstruction::set_compute_unit_price(1000);

        // Build and send transaction
        let recent_blockhash = self
            .rpc
            .get_latest_blockhash()
            .context("failed to get blockhash")?;

        let tx = Transaction::new_signed_with_payer(
            &[compute_budget_ix, priority_fee_ix, verify_ix],
            Some(&self.sequencer_keypair.pubkey()),
            &[&self.sequencer_keypair],
            recent_blockhash,
        );

        let signature = self.rpc.send_and_confirm_transaction(&tx).map_err(|e| {
            tracing::error!("Sunspot direct verification failed: {:?}", e);
            anyhow::anyhow!("failed to verify Sunspot proof: {}", e)
        })?;

        tracing::info!(
            "Sunspot proof verified directly on L1: batch={}, tx={}",
            inputs.batch_id,
            signature
        );

        Ok(SettlementResult {
            tx_signature: signature.to_string(),
            batch_id: inputs.batch_id,
            confirmed: true,
            slot: None,
        })
    }

    /// Determine if a proof is a Noir/Sunspot proof based on size
    pub fn is_noir_proof(proof: &BatchProof) -> bool {
        proof.proof_bytes.len() == NoirProofData::PROOF_SIZE
    }

    /// Submit proof with automatic format detection
    ///
    /// Automatically detects whether the proof is Groth16 (256 bytes) or
    /// Noir/Sunspot (388 bytes) and routes to the appropriate submission method.
    pub fn submit_proof_auto(
        &self,
        proof: &BatchProof,
        prev_batch_id: u64,
    ) -> Result<SettlementResult> {
        if Self::is_noir_proof(proof) {
            tracing::info!(
                "Detected Noir/Sunspot proof ({} bytes), using Sunspot verifier",
                proof.proof_bytes.len()
            );
            self.submit_sunspot_proof(proof, prev_batch_id)
        } else {
            tracing::info!(
                "Detected Groth16 proof ({} bytes), using arkworks verifier",
                proof.proof_bytes.len()
            );
            self.submit_state_update(proof, prev_batch_id)
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

    /// Execute withdrawals in batched Solana transactions with retry logic
    ///
    /// Groups withdrawals into batches of WITHDRAWAL_BATCH_SIZE and submits each batch
    /// as a single Solana transaction. Failed individual withdrawals are retried up to
    /// MAX_WITHDRAWAL_RETRIES times.
    ///
    /// Returns a list of withdrawal results including success/failure status.
    pub fn execute_withdrawals_batched(
        &self,
        batch_id: u64,
        withdrawals: &[TrackedWithdrawal],
    ) -> Vec<WithdrawalExecutionResult> {
        const WITHDRAWAL_BATCH_SIZE: usize = 6;
        const MAX_WITHDRAWAL_RETRIES: u32 = 3;

        let mut results = Vec::with_capacity(withdrawals.len());

        // Process in batches
        for chunk in withdrawals.chunks(WITHDRAWAL_BATCH_SIZE) {
            match self.execute_withdrawal_batch(batch_id, chunk) {
                Ok(signatures) => {
                    // All withdrawals in batch succeeded
                    for (i, w) in chunk.iter().enumerate() {
                        results.push(WithdrawalExecutionResult {
                            tx_hash: w.tx_hash,
                            success: true,
                            l1_signature: Some(signatures[i].clone()),
                            error: None,
                            retries: 0,
                        });
                    }
                }
                Err(batch_error) => {
                    // Batch failed, try individual withdrawals with retry
                    tracing::warn!(
                        "Batch withdrawal failed: {}. Retrying individually.",
                        batch_error
                    );

                    for w in chunk {
                        let mut retries = 0;
                        let mut last_error = None;

                        while retries < MAX_WITHDRAWAL_RETRIES {
                            match self.process_single_withdrawal(batch_id, w) {
                                Ok(sig) => {
                                    results.push(WithdrawalExecutionResult {
                                        tx_hash: w.tx_hash,
                                        success: true,
                                        l1_signature: Some(sig),
                                        error: None,
                                        retries,
                                    });
                                    break;
                                }
                                Err(e) => {
                                    retries += 1;
                                    last_error = Some(e.to_string());
                                    tracing::warn!(
                                        "Withdrawal {:?} retry {}/{}: {}",
                                        hex::encode(&w.tx_hash[..8]),
                                        retries,
                                        MAX_WITHDRAWAL_RETRIES,
                                        last_error.as_ref().unwrap()
                                    );

                                    // Brief delay before retry
                                    std::thread::sleep(std::time::Duration::from_millis(500));
                                }
                            }
                        }

                        // If we exhausted retries, record failure
                        if retries >= MAX_WITHDRAWAL_RETRIES {
                            results.push(WithdrawalExecutionResult {
                                tx_hash: w.tx_hash,
                                success: false,
                                l1_signature: None,
                                error: last_error,
                                retries,
                            });
                        }
                    }
                }
            }
        }

        results
    }

    /// Execute a batch of withdrawals in a single Solana transaction
    fn execute_withdrawal_batch(
        &self,
        _batch_id: u64,
        withdrawals: &[TrackedWithdrawal],
    ) -> Result<Vec<String>> {
        if withdrawals.is_empty() {
            return Ok(Vec::new());
        }

        let (config_pda, _state_pda) = self.get_pdas()?;

        // Derive vault PDA for SOL transfer
        let (vault_pda, _) =
            Pubkey::find_program_address(&[b"vault", &self.config.domain], &self.program_id);

        // Build multiple instructions, one per withdrawal
        let mut instructions = Vec::with_capacity(withdrawals.len());
        let mut sigs = Vec::with_capacity(withdrawals.len());

        for withdrawal in withdrawals {
            // Recipient pubkey
            let recipient = Pubkey::new_from_array(withdrawal.to_l1_address);

            // Derive nullifier PDA (use tx_hash as nullifier to prevent replay)
            let (nullifier_pda, _) = Pubkey::find_program_address(
                &[b"nullifier", &self.config.domain, &withdrawal.tx_hash],
                &self.program_id,
            );

            // Build instruction data matching WithdrawAttestedParams layout:
            // - recipient: Pubkey (32 bytes)
            // - amount: u64 (8 bytes)
            // - nullifier: [u8; 32] (32 bytes)
            // Instruction discriminator: 2 = WithdrawAttested
            let mut data = vec![2u8];
            data.extend_from_slice(&withdrawal.to_l1_address); // recipient (32 bytes)
            data.extend_from_slice(&withdrawal.amount.to_le_bytes()); // amount (8 bytes)
            data.extend_from_slice(&withdrawal.tx_hash); // nullifier (32 bytes) - using tx_hash

            // Accounts per IDL:
            // 0. sequencer (signer)
            // 1. config
            // 2. vault (writable)
            // 3. recipient (writable)
            // 4. used_nullifier (writable)
            // 5. system_program
            let instruction = Instruction {
                program_id: self.program_id,
                accounts: vec![
                    AccountMeta::new(self.sequencer_keypair.pubkey(), true), // sequencer (signer)
                    AccountMeta::new_readonly(config_pda, false),            // config
                    AccountMeta::new(vault_pda, false),                      // vault (writable)
                    AccountMeta::new(recipient, false),                      // recipient (writable)
                    AccountMeta::new(nullifier_pda, false), // used_nullifier (writable)
                    AccountMeta::new_readonly(
                        Pubkey::from_str("11111111111111111111111111111111").unwrap(),
                        false,
                    ), // system program
                ],
                data,
            };

            instructions.push(instruction);
        }

        // Build and send transaction
        let recent_blockhash = self
            .rpc
            .get_latest_blockhash()
            .context("failed to get blockhash")?;

        let tx = Transaction::new_signed_with_payer(
            &instructions,
            Some(&self.sequencer_keypair.pubkey()),
            &[&self.sequencer_keypair],
            recent_blockhash,
        );

        let signature = self
            .rpc
            .send_and_confirm_transaction(&tx)
            .context("failed to execute withdrawal batch")?;

        // All withdrawals in this batch share the same L1 signature
        let sig_str = signature.to_string();
        for _ in withdrawals {
            sigs.push(sig_str.clone());
        }

        Ok(sigs)
    }
}

/// Result of executing a single withdrawal
#[derive(Debug, Clone)]
pub struct WithdrawalExecutionResult {
    /// L2 transaction hash
    pub tx_hash: [u8; 32],
    /// Whether the withdrawal succeeded
    pub success: bool,
    /// L1 transaction signature (if successful)
    pub l1_signature: Option<String>,
    /// Error message (if failed)
    pub error: Option<String>,
    /// Number of retries attempted
    pub retries: u32,
}

// ============================================================================
// Noir/Sunspot Proof Types
// ============================================================================

/// Sunspot verifier program ID (devnet verified)
pub const SUNSPOT_VERIFIER_PROGRAM_ID: &str = "EZzyLrTrC4uyU488jVAs4GKeCR1s9GmoFggeiDqwDeNK";

/// Noir proof format for Sunspot verification
#[derive(Debug, Clone)]
pub struct NoirProofData {
    /// Proof bytes (388 bytes for Noir/Sunspot)
    pub proof_bytes: Vec<u8>,
    /// Public witness (236 bytes - 7 field elements)
    pub public_witness: Vec<u8>,
}

impl NoirProofData {
    /// Expected proof size for Noir/Sunspot proofs
    pub const PROOF_SIZE: usize = 388;
    /// Expected public witness size (7 field elements × ~33-34 bytes)
    pub const PUBLIC_WITNESS_SIZE: usize = 236;

    /// Validate proof format
    pub fn validate(&self) -> Result<()> {
        if self.proof_bytes.len() != Self::PROOF_SIZE {
            bail!(
                "invalid Noir proof size: {} bytes (expected {})",
                self.proof_bytes.len(),
                Self::PROOF_SIZE
            );
        }
        if self.public_witness.len() != Self::PUBLIC_WITNESS_SIZE {
            bail!(
                "invalid public witness size: {} bytes (expected {})",
                self.public_witness.len(),
                Self::PUBLIC_WITNESS_SIZE
            );
        }
        Ok(())
    }

    /// Build from BatchProof (for proofs generated by Noir prover)
    pub fn from_batch_proof(proof: &BatchProof) -> Result<Self> {
        // Check if this is a Noir proof (388 bytes) or Groth16 (256 bytes)
        if proof.proof_bytes.len() == Self::PROOF_SIZE {
            // Extract public witness from BatchPublicInputs
            let mut public_witness = Vec::with_capacity(Self::PUBLIC_WITNESS_SIZE);

            // 7 public inputs for Noir circuit:
            // 1. pre_state_root (32 bytes + length prefix)
            // 2. post_state_root (32 bytes + length prefix)
            // 3. pre_shielded_root (32 bytes + length prefix)
            // 4. post_shielded_root (32 bytes + length prefix)
            // 5. withdrawal_root (32 bytes + length prefix)
            // 6. batch_hash (32 bytes + length prefix)
            // 7. batch_id (field element)

            // For now, build the witness as the raw field elements
            // The prover-coordinator returns formatted public witness
            public_witness.extend_from_slice(&proof.public_inputs.pre_state_root);
            public_witness.extend_from_slice(&proof.public_inputs.post_state_root);
            public_witness.extend_from_slice(&proof.public_inputs.pre_shielded_root);
            public_witness.extend_from_slice(&proof.public_inputs.post_shielded_root);
            public_witness.extend_from_slice(&proof.public_inputs.withdrawal_root);
            public_witness.extend_from_slice(&proof.public_inputs.batch_hash);
            public_witness.extend_from_slice(&proof.public_inputs.batch_id.to_le_bytes());

            // Pad to expected size if needed (Noir uses 32-byte fields)
            while public_witness.len() < Self::PUBLIC_WITNESS_SIZE {
                public_witness.push(0);
            }

            Ok(Self {
                proof_bytes: proof.proof_bytes.clone(),
                public_witness,
            })
        } else {
            bail!(
                "proof is not a Noir proof: {} bytes (expected {})",
                proof.proof_bytes.len(),
                Self::PROOF_SIZE
            );
        }
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

    /// Submit a batch for settlement (without withdrawals)
    pub async fn submit(&self, proof: &BatchProof, prev_batch_id: u64) -> Result<SettlementResult> {
        let settler = self.settler.lock().await;
        settler.submit_state_update(proof, prev_batch_id)
    }

    /// Submit a batch for settlement with withdrawals
    pub async fn submit_with_withdrawals(
        &self,
        proof: &BatchProof,
        prev_batch_id: u64,
        withdrawals: &[TrackedWithdrawal],
    ) -> Result<SettlementResult> {
        let settler = self.settler.lock().await;
        settler.submit_state_update_with_withdrawals(proof, prev_batch_id, withdrawals)
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

    /// Execute withdrawals with batching and retry logic
    ///
    /// Groups withdrawals into batches of 6 and submits each batch as a single
    /// Solana transaction. Failed individual withdrawals are retried up to 3 times.
    pub async fn execute_withdrawals_batched(
        &self,
        batch_id: u64,
        withdrawals: &[TrackedWithdrawal],
    ) -> Vec<WithdrawalExecutionResult> {
        let settler = self.settler.lock().await;
        settler.execute_withdrawals_batched(batch_id, withdrawals)
    }

    /// Submit a Noir/Sunspot proof for verification on L1
    ///
    /// Uses the Sunspot verifier program for 388-byte Noir proofs.
    pub async fn submit_sunspot(
        &self,
        proof: &BatchProof,
        prev_batch_id: u64,
    ) -> Result<SettlementResult> {
        let settler = self.settler.lock().await;
        settler.submit_sunspot_proof(proof, prev_batch_id)
    }

    /// Submit proof with automatic format detection
    ///
    /// Automatically detects whether the proof is Groth16 (256 bytes) or
    /// Noir/Sunspot (388 bytes) and routes to the appropriate verifier.
    pub async fn submit_auto(
        &self,
        proof: &BatchProof,
        prev_batch_id: u64,
    ) -> Result<SettlementResult> {
        let settler = self.settler.lock().await;
        settler.submit_proof_auto(proof, prev_batch_id)
    }

    /// Check if a proof is a Noir/Sunspot proof
    pub fn is_noir_proof(proof: &BatchProof) -> bool {
        Settler::is_noir_proof(proof)
    }

    /// Verify a Noir/Sunspot proof directly against the Sunspot verifier.
    ///
    /// This bypasses the Bridge program and calls the Sunspot verifier directly.
    /// Use this for testing or when Bridge is not deployed.
    pub async fn verify_sunspot_direct(&self, proof: &BatchProof) -> Result<SettlementResult> {
        let settler = self.settler.lock().await;
        settler.verify_sunspot_direct(proof)
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
    use crate::sequencer::settlement::prover::BatchPublicInputs;

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
