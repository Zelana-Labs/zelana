# Zephyr Protocol Implementation Analysis

## 1. Overview

Zephyr is a lightweight, encrypted UDP transport layer for the Zelana Rollup. It is designed to
replace HTTP/TCP JSON-RPC for high-frequency trading (HFT) and real-time gaming use cases where
latency and Head-of-Line (HoL) blocking are critical. The implementation lives in
`sdk/zephyr` (crate name: `zephyr`).

### Source Files and Their Purposes

| File        | Path                                                      | Purpose                                             |
|-------------|-----------------------------------------------------------|-----------------------------------------------------|
| README.md   | sdk/zephyr/README.md                       | Protocol specification and documentation            |
| Cargo.toml  | sdk/zephyr/Cargo.toml                      | Crate dependencies and features                     |
| lib.rs      | sdk/zephyr/src/lib.rs                      | Module exports and unit tests for handshake/encryption |
| packet.rs   | sdk/zephyr/src/packet.rs                   | Zero-copy packet parsing and type definitions       |
| keys.rs     | sdk/zephyr/src/keys.rs                     | X25519 key exchange and ChaCha20-Poly1305 AEAD encryption |
| client.rs   | sdk/zephyr/src/client.rs                   | Async UDP client with handshake and transaction sending |

---

## 2. Protocol State Machine

### Connection States

```
[Disconnected]
    |
    | UdpSocket::bind() + connect()
    v
[Connected (Unencrypted)]
    |
    | Send ClientHello (ephemeral pubkey)
    v
[Awaiting ServerHello]
    |
    | Receive ServerHello (server's ephemeral pubkey)
    | Compute shared secret via X25519 ECDH
    | Derive session keys via HKDF
    v
[Session Established (Encrypted)]
    |
    | Send/Receive AppData packets
    v
[Active Session]
```

### Handshake Flow

- **ClientHello**: Client generates ephemeral X25519 keypair, sends public key (32 bytes)
- **ServerHello**: Server generates ephemeral keypair, computes shared secret, sends public key
- **Key Derivation**: Both parties independently derive session keys:
  - `SharedSecret = X25519(MyPrivateKey, TheirPublicKey)`
  - `Salt = SHA256(ClientPubKey || ServerPubKey)`
  - `SessionKeys = HKDF-SHA256(SharedSecret, Salt, info="zelana-v2-session")`

---

## 3. Message Types and Packet Formats

### Packet Type Constants

```rust
pub const KIND_CLIENT_HELLO: u8 = 1;
pub const KIND_SERVER_HELLO: u8 = 2;
pub const KIND_APP_DATA: u8 = 3;
pub const HEADER_SIZE: usize = 1 + 12;  // 1 byte kind + 12 bytes nonce
```

### Packet Enum

```rust
#[derive(Debug)]
pub enum Packet<'a> {
    ClientHello {
      public_key: &'a [u8; 32],
    },
    ServerHello {
      public_key: &'a [u8; 32],
    },
    AppData {
      nonce: &'a [u8; 12],
      ciphertext: &'a [u8],
    },
}
```

### Wire Format

| Packet Type   | Hex ID | Format                                             |
|---------------|--------|---------------------------------------------------|
| ClientHello   | 0x01   | [Kind (1B)] [Ephemeral PubKey (32B)] = 33 bytes   |
| ServerHello   | 0x02   | [Kind (1B)] [Ephemeral PubKey (32B)] = 33 bytes   |
| AppData       | 0x03   | [Kind (1B)] [Nonce (12B)] [Ciphertext (N bytes)] = 13+ bytes |

### Parsing Implementation

```rust
impl<'a> Packet<'a> {
    pub fn parse(buf: &'a [u8]) -> Result<Self> {
      if buf.is_empty() { bail!("Empty packet"); }

      match buf[0] {
        KIND_CLIENT_HELLO => {
            if buf.len() < 33 { bail!("Malformed ClientHello"); }
            let pk = array_ref(buf, 1);
            Ok(Packet::ClientHello { public_key: pk })
        }
        KIND_SERVER_HELLO => {
            if buf.len() < 33 { bail!("Malformed ServerHello"); }
            let pk = array_ref(buf, 1);
            Ok(Packet::ServerHello { public_key: pk })
        }
        KIND_APP_DATA => {
            if buf.len() < 13 { bail!("Malformed AppData (Header too small)"); }
            let nonce = array_ref_12(buf, 1);
            let ciphertext = &buf[13..];
            Ok(Packet::AppData { nonce, ciphertext })
        }
        _ => bail!("Unknown packet kind: {}", buf[0]),
      }
    }
}
```

---

## 4. Encryption Scheme

### Key Exchange: X25519 Diffie-Hellman

```rust
pub struct EphemeralKeyPair {
    pub sk: EphemeralSecret,
    pub pk: PublicKey,
}

impl EphemeralKeyPair {
    pub fn generate() -> Self {
      let sk = EphemeralSecret::random_from_rng(OsRng);
      let pk = PublicKey::from(&sk);
      Self { sk, pk }
    }
}
```

### Session Key Derivation: HKDF-SHA256

```rust
pub struct SessionKeys {
    aead: ChaCha20Poly1305,      // 256-bit key for AEAD
    base_iv: [u8; 12],           // Base initialization vector
    tx_counter: u64,             // Outgoing message counter
    rx_counter: u64,             // Incoming message counter (reserved for replay protection)
}

impl SessionKeys {
    pub fn derive(shared_secret: [u8; 32], client_pk: &[u8; 32], server_pk: &[u8; 32]) -> Self {
      // 1. Compute Salt = SHA256(client_pk || server_pk)
      let mut hasher = Sha256::new();
      hasher.update(client_pk);
      hasher.update(server_pk);
      let salt = hasher.finalize();

      // 2. HKDF Expand: Extract 44 bytes (32 key + 12 IV)
      let hk = Hkdf::<Sha256>::new(Some(&salt), &shared_secret);
      let mut okm = [0u8; 44];
      hk.expand(b"zelana-v2-session", &mut okm)
        .expect("HKDF expansion failed");

      let key = Key::from_slice(&okm[0..32]);
      let iv: [u8; 12] = okm[32..44].try_into().unwrap();

      Self {
        aead: ChaCha20Poly1305::new(key),
        base_iv: iv,
        tx_counter: 0,
        rx_counter: 0,
      }
    }
}
```

### AEAD Encryption: ChaCha20-Poly1305

#### Nonce Generation (WireGuard-style XOR counter):

```rust
fn compute_nonce(base_iv: &[u8; 12], counter: u64) -> Nonce {
    let mut n = *base_iv;
    let c = counter.to_be_bytes();
    // XOR the counter into the last 8 bytes of the IV
    for i in 0..8 {
      n[11 - i] ^= c[7 - i];
    }
    *Nonce::from_slice(&n)
}
```

#### Encryption

```rust
pub fn encrypt(&mut self, plaintext: &[u8]) -> anyhow::Result<Vec<u8>> {
    self.tx_counter += 1;
    let nonce = compute_nonce(&self.base_iv, self.tx_counter);

    let ciphertext = self.aead.encrypt(&nonce, plaintext)
      .map_err(|_| anyhow::anyhow!("Encryption failure"))?;

    // Output format: [Nonce (12B)] [Ciphertext (N bytes)]
    let mut output = Vec::with_capacity(12 + ciphertext.len());
    output.extend_from_slice(nonce.as_slice());
    output.extend_from_slice(&ciphertext);

    Ok(output)
}
```

#### Decryption

```rust
pub fn decrypt(&mut self, nonce_bytes: &[u8], ciphertext: &[u8]) -> anyhow::Result<Vec<u8>> {
    if nonce_bytes.len() != 12 {
      return Err(anyhow::anyhow!("Invalid nonce length"));
    }
    let nonce = Nonce::from_slice(nonce_bytes);

    let plaintext = self.aead.decrypt(nonce, ciphertext)
      .map_err(|_| anyhow::anyhow!("Decryption failure (Bad Key or Mac)"))?;

    Ok(plaintext)
}
```

---

## 5. Session Establishment and Management

### Client-Side Session Establishment

```rust
pub struct ZelanaClient {
    socket: UdpSocket,
    server_addr: String,
    session: SessionKeys,
}

impl ZelanaClient {
    pub async fn connect(server_addr: &str) -> Result<Self> {
      // 1. Bind to random local port
      let socket = UdpSocket::bind("0.0.0.0:0").await?;
      socket.connect(server_addr).await?;

      // 2. Generate ephemeral keypair
      let my_keys = EphemeralKeyPair::generate();
      let my_pub_bytes = *my_keys.pk.as_bytes();

      // 3. Send ClientHello
      let mut hello_buf = Vec::with_capacity(33);
      hello_buf.push(KIND_CLIENT_HELLO);
      hello_buf.extend_from_slice(&my_pub_bytes);
      socket.send(&hello_buf).await?;

      // 4. Wait for ServerHello (2s timeout)
      let mut buf = vec![0u8; 1024];
      let len = timeout(Duration::from_secs(2), socket.recv(&mut buf))
        .await.map_err(|_| anyhow!("Handshake timed out"))??;

      // 5. Parse ServerHello
      let server_pk_bytes = match Packet::parse(&buf[..len])? {
        Packet::ServerHello { public_key } => public_key,
        _ => return Err(anyhow!("Expected ServerHello")),
      };

      // 6. Derive session keys
      let server_public = x25519_dalek::PublicKey::from(*server_pk_bytes);
      let shared = my_keys.sk.diffie_hellman(&server_public);
      let session = SessionKeys::derive(shared.to_bytes(), &my_pub_bytes, server_pk_bytes);

      Ok(Self { socket, server_addr: server_addr.to_string(), session })
    }
}
```

### Server-Side Session Management

Server-side session tracking is handled by the sequencer and is still evolving.
The `zephyr` crate focuses on the client handshake + encryption flow. For
sequencer-side lifecycle details, see the
[sequencer state machine documentation](./state-machines/sequencer.md).

---

## 6. Complete Protocol Flow

### Connection to Data Transfer

```
|__________|                                   |__________|
| CLIENT   |                                   | SERVER   |
|__________|                                   |__________|
     |                                              |
     |-- [1] UDP bind("0.0.0.0:0") ---------------->|
     |-- [2] UDP connect(server_addr) ------------->|
     |                                              |
     |-- [3] ClientHello {pubkey: X25519} --------->|  (0x01 + 32B key)
     |                                              |
     |<-- [4] ServerHello {pubkey: X25519} ---------|  (0x02 + 32B key)
     |                                              |
     |   [5] Both compute:                          |
     |     shared = X25519(my_sk, their_pk)         |
     |     salt = SHA256(client_pk || server_pk)    |
     |     keys = HKDF(shared, salt, "zelana-v2-session") |
     |                                              |
     |== SESSION ESTABLISHED =======================|
     |                                              |
     |-- [6] AppData {nonce, ciphertext} ---------->|
     |   - Plaintext: wincode::serialize(TransactionType::Transfer(tx))
     |   - Encrypted with ChaCha20-Poly1305         |
     |   - Nonce = base_iv XOR counter              |
     |                                              |
     |<-- [7] (Optional responses) -----------------|
     |                                              |
```

### Transaction Sending

```rust
pub async fn send_transaction(&mut self, tx: SignedTransaction) -> Result<()> {
    // 1. Wrap in TransactionType enum
    let l2_tx = TransactionType::Transfer(tx);

    // 2. Serialize with wincode
    let plaintext = wincode::serialize(&l2_tx)?;

    // 3. Encrypt (automatically handles nonce increment)
    let payload = self.session.encrypt(&plaintext)?;

    // 4. Build frame: [KIND_APP_DATA (1B)] [Nonce (12B)] [Ciphertext]
    let mut frame = Vec::with_capacity(1 + payload.len());
    frame.push(KIND_APP_DATA);
    frame.extend_from_slice(&payload);

    // 5. Send via UDP (fire-and-forget)
    self.socket.send(&frame).await?;

    Ok(())
}
```

---

## 7. Current Integration Points

### Crate Dependencies

From `Cargo.toml` (workspace):
```toml
zephyr = { path = "sdk/zephyr" }
```
From `core/Cargo.toml`:
```toml
zephyr = { workspace = true }
```

### Usage in Examples

| Example File                              | Usage                                 |
|-------------------------------------------|---------------------------------------|
| core/examples/full_lifecycle.rs           | Full deposit + L2 transfer workflow   |
| core/examples/l2tx.rs                     | L2 transaction sending                |
| core/examples/bench_throughput.rs         | Throughput benchmarking (10k txs)     |
| core/examples/transaction.rs              | Transaction example (Zephyr client)   |

### Dependencies

```toml
anyhow = { workspace = true }
chacha20poly1305 = { workspace = true }
hkdf = "0.12"
rand_core = { workspace = true }
sha2 = { workspace = true }
thiserror = { workspace = true }
tokio = { workspace = true, optional = true }
wincode = { workspace = true }
x25519-dalek = { workspace = true, features = ["static_secrets"] }
zelana-transaction = { workspace = true }
```

## Implementation Links (GitHub)

Use these links to jump directly to the implementation files:

- [sdk/zephyr/README.md](https://github.com/zelana-Labs/zelana/blob/main/sdk/zephyr/README.md)
- [sdk/zephyr/src/lib.rs](https://github.com/zelana-Labs/zelana/blob/main/sdk/zephyr/src/lib.rs)
- [sdk/zephyr/src/packet.rs](https://github.com/zelana-Labs/zelana/blob/main/sdk/zephyr/src/packet.rs)
- [sdk/zephyr/src/keys.rs](https://github.com/zelana-Labs/zelana/blob/main/sdk/zephyr/src/keys.rs)
- [sdk/zephyr/src/client.rs](https://github.com/zelana-Labs/zelana/blob/main/sdk/zephyr/src/client.rs)
- [core/src/api/udp_server.rs](https://github.com/zelana-Labs/zelana/blob/main/core/src/api/udp_server.rs)
