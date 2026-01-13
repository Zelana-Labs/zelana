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
use std::time::Duration;

use anyhow::{Context, Result};
use tokio::sync::{Mutex, mpsc, oneshot};
use tracing::{debug, error, info, warn};

use crate::sequencer::batch::{Batch, BatchConfig, BatchManager, BatchManagerStats, BatchState};
use crate::sequencer::db::RocksDbStore;
use crate::sequencer::prover::{
    BatchProof, BatchProver, BatchPublicInputs, BatchWitness, MockProver, build_public_inputs,
    build_witness,
};
use crate::sequencer::settler::{MockSettler, SettlementResult, SettlerConfig, SettlerService};
use zelana_transaction::TransactionType;

// ============================================================================
// Configuration
// ============================================================================

/// Pipeline configuration
#[derive(Debug, Clone)]
pub struct PipelineConfig {
    /// Use MockProver instead of real Groth16 (default: true for MVP)
    pub mock_prover: bool,
    /// Enable L1 settlement (default: false for local testing)
    pub settlement_enabled: bool,
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
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            mock_prover: true,
            settlement_enabled: false,
            max_settlement_retries: 5,
            settlement_retry_base_ms: 5000,
            poll_interval_ms: 100,
            batch_config: BatchConfig::default(),
            settler_config: None,
        }
    }
}

// ============================================================================
// Pipeline State
// ============================================================================

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

// ============================================================================
// Pipeline Commands
// ============================================================================

/// Commands for the pipeline service
pub enum PipelineCommand {
    /// Submit a transaction
    Submit(TransactionType, oneshot::Sender<Result<()>>),
    /// Force seal the current batch
    Seal(oneshot::Sender<Result<Option<u64>>>),
    /// Get pipeline statistics
    Stats(oneshot::Sender<PipelineStats>),
    /// Pause the pipeline
    Pause(String, oneshot::Sender<()>),
    /// Resume the pipeline
    Resume(oneshot::Sender<Result<()>>),
    /// Shutdown
    Shutdown,
}

// ============================================================================
// Pipeline Orchestrator
// ============================================================================

/// The pipeline orchestrator coordinates batch proving and settlement
pub struct PipelineOrchestrator {
    /// Batch manager (shared with command handler)
    batch_manager: Arc<Mutex<BatchManager>>,
    /// Prover implementation
    prover: Arc<dyn BatchProver>,
    /// Mock settler (for local testing)
    mock_settler: Option<Arc<Mutex<MockSettler>>>,
    /// Real settler service (for L1 settlement)
    settler_service: Option<Arc<SettlerService>>,
    /// Configuration
    config: PipelineConfig,
    /// Current pipeline state
    state: PipelineState,
    /// Statistics
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
        let batch_manager = BatchManager::new(db, config.batch_config.clone())?;

        // Create prover based on config
        let prover: Arc<dyn BatchProver> = if config.mock_prover {
            Arc::new(MockProver::new())
        } else {
            // For real prover, we'd load keys from files
            // For now, fall back to mock
            warn!("Real Groth16 prover not configured, using MockProver");
            Arc::new(MockProver::new())
        };

        // Create settler based on config
        let (mock_settler, settler_svc) = if config.settlement_enabled {
            if let Some(svc) = settler_service {
                (None, Some(Arc::new(svc)))
            } else {
                warn!("Settlement enabled but no settler service provided, using MockSettler");
                (Some(Arc::new(Mutex::new(MockSettler::new()))), None)
            }
        } else {
            // Local testing mode: use mock settler
            (Some(Arc::new(Mutex::new(MockSettler::new()))), None)
        };

        Ok(Self {
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

        // Find next batch ready for settlement
        let batch_to_settle = manager.next_for_settlement().map(|b| {
            let batch_id = b.id;
            let proof = BatchProof {
                public_inputs: BatchPublicInputs {
                    pre_state_root: b.pre_state_root,
                    post_state_root: b.post_state_root.unwrap_or([0u8; 32]),
                    pre_shielded_root: b.pre_shielded_root,
                    post_shielded_root: b.post_shielded_root.unwrap_or([0u8; 32]),
                    withdrawal_root: [0u8; 32], // TODO: withdrawal root
                    batch_hash: [0u8; 32],      // TODO: batch hash
                    batch_id,
                },
                proof_bytes: b.proof.clone().unwrap_or_default(),
                proving_time_ms: 0,
            };
            (batch_id, proof)
        });

        drop(manager);

        let Some((batch_id, proof)) = batch_to_settle else {
            return Ok(false);
        };

        info!(batch_id, "Starting settlement");
        self.settling_batch = Some(batch_id);

        // Calculate prev_batch_id (the batch before this one)
        let prev_batch_id = if batch_id > 1 { batch_id - 1 } else { 0 };

        // Settle using mock or real settler
        let result = if let Some(ref settler_svc) = self.settler_service {
            settler_svc.submit(&proof, prev_batch_id).await
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
                    "Batch settled successfully"
                );

                // Update batch state
                let mut manager = self.batch_manager.lock().await;
                manager.batch_settled(batch_id, settlement.tx_signature)?;

                // For MVP, immediately finalize (no challenge period)
                let _diff = manager.batch_finalized(batch_id)?;
                drop(manager);

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

// ============================================================================
// Pipeline Service
// ============================================================================

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

// ============================================================================
// Tests
// ============================================================================

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
        assert!(config.mock_prover);
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
