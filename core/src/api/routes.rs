//! API Routes
//!
//! Router configuration for the HTTP API.

use axum::{
    Router,
    routing::{get, post},
};
use tower_http::cors::CorsLayer;

use super::handlers::{self, ApiState};

/// Create the API router with all routes
pub fn create_router(state: ApiState) -> Router {
    let mut router = Router::new()
        // Health & Status
        .route("/health", get(handlers::health))
        .route("/status/roots", get(handlers::get_state_roots))
        .route("/status/batch", get(handlers::get_batch_status))
        .route("/status/stats", get(handlers::get_global_stats))
        // Account operations
        .route("/account", post(handlers::get_account))
        // Transfer operations (transparent L2 transfers)
        .route("/transfer", post(handlers::submit_transfer))
        // Shielded operations
        .route("/shielded/submit", post(handlers::submit_shielded))
        .route("/shielded/merkle_path", post(handlers::get_merkle_path))
        .route("/shielded/scan", post(handlers::scan_notes))
        // Withdrawal operations
        .route("/withdraw", post(handlers::submit_withdrawal))
        .route("/withdraw/status", post(handlers::get_withdrawal_status))
        // Fast withdrawal operations
        .route("/withdraw/fast/quote", post(handlers::fast_withdraw_quote))
        .route(
            "/withdraw/fast/execute",
            post(handlers::execute_fast_withdraw),
        )
        .route("/withdraw/fast/register_lp", post(handlers::register_lp))
        // Encrypted mempool operations (threshold encryption)
        .route("/encrypted/submit", post(handlers::submit_encrypted_tx))
        .route("/encrypted/committee", get(handlers::get_committee_info))
        // Batch query operations
        .route("/batch", post(handlers::get_batch))
        .route("/batches", post(handlers::list_batches))
        // Transaction query operations
        .route("/tx", post(handlers::get_transaction))
        .route("/txs", post(handlers::list_transactions));

    // Dev mode endpoints (always registered, but handlers check dev_mode flag)
    // This allows consistent routing while the handlers gate access
    if state.dev_mode {
        router = router
            .route("/dev/deposit", post(handlers::dev_deposit))
            .route("/dev/seal", post(handlers::dev_seal));
    }

    router
        // CORS
        .layer(CorsLayer::permissive())
        .with_state(state)
}
