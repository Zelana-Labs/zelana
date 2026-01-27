//! Chunk Dispatcher Module
//!
//! Handles slicing batches into chunks and dispatching to workers.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::time::Instant;
use tracing::{error, info, warn};

// ============================================================================
// Types
// ============================================================================

/// A transaction in the batch
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchTransaction {
    pub sender_pubkey: String,
    pub receiver_pubkey: String,
    pub amount: u64,
    pub signature: String,
    /// Merkle path for sender's account
    pub merkle_path: Vec<String>,
}

/// A batch of transactions to be proven
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Batch {
    /// Unique batch ID
    pub batch_id: String,
    /// Initial state root before any transactions
    pub initial_root: String,
    /// All transactions in this batch
    pub transactions: Vec<BatchTransaction>,
}

/// A chunk is a subset of the batch assigned to one worker
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chunk {
    /// Chunk ID (0, 1, 2, ...)
    pub chunk_id: u32,
    /// Pre-state root for this chunk
    pub pre_root: String,
    /// Post-state root for this chunk (computed by dispatcher)
    pub post_root: String,
    /// Transactions in this chunk
    pub transactions: Vec<BatchTransaction>,
}

/// Worker assignment
#[derive(Debug, Clone)]
pub struct WorkerAssignment {
    /// Worker URL
    pub worker_url: String,
    /// Chunk to prove
    pub chunk: Chunk,
}

/// Result from a worker
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkProof {
    /// Chunk ID
    pub chunk_id: u32,
    /// Worker ID that produced this proof
    pub worker_id: u32,
    /// Proof bytes (hex)
    pub proof: String,
    /// Public inputs
    pub public_inputs: Vec<String>,
    /// Proving time in ms
    pub proving_time_ms: u64,
}

/// All proofs for a batch
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchProofs {
    /// Batch ID
    pub batch_id: String,
    /// Ordered chunk proofs (chunk 0, 1, 2, ...)
    pub proofs: Vec<ChunkProof>,
    /// Total proving time
    pub total_time_ms: u64,
    /// Number of workers used
    pub workers_used: usize,
}

/// Dispatcher configuration
#[derive(Debug, Clone)]
pub struct DispatcherConfig {
    /// Worker URLs
    pub worker_urls: Vec<String>,
    /// Transactions per chunk
    pub chunk_size: usize,
    /// HTTP client
    pub client: reqwest::Client,
    /// Timeout for each proof request (ms)
    pub proof_timeout_ms: u64,
}

// ============================================================================
// State Computation
// ============================================================================

/// Computes intermediate state roots by applying transactions.
///
/// This is a simplified version that hashes the transactions to produce
/// deterministic roots. In production, this would actually execute the
/// transactions against the state tree.
pub fn compute_intermediate_roots(
    initial_root: &str,
    transactions: &[BatchTransaction],
    chunk_size: usize,
) -> Vec<String> {
    let mut roots = vec![initial_root.to_string()];
    let mut current_root = initial_root.to_string();

    for (i, chunk) in transactions.chunks(chunk_size).enumerate() {
        // Compute new root by hashing current root + all tx in chunk
        let mut hasher = Sha256::new();
        hasher.update(current_root.as_bytes());

        for tx in chunk {
            hasher.update(tx.sender_pubkey.as_bytes());
            hasher.update(tx.receiver_pubkey.as_bytes());
            hasher.update(tx.amount.to_le_bytes());
        }

        let hash = hasher.finalize();
        current_root = format!("0x{}", hex::encode(&hash));
        roots.push(current_root.clone());

        info!("Computed root for chunk {}: {}", i, &current_root[..18]);
    }

    roots
}

/// Slice a batch into chunks with pre-computed state roots
pub fn slice_batch(batch: &Batch, chunk_size: usize) -> Vec<Chunk> {
    let roots = compute_intermediate_roots(&batch.initial_root, &batch.transactions, chunk_size);

    let mut chunks = Vec::new();

    for (i, tx_chunk) in batch.transactions.chunks(chunk_size).enumerate() {
        chunks.push(Chunk {
            chunk_id: i as u32,
            pre_root: roots[i].clone(),
            post_root: roots[i + 1].clone(),
            transactions: tx_chunk.to_vec(),
        });
    }

    info!(
        "Sliced batch {} into {} chunks (chunk_size={})",
        batch.batch_id,
        chunks.len(),
        chunk_size
    );

    chunks
}

// ============================================================================
// Worker Dispatch
// ============================================================================

/// Request to worker /prove endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerProveRequest {
    pub chunk_id: u32,
    pub pre_root: String,
    pub post_root: String,
    pub transactions: Vec<WorkerTransaction>,
}

/// Transaction format for worker
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerTransaction {
    pub sender_pubkey: String,
    pub receiver_pubkey: String,
    pub amount: u64,
    pub signature: String,
    pub merkle_path: Vec<String>,
}

/// Worker response wrapper
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "lowercase")]
pub enum WorkerResponse<T> {
    Success { data: T },
    Error { message: String },
}

/// Worker prove response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerProveResponse {
    pub job_id: String,
    pub chunk_id: u32,
    pub worker_id: u32,
    pub proof: String,
    pub public_inputs: Vec<String>,
    pub proving_time_ms: u64,
}

impl From<&BatchTransaction> for WorkerTransaction {
    fn from(tx: &BatchTransaction) -> Self {
        WorkerTransaction {
            sender_pubkey: tx.sender_pubkey.clone(),
            receiver_pubkey: tx.receiver_pubkey.clone(),
            amount: tx.amount,
            signature: tx.signature.clone(),
            merkle_path: tx.merkle_path.clone(),
        }
    }
}

/// Dispatcher for sending chunks to workers
pub struct Dispatcher {
    config: DispatcherConfig,
}

impl Dispatcher {
    pub fn new(config: DispatcherConfig) -> Self {
        Self { config }
    }

    /// Dispatch a single chunk to a worker
    pub async fn dispatch_chunk(
        &self,
        worker_url: &str,
        chunk: &Chunk,
    ) -> Result<ChunkProof, String> {
        let request = WorkerProveRequest {
            chunk_id: chunk.chunk_id,
            pre_root: chunk.pre_root.clone(),
            post_root: chunk.post_root.clone(),
            transactions: chunk
                .transactions
                .iter()
                .map(WorkerTransaction::from)
                .collect(),
        };

        info!(
            "Dispatching chunk {} to worker {} (txs: {})",
            chunk.chunk_id,
            worker_url,
            chunk.transactions.len()
        );

        let response = self
            .config
            .client
            .post(format!("{}/prove", worker_url))
            .json(&request)
            .timeout(std::time::Duration::from_millis(
                self.config.proof_timeout_ms,
            ))
            .send()
            .await
            .map_err(|e| format!("Failed to contact worker {}: {}", worker_url, e))?;

        if !response.status().is_success() {
            return Err(format!(
                "Worker {} returned error status: {}",
                worker_url,
                response.status()
            ));
        }

        let worker_response: WorkerResponse<WorkerProveResponse> = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse worker response: {}", e))?;

        match worker_response {
            WorkerResponse::Success { data } => {
                info!(
                    "Chunk {} proved by worker {} in {}ms",
                    chunk.chunk_id, data.worker_id, data.proving_time_ms
                );
                Ok(ChunkProof {
                    chunk_id: data.chunk_id,
                    worker_id: data.worker_id,
                    proof: data.proof,
                    public_inputs: data.public_inputs,
                    proving_time_ms: data.proving_time_ms,
                })
            }
            WorkerResponse::Error { message } => {
                Err(format!("Worker {} error: {}", worker_url, message))
            }
        }
    }

    /// Dispatch all chunks in parallel to workers
    pub async fn dispatch_batch(
        &self,
        batch: &Batch,
        chunk_size: usize,
    ) -> Result<BatchProofs, String> {
        let start = Instant::now();

        // Slice batch into chunks
        let chunks = slice_batch(batch, chunk_size);

        if chunks.is_empty() {
            return Err("No chunks to prove".to_string());
        }

        if chunks.len() > self.config.worker_urls.len() {
            warn!(
                "More chunks ({}) than workers ({}), some workers will prove multiple chunks",
                chunks.len(),
                self.config.worker_urls.len()
            );
        }

        // Assign chunks to workers (round-robin)
        let assignments: Vec<_> = chunks
            .iter()
            .enumerate()
            .map(|(i, chunk)| {
                let worker_url = &self.config.worker_urls[i % self.config.worker_urls.len()];
                (worker_url.clone(), chunk.clone())
            })
            .collect();

        // Dispatch all chunks in parallel
        let mut handles = Vec::new();
        for (worker_url, chunk) in assignments {
            let client = self.config.client.clone();
            let timeout = self.config.proof_timeout_ms;

            let handle = tokio::spawn(async move {
                let dispatcher = Dispatcher {
                    config: DispatcherConfig {
                        worker_urls: vec![],
                        chunk_size: 0,
                        client,
                        proof_timeout_ms: timeout,
                    },
                };
                dispatcher.dispatch_chunk(&worker_url, &chunk).await
            });
            handles.push(handle);
        }

        // Collect results
        let mut proofs = Vec::new();
        let mut errors = Vec::new();

        for handle in handles {
            match handle.await {
                Ok(Ok(proof)) => proofs.push(proof),
                Ok(Err(e)) => errors.push(e),
                Err(e) => errors.push(format!("Task panicked: {}", e)),
            }
        }

        if !errors.is_empty() {
            error!("Some chunks failed: {:?}", errors);
            // For now, we fail if any chunk fails. Could implement retry logic.
            return Err(format!("Failed chunks: {}", errors.join(", ")));
        }

        // Sort proofs by chunk ID
        proofs.sort_by_key(|p| p.chunk_id);

        let total_time_ms = start.elapsed().as_millis() as u64;
        let workers_used = proofs
            .iter()
            .map(|p| p.worker_id)
            .collect::<std::collections::HashSet<_>>()
            .len();

        info!(
            "Batch {} proved: {} chunks, {} workers, {}ms total",
            batch.batch_id,
            proofs.len(),
            workers_used,
            total_time_ms
        );

        Ok(BatchProofs {
            batch_id: batch.batch_id.clone(),
            proofs,
            total_time_ms,
            workers_used,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_intermediate_roots() {
        let initial_root = "0x1234567890abcdef";
        let transactions = vec![
            BatchTransaction {
                sender_pubkey: "0xabc".to_string(),
                receiver_pubkey: "0xdef".to_string(),
                amount: 100,
                signature: "0xsig1".to_string(),
                merkle_path: vec![],
            },
            BatchTransaction {
                sender_pubkey: "0x111".to_string(),
                receiver_pubkey: "0x222".to_string(),
                amount: 200,
                signature: "0xsig2".to_string(),
                merkle_path: vec![],
            },
            BatchTransaction {
                sender_pubkey: "0x333".to_string(),
                receiver_pubkey: "0x444".to_string(),
                amount: 300,
                signature: "0xsig3".to_string(),
                merkle_path: vec![],
            },
            BatchTransaction {
                sender_pubkey: "0x555".to_string(),
                receiver_pubkey: "0x666".to_string(),
                amount: 400,
                signature: "0xsig4".to_string(),
                merkle_path: vec![],
            },
        ];

        // Chunk size 2 should give us 3 roots: initial, after chunk 0, after chunk 1
        let roots = compute_intermediate_roots(initial_root, &transactions, 2);

        assert_eq!(roots.len(), 3);
        assert_eq!(roots[0], initial_root);
        assert!(roots[1].starts_with("0x"));
        assert!(roots[2].starts_with("0x"));
        // Each root should be different
        assert_ne!(roots[0], roots[1]);
        assert_ne!(roots[1], roots[2]);
    }

    #[test]
    fn test_slice_batch() {
        let batch = Batch {
            batch_id: "test-batch-1".to_string(),
            initial_root: "0x0000".to_string(),
            transactions: vec![
                BatchTransaction {
                    sender_pubkey: "0xa".to_string(),
                    receiver_pubkey: "0xb".to_string(),
                    amount: 100,
                    signature: "0xs".to_string(),
                    merkle_path: vec![],
                },
                BatchTransaction {
                    sender_pubkey: "0xc".to_string(),
                    receiver_pubkey: "0xd".to_string(),
                    amount: 200,
                    signature: "0xs".to_string(),
                    merkle_path: vec![],
                },
                BatchTransaction {
                    sender_pubkey: "0xe".to_string(),
                    receiver_pubkey: "0xf".to_string(),
                    amount: 300,
                    signature: "0xs".to_string(),
                    merkle_path: vec![],
                },
            ],
        };

        let chunks = slice_batch(&batch, 2);

        assert_eq!(chunks.len(), 2);

        // First chunk has 2 txs
        assert_eq!(chunks[0].chunk_id, 0);
        assert_eq!(chunks[0].transactions.len(), 2);
        assert_eq!(chunks[0].pre_root, "0x0000");

        // Second chunk has 1 tx
        assert_eq!(chunks[1].chunk_id, 1);
        assert_eq!(chunks[1].transactions.len(), 1);

        // Roots should chain
        assert_eq!(chunks[0].post_root, chunks[1].pre_root);
    }
}
