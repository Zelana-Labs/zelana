#![allow(dead_code)] // BatchService methods reserved for future use
//! Batch Manager
//!
//! Manages batch lifecycle with pipeline support for proving while accumulating.
//!
//! ```text
//! -------------------------------------------------------------------
//! -                    Batch Pipeline                               -
//! -                                                                  -
//! -  ---------------    ---------------    -----------------------  -
//! -  - Accumulating----▶-   Proving   ----▶-     Settling        -  -
//! -  -   Batch N   -    -   Batch N-1 -    -     Batch N-2       -  -
//! -  ---------------    ---------------    -----------------------  -
//! -        -                   -                     -              -
//! -        ▼                   ▼                     ▼              -
//! -  -----------------------------------------------------------   -
//! -  -              Parallel Execution                          -   -
//! -  -----------------------------------------------------------   -
//! -------------------------------------------------------------------
//!
//! Batch Lifecycle:
//! 1. Accumulating: Receiving and executing transactions
//! 2. Sealed: No more txs, ready for proving
//! 3. Proving: ZK proof generation in progress
//! 4. Proved: Proof ready, waiting for settlement
//! 5. Settling: L1 transaction submitted
//! 6. Finalized: L1 confirmed, batch complete
//! ```

use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use tokio::sync::{Mutex, mpsc, oneshot};
use tracing::warn;

use super::tx_router::{BatchDiff, TxResult, TxResultType, TxRouter};
use crate::api::types::{TxStatus, TxSummary, TxType};
use crate::sequencer::settlement::prover::{
    BatchPublicInputs, BatchWitness, build_public_inputs, build_witness_with_proofs,
};
use crate::sequencer::storage::compute_withdrawal_root_mimc;
use crate::sequencer::storage::db::RocksDbStore;
use crate::storage::StateStore;
use zelana_account::{AccountId, AccountState};
use zelana_transaction::TransactionType;

// Configuration

/// Batch configuration
#[derive(Debug, Clone)]
pub struct BatchConfig {
    /// Maximum transactions per batch
    pub max_transactions: usize,
    /// Maximum time before forced seal (seconds)
    pub max_batch_age_secs: u64,
    /// Maximum shielded transactions per batch (more expensive to prove)
    pub max_shielded: usize,
    /// Minimum transactions before considering seal (unless timeout)
    pub min_transactions: usize,
}

impl Default for BatchConfig {
    fn default() -> Self {
        Self {
            max_transactions: 100,
            max_batch_age_secs: 60,
            max_shielded: 10,
            min_transactions: 1,
        }
    }
}

// Batch State

/// The lifecycle state of a batch
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BatchState {
    /// Actively receiving transactions
    Accumulating,
    /// Sealed, ready for proving
    Sealed,
    /// Proof generation in progress
    Proving,
    /// Proof ready, waiting for L1 submission
    Proved,
    /// L1 transaction submitted, waiting for confirmation
    Settling,
    /// Fully confirmed on L1
    Finalized,
}

/// A single batch in the pipeline
#[derive(Debug)]
pub struct Batch {
    /// Unique batch ID (monotonically increasing)
    pub id: u64,
    /// Current state
    pub state: BatchState,
    /// Transactions in this batch
    pub transactions: Vec<TransactionType>,
    /// Execution results
    pub results: Vec<TxResult>,
    /// State diff from execution
    pub diff: Option<BatchDiff>,
    /// When this batch started accumulating
    pub started_at: Instant,
    /// Count of shielded transactions
    pub shielded_count: usize,
    /// Count of withdrawal transactions
    pub withdrawal_count: usize,
    /// Pre-batch state root (transparent)
    pub pre_state_root: [u8; 32],
    /// Post-batch state root (transparent)
    pub post_state_root: Option<[u8; 32]>,
    /// Pre-batch shielded root
    pub pre_shielded_root: [u8; 32],
    /// Post-batch shielded root
    pub post_shielded_root: Option<[u8; 32]>,
    /// Generated proof (after proving)
    pub proof: Option<Vec<u8>>,
    /// L1 transaction signature (after submission)
    pub l1_tx_sig: Option<String>,
    /// Whether state diff was already committed (dev mode immediate commit)
    pub committed: bool,
    /// Pre-computed witness (built before commit in DEV mode)
    pub witness: Option<BatchWitness>,
}

impl Batch {
    pub fn new(id: u64, pre_state_root: [u8; 32], pre_shielded_root: [u8; 32]) -> Self {
        Self {
            id,
            state: BatchState::Accumulating,
            transactions: Vec::new(),
            results: Vec::new(),
            diff: None,
            started_at: Instant::now(),
            shielded_count: 0,
            withdrawal_count: 0,
            pre_state_root,
            post_state_root: None,
            pre_shielded_root,
            post_shielded_root: None,
            proof: None,
            l1_tx_sig: None,
            committed: false,
            witness: None,
        }
    }

    /// Check if batch should be sealed based on config
    pub fn should_seal(&self, config: &BatchConfig) -> bool {
        // Seal if we hit max transactions
        if self.transactions.len() >= config.max_transactions {
            return true;
        }

        // Seal if we hit max shielded transactions
        if self.shielded_count >= config.max_shielded {
            return true;
        }

        // Seal immediately after a withdrawal (limit 1 per batch for reliable proving)
        if self.withdrawal_count >= 1 {
            return true;
        }

        // Seal immediately after a shielded transaction (limit 1 per batch for reliable proving)
        if self.shielded_count >= 1 {
            return true;
        }

        // Seal if batch is too old and has minimum transactions
        let age = self.started_at.elapsed();
        if age >= Duration::from_secs(config.max_batch_age_secs)
            && self.transactions.len() >= config.min_transactions
        {
            return true;
        }

        false
    }

    /// Add a transaction (must be in Accumulating state)
    pub fn add_transaction(&mut self, tx: TransactionType) -> Result<()> {
        if self.state != BatchState::Accumulating {
            anyhow::bail!("batch not accepting transactions");
        }

        // Track shielded count
        if matches!(tx, TransactionType::Shielded(_)) {
            self.shielded_count += 1;
        }

        // Track withdrawal count
        if matches!(tx, TransactionType::Withdraw(_)) {
            self.withdrawal_count += 1;
        }

        self.transactions.push(tx);
        Ok(())
    }

    /// Seal the batch (no more transactions)
    pub fn seal(&mut self) {
        self.state = BatchState::Sealed;
    }

    /// Set execution results
    pub fn set_execution_results(
        &mut self,
        diff: BatchDiff,
        post_state_root: [u8; 32],
        post_shielded_root: [u8; 32],
    ) {
        self.results = diff.results.clone();
        self.diff = Some(diff);
        self.post_state_root = Some(post_state_root);
        self.post_shielded_root = Some(post_shielded_root);
    }

    /// Transition to proving state
    pub fn start_proving(&mut self) {
        self.state = BatchState::Proving;
    }

    /// Set the generated proof
    pub fn set_proof(&mut self, proof: Vec<u8>) {
        self.proof = Some(proof);
        self.state = BatchState::Proved;
    }

    /// Start L1 settlement
    pub fn start_settling(&mut self, l1_sig: String) {
        self.l1_tx_sig = Some(l1_sig);
        self.state = BatchState::Settling;
    }

    /// Mark as finalized
    pub fn finalize(&mut self) {
        self.state = BatchState::Finalized;
    }
}

// Batch Manager

/// Manages the batch pipeline
pub struct BatchManager {
    /// Database
    db: Arc<RocksDbStore>,
    /// Transaction router for execution
    router: TxRouter,
    /// Configuration
    config: BatchConfig,
    /// Next batch ID
    next_batch_id: u64,
    /// Current accumulating batch
    current_batch: Option<Batch>,
    /// Batches in proving stage
    proving_batches: Vec<Batch>,
    /// Batches waiting for settlement
    pending_settlement: Vec<Batch>,
    /// Pending account states for transactions in current batch (not yet executed)
    /// This tracks expected balances/nonces for rapid successive transactions
    pending_states: std::collections::HashMap<AccountId, AccountState>,
}

impl BatchManager {
    /// Create a new batch manager
    pub fn new(db: Arc<RocksDbStore>, config: BatchConfig) -> Result<Self> {
        let router = TxRouter::load(db.clone())?;

        // Resume from last batch ID if available
        let next_batch_id = db
            .get_latest_batch_id()
            .unwrap_or(None)
            .map(|id| id + 1)
            .unwrap_or(1);

        Ok(Self {
            db,
            router,
            config,
            next_batch_id,
            current_batch: None,
            proving_batches: Vec::new(),
            pending_settlement: Vec::new(),
            pending_states: std::collections::HashMap::new(),
        })
    }

    /// Start a new batch if none is active
    fn ensure_batch(&mut self) {
        if self.current_batch.is_none() {
            let batch = Batch::new(
                self.next_batch_id,
                self.router.transparent_root(),
                self.router.shielded_root(),
            );
            self.next_batch_id += 1;
            self.current_batch = Some(batch);
        }
    }

    /// Submit a transaction to the current batch
    pub fn submit_transaction(&mut self, tx: TransactionType) -> Result<()> {
        self.ensure_batch();

        // Track pending state changes for rapid successive transactions
        // This allows get_pending_account to return correct nonces before batch execution
        self.update_pending_state(&tx);

        let batch = self.current_batch.as_mut().unwrap();
        batch.add_transaction(tx)?;

        // Check if batch should be sealed
        if batch.should_seal(&self.config) {
            self.seal_current_batch()?;
        }

        Ok(())
    }

    /// Update pending states based on a transaction being added to the batch
    /// This tracks expected balances/nonces before the batch is actually executed
    fn update_pending_state(&mut self, tx: &TransactionType) {
        match tx {
            TransactionType::Transfer(signed_tx) => {
                let from = AccountId(signed_tx.signer_pubkey);
                let to = signed_tx.data.to;
                let amount = signed_tx.data.amount;

                // Get current state (from pending_states or DB)
                let from_state = self.get_account_state_internal(&from);
                let to_state = self.get_account_state_internal(&to);

                // Update sender: debit + nonce increment
                let new_from = AccountState {
                    balance: from_state.balance.saturating_sub(amount),
                    nonce: from_state.nonce + 1,
                };
                self.pending_states.insert(from, new_from);

                // Update receiver: credit (no nonce change)
                if from != to {
                    let new_to = AccountState {
                        balance: to_state.balance + amount,
                        nonce: to_state.nonce,
                    };
                    self.pending_states.insert(to, new_to);
                }
            }
            TransactionType::Withdraw(withdraw) => {
                let from = withdraw.from;
                let amount = withdraw.amount;

                let from_state = self.get_account_state_internal(&from);
                let new_from = AccountState {
                    balance: from_state.balance.saturating_sub(amount),
                    nonce: from_state.nonce + 1,
                };
                self.pending_states.insert(from, new_from);
            }
            TransactionType::Deposit(deposit) => {
                let to = deposit.to;
                let amount = deposit.amount;

                let to_state = self.get_account_state_internal(&to);
                let new_to = AccountState {
                    balance: to_state.balance + amount,
                    nonce: to_state.nonce,
                };
                self.pending_states.insert(to, new_to);
            }
            TransactionType::Shielded(_) => {
                // Shielded transactions don't affect transparent account state
            }
        }
    }

    /// Get account state from pending_states, router cache, or database
    fn get_account_state_internal(&self, id: &AccountId) -> AccountState {
        // First check pending_states (transactions added to batch but not executed)
        if let Some(state) = self.pending_states.get(id) {
            return state.clone();
        }
        // Then check router's account_cache (executed but not committed)
        if let Some(state) = self.router.get_pending_account(id) {
            return state.clone();
        }
        // Finally check database
        self.db.get_account_state(id).unwrap_or_default()
    }

    /// Submit multiple transactions
    pub fn submit_transactions(&mut self, txs: Vec<TransactionType>) -> Result<()> {
        for tx in txs {
            self.submit_transaction(tx)?;
        }
        Ok(())
    }

    /// Force seal the current batch (e.g., on timeout)
    pub fn seal_current_batch(&mut self) -> Result<Option<u64>> {
        self.seal_current_batch_inner(false)
    }

    /// Force seal the current batch and immediately commit state (DEV MODE)
    ///
    /// This bypasses the prove/settle cycle and commits state immediately,
    /// making balances available right after seal. Only use for development/testing.
    pub fn seal_current_batch_immediate(&mut self) -> Result<Option<u64>> {
        self.seal_current_batch_inner(true)
    }

    /// Internal seal implementation
    fn seal_current_batch_inner(&mut self, immediate_commit: bool) -> Result<Option<u64>> {
        let Some(mut batch) = self.current_batch.take() else {
            return Ok(None);
        };

        if batch.transactions.is_empty() {
            // Don't seal empty batches
            self.current_batch = Some(batch);
            return Ok(None);
        }

        let batch_id = batch.id;

        // Execute all transactions
        let txs = std::mem::take(&mut batch.transactions);
        let diff = self.router.execute_batch(txs.clone());
        batch.transactions = txs;

        // Clear pending_states - the router's account_cache now has the executed state
        self.pending_states.clear();

        // Get post-execution roots
        // CRITICAL: We must compute post_state_root by processing transactions in the SAME ORDER
        // as the Noir circuit. The circuit processes transfers as: sender_update → receiver_update
        // for each transaction sequentially. Using HashMap iteration (undefined order) causes
        // root mismatch because Merkle tree roots depend on insertion order.
        // Also: Deposits are NOT processed by the circuit, so we skip them here.
        let post_state_root = {
            let mut sim_tree = self.router.account_tree().clone();
            let mut account_states: std::collections::HashMap<AccountId, AccountState> =
                std::collections::HashMap::new();

            // Pre-populate account states from DB for all accounts involved in transactions
            for tx in &batch.transactions {
                match tx {
                    TransactionType::Transfer(t) => {
                        let sender_id = AccountId(t.signer_pubkey);
                        let receiver_id = t.data.to;
                        if !account_states.contains_key(&sender_id) {
                            account_states.insert(
                                sender_id,
                                self.db.get_account_state(&sender_id).unwrap_or_default(),
                            );
                        }
                        if !account_states.contains_key(&receiver_id) {
                            account_states.insert(
                                receiver_id,
                                self.db.get_account_state(&receiver_id).unwrap_or_default(),
                            );
                        }
                    }
                    TransactionType::Withdraw(w) => {
                        let sender_id = w.from;
                        if !account_states.contains_key(&sender_id) {
                            account_states.insert(
                                sender_id,
                                self.db.get_account_state(&sender_id).unwrap_or_default(),
                            );
                        }
                    }
                    _ => {} // Deposits and Shielded don't affect transparent state root in circuit
                }
            }

            // Process transactions in CIRCUIT ORDER (same as Noir circuit)
            for tx in &batch.transactions {
                match tx {
                    TransactionType::Transfer(t) => {
                        // 1. Update sender (debit + nonce increment) - circuit does this first
                        let sender_id = AccountId(t.signer_pubkey);
                        let sender_state =
                            account_states.get(&sender_id).cloned().unwrap_or_default();
                        let new_sender = AccountState {
                            balance: sender_state.balance.saturating_sub(t.data.amount),
                            nonce: sender_state.nonce + 1,
                        };
                        sim_tree.insert(&sender_id, &new_sender);
                        account_states.insert(sender_id, new_sender);

                        // 2. Update receiver (credit, no nonce change) - circuit does this second
                        let receiver_id = t.data.to;
                        let receiver_state = account_states
                            .get(&receiver_id)
                            .cloned()
                            .unwrap_or_default();
                        let new_receiver = AccountState {
                            balance: receiver_state.balance + t.data.amount,
                            nonce: receiver_state.nonce,
                        };
                        sim_tree.insert(&receiver_id, &new_receiver);
                        account_states.insert(receiver_id, new_receiver);
                    }
                    TransactionType::Withdraw(w) => {
                        // Update sender (debit + nonce increment)
                        let sender_id = w.from;
                        let sender_state =
                            account_states.get(&sender_id).cloned().unwrap_or_default();
                        let new_sender = AccountState {
                            balance: sender_state.balance.saturating_sub(w.amount),
                            nonce: sender_state.nonce + 1,
                        };
                        sim_tree.insert(&sender_id, &new_sender);
                        account_states.insert(sender_id, new_sender);
                    }
                    TransactionType::Deposit(_) | TransactionType::Shielded(_) => {
                        // Circuit doesn't process deposits or shielded txs for transparent state
                        // Skip them to match circuit behavior
                    }
                }
            }

            sim_tree.root()
        };
        // For shielded state: execute_shielded() already updates shielded_state directly,
        // so shielded_root() returns the correct post-execution root
        let post_shielded_root = self.router.shielded_root();

        // Store transaction summaries for API queries
        self.store_tx_summaries(batch_id, &diff.results);

        // IMPORTANT: Build witness BEFORE commit in DEV mode
        // In DEV mode, commit() updates the account_tree and db immediately.
        // The witness builder needs the PRE-batch tree state to compute correct merkle paths.
        // So we must build the witness here, before commit modifies the tree.
        if immediate_commit {
            log::info!(
                "[DEV] Building witness before commit for batch {} ({} txs)",
                batch_id,
                batch.transactions.len()
            );
            let witness = build_witness_with_proofs(&batch, self.router.account_tree(), &self.db);
            batch.witness = Some(witness);

            // Now commit - this updates account_tree and db
            self.router.commit(diff.clone())?;
            batch.committed = true;
            log::info!("[DEV] Immediately committed state for batch {}", batch_id);
        }

        batch.set_execution_results(diff, post_state_root, post_shielded_root);
        batch.seal();

        // Move to proving queue
        self.proving_batches.push(batch);

        Ok(Some(batch_id))
    }

    /// Check for batch timeout and seal if needed
    pub fn check_timeout(&mut self) -> Result<Option<u64>> {
        if let Some(batch) = &self.current_batch {
            let age = batch.started_at.elapsed();
            if age >= Duration::from_secs(self.config.max_batch_age_secs)
                && !batch.transactions.is_empty()
            {
                return self.seal_current_batch();
            }
        }
        Ok(None)
    }

    /// Get next batch ready for proving
    pub fn next_for_proving(&mut self) -> Option<&mut Batch> {
        self.proving_batches
            .iter_mut()
            .find(|b| b.state == BatchState::Sealed)
    }

    /// Mark a batch as proved
    pub fn batch_proved(&mut self, batch_id: u64, proof: Vec<u8>) -> Result<()> {
        let batch = self
            .proving_batches
            .iter_mut()
            .find(|b| b.id == batch_id)
            .context("batch not found in proving queue")?;

        batch.set_proof(proof);

        // Move to settlement queue
        if let Some(idx) = self.proving_batches.iter().position(|b| b.id == batch_id) {
            let batch = self.proving_batches.remove(idx);
            self.pending_settlement.push(batch);
        }

        Ok(())
    }

    /// Get next batch ready for settlement
    pub fn next_for_settlement(&mut self) -> Option<&mut Batch> {
        self.pending_settlement
            .iter_mut()
            .find(|b| b.state == BatchState::Proved)
    }

    /// Mark a batch as settled on L1
    pub fn batch_settled(&mut self, batch_id: u64, l1_sig: String) -> Result<()> {
        let batch = self
            .pending_settlement
            .iter_mut()
            .find(|b| b.id == batch_id)
            .context("batch not found in settlement queue")?;

        batch.start_settling(l1_sig);
        Ok(())
    }

    /// Finalize a batch after L1 confirmation
    pub fn batch_finalized(&mut self, batch_id: u64) -> Result<BatchDiff> {
        let idx = self
            .pending_settlement
            .iter()
            .position(|b| b.id == batch_id)
            .context("batch not found for finalization")?;

        let mut batch = self.pending_settlement.remove(idx);
        batch.finalize();

        // Commit the state diff to database (skip if already committed in dev mode)
        let diff = batch.diff.take().context("batch has no diff")?;
        if !batch.committed {
            self.router.commit(diff.clone())?;
        } else {
            log::debug!(
                "Skipping commit for batch {} (already committed in dev mode)",
                batch_id
            );
        }

        Ok(diff)
    }

    /// Get statistics about the pipeline
    pub fn stats(&self) -> BatchManagerStats {
        BatchManagerStats {
            current_batch_txs: self
                .current_batch
                .as_ref()
                .map(|b| b.transactions.len())
                .unwrap_or(0),
            proving_count: self.proving_batches.len(),
            pending_settlement_count: self.pending_settlement.len(),
            next_batch_id: self.next_batch_id,
        }
    }

    /// Get the number of transactions in the current batch
    pub fn current_batch_tx_count(&self) -> usize {
        self.current_batch
            .as_ref()
            .map(|b| b.transactions.len())
            .unwrap_or(0)
    }

    /// Get a reference to the transaction router
    pub fn router(&self) -> &TxRouter {
        &self.router
    }

    /// Get a mutable reference to the transaction router
    pub fn router_mut(&mut self) -> &mut TxRouter {
        &mut self.router
    }

    /// Get pending account state from the current batch's in-memory cache.
    /// Returns None if the account has no pending changes.
    ///
    /// This checks in order:
    /// 1. pending_states - transactions added to batch but not yet executed
    /// 2. router's account_cache - transactions executed but not yet committed
    pub fn get_pending_account(&self, id: &AccountId) -> Option<AccountState> {
        // First check pending_states (transactions added but not executed)
        if let Some(state) = self.pending_states.get(id) {
            return Some(state.clone());
        }
        // Then check router's cache (executed but not committed)
        self.router.get_pending_account(id).cloned()
    }

    /// Prepare a batch for proving by building witness with merkle proofs.
    /// Returns (batch_id, public_inputs, witness) and marks the batch as proving.
    /// This method handles the borrow checker issues by accessing both
    /// the batch and account tree within the same method scope.
    pub fn prepare_batch_for_proving(
        &mut self,
    ) -> Option<(u64, Result<BatchPublicInputs>, BatchWitness)> {
        // Find the next sealed batch
        log::trace!(
            "prepare_batch_for_proving: checking {} proving_batches, states: {:?}",
            self.proving_batches.len(),
            self.proving_batches
                .iter()
                .map(|b| &b.state)
                .collect::<Vec<_>>()
        );

        let batch_idx = self
            .proving_batches
            .iter()
            .position(|b| b.state == BatchState::Sealed)?;

        // Build witness with proofs from account tree
        let batch = &self.proving_batches[batch_idx];
        let batch_id = batch.id;
        log::info!(
            "prepare_batch_for_proving: found sealed batch {} with {} txs",
            batch_id,
            batch.transactions.len()
        );

        // Compute withdrawal root using MiMC (matches Noir circuit)
        // For now, we only count withdrawals (deposit-only batches have 0)
        let num_withdrawals = batch
            .transactions
            .iter()
            .filter(|tx| matches!(tx, TransactionType::Withdraw(_)))
            .count() as u64;
        let withdrawal_root = compute_withdrawal_root_mimc(batch_id, num_withdrawals);

        let inputs = build_public_inputs(batch, withdrawal_root);

        // Use pre-computed witness if available (DEV mode builds it before commit),
        // otherwise build it now (production mode - commit hasn't happened yet)
        let witness = if let Some(w) = batch.witness.clone() {
            log::info!(
                "Using pre-computed witness for batch {} (DEV mode)",
                batch_id
            );
            w
        } else {
            log::info!("Building witness for batch {} (production mode)", batch_id);
            build_witness_with_proofs(batch, self.router.account_tree(), &self.db)
        };

        // Mark as proving
        self.proving_batches[batch_idx].start_proving();

        Some((batch_id, inputs, witness))
    }

    /// Store transaction summaries for API queries
    fn store_tx_summaries(&self, batch_id: u64, results: &[TxResult]) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        for result in results {
            let (tx_type, amount, from, to) = match &result.tx_type {
                TxResultType::Shielded { .. } => (TxType::Shielded, None, None, None),
                TxResultType::Transfer { from, to, amount } => (
                    TxType::Transfer,
                    Some(*amount),
                    Some(hex::encode(&from.0)),
                    Some(hex::encode(&to.0)),
                ),
                TxResultType::Deposit { to, amount } => (
                    TxType::Deposit,
                    Some(*amount),
                    None,
                    Some(hex::encode(&to.0)),
                ),
                TxResultType::Withdrawal {
                    from,
                    to_l1,
                    amount,
                } => (
                    TxType::Withdrawal,
                    Some(*amount),
                    Some(hex::encode(&from.0)),
                    Some(hex::encode(to_l1)),
                ),
            };

            let status = if result.success {
                TxStatus::Executed
            } else {
                TxStatus::Failed
            };

            let summary = TxSummary {
                tx_hash: hex::encode(&result.tx_hash),
                tx_type,
                batch_id: Some(batch_id),
                status,
                received_at: now,
                executed_at: Some(now),
                amount,
                from,
                to,
            };

            if let Err(e) = self.db.store_tx_summary(&result.tx_hash, &summary) {
                warn!(tx_hash = %hex::encode(&result.tx_hash[..8]), error = %e, "Failed to store tx summary");
            }
        }
    }
}

/// Pipeline statistics
#[derive(Debug, Clone)]
pub struct BatchManagerStats {
    pub current_batch_txs: usize,
    pub proving_count: usize,
    pub pending_settlement_count: usize,
    pub next_batch_id: u64,
}

// Async Batch Service

/// Messages for the batch service
pub enum BatchCommand {
    /// Submit a transaction
    Submit(TransactionType, oneshot::Sender<Result<()>>),
    /// Force seal the current batch
    Seal(oneshot::Sender<Result<Option<u64>>>),
    /// Get statistics
    Stats(oneshot::Sender<BatchManagerStats>),
    /// Shutdown
    Shutdown,
}

/// Async wrapper around BatchManager for use with tokio
pub struct BatchService {
    command_tx: mpsc::Sender<BatchCommand>,
}

impl BatchService {
    /// Start the batch service
    pub fn start(db: Arc<RocksDbStore>, config: BatchConfig) -> Result<Self> {
        let (command_tx, mut command_rx) = mpsc::channel::<BatchCommand>(1000);

        let manager = Arc::new(Mutex::new(BatchManager::new(db, config.clone())?));

        // Spawn the main service loop
        let manager_clone = manager.clone();
        tokio::spawn(async move {
            let timeout_interval = Duration::from_secs(config.max_batch_age_secs / 2);
            let mut timeout_check = tokio::time::interval(timeout_interval);

            loop {
                tokio::select! {
                    Some(cmd) = command_rx.recv() => {
                        match cmd {
                            BatchCommand::Submit(tx, reply) => {
                                let result = manager_clone.lock().await.submit_transaction(tx);
                                let _ = reply.send(result);
                            }
                            BatchCommand::Seal(reply) => {
                                let result = manager_clone.lock().await.seal_current_batch();
                                let _ = reply.send(result);
                            }
                            BatchCommand::Stats(reply) => {
                                let stats = manager_clone.lock().await.stats();
                                let _ = reply.send(stats);
                            }
                            BatchCommand::Shutdown => {
                                break;
                            }
                        }
                    }
                    _ = timeout_check.tick() => {
                        // Periodic timeout check
                        let _ = manager_clone.lock().await.check_timeout();
                    }
                }
            }
        });

        Ok(Self { command_tx })
    }

    /// Submit a transaction
    pub async fn submit(&self, tx: TransactionType) -> Result<()> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.command_tx
            .send(BatchCommand::Submit(tx, reply_tx))
            .await
            .context("batch service unavailable")?;
        reply_rx.await.context("batch service crashed")?
    }

    /// Force seal the current batch
    pub async fn seal(&self) -> Result<Option<u64>> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.command_tx
            .send(BatchCommand::Seal(reply_tx))
            .await
            .context("batch service unavailable")?;
        reply_rx.await.context("batch service crashed")?
    }

    /// Get statistics
    pub async fn stats(&self) -> Result<BatchManagerStats> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.command_tx
            .send(BatchCommand::Stats(reply_tx))
            .await
            .context("batch service unavailable")?;
        Ok(reply_rx.await.context("batch service crashed")?)
    }

    /// Shutdown the service
    pub async fn shutdown(&self) -> Result<()> {
        self.command_tx
            .send(BatchCommand::Shutdown)
            .await
            .context("batch service unavailable")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_batch_config_default() {
        let config = BatchConfig::default();
        assert_eq!(config.max_transactions, 100);
        assert_eq!(config.max_batch_age_secs, 60);
        assert_eq!(config.max_shielded, 10);
    }

    #[test]
    fn test_batch_new() {
        let batch = Batch::new(1, [0u8; 32], [0u8; 32]);
        assert_eq!(batch.id, 1);
        assert_eq!(batch.state, BatchState::Accumulating);
        assert!(batch.transactions.is_empty());
    }

    #[test]
    fn test_batch_should_seal_on_max_txs() {
        let mut batch = Batch::new(1, [0u8; 32], [0u8; 32]);
        let config = BatchConfig {
            max_transactions: 2,
            ..Default::default()
        };

        // Mock transactions
        batch
            .transactions
            .push(TransactionType::Deposit(zelana_transaction::DepositEvent {
                to: zelana_account::AccountId([0; 32]),
                amount: 100,
                l1_seq: 1,
            }));
        assert!(!batch.should_seal(&config));

        batch
            .transactions
            .push(TransactionType::Deposit(zelana_transaction::DepositEvent {
                to: zelana_account::AccountId([0; 32]),
                amount: 100,
                l1_seq: 2,
            }));
        assert!(batch.should_seal(&config));
    }
}
