use {
    crate::keys::{EphemeralKeyPair, SessionKeys},
    crate::packet::{KIND_APP_DATA, KIND_CLIENT_HELLO, Packet},
    anyhow::{Context, Result, anyhow},
    tokio::{
        net::UdpSocket,
        time::{Duration, timeout},
    },
    zelana_transaction::{SignedTransaction, TransactionType},
};

pub struct ZelanaClient {
    socket: UdpSocket,
    #[allow(dead_code)] // Reserved for reconnection logic
    server_addr: String,
    session: SessionKeys,
}

impl ZelanaClient {
    /// Establishes a secure, encrypted session with the Sequencer.
    /// This performs the Diffie-Hellman Handshake.
    pub async fn connect(server_addr: &str) -> Result<Self> {
        // 1. Bind to a random local port
        let socket = UdpSocket::bind("0.0.0.0:0")
            .await
            .context("Failed to bind UDP socket")?;
        socket
            .connect(server_addr)
            .await
            .context("Failed to connect to server")?;

        // 2. Generate Ephemeral Keys for this session
        let my_keys = EphemeralKeyPair::generate();
        let my_pub_bytes = *my_keys.pk.as_bytes();

        // 3. Send ClientHello
        let mut hello_buf = Vec::with_capacity(33);
        hello_buf.push(KIND_CLIENT_HELLO);
        hello_buf.extend_from_slice(&my_pub_bytes);

        socket.send(&hello_buf).await?;

        // 4. Wait for ServerHello (with timeout)
        let mut buf = vec![0u8; 1024];
        let len = timeout(Duration::from_secs(2), socket.recv(&mut buf))
            .await
            .map_err(|_| anyhow!("Handshake timed out"))??;

        // 5. Parse ServerHello to get Server's Ephemeral Key
        let server_pk_bytes = match Packet::parse(&buf[..len])? {
            Packet::ServerHello { public_key } => public_key, // [u8;32]
            _ => return Err(anyhow!("Expected ServerHello, got something else")),
        };

        // Convert raw bytes → PublicKey
        let server_public = x25519_dalek::PublicKey::from(*server_pk_bytes);

        // 6. Derive Shared Session Keys
        let shared = my_keys.sk.diffie_hellman(&server_public);
        let shared_secret: [u8; 32] = shared.to_bytes();

        let session = SessionKeys::derive(shared_secret, &my_pub_bytes, server_pk_bytes);
        Ok(Self {
            socket,
            server_addr: server_addr.to_string(),
            session,
        })
    }

    /// Encrypts and sends a signed transaction.
    /// This is a "Fire and Forget" operation over UDP.
    pub async fn send_transaction(&mut self, tx: SignedTransaction) -> Result<()> {
        // 1. Wrap in TransactionType enum
        let l2_tx = TransactionType::Transfer(tx);

        // 2. Serialize
        let plaintext = wincode::serialize(&l2_tx).context("Serialization failed")?;

        // 3. Encrypt (adds Nonce automatically)
        let payload = self.session.encrypt(&plaintext)?;

        // 4. Prepend AppData Header
        let mut frame = Vec::with_capacity(1 + payload.len());
        frame.push(KIND_APP_DATA);
        frame.extend_from_slice(&payload);

        // 5. Blast it
        self.socket.send(&frame).await?;

        Ok(())
    }
}
