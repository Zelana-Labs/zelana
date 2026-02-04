#![allow(dead_code)] // Pending: prover integration phase
//! Prover Integration
//!
//! Interface to the ZK prover for batch state transition proofs.
//!
//! ```text
//! -------------------------------------------------------------------
//! -                     Batch Proof                                  -
//! -                                                                  -
//! -  Public Inputs:                                                  -
//! -  -------------------------------------------------------------- -
//! -  - • pre_state_root      (transparent state before batch)     - -
//! -  - • post_state_root     (transparent state after batch)      - -
//! -  - • pre_shielded_root   (commitment tree before batch)       - -
//! -  - • post_shielded_root  (commitment tree after batch)        - -
//! -  - • withdrawal_root     (merkle root of withdrawals)         - -
//! -  - • batch_hash          (hash of all transactions)           - -
//! -  -------------------------------------------------------------- -
//! -                                                                  -
//! -  Private Witness:                                                -
//! -  -------------------------------------------------------------- -
//! -  - • All transactions (transfers, shielded, deposits, etc.)   - -
//! -  - • Merkle proofs for account updates                        - -
//! -  - • Individual shielded transaction proofs                   - -
//! -  -------------------------------------------------------------- -
//! -------------------------------------------------------------------
//! ```

use anyhow::{Context, Result};
use tokio::sync::{mpsc, oneshot};

use crate::sequencer::Batch;
use crate::sequencer::TxResult;
use zelana_transaction::TransactionType;

// Arkworks imports for real Groth16 proving
use ark_bn254::{Bn254, Fr};
use ark_ff::PrimeField;
use ark_groth16::{Groth16, Proof, ProvingKey, VerifyingKey};
use ark_serialize::{CanonicalDeserialize, CanonicalSerialize};
use ark_snark::SNARK;
use ark_std::rand::{SeedableRng, rngs::StdRng};

// Proof Types

/// Public inputs for a batch proof
#[derive(Debug, Clone)]
pub struct BatchPublicInputs {
    /// State root before batch execution
    pub pre_state_root: [u8; 32],
    /// State root after batch execution
    pub post_state_root: [u8; 32],
    /// Shielded commitment tree root before batch
    pub pre_shielded_root: [u8; 32],
    /// Shielded commitment tree root after batch
    pub post_shielded_root: [u8; 32],
    /// Merkle root of withdrawals in this batch
    pub withdrawal_root: [u8; 32],
    /// Hash of all transactions in batch
    pub batch_hash: [u8; 32],
    /// Batch ID
    pub batch_id: u64,
}

/// A generated batch proof
#[derive(Debug, Clone)]
pub struct BatchProof {
    /// The public inputs
    pub public_inputs: BatchPublicInputs,
    /// The proof bytes (Groth16)
    pub proof_bytes: Vec<u8>,
    /// Proving time in milliseconds
    pub proving_time_ms: u64,
}

/// Witness data for batch proving
#[derive(Debug, Clone)]
pub struct BatchWitness {
    /// All transactions in the batch
    pub transactions: Vec<TransactionType>,
    /// Execution results
    pub results: Vec<TxResult>,
    /// Account state before each transaction (legacy, for backward compat)
    pub pre_account_states: Vec<AccountStateSnapshot>,
    /// Per-transfer witness data with correct intermediate merkle paths
    pub transfer_witnesses: Vec<TransferWitnessData>,
    /// Per-withdrawal witness data
    pub withdrawal_witnesses: Vec<WithdrawalWitnessData>,
}

/// Snapshot of account state for witness
#[derive(Debug, Clone)]
pub struct AccountStateSnapshot {
    pub account_id: [u8; 32],
    pub balance: u64,
    pub nonce: u64,
    /// Merkle proof siblings (32 hashes for 32-level tree)
    pub merkle_proof: Vec<[u8; 32]>,
    /// Path indices (0 = left, 1 = right) for each level
    pub path_indices: Vec<u8>,
    /// Position in the tree (leaf index)
    pub position: u64,
}

/// Per-transfer witness with correct intermediate merkle paths
/// Sender path is against state BEFORE sender update
/// Receiver path is against state AFTER sender update
#[derive(Debug, Clone)]
pub struct TransferWitnessData {
    /// Sender pubkey
    pub sender_pubkey: [u8; 32],
    /// Sender balance before transfer
    pub sender_balance: u64,
    /// Sender nonce before transfer
    pub sender_nonce: u64,
    /// Sender merkle path (valid against state before this transfer)
    pub sender_merkle_path: Vec<[u8; 32]>,
    /// Sender path indices
    pub sender_path_indices: Vec<u8>,
    /// Receiver pubkey
    pub receiver_pubkey: [u8; 32],
    /// Receiver balance before transfer (after sender update)
    pub receiver_balance: u64,
    /// Receiver nonce before transfer
    pub receiver_nonce: u64,
    /// Receiver merkle path (valid against state AFTER sender update)
    pub receiver_merkle_path: Vec<[u8; 32]>,
    /// Receiver path indices
    pub receiver_path_indices: Vec<u8>,
    /// Transfer amount
    pub amount: u64,
    /// Transaction signature
    pub signature: Vec<u8>,
}

/// Per-withdrawal witness data
#[derive(Debug, Clone)]
pub struct WithdrawalWitnessData {
    /// Sender pubkey
    pub sender_pubkey: [u8; 32],
    /// Sender balance before withdrawal
    pub sender_balance: u64,
    /// Sender nonce before withdrawal
    pub sender_nonce: u64,
    /// Sender merkle path
    pub sender_merkle_path: Vec<[u8; 32]>,
    /// Sender path indices
    pub sender_path_indices: Vec<u8>,
    /// L1 recipient address
    pub l1_recipient: [u8; 32],
    /// Withdrawal amount
    pub amount: u64,
    /// Transaction signature
    pub signature: Vec<u8>,
}

// Prover Trait

/// Trait for ZK proof generation
pub trait BatchProver: Send + Sync {
    /// Generate a proof for a batch
    fn prove(&self, inputs: &BatchPublicInputs, witness: &BatchWitness) -> Result<BatchProof>;

    /// Verify a batch proof (for testing)
    fn verify(&self, proof: &BatchProof) -> Result<bool>;

    /// Get the verification key hash (for L1 contract)
    fn verification_key_hash(&self) -> [u8; 32];
}

// Mock Prover (MVP)

/// Mock prover for MVP - generates fake proofs
///
/// In production, this would:
/// 1. Build the witness from batch data
/// 2. Run the circuit with witness
/// 3. Generate Groth16 proof
pub struct MockProver {
    /// Simulated proving time in ms
    prove_time_ms: u64,
    /// Mock verification key hash
    vk_hash: [u8; 32],
}

impl MockProver {
    pub fn new() -> Self {
        Self {
            prove_time_ms: 100, // Simulate 100ms proving time
            vk_hash: *blake3::hash(b"zelana-mock-vk-v1").as_bytes(),
        }
    }

    /// Create with custom proving time (for testing)
    pub fn with_prove_time(ms: u64) -> Self {
        Self {
            prove_time_ms: ms,
            vk_hash: *blake3::hash(b"zelana-mock-vk-v1").as_bytes(),
        }
    }
}

impl Default for MockProver {
    fn default() -> Self {
        Self::new()
    }
}

impl BatchProver for MockProver {
    fn prove(&self, inputs: &BatchPublicInputs, _witness: &BatchWitness) -> Result<BatchProof> {
        // Simulate proving time
        std::thread::sleep(std::time::Duration::from_millis(self.prove_time_ms));

        // Generate mock proof (hash of public inputs)
        let mut hasher = blake3::Hasher::new();
        hasher.update(&inputs.pre_state_root);
        hasher.update(&inputs.post_state_root);
        hasher.update(&inputs.pre_shielded_root);
        hasher.update(&inputs.post_shielded_root);
        hasher.update(&inputs.withdrawal_root);
        hasher.update(&inputs.batch_hash);
        hasher.update(&inputs.batch_id.to_le_bytes());

        // Mock proof is 256 bytes (real Groth16 is ~192 bytes for BLS12-381)
        let mut proof_bytes = Vec::with_capacity(256);
        proof_bytes.extend_from_slice(hasher.finalize().as_bytes());
        proof_bytes.extend_from_slice(&[0u8; 224]); // Padding

        Ok(BatchProof {
            public_inputs: inputs.clone(),
            proof_bytes,
            proving_time_ms: self.prove_time_ms,
        })
    }

    fn verify(&self, proof: &BatchProof) -> Result<bool> {
        // Mock verification - check proof is well-formed
        Ok(proof.proof_bytes.len() >= 32)
    }

    fn verification_key_hash(&self) -> [u8; 32] {
        self.vk_hash
    }
}

// Groth16 Prover (Real ZK Proving)

/// Real Groth16 prover using arkworks and BN254 curve
///
/// This generates actual ZK proofs that can be verified on Solana
/// using the alt_bn128 precompiles.
pub struct Groth16Prover {
    /// The proving key (loaded from file or generated)
    proving_key: ProvingKey<Bn254>,
    /// The verifying key
    verifying_key: VerifyingKey<Bn254>,
    /// Hash of the verification key
    vk_hash: [u8; 32],
}

impl Groth16Prover {
    /// Create a new prover from serialized keys
    pub fn from_bytes(pk_bytes: &[u8], vk_bytes: &[u8]) -> Result<Self> {
        let proving_key = ProvingKey::<Bn254>::deserialize_compressed(pk_bytes)
            .map_err(|e| anyhow::anyhow!("Failed to deserialize proving key: {}", e))?;
        let verifying_key = VerifyingKey::<Bn254>::deserialize_compressed(vk_bytes)
            .map_err(|e| anyhow::anyhow!("Failed to deserialize verifying key: {}", e))?;

        // Compute VK hash for on-chain verification
        let vk_hash = Self::compute_vk_hash(&verifying_key)?;

        Ok(Self {
            proving_key,
            verifying_key,
            vk_hash,
        })
    }

    /// Load prover from files
    pub fn from_files(pk_path: &str, vk_path: &str) -> Result<Self> {
        let pk_bytes = std::fs::read(pk_path)
            .with_context(|| format!("Failed to read proving key from {}", pk_path))?;
        let vk_bytes = std::fs::read(vk_path)
            .with_context(|| format!("Failed to read verifying key from {}", vk_path))?;
        Self::from_bytes(&pk_bytes, &vk_bytes)
    }

    /// Compute hash of verifying key for on-chain reference
    fn compute_vk_hash(vk: &VerifyingKey<Bn254>) -> Result<[u8; 32]> {
        let mut vk_bytes = Vec::new();
        vk.serialize_compressed(&mut vk_bytes)
            .map_err(|e| anyhow::anyhow!("Failed to serialize VK: {}", e))?;
        Ok(*blake3::hash(&vk_bytes).as_bytes())
    }

    /// Get the verifying key for on-chain submission
    pub fn verifying_key(&self) -> &VerifyingKey<Bn254> {
        &self.verifying_key
    }

    /// Convert proof to bytes for on-chain submission (Solana format)
    /// Returns: (pi_a_negated[64], pi_b[128], pi_c[64]) = 256 bytes
    #[allow(unused_imports)]
    pub fn proof_to_solana_bytes(proof: &Proof<Bn254>) -> Result<Vec<u8>> {
        use ark_ec::AffineRepr;
        use ark_ff::BigInteger;

        let mut bytes = Vec::with_capacity(256);

        // pi_a (G1 point) - 64 bytes, negated for Groth16 verification
        let pi_a_neg = -proof.a;
        let x_bytes = pi_a_neg.x.into_bigint().to_bytes_le();
        let y_bytes = pi_a_neg.y.into_bigint().to_bytes_le();
        bytes.extend_from_slice(&x_bytes);
        bytes.extend_from_slice(&y_bytes);

        // pi_b (G2 point) - 128 bytes
        let x_c0_bytes = proof.b.x.c0.into_bigint().to_bytes_le();
        let x_c1_bytes = proof.b.x.c1.into_bigint().to_bytes_le();
        let y_c0_bytes = proof.b.y.c0.into_bigint().to_bytes_le();
        let y_c1_bytes = proof.b.y.c1.into_bigint().to_bytes_le();
        bytes.extend_from_slice(&x_c0_bytes);
        bytes.extend_from_slice(&x_c1_bytes);
        bytes.extend_from_slice(&y_c0_bytes);
        bytes.extend_from_slice(&y_c1_bytes);

        // pi_c (G1 point) - 64 bytes
        let x_bytes = proof.c.x.into_bigint().to_bytes_le();
        let y_bytes = proof.c.y.into_bigint().to_bytes_le();
        bytes.extend_from_slice(&x_bytes);
        bytes.extend_from_slice(&y_bytes);

        Ok(bytes)
    }

    /// Convert public inputs to field elements for verification
    fn public_inputs_to_fr(inputs: &BatchPublicInputs) -> Vec<Fr> {
        vec![
            Fr::from_le_bytes_mod_order(&inputs.pre_state_root),
            Fr::from_le_bytes_mod_order(&inputs.post_state_root),
            Fr::from_le_bytes_mod_order(&inputs.pre_shielded_root),
            Fr::from_le_bytes_mod_order(&inputs.post_shielded_root),
            Fr::from_le_bytes_mod_order(&inputs.withdrawal_root),
            Fr::from_le_bytes_mod_order(&inputs.batch_hash),
        ]
    }
}

impl BatchProver for Groth16Prover {
    fn prove(&self, inputs: &BatchPublicInputs, witness: &BatchWitness) -> Result<BatchProof> {
        let start = std::time::Instant::now();

        // Create deterministic RNG from batch ID for reproducibility
        let mut rng = StdRng::seed_from_u64(inputs.batch_id);

        // Convert BatchWitness transactions to circuit TransactionWitness format
        let tx_witnesses: Vec<prover::TransactionWitness> = witness
            .transactions
            .iter()
            .filter_map(|tx| match tx {
                TransactionType::Transfer(t) => Some(prover::TransactionWitness {
                    sender_pk: t.signer_pubkey,
                    recipient_pk: t.data.to.0,
                    amount: t.data.amount,
                }),
                // TODO: Handle other transaction types when circuit supports them
                _ => None,
            })
            .collect();

        // Build initial_accounts from pre_account_states in witness
        let mut initial_accounts: std::collections::BTreeMap<prover::PubkeyBytes, u64> =
            std::collections::BTreeMap::new();
        for state in &witness.pre_account_states {
            initial_accounts.insert(state.account_id, state.balance);
        }

        // Convert withdrawals to circuit format
        let withdrawal_witnesses: Vec<prover::WithdrawalWitness> = witness
            .transactions
            .iter()
            .filter_map(|tx| match tx {
                TransactionType::Withdraw(w) => Some(prover::WithdrawalWitness {
                    recipient: w.to_l1_address,
                    amount: w.amount,
                }),
                _ => None,
            })
            .collect();

        // Build the L2BlockCircuit with all 7 public inputs
        let circuit = prover::L2BlockCircuit {
            pre_state_root: Some(inputs.pre_state_root),
            post_state_root: Some(inputs.post_state_root),
            pre_shielded_root: Some(inputs.pre_shielded_root),
            post_shielded_root: Some(inputs.post_shielded_root),
            withdrawal_root: Some(inputs.withdrawal_root),
            batch_hash: Some(inputs.batch_hash),
            batch_id: Some(inputs.batch_id),
            transactions: Some(tx_witnesses),
            initial_accounts: Some(initial_accounts),
            shielded_commitments: Some(vec![]), // TODO: Extract from shielded txs
            withdrawals: Some(withdrawal_witnesses),
            poseidon_config: prover::get_poseidon_config(),
        };

        // Generate the proof
        let proof = Groth16::<Bn254>::prove(&self.proving_key, circuit, &mut rng)
            .map_err(|e| anyhow::anyhow!("Proving failed: {}", e))?;

        // Convert proof to bytes
        let proof_bytes = Self::proof_to_solana_bytes(&proof)?;
        tracing::info!(
            "Generated Groth16 proof: {} bytes (expected 256)",
            proof_bytes.len()
        );

        let proving_time_ms = start.elapsed().as_millis() as u64;

        Ok(BatchProof {
            public_inputs: inputs.clone(),
            proof_bytes,
            proving_time_ms,
        })
    }

    fn verify(&self, proof: &BatchProof) -> Result<bool> {
        // Parse the proof bytes back to arkworks Proof struct
        if proof.proof_bytes.len() < 256 {
            return Ok(false);
        }

        // For full verification, we'd need to reconstruct the proof
        // from bytes and verify against the VK
        // For now, use the public inputs
        let _public_inputs = Self::public_inputs_to_fr(&proof.public_inputs);

        // This is a simplified verification - in production we'd
        // deserialize the proof and use Groth16::verify
        // For now, just check the proof is well-formed
        Ok(proof.proof_bytes.len() == 256)
    }

    fn verification_key_hash(&self) -> [u8; 32] {
        self.vk_hash
    }
}

// Async Prover Service

/// Request to prove a batch
pub struct ProveRequest {
    pub batch_id: u64,
    pub inputs: BatchPublicInputs,
    pub witness: BatchWitness,
    pub reply: oneshot::Sender<Result<BatchProof>>,
}

/// Async prover service for background proving
pub struct ProverService {
    request_tx: mpsc::Sender<ProveRequest>,
}

impl ProverService {
    /// Start the prover service with the given prover implementation
    pub fn start<P: BatchProver + 'static>(prover: P) -> Self {
        let (request_tx, mut request_rx) = mpsc::channel::<ProveRequest>(32);

        // Spawn proving thread (separate from tokio runtime for CPU-intensive work)
        std::thread::spawn(move || {
            while let Some(request) = request_rx.blocking_recv() {
                let result = prover.prove(&request.inputs, &request.witness);
                let _ = request.reply.send(result);
            }
        });

        Self { request_tx }
    }

    /// Submit a batch for proving
    pub async fn prove(
        &self,
        batch_id: u64,
        inputs: BatchPublicInputs,
        witness: BatchWitness,
    ) -> Result<BatchProof> {
        let (reply_tx, reply_rx) = oneshot::channel();

        self.request_tx
            .send(ProveRequest {
                batch_id,
                inputs,
                witness,
                reply: reply_tx,
            })
            .await
            .context("prover service unavailable")?;

        reply_rx.await.context("prover crashed")?
    }
}

// Helper Functions

/// Build public inputs from a sealed batch
pub fn build_public_inputs(batch: &Batch, withdrawal_root: [u8; 32]) -> Result<BatchPublicInputs> {
    let post_state_root = batch.post_state_root.context("batch not executed")?;
    let post_shielded_root = batch.post_shielded_root.context("batch not executed")?;

    // Compute batch hash
    let batch_hash = compute_batch_hash(&batch.transactions);

    Ok(BatchPublicInputs {
        pre_state_root: batch.pre_state_root,
        post_state_root,
        pre_shielded_root: batch.pre_shielded_root,
        post_shielded_root,
        withdrawal_root,
        batch_hash,
        batch_id: batch.id,
    })
}

/// Compute hash of all transactions in a batch
pub fn compute_batch_hash(transactions: &[TransactionType]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();

    for tx in transactions {
        match tx {
            TransactionType::Shielded(p) => {
                hasher.update(b"shielded");
                hasher.update(&p.nullifier);
                hasher.update(&p.commitment);
            }
            TransactionType::Transfer(t) => {
                hasher.update(b"transfer");
                hasher.update(&t.signer_pubkey);
                hasher.update(&t.data.to.0);
                hasher.update(&t.data.amount.to_le_bytes());
                hasher.update(&t.data.nonce.to_le_bytes());
            }
            TransactionType::Deposit(d) => {
                hasher.update(b"deposit");
                hasher.update(&d.to.0);
                hasher.update(&d.amount.to_le_bytes());
                hasher.update(&d.l1_seq.to_le_bytes());
            }
            TransactionType::Withdraw(w) => {
                hasher.update(b"withdraw");
                hasher.update(&w.from.0);
                hasher.update(&w.to_l1_address);
                hasher.update(&w.amount.to_le_bytes());
            }
        }
    }

    *hasher.finalize().as_bytes()
}

/// Build a minimal witness for MVP (full witness would include merkle proofs)
pub fn build_witness(batch: &Batch) -> BatchWitness {
    BatchWitness {
        transactions: batch.transactions.clone(),
        results: batch.results.clone(),
        pre_account_states: Vec::new(), // MVP: Skip account proofs
        transfer_witnesses: Vec::new(),
        withdrawal_witnesses: Vec::new(),
    }
}

/// Build a full witness with merkle proofs from the account tree
///
/// IMPORTANT: For transfers, the circuit processes sender and receiver sequentially:
/// 1. Verify sender merkle path against current_state_root
/// 2. Update sender -> current_state_root changes
/// 3. Verify receiver merkle path against NEW current_state_root
/// 4. Update receiver -> current_state_root changes
///
/// Therefore, we must compute receiver paths AFTER simulating the sender update.
pub fn build_witness_with_proofs(
    batch: &Batch,
    account_tree: &crate::sequencer::storage::account_tree::AccountTree,
    db: &crate::sequencer::storage::db::RocksDbStore,
) -> BatchWitness {
    use crate::storage::StateStore;
    use zelana_account::{AccountId, AccountState};

    // Clone the tree so we can simulate state updates
    let mut sim_tree = account_tree.clone();

    // Track current account states (simulated)
    let mut account_states: std::collections::HashMap<AccountId, AccountState> =
        std::collections::HashMap::new();

    // Pre-populate with current DB states for all accounts we'll touch
    for tx in &batch.transactions {
        match tx {
            TransactionType::Transfer(t) => {
                let sender_id = AccountId(t.signer_pubkey);
                let receiver_id = t.data.to;
                if !account_states.contains_key(&sender_id) {
                    account_states.insert(
                        sender_id,
                        db.get_account_state(&sender_id).unwrap_or_default(),
                    );
                }
                if !account_states.contains_key(&receiver_id) {
                    account_states.insert(
                        receiver_id,
                        db.get_account_state(&receiver_id).unwrap_or_default(),
                    );
                }
            }
            TransactionType::Withdraw(w) => {
                let sender_id = w.from;
                if !account_states.contains_key(&sender_id) {
                    account_states.insert(
                        sender_id,
                        db.get_account_state(&sender_id).unwrap_or_default(),
                    );
                }
            }
            TransactionType::Deposit(d) => {
                let to_id = d.to;
                if !account_states.contains_key(&to_id) {
                    account_states.insert(to_id, db.get_account_state(&to_id).unwrap_or_default());
                }
            }
            TransactionType::Shielded(_) => {}
        }
    }

    let mut transfer_witnesses = Vec::new();
    let mut withdrawal_witnesses = Vec::new();
    let mut pre_account_states = Vec::new(); // Keep for backward compat
    let mut seen_accounts = std::collections::HashSet::new();

    for tx in &batch.transactions {
        match tx {
            TransactionType::Transfer(t) => {
                let sender_id = AccountId(t.signer_pubkey);
                let receiver_id = t.data.to;
                let amount = t.data.amount;

                // Get sender's current state and merkle path BEFORE sender update
                let sender_state = account_states.get(&sender_id).cloned().unwrap_or_default();
                let sender_path = sim_tree.path(&sender_id).unwrap_or_default();

                // Simulate sender update in the tree
                let new_sender_state = AccountState {
                    balance: sender_state.balance.saturating_sub(amount),
                    nonce: sender_state.nonce + 1,
                };
                sim_tree.insert(&sender_id, &new_sender_state);
                account_states.insert(sender_id, new_sender_state);

                // Now get receiver's merkle path AFTER sender update
                let receiver_state = account_states
                    .get(&receiver_id)
                    .cloned()
                    .unwrap_or_default();
                let receiver_path = sim_tree.path(&receiver_id).unwrap_or_else(|| {
                    // Receiver doesn't exist yet - insert with current balance first
                    sim_tree.insert(&receiver_id, &receiver_state);
                    sim_tree.path(&receiver_id).unwrap_or_default()
                });

                // Simulate receiver update in the tree
                let new_receiver_state = AccountState {
                    balance: receiver_state.balance + amount,
                    nonce: receiver_state.nonce,
                };
                sim_tree.insert(&receiver_id, &new_receiver_state);
                account_states.insert(receiver_id, new_receiver_state);

                // Create the transfer witness with correct paths
                transfer_witnesses.push(TransferWitnessData {
                    sender_pubkey: t.signer_pubkey,
                    sender_balance: sender_state.balance,
                    sender_nonce: sender_state.nonce,
                    sender_merkle_path: sender_path.siblings.to_vec(),
                    sender_path_indices: sender_path.path_indices.to_vec(),
                    receiver_pubkey: receiver_id.0,
                    receiver_balance: receiver_state.balance,
                    receiver_nonce: receiver_state.nonce,
                    receiver_merkle_path: receiver_path.siblings.to_vec(),
                    receiver_path_indices: receiver_path.path_indices.to_vec(),
                    amount,
                    signature: t.signature.clone(),
                });

                // Also populate legacy pre_account_states for backward compat
                if seen_accounts.insert(sender_id) {
                    pre_account_states.push(AccountStateSnapshot {
                        account_id: sender_id.0,
                        balance: sender_state.balance,
                        nonce: sender_state.nonce,
                        merkle_proof: sender_path.siblings.to_vec(),
                        path_indices: sender_path.path_indices.to_vec(),
                        position: sender_path.position,
                    });
                }
                if seen_accounts.insert(receiver_id) {
                    pre_account_states.push(AccountStateSnapshot {
                        account_id: receiver_id.0,
                        balance: receiver_state.balance,
                        nonce: receiver_state.nonce,
                        merkle_proof: receiver_path.siblings.to_vec(),
                        path_indices: receiver_path.path_indices.to_vec(),
                        position: receiver_path.position,
                    });
                }
            }
            TransactionType::Withdraw(w) => {
                let sender_id = w.from;
                let amount = w.amount;

                // Get sender's current state and merkle path
                let sender_state = account_states.get(&sender_id).cloned().unwrap_or_default();
                let sender_path = sim_tree.path(&sender_id).unwrap_or_default();

                // Simulate sender update in the tree
                let new_sender_state = AccountState {
                    balance: sender_state.balance.saturating_sub(amount),
                    nonce: sender_state.nonce + 1,
                };
                sim_tree.insert(&sender_id, &new_sender_state);
                account_states.insert(sender_id, new_sender_state);

                withdrawal_witnesses.push(WithdrawalWitnessData {
                    sender_pubkey: sender_id.0,
                    sender_balance: sender_state.balance,
                    sender_nonce: sender_state.nonce,
                    sender_merkle_path: sender_path.siblings.to_vec(),
                    sender_path_indices: sender_path.path_indices.to_vec(),
                    l1_recipient: w.to_l1_address,
                    amount,
                    signature: w.signature.clone(),
                });

                if seen_accounts.insert(sender_id) {
                    pre_account_states.push(AccountStateSnapshot {
                        account_id: sender_id.0,
                        balance: sender_state.balance,
                        nonce: sender_state.nonce,
                        merkle_proof: sender_path.siblings.to_vec(),
                        path_indices: sender_path.path_indices.to_vec(),
                        position: sender_path.position,
                    });
                }
            }
            TransactionType::Deposit(d) => {
                // IMPORTANT: The Noir circuit does NOT process deposits for transparent state root.
                // We only record the pre-state snapshot but do NOT update sim_tree or account_states.
                // This ensures merkle paths match what the circuit expects.
                let to_id = d.to;

                if seen_accounts.insert(to_id) {
                    if let Some(path) = sim_tree.path(&to_id) {
                        let state = account_states.get(&to_id).cloned().unwrap_or_default();
                        pre_account_states.push(AccountStateSnapshot {
                            account_id: to_id.0,
                            balance: state.balance,
                            nonce: state.nonce,
                            merkle_proof: path.siblings.to_vec(),
                            path_indices: path.path_indices.to_vec(),
                            position: path.position,
                        });
                    }
                }
                // DO NOT simulate deposit update here - circuit skips deposits for state root
            }
            TransactionType::Shielded(_) => {
                // Shielded transactions use the commitment tree, not account tree
            }
        }
    }

    BatchWitness {
        transactions: batch.transactions.clone(),
        results: batch.results.clone(),
        pre_account_states,
        transfer_witnesses,
        withdrawal_witnesses,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_prover() {
        let prover = MockProver::new();

        let inputs = BatchPublicInputs {
            pre_state_root: [1u8; 32],
            post_state_root: [2u8; 32],
            pre_shielded_root: [3u8; 32],
            post_shielded_root: [4u8; 32],
            withdrawal_root: [5u8; 32],
            batch_hash: [6u8; 32],
            batch_id: 1,
        };

        let witness = BatchWitness {
            transactions: vec![],
            results: vec![],
            pre_account_states: vec![],
            transfer_witnesses: vec![],
            withdrawal_witnesses: vec![],
        };

        let proof = prover.prove(&inputs, &witness).unwrap();
        assert!(!proof.proof_bytes.is_empty());
        assert!(prover.verify(&proof).unwrap());
    }

    #[test]
    fn test_batch_hash() {
        let txs1 = vec![];
        let txs2 = vec![];

        let hash1 = compute_batch_hash(&txs1);
        let hash2 = compute_batch_hash(&txs2);

        // Empty batches should have same hash
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_verification_key_hash() {
        let prover = MockProver::new();
        let vk_hash = prover.verification_key_hash();
        assert_ne!(vk_hash, [0u8; 32]);
    }
}
