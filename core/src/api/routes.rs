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
    Router::new()
        // Health & Status
        .route("/health", get(handlers::health))
        .route("/status/roots", get(handlers::get_state_roots))
        .route("/status/batch", get(handlers::get_batch_status))
        // Account operations
        .route("/account", post(handlers::get_account))
        // Shielded operations
        .route("/shielded/submit", post(handlers::submit_shielded))
        .route("/shielded/merkle_path", post(handlers::get_merkle_path))
        .route("/shielded/scan", post(handlers::scan_notes))
        // Withdrawal operations
        .route("/withdraw", post(handlers::submit_withdrawal))
        .route("/withdraw/status", post(handlers::get_withdrawal_status))
        // CORS
        .layer(CorsLayer::permissive())
        .with_state(state)
}

/// Create a router for the legacy ingest endpoint (backward compatibility)
pub fn create_legacy_router(state: ApiState) -> Router {
    Router::new()
        // Legacy submit_tx endpoint would go here
        .layer(CorsLayer::permissive())
        .with_state(state)
}
