//! API Module
//!
//! HTTP API endpoints for the Zelana sequencer.
//! Also includes the Zephyr UDP server for low-latency transaction submission.

pub mod handlers;
pub mod routes;
pub mod types;
pub mod udp_server;

// Re-export UDP server types (used by main.rs)
pub use udp_server::{UdpServerConfig, start_udp_server};
