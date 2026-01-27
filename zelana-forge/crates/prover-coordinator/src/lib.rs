//! Prover Coordinator Library
//!
//! This library provides the core functionality for the prover coordinator,
//! including dispatching, settlement, and Solana verification.
//!
//! ## Modules
//!
//! - `dispatcher` - Chunk-based batch dispatching to workers
//! - `settler` - Settlement to Solana L1
//! - `solana_client` - Solana RPC client for verification
//! - `core_api` - HTTP API for Core Sequencer integration (SSE)

pub mod core_api;
pub mod dispatcher;
pub mod settler;
pub mod solana_client;

pub use dispatcher::{
    Batch, BatchProofs, BatchTransaction, Chunk, ChunkProof, Dispatcher, DispatcherConfig,
};
pub use settler::{
    BatchSettlement, MockSettler, ProofSettlement, SettlementMode, Settler, SettlerConfig,
};
pub use solana_client::{
    ProofData, SolanaClientError, SolanaVerifierClient, SolanaVerifierConfig, VerificationResult,
};

// Core API types for integration with Zelana Core Sequencer
pub use core_api::{
    CoreApiConfig, CoreApiState, CoreBatchProveRequest, CoreBatchProveResponse, CoreProofResult,
    CoreShieldedWitness, CoreTransferWitness, CoreWithdrawalWitness, ProofCache, ProofJobState,
    ProofJobStatus, ProofStatusEvent, SharedCoreApiState, core_api_router,
};
