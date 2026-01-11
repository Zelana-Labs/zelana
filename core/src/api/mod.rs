//! API Module
//!
//! HTTP API endpoints for the Zelana sequencer.

pub mod handlers;
pub mod routes;
pub mod types;

pub use routes::create_router;
