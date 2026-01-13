//! API Module
//!
//! HTTP API endpoints for the Zelana sequencer.
//! Also includes the Zephyr UDP server for low-latency transaction submission.

pub mod handlers;
pub mod routes;
pub mod types;
pub mod udp_server;

pub use routes::create_router;
pub use udp_server::{UdpServerConfig, ZephyrUdpServer, start_udp_server};
