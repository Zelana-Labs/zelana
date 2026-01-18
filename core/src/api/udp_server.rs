#![allow(dead_code)] // UDP server for Zephyr protocol (optional transport)
//! Zephyr UDP Server
//!
//! Low-latency UDP transport for transaction submission using the Zephyr protocol.
//!
//! Protocol:
//! 1. ClientHello (1 byte kind + 32 byte X25519 pubkey)
//! 2. ServerHello (1 byte kind + 32 byte X25519 pubkey)
//! 3. AppData (1 byte kind + 12 byte nonce + ciphertext)
//!
//! The server maintains per-client session state for encrypted communication.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use log::{debug, error, info, warn};
use tokio::net::UdpSocket;
use tokio::sync::RwLock;
use x25519_dalek::{PublicKey, StaticSecret};

use zelana_transaction::TransactionType;
use zephyr::keys::SessionKeys;
use zephyr::packet::{KIND_SERVER_HELLO, Packet};

use super::handlers::ApiState;

// ============================================================================
// Constants
// ============================================================================

/// Maximum UDP packet size
const MAX_PACKET_SIZE: usize = 65535;

/// Session timeout (5 minutes of inactivity)
const SESSION_TIMEOUT: Duration = Duration::from_secs(300);

/// Session cleanup interval
const CLEANUP_INTERVAL: Duration = Duration::from_secs(60);

// ============================================================================
// Session Management
// ============================================================================

/// Per-client session state
struct ClientSession {
    /// Session keys for encryption/decryption
    session_keys: SessionKeys,
    /// Last activity timestamp
    last_seen: Instant,
    /// Client's public key (for identification)
    client_pubkey: [u8; 32],
}

impl ClientSession {
    fn new(session_keys: SessionKeys, client_pubkey: [u8; 32]) -> Self {
        Self {
            session_keys,
            last_seen: Instant::now(),
            client_pubkey,
        }
    }

    fn touch(&mut self) {
        self.last_seen = Instant::now();
    }

    fn is_expired(&self) -> bool {
        self.last_seen.elapsed() > SESSION_TIMEOUT
    }
}

/// Thread-safe session store
type SessionStore = Arc<RwLock<HashMap<SocketAddr, ClientSession>>>;

// ============================================================================
// UDP Server
// ============================================================================

/// Zephyr UDP server configuration
#[derive(Debug, Clone)]
pub struct UdpServerConfig {
    /// UDP port to bind to
    pub port: u16,
    /// Maximum concurrent sessions
    pub max_sessions: usize,
}

impl Default for UdpServerConfig {
    fn default() -> Self {
        Self {
            port: 8081,
            max_sessions: 10000,
        }
    }
}

/// Zephyr UDP server
pub struct ZephyrUdpServer {
    config: UdpServerConfig,
    /// Server's static secret for DH key exchange
    server_secret: StaticSecret,
    /// Server's public key
    server_pubkey: PublicKey,
    /// Active sessions
    sessions: SessionStore,
    /// API state for transaction processing
    api_state: ApiState,
}

impl ZephyrUdpServer {
    /// Create a new UDP server
    pub fn new(config: UdpServerConfig, api_state: ApiState) -> Self {
        let server_secret = StaticSecret::random();
        let server_pubkey = PublicKey::from(&server_secret);

        Self {
            config,
            server_secret,
            server_pubkey,
            sessions: Arc::new(RwLock::new(HashMap::new())),
            api_state,
        }
    }

    /// Start the UDP server
    pub async fn run(self) -> Result<()> {
        let addr = format!("0.0.0.0:{}", self.config.port);
        let socket = UdpSocket::bind(&addr)
            .await
            .context("Failed to bind UDP socket")?;

        info!("Zephyr UDP server listening on {}", addr);

        // Wrap self in Arc for sharing across tasks
        let server = Arc::new(self);

        // Spawn session cleanup task
        {
            let server_clone = server.clone();
            tokio::spawn(async move {
                server_clone.cleanup_loop().await;
            });
        }

        // Main receive loop
        let mut buf = vec![0u8; MAX_PACKET_SIZE];
        loop {
            match socket.recv_from(&mut buf).await {
                Ok((len, src)) => {
                    let packet_data = buf[..len].to_vec();
                    let server_clone = server.clone();
                    let socket_clone = socket.local_addr().ok();

                    // Handle packet in a separate task to avoid blocking
                    tokio::spawn(async move {
                        if let Err(e) = server_clone
                            .handle_packet(&packet_data, src, socket_clone)
                            .await
                        {
                            debug!("Error handling packet from {}: {}", src, e);
                        }
                    });
                }
                Err(e) => {
                    error!("UDP recv error: {}", e);
                }
            }
        }
    }

    /// Handle an incoming packet
    async fn handle_packet(
        &self,
        data: &[u8],
        src: SocketAddr,
        _local_addr: Option<SocketAddr>,
    ) -> Result<()> {
        let packet = Packet::parse(data).context("Failed to parse packet")?;

        match packet {
            Packet::ClientHello { public_key } => {
                self.handle_client_hello(src, public_key).await?;
            }
            Packet::AppData { nonce, ciphertext } => {
                self.handle_app_data(src, nonce, ciphertext).await?;
            }
            Packet::ServerHello { .. } => {
                warn!("Received ServerHello from client {} (unexpected)", src);
            }
        }

        Ok(())
    }

    /// Handle ClientHello - perform DH key exchange
    async fn handle_client_hello(&self, src: SocketAddr, client_pubkey: &[u8; 32]) -> Result<()> {
        debug!("ClientHello from {}", src);

        // Check session limit
        {
            let sessions = self.sessions.read().await;
            if sessions.len() >= self.config.max_sessions {
                warn!("Max sessions reached, rejecting connection from {}", src);
                return Ok(());
            }
        }

        // Perform DH key exchange
        let client_pk = PublicKey::from(*client_pubkey);
        let shared_secret = self.server_secret.diffie_hellman(&client_pk);
        let server_pubkey_bytes = *self.server_pubkey.as_bytes();

        // Derive session keys
        let session_keys = SessionKeys::derive(
            shared_secret.to_bytes(),
            client_pubkey,
            &server_pubkey_bytes,
        );

        // Store session
        {
            let mut sessions = self.sessions.write().await;
            sessions.insert(src, ClientSession::new(session_keys, *client_pubkey));
        }

        // Send ServerHello response
        // Note: We need to send from the same socket, but we don't have direct access here
        // In production, you'd pass the socket or use a channel
        // For now, we'll create a new socket to send the response
        self.send_server_hello(src, &server_pubkey_bytes).await?;

        info!("Session established with {}", src);
        Ok(())
    }

    /// Send ServerHello response
    async fn send_server_hello(&self, dest: SocketAddr, pubkey: &[u8; 32]) -> Result<()> {
        let socket = UdpSocket::bind("0.0.0.0:0").await?;

        let mut response = Vec::with_capacity(33);
        response.push(KIND_SERVER_HELLO);
        response.extend_from_slice(pubkey);

        socket.send_to(&response, dest).await?;
        Ok(())
    }

    /// Handle AppData - decrypt and process transaction
    async fn handle_app_data(
        &self,
        src: SocketAddr,
        nonce: &[u8; 12],
        ciphertext: &[u8],
    ) -> Result<()> {
        // Get and update session
        let plaintext = {
            let mut sessions = self.sessions.write().await;
            let session = sessions
                .get_mut(&src)
                .ok_or_else(|| anyhow::anyhow!("No session for {}", src))?;

            session.touch();
            session.session_keys.decrypt(nonce, ciphertext)?
        };

        // Deserialize transaction
        let tx: TransactionType =
            wincode::deserialize(&plaintext).context("Failed to deserialize transaction")?;

        // Route transaction based on type
        self.process_transaction(tx, src).await
    }

    /// Process a decrypted transaction
    async fn process_transaction(&self, tx: TransactionType, src: SocketAddr) -> Result<()> {
        match tx {
            TransactionType::Transfer(signed_tx) => {
                debug!("Processing transfer from {} via UDP", src);

                // Submit to pipeline service
                let result = self
                    .api_state
                    .pipeline_service
                    .submit(TransactionType::Transfer(signed_tx))
                    .await;

                match result {
                    Ok(_) => {
                        debug!("Transfer from {} accepted", src);
                    }
                    Err(e) => {
                        warn!("Transfer from {} rejected: {}", src, e);
                    }
                }
            }
            TransactionType::Shielded(private_tx) => {
                debug!("Processing shielded tx from {} via UDP", src);

                let result = self
                    .api_state
                    .pipeline_service
                    .submit(TransactionType::Shielded(private_tx))
                    .await;

                match result {
                    Ok(_) => {
                        debug!("Shielded tx from {} accepted", src);
                    }
                    Err(e) => {
                        warn!("Shielded tx from {} rejected: {}", src, e);
                    }
                }
            }
            TransactionType::Deposit(deposit) => {
                debug!("Processing deposit from {} via UDP", src);

                let result = self
                    .api_state
                    .pipeline_service
                    .submit(TransactionType::Deposit(deposit))
                    .await;

                match result {
                    Ok(_) => debug!("Deposit from {} accepted", src),
                    Err(e) => warn!("Deposit from {} rejected: {}", src, e),
                }
            }
            TransactionType::Withdraw(withdraw) => {
                debug!("Processing withdrawal from {} via UDP", src);

                let result = self
                    .api_state
                    .pipeline_service
                    .submit(TransactionType::Withdraw(withdraw))
                    .await;

                match result {
                    Ok(_) => debug!("Withdrawal from {} accepted", src),
                    Err(e) => warn!("Withdrawal from {} rejected: {}", src, e),
                }
            }
        }

        Ok(())
    }

    /// Periodic cleanup of expired sessions
    async fn cleanup_loop(&self) {
        loop {
            tokio::time::sleep(CLEANUP_INTERVAL).await;

            let mut sessions = self.sessions.write().await;
            let before = sessions.len();

            sessions.retain(|_, session| !session.is_expired());

            let removed = before - sessions.len();
            if removed > 0 {
                debug!("Cleaned up {} expired sessions", removed);
            }
        }
    }

    /// Get current session count
    pub async fn session_count(&self) -> usize {
        self.sessions.read().await.len()
    }
}

// ============================================================================
// Helper to start UDP server from main
// ============================================================================

/// Start the Zephyr UDP server as a background task
pub async fn start_udp_server(config: UdpServerConfig, api_state: ApiState) {
    let server = ZephyrUdpServer::new(config, api_state);

    if let Err(e) = server.run().await {
        error!("UDP server error: {}", e);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = UdpServerConfig::default();
        assert_eq!(config.port, 8081);
        assert_eq!(config.max_sessions, 10000);
    }
}
