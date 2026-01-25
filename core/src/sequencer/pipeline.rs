#![allow(dead_code)] // seal/pause/resume reserved for operator controls
//! Pipeline Orchestrator
//!
//! Connects BatchService, Prover, and Settler into a complete pipeline.
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────────┐
//! │                         Pipeline Orchestrator                            │
//! │                                                                          │
//! │  ┌─────────────┐    ┌─────────────┐    ┌─────────────┐    ┌───────────┐ │
//! │  │ Accumulating│───▶│   Sealed    │───▶│   Proving   │───▶│  Proved   │ │
//! │  │             │    │             │    │             │    │           │ │
//! │  └─────────────┘    └─────────────┘    └─────────────┘    └───────────┘ │
//! │         │                  │                  │                  │      │
//! │         │                  │                  │                  ▼      │
//! │         │                  │                  │           ┌───────────┐ │
//! │         │                  │                  │           │  Settling │ │
//! │         │                  │                  │           │           │ │
//! │         │                  │                  │           └───────────┘ │
//! │         │                  │                  │                  │      │
//! │         ▼                  ▼                  ▼                  ▼      │
//! │  ┌─────────────────────────────────────────────────────────────────────┐│
//! │  │                        Parallel Execution                           ││
//! │  │  • Batch N accumulating while Batch N-1 proving                     ││
//! │  │  • Batch N-2 settling while both above in progress                  ││
//! │  └─────────────────────────────────────────────────────────────────────┘│
//! └─────────────────────────────────────────────────────────────────────────┘
//! ```

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use tokio::sync::{Mutex, mpsc, oneshot};
use tracing::{debug, error, info, warn};

use crate::api::types::{BatchStatus, BatchSummary, TxStatus};
use crate::sequencer::bridge::withdrawals::{
    TrackedWithdrawal, WithdrawalState, build_withdrawal_merkle_root,
};
use crate::sequencer::execution::batch::{BatchConfig, BatchManager, BatchManagerStats};
use crate::sequencer::execution::tx_router::TxResultType;
use crate::sequencer::settlement::noir_client::{NoirProverClient, NoirProverConfig};
use crate::sequencer::settlement::prover::compute_batch_hash;
use crate::sequencer::settlement::prover::{
    BatchProof, BatchProver, BatchPublicInputs, Groth16Prover, MockProver, build_public_inputs,
    build_witness,
};
use crate::sequencer::settlement::settler::{
    MockSettler, SettlementResult, SettlerConfig, SettlerService,
};
use crate::sequencer::storage::db::RocksDbStore;
use zelana_transaction::TransactionType;

// Configuration

/// Prover mode selection
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ProverMode {
    /// Mock prover for testing (default)
    #[default]
    Mock,
    /// Real Groth16 prover using arkworks
    Groth16,
    /// Noir/Sunspot prover via HTTP coordinator
    Noir,
}

/// Pipeline configuration
#[derive(Debug, Clone)]
pub struct PipelineConfig {
    /// Prover mode selection (Mock, Groth16, or Noir)
    pub prover_mode: ProverMode,
    /// Path to proving key (for Groth16 mode)
    pub proving_key_path: Option<String>,
    /// Path to verifying key (for Groth16 mode)
    pub verifying_key_path: Option<String>,
    /// Noir coordinator URL (for Noir mode, e.g., "http://localhost:8080")
    pub noir_coordinator_url: Option<String>,
    /// Noir proof generation timeout in seconds (default: 300)
    pub noir_proof_timeout_secs: Option<u64>,
    pub settlement_enabled: bool,
    pub sequencer_keypair_path: Option<String>,
    /// Maximum retry attempts for settlement
    pub max_settlement_retries: u32,
    /// Base delay between settlement retries (exponential backoff)
    pub settlement_retry_base_ms: u64,
    /// Interval to poll for pipeline work (ms)
    pub poll_interval_ms: u64,
    /// Batch configuration
    pub batch_config: BatchConfig,
    /// Settler configuration (if settlement enabled)
    pub settler_config: Option<SettlerConfig>,
    /// Dev mode - enables immediate state commit on seal (bypasses prove/settle)
    pub dev_mode: bool,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            prover_mode: ProverMode::Mock,
            proving_key_path: None,
            verifying_key_path: None,
            noir_coordinator_url: None,
            noir_proof_timeout_secs: None,
            settlement_enabled: false,
            sequencer_keypair_path: None,
            max_settlement_retries: 5,
            settlement_retry_base_ms: 5000,
            poll_interval_ms: 100,
            batch_config: BatchConfig::default(),
            settler_config: None,
            dev_mode: false,
        }
    }
}

// Pipeline State

/// Pipeline operational state
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PipelineState {
    /// Pipeline is running normally
    Running,
    /// Pipeline is paused (e.g., due to settlement failures)
    Paused { reason: String },
    /// Pipeline is shutting down
    Stopping,
}

/// Statistics about the pipeline
#[derive(Debug, Clone)]
pub struct PipelineStats {
    /// Batch manager stats
    pub batch_stats: BatchManagerStats,
    /// Current pipeline state
    pub state: PipelineState,
    /// Total batches proved
    pub batches_proved: u64,
    /// Total batches settled
    pub batches_settled: u64,
    /// Last proved batch ID
    pub last_proved_batch: Option<u64>,
    /// Last settled batch ID
    pub last_settled_batch: Option<u64>,
    /// Whether prover is currently working
    pub prover_busy: bool,
    /// Whether settler is currently working
    pub settler_busy: bool,
}

/// Result of sealing a batch (for dev/testing)
#[derive(Debug, Clone)]
pub struct SealResult {
    /// Batch ID that was sealed
    pub batch_id: u64,
    /// Number of transactions in the batch
    pub tx_count: usize,
}

// Pipeline Commands

/// Commands for the pipeline service
pub enum PipelineCommand {
    /// Submit a transaction
    Submit(TransactionType, oneshot::Sender<Result<()>>),
    /// Force seal the current batch (returns batch_id only)
    Seal(oneshot::Sender<Result<Option<u64>>>),
    /// Force seal with extended info (for dev mode)
    ForceSeal(oneshot::Sender<Result<SealResult>>),
    /// Get pipeline statistics
    Stats(oneshot::Sender<PipelineStats>),
    /// Pause the pipeline
    Pause(String, oneshot::Sender<()>),
    /// Resume the pipeline
    Resume(oneshot::Sender<Result<()>>),
    /// Shutdown
    Shutdown,
}

// Pipeline Orchestrator

/// The pipeline orchestrator coordinates batch proving and settlement
pub struct PipelineOrchestrator {
    db: Arc<RocksDbStore>,
    /// Batch manager (shared with command handler)
    batch_manager: Arc<Mutex<BatchManager>>,
    prover: Arc<dyn BatchProver>,
    mock_settler: Option<Arc<Mutex<MockSettler>>>, // only for local tests
    settler_service: Option<Arc<SettlerService>>,  // settlement service
    config: PipelineConfig,
    state: PipelineState,
    batches_proved: u64,
    batches_settled: u64,
    last_proved_batch: Option<u64>,
    last_settled_batch: Option<u64>,
    /// Currently proving (batch_id if active)
    proving_batch: Option<u64>,
    /// Currently settling (batch_id if active)
    settling_batch: Option<u64>,
    /// Settlement retry count for current batch
    settlement_retries: u32,
}

impl PipelineOrchestrator {
    /// Create a new pipeline orchestrator
    pub fn new(
        db: Arc<RocksDbStore>,
        config: PipelineConfig,
        settler_service: Option<SettlerService>,
    ) -> Result<Self> {
        let batch_manager = BatchManager::new(db.clone(), config.batch_config.clone())?;

        // Create prover based on config
        let prover: Arc<dyn BatchProver> = match &config.prover_mode {
            ProverMode::Mock => {
                info!("Using MockProver for batch proving");
                Arc::new(MockProver::new())
            }
            ProverMode::Groth16 => {
                // Try to load real Groth16 prover from key files
                match (&config.proving_key_path, &config.verifying_key_path) {
                    (Some(pk_path), Some(vk_path)) => {
                        info!("Loading Groth16 prover from key files");
                        info!("  Proving key:   {}", pk_path);
                        info!("  Verifying key: {}", vk_path);
                        match Groth16Prover::from_files(pk_path, vk_path) {
                            Ok(prover) => {
                                info!("Groth16 prover initialized successfully");
                                Arc::new(prover)
                            }
                            Err(e) => {
                                warn!(
                                    "Failed to load Groth16 prover: {}. Falling back to MockProver",
                                    e
                                );
                                Arc::new(MockProver::new())
                            }
                        }
                    }
                    _ => {
                        warn!(
                            "Groth16 prover requested but key paths not configured. \
                            Set ZL_PROVING_KEY and ZL_VERIFYING_KEY environment variables. \
                            Using MockProver instead."
                        );
                        Arc::new(MockProver::new())
                    }
                }
            }
            ProverMode::Noir => {
                // Create Noir prover client for HTTP coordinator
                match &config.noir_coordinator_url {
                    Some(url) => {
                        info!("Using Noir prover via HTTP coordinator");
                        info!("  Coordinator URL: {}", url);
                        let mut noir_config = NoirProverConfig::default();
                        noir_config.coordinator_url = url.clone();
                        if let Some(timeout_secs) = config.noir_proof_timeout_secs {
                            noir_config.proof_timeout =
                                std::time::Duration::from_secs(timeout_secs);
                        }
                        info!(
                            "  Proof timeout:   {} seconds",
                            noir_config.proof_timeout.as_secs()
                        );
                        Arc::new(NoirProverClient::new(noir_config))
                    }
                    None => {
                        warn!(
                            "Noir prover requested but coordinator URL not configured. \
                            Set ZL_NOIR_COORDINATOR_URL environment variable. \
                            Using MockProver instead."
                        );
                        Arc::new(MockProver::new())
                    }
                }
            }
        };

        // Create settler based on config
        let (mock_settler, settler_svc) = if config.settlement_enabled {
            if let Some(svc) = settler_service {
                // Use externally provided settler service
                info!("Using externally provided SettlerService for L1 settlement");
                (None, Some(Arc::new(svc)))
            } else if let (Some(settler_cfg), Some(keypair_path)) =
                (&config.settler_config, &config.sequencer_keypair_path)
            {
                // Create settler from config
                info!("Creating SettlerService from configuration");
                info!("  RPC URL:        {}", settler_cfg.rpc_url);
                info!("  Bridge program: {}", settler_cfg.bridge_program_id);
                info!("  Keypair path:   {}", keypair_path);

                match Self::load_keypair_and_create_settler(settler_cfg.clone(), keypair_path) {
                    Ok(svc) => {
                        info!("SettlerService initialized successfully");
                        (None, Some(Arc::new(svc)))
                    }
                    Err(e) => {
                        warn!(
                            "Failed to create SettlerService: {}. Using MockSettler instead.",
                            e
                        );
                        (Some(Arc::new(Mutex::new(MockSettler::new()))), None)
                    }
                }
            } else {
                warn!(
                    "Settlement enabled but settler config or keypair path not provided. \
                    Set ZL_SEQUENCER_KEYPAIR and ensure settler config is complete. \
                    Using MockSettler instead."
                );
                (Some(Arc::new(Mutex::new(MockSettler::new()))), None)
            }
        } else {
            // Local testing mode: use mock settler
            debug!("Settlement disabled, using MockSettler for local testing");
            (Some(Arc::new(Mutex::new(MockSettler::new()))), None)
        };

        Ok(Self {
            db,
            batch_manager: Arc::new(Mutex::new(batch_manager)),
            prover,
            mock_settler,
            settler_service: settler_svc,
            config,
            state: PipelineState::Running,
            batches_proved: 0,
            batches_settled: 0,
            last_proved_batch: None,
            last_settled_batch: None,
            proving_batch: None,
            settling_batch: None,
            settlement_retries: 0,
        })
    }

    /// Load keypair from file and create SettlerService
    fn load_keypair_and_create_settler(
        settler_config: SettlerConfig,
        keypair_path: &str,
    ) -> Result<SettlerService> {
        use solana_sdk::signer::Signer;
        use solana_sdk::signer::keypair::read_keypair_file;

        // Read keypair file (supports JSON array format from solana-keygen)
        let keypair = read_keypair_file(keypair_path).map_err(|e| {
            anyhow::anyhow!("Failed to read keypair file '{}': {}", keypair_path, e)
        })?;

        info!("Loaded sequencer keypair: {}", keypair.pubkey());

        SettlerService::new(settler_config, keypair)
    }

    /// Get a handle to the batch manager
    pub fn batch_manager(&self) -> Arc<Mutex<BatchManager>> {
        self.batch_manager.clone()
    }

    /// Check for and process proving work
    async fn try_prove(&mut self) -> Result<bool> {
        // Skip if already proving or paused
        if self.proving_batch.is_some() || self.state != PipelineState::Running {
            return Ok(false);
        }

        let mut manager = self.batch_manager.lock().await;

        // Find next batch ready for proving
        let batch_to_prove = manager.next_for_proving().map(|b| {
            let batch_id = b.id;
            let inputs = build_public_inputs(b, [0u8; 32]); // TODO: withdrawal root
            let witness = build_witness(b);
            b.start_proving();
            (batch_id, inputs, witness)
        });

        drop(manager);

        let Some((batch_id, inputs_result, witness)) = batch_to_prove else {
            return Ok(false);
        };

        let inputs = inputs_result.context("failed to build public inputs")?;

        info!(batch_id, "Starting proof generation");
        self.proving_batch = Some(batch_id);

        // Prove (this may block for real proofs)
        let prover = self.prover.clone();
        let proof_result = tokio::task::spawn_blocking(move || prover.prove(&inputs, &witness))
            .await
            .context("prover task panicked")?;

        match proof_result {
            Ok(proof) => {
                info!(
                    batch_id,
                    proving_time_ms = proof.proving_time_ms,
                    "Proof generated successfully"
                );

                // Update batch state
                let mut manager = self.batch_manager.lock().await;
                manager.batch_proved(batch_id, proof.proof_bytes.clone())?;
                drop(manager);

                self.batches_proved += 1;
                self.last_proved_batch = Some(batch_id);
                self.proving_batch = None;

                Ok(true)
            }
            Err(e) => {
                error!(batch_id, error = %e, "Proof generation failed");
                self.proving_batch = None;
                Err(e)
            }
        }
    }

    /// Check for and process settlement work
    async fn try_settle(&mut self) -> Result<bool> {
        // Skip if already settling, paused, or no settlement configured
        if self.settling_batch.is_some() || self.state != PipelineState::Running {
            return Ok(false);
        }

        let mut manager = self.batch_manager.lock().await;

        // Find next batch ready for settlement and extract withdrawal info
        let batch_to_settle = manager.next_for_settlement().map(|b| {
            let batch_id = b.id;
            let tx_count = b.transactions.len();
            let post_state_root = b.post_state_root.unwrap_or([0u8; 32]);
            let post_shielded_root = b.post_shielded_root.unwrap_or([0u8; 32]);

            // Collect tx hashes for status update
            let tx_hashes: Vec<[u8; 32]> = b.results.iter().map(|r| r.tx_hash).collect();

            // Extract withdrawals from transaction results
            let withdrawals: Vec<TrackedWithdrawal> = b
                .results
                .iter()
                .filter_map(|r| {
                    if let TxResultType::Withdrawal {
                        from,
                        to_l1,
                        amount,
                    } = &r.tx_type
                    {
                        if r.success {
                            Some(TrackedWithdrawal {
                                tx_hash: r.tx_hash,
                                from: from.clone(),
                                to_l1_address: *to_l1,
                                amount: *amount,
                                l2_nonce: 0, // Not tracked in result, but not needed for L1
                                state: WithdrawalState::InBatch { batch_id },
                                created_at: std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .unwrap_or_default()
                                    .as_secs(),
                                batch_id: Some(batch_id),
                            })
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                })
                .collect();

            // Compute withdrawal merkle root
            let withdrawal_root = build_withdrawal_merkle_root(&withdrawals);

            // Compute batch hash
            let batch_hash = compute_batch_hash(&b.transactions);

            let proof = BatchProof {
                public_inputs: BatchPublicInputs {
                    pre_state_root: b.pre_state_root,
                    post_state_root,
                    pre_shielded_root: b.pre_shielded_root,
                    post_shielded_root,
                    withdrawal_root,
                    batch_hash,
                    batch_id,
                },
                proof_bytes: b.proof.clone().unwrap_or_default(),
                proving_time_ms: 0,
            };
            (
                batch_id,
                proof,
                withdrawals,
                tx_count,
                tx_hashes,
                post_state_root,
                post_shielded_root,
            )
        });

        drop(manager);

        let Some((
            batch_id,
            proof,
            withdrawals,
            tx_count,
            tx_hashes,
            post_state_root,
            post_shielded_root,
        )) = batch_to_settle
        else {
            return Ok(false);
        };

        let withdrawal_count = withdrawals.len();
        info!(batch_id, withdrawal_count, "Starting settlement");
        self.settling_batch = Some(batch_id);

        // Calculate prev_batch_id (the batch before this one)
        let prev_batch_id = if batch_id > 1 { batch_id - 1 } else { 0 };

        // Settle using mock or real settler (with withdrawals)
        // Uses automatic proof format detection to route to correct verifier
        let result = if let Some(ref settler_svc) = self.settler_service {
            // Use submit_auto to automatically detect Groth16 vs Noir/Sunspot proofs
            // For now, withdrawals are only supported with Groth16 proofs
            if withdrawals.is_empty() {
                settler_svc.submit_auto(&proof, prev_batch_id).await
            } else {
                // Withdrawals require the full submit_with_withdrawals flow
                // which currently only works with Groth16 proofs
                if SettlerService::is_noir_proof(&proof) {
                    warn!(
                        "Noir proofs with withdrawals not yet supported. \
                        Submitting proof without withdrawal processing."
                    );
                    settler_svc.submit_auto(&proof, prev_batch_id).await
                } else {
                    settler_svc
                        .submit_with_withdrawals(&proof, prev_batch_id, &withdrawals)
                        .await
                }
            }
        } else if let Some(ref mock_settler) = self.mock_settler {
            let mut settler = mock_settler.lock().await;
            Ok(settler.submit(&proof))
        } else {
            // No settler configured, just mark as settled
            Ok(SettlementResult {
                tx_signature: format!("local_{}", batch_id),
                batch_id,
                confirmed: true,
                slot: None,
            })
        };

        match result {
            Ok(settlement) => {
                info!(
                    batch_id,
                    tx_sig = %settlement.tx_signature,
                    withdrawal_count,
                    "Batch settled successfully"
                );

                // Update batch state
                let mut manager = self.batch_manager.lock().await;
                manager.batch_settled(batch_id, settlement.tx_signature.clone())?;

                // For MVP, immediately finalize (no challenge period)
                let _diff = manager.batch_finalized(batch_id)?;
                drop(manager);

                // Store batch summary for API queries
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);

                let batch_summary = BatchSummary {
                    batch_id,
                    tx_count,
                    state_root: hex::encode(post_state_root),
                    shielded_root: hex::encode(post_shielded_root),
                    l1_tx_sig: Some(settlement.tx_signature.clone()),
                    status: BatchStatus::Settled,
                    created_at: now, // Approximation - could track actual creation time
                    settled_at: Some(now),
                };
                info!("{:?}", batch_summary);
                if let Err(e) = self.db.store_batch_summary(&batch_summary) {
                    warn!(batch_id, error = %e, "Failed to store batch summary");
                }

                // Update transaction statuses: Failed stays Failed, others become Settled
                for tx_hash in &tx_hashes {
                    // Check current status - don't overwrite Failed transactions
                    if let Ok(Some(summary)) = self.db.get_tx_summary(tx_hash) {
                        if summary.status == TxStatus::Failed {
                            // Transaction already failed during execution, don't change to Settled
                            continue;
                        }
                    }
                    if let Err(e) =
                        self.db
                            .update_tx_status(tx_hash, TxStatus::Settled, Some(batch_id))
                    {
                        warn!(tx_hash = %hex::encode(&tx_hash[..8]), error = %e, "Failed to update tx status");
                    }
                }

                // Execute withdrawals on L1 (transfer SOL from vault to recipients)
                if !withdrawals.is_empty() {
                    if let Some(ref settler_svc) = self.settler_service {
                        info!(batch_id, withdrawal_count, "Executing withdrawals on L1");

                        let results = settler_svc
                            .execute_withdrawals_batched(batch_id, &withdrawals)
                            .await;

                        // Log results
                        let succeeded = results.iter().filter(|r| r.success).count();
                        let failed = results.iter().filter(|r| !r.success).count();

                        if failed > 0 {
                            warn!(batch_id, succeeded, failed, "Some withdrawals failed");
                            // Log individual failures
                            for r in results.iter().filter(|r| !r.success) {
                                warn!(
                                    tx_hash = %hex::encode(&r.tx_hash[..8]),
                                    error = ?r.error,
                                    retries = r.retries,
                                    "Withdrawal failed"
                                );
                            }
                        } else {
                            info!(batch_id, succeeded, "All withdrawals executed successfully");
                        }
                    } else {
                        // Mock mode - just log
                        debug!(
                            batch_id,
                            withdrawal_count, "Withdrawals skipped (mock settler)"
                        );
                    }
                }

                self.batches_settled += 1;
                self.last_settled_batch = Some(batch_id);
                self.settling_batch = None;
                self.settlement_retries = 0;

                Ok(true)
            }
            Err(e) => {
                self.settlement_retries += 1;
                warn!(
                    batch_id,
                    error = %e,
                    retry = self.settlement_retries,
                    max_retries = self.config.max_settlement_retries,
                    "Settlement failed"
                );

                if self.settlement_retries >= self.config.max_settlement_retries {
                    // Pause the pipeline
                    let reason = format!(
                        "Settlement failed {} times for batch {}: {}",
                        self.settlement_retries, batch_id, e
                    );
                    error!("{}", reason);
                    self.state = PipelineState::Paused {
                        reason: reason.clone(),
                    };
                    self.settling_batch = None;
                    return Err(anyhow::anyhow!(reason));
                }

                // Will retry on next tick
                self.settling_batch = None;

                // Exponential backoff
                let delay = self.config.settlement_retry_base_ms * (1 << self.settlement_retries);
                tokio::time::sleep(Duration::from_millis(delay)).await;

                Ok(false)
            }
        }
    }

    /// Run one iteration of the pipeline loop
    pub async fn tick(&mut self) -> Result<()> {
        if self.state != PipelineState::Running {
            return Ok(());
        }

        // Check for batch timeout and seal if needed
        {
            let mut manager = self.batch_manager.lock().await;
            if let Ok(Some(batch_id)) = manager.check_timeout() {
                info!(batch_id, "Batch sealed by timeout");
            }
        }

        // Try proving (non-blocking check, blocking prove)
        if let Err(e) = self.try_prove().await {
            error!(error = %e, "Proving error");
        }

        // Try settling
        if let Err(e) = self.try_settle().await {
            error!(error = %e, "Settlement error");
        }

        Ok(())
    }

    /// Get current statistics
    pub async fn stats(&self) -> PipelineStats {
        let batch_stats = self.batch_manager.lock().await.stats();

        PipelineStats {
            batch_stats,
            state: self.state.clone(),
            batches_proved: self.batches_proved,
            batches_settled: self.batches_settled,
            last_proved_batch: self.last_proved_batch,
            last_settled_batch: self.last_settled_batch,
            prover_busy: self.proving_batch.is_some(),
            settler_busy: self.settling_batch.is_some(),
        }
    }

    /// Pause the pipeline
    pub fn pause(&mut self, reason: String) {
        warn!(reason = %reason, "Pipeline paused");
        self.state = PipelineState::Paused { reason };
    }

    /// Resume the pipeline
    pub fn resume(&mut self) -> Result<()> {
        match &self.state {
            PipelineState::Paused { .. } => {
                info!("Pipeline resumed");
                self.state = PipelineState::Running;
                self.settlement_retries = 0;
                Ok(())
            }
            PipelineState::Running => Ok(()),
            PipelineState::Stopping => Err(anyhow::anyhow!("cannot resume stopping pipeline")),
        }
    }
}

// Pipeline Service

/// Async service that runs the pipeline
pub struct PipelineService {
    command_tx: mpsc::Sender<PipelineCommand>,
}

impl PipelineService {
    /// Start the pipeline service
    pub fn start(
        db: Arc<RocksDbStore>,
        config: PipelineConfig,
        settler_service: Option<SettlerService>,
    ) -> Result<Self> {
        let (command_tx, mut command_rx) = mpsc::channel::<PipelineCommand>(1000);

        let mut orchestrator = PipelineOrchestrator::new(db, config.clone(), settler_service)?;
        let batch_manager = orchestrator.batch_manager();

        // Spawn the main service loop
        tokio::spawn(async move {
            let poll_interval = Duration::from_millis(config.poll_interval_ms);
            let mut ticker = tokio::time::interval(poll_interval);

            loop {
                tokio::select! {
                    Some(cmd) = command_rx.recv() => {
                        match cmd {
                            PipelineCommand::Submit(tx, reply) => {
                                let result = batch_manager.lock().await.submit_transaction(tx);
                                let _ = reply.send(result);
                            }
                            PipelineCommand::Seal(reply) => {
                                let result = batch_manager.lock().await.seal_current_batch();
                                let _ = reply.send(result);
                            }
                            PipelineCommand::ForceSeal(reply) => {
                                let mut bm = batch_manager.lock().await;
                                // Get tx count before sealing
                                let tx_count = bm.current_batch_tx_count();
                                // In dev mode, use immediate commit to make balances available right away
                                let result = if config.dev_mode {
                                    bm.seal_current_batch_immediate()
                                } else {
                                    bm.seal_current_batch()
                                };
                                let _ = reply.send(result.map(|opt_id| {
                                    SealResult {
                                        batch_id: opt_id.unwrap_or(0),
                                        tx_count,
                                    }
                                }));
                            }
                            PipelineCommand::Stats(reply) => {
                                let stats = orchestrator.stats().await;
                                let _ = reply.send(stats);
                            }
                            PipelineCommand::Pause(reason, reply) => {
                                orchestrator.pause(reason);
                                let _ = reply.send(());
                            }
                            PipelineCommand::Resume(reply) => {
                                let result = orchestrator.resume();
                                let _ = reply.send(result);
                            }
                            PipelineCommand::Shutdown => {
                                info!("Pipeline shutting down");
                                orchestrator.state = PipelineState::Stopping;

                                // Seal any pending transactions before shutdown
                                let mut bm = batch_manager.lock().await;
                                let pending_count = bm.current_batch_tx_count();
                                if pending_count > 0 {
                                    info!("Sealing {} pending transactions before shutdown", pending_count);
                                    if let Err(e) = bm.seal_current_batch_immediate() {
                                        warn!("Failed to seal pending batch on shutdown: {}", e);
                                    }
                                }
                                drop(bm);

                                info!("Pipeline shutdown complete");
                                break;
                            }
                        }
                    }
                    _ = ticker.tick() => {
                        // Run pipeline iteration
                        if let Err(e) = orchestrator.tick().await {
                            error!(error = %e, "Pipeline tick error");
                        }
                    }
                }
            }
        });

        Ok(Self { command_tx })
    }

    /// Submit a transaction to the pipeline
    pub async fn submit(&self, tx: TransactionType) -> Result<()> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.command_tx
            .send(PipelineCommand::Submit(tx, reply_tx))
            .await
            .context("pipeline unavailable")?;
        reply_rx.await.context("pipeline crashed")?
    }

    /// Force seal the current batch
    pub async fn seal(&self) -> Result<Option<u64>> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.command_tx
            .send(PipelineCommand::Seal(reply_tx))
            .await
            .context("pipeline unavailable")?;
        reply_rx.await.context("pipeline crashed")?
    }

    /// Force seal the current batch with extended info (for dev mode)
    pub async fn force_seal(&self) -> Result<SealResult> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.command_tx
            .send(PipelineCommand::ForceSeal(reply_tx))
            .await
            .context("pipeline unavailable")?;
        reply_rx.await.context("pipeline crashed")?
    }

    /// Get pipeline statistics
    pub async fn stats(&self) -> Result<PipelineStats> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.command_tx
            .send(PipelineCommand::Stats(reply_tx))
            .await
            .context("pipeline unavailable")?;
        Ok(reply_rx.await.context("pipeline crashed")?)
    }

    /// Pause the pipeline
    pub async fn pause(&self, reason: String) -> Result<()> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.command_tx
            .send(PipelineCommand::Pause(reason, reply_tx))
            .await
            .context("pipeline unavailable")?;
        reply_rx.await.context("pipeline crashed")?;
        Ok(())
    }

    /// Resume the pipeline
    pub async fn resume(&self) -> Result<()> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.command_tx
            .send(PipelineCommand::Resume(reply_tx))
            .await
            .context("pipeline unavailable")?;
        reply_rx.await.context("pipeline crashed")?
    }

    /// Shutdown the pipeline
    pub async fn shutdown(&self) -> Result<()> {
        self.command_tx
            .send(PipelineCommand::Shutdown)
            .await
            .context("pipeline unavailable")?;
        Ok(())
    }
}

// Tests

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use zelana_transaction::DepositEvent;

    fn create_test_db() -> (TempDir, Arc<RocksDbStore>) {
        let temp_dir = TempDir::new().unwrap();
        let db = RocksDbStore::open(temp_dir.path().to_str().unwrap()).unwrap();
        (temp_dir, Arc::new(db))
    }

    #[tokio::test]
    async fn test_pipeline_config_default() {
        let config = PipelineConfig::default();
        assert_eq!(config.prover_mode, ProverMode::Mock);
        assert!(!config.settlement_enabled);
        assert_eq!(config.max_settlement_retries, 5);
    }

    #[tokio::test]
    async fn test_pipeline_orchestrator_new() {
        let (_temp_dir, db) = create_test_db();
        let config = PipelineConfig::default();

        let orchestrator = PipelineOrchestrator::new(db, config, None).unwrap();
        let stats = orchestrator.stats().await;

        assert_eq!(stats.state, PipelineState::Running);
        assert_eq!(stats.batches_proved, 0);
        assert_eq!(stats.batches_settled, 0);
    }

    #[tokio::test]
    async fn test_pipeline_submit_and_seal() {
        let (_temp_dir, db) = create_test_db();
        let config = PipelineConfig::default();

        let service = PipelineService::start(db, config, None).unwrap();

        // Submit a transaction
        let tx = TransactionType::Deposit(DepositEvent {
            to: zelana_account::AccountId([1u8; 32]),
            amount: 1000,
            l1_seq: 1,
        });

        service.submit(tx).await.unwrap();

        // Check stats
        let stats = service.stats().await.unwrap();
        assert_eq!(stats.batch_stats.current_batch_txs, 1);

        // Seal batch
        let batch_id = service.seal().await.unwrap();
        assert_eq!(batch_id, Some(1));

        // Wait for proving and settlement
        tokio::time::sleep(Duration::from_millis(300)).await;

        let stats = service.stats().await.unwrap();
        // With MockProver and MockSettler, should be proved and settled quickly
        assert!(stats.batches_proved >= 1 || stats.batch_stats.proving_count >= 1);

        service.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_pipeline_pause_resume() {
        let (_temp_dir, db) = create_test_db();
        let config = PipelineConfig::default();

        let service = PipelineService::start(db, config, None).unwrap();

        // Pause
        service.pause("test pause".to_string()).await.unwrap();
        let stats = service.stats().await.unwrap();
        assert!(matches!(stats.state, PipelineState::Paused { .. }));

        // Resume
        service.resume().await.unwrap();
        let stats = service.stats().await.unwrap();
        assert_eq!(stats.state, PipelineState::Running);

        service.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_pipeline_full_flow() {
        let (_temp_dir, db) = create_test_db();
        let mut config = PipelineConfig::default();
        config.poll_interval_ms = 10; // Fast polling for test

        let service = PipelineService::start(db, config, None).unwrap();

        // Submit multiple transactions
        for i in 0..5 {
            let tx = TransactionType::Deposit(DepositEvent {
                to: zelana_account::AccountId([i as u8; 32]),
                amount: 1000 * (i + 1) as u64,
                l1_seq: i as u64,
            });
            service.submit(tx).await.unwrap();
        }

        // Seal batch
        service.seal().await.unwrap();

        // Wait for full pipeline to complete
        for _ in 0..50 {
            tokio::time::sleep(Duration::from_millis(20)).await;
            let stats = service.stats().await.unwrap();
            if stats.batches_settled >= 1 {
                break;
            }
        }

        let stats = service.stats().await.unwrap();
        assert_eq!(stats.batches_proved, 1);
        assert_eq!(stats.batches_settled, 1);
        assert_eq!(stats.last_proved_batch, Some(1));
        assert_eq!(stats.last_settled_batch, Some(1));

        service.shutdown().await.unwrap();
    }
}
