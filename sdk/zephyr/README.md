# Net (Zephyr Protocol)

**The high-performance, encrypted UDP transport layer for the Zelana Rollup.**

`zelana-net` implements the **Zephyr Protocol**, a lightweight application-layer protocol built over UDP. It is designed to replace standard HTTP/TCP JSON-RPC for high-frequency trading (HFT) and real-time gaming use cases, where latency and Head-of-Line (HoL) blocking are critical bottlenecks.

## Key Features

* **UDP-First:** "Fire-and-Forget" architecture prevents TCP Head-of-Line blocking.
* **Encrypted by Default:** All application data is encrypted using **ChaCha20-Poly1305**.
* **Ephemeral Security:** Uses **X25519** Diffie-Hellman key exchange for Perfect Forward Secrecy (PFS). Session keys are generated per connection and discarded on disconnect.
* **Zero-Copy Parsing:** Packet parsers operate on raw byte slices to minimize memory allocation in the hot loop.
* **Replay Protection:** Enforces XOR-based nonce counters (inspired by WireGuard) to reject replayed packets.

## Protocol Specification

The protocol consists of three packet types identified by a 1-byte header.

### Packet Types

| Kind          | Hex    | Payload Description                           |
| :------------ | :----- | :-------------------------------------------- |
| `ClientHello` | `0x01` | `[Ephemeral PubKey (32 bytes)]`               |
| `ServerHello` | `0x02` | `[Ephemeral PubKey (32 bytes)]`               |
| `AppData`     | `0x03` | `[Nonce (12 bytes)]` `[Ciphertext (N bytes)]` |

### Handshake Flow

1. **ClientHello:** Client generates an ephemeral keypair and sends its Public Key to the server.
2. **ServerHello:** Server generates its own ephemeral keypair, computes the shared secret, and sends its Public Key back.
3. **Session Established:** Both parties independently derive the session keys using HKDF.

   * `SharedSecret = X25519(MyPriv, TheirPub)`
   * `SessionKeys = HKDF(SharedSecret, Salt=Hash(ClientPub || ServerPub))`

## Usage

This crate provides the low-level primitives used by `zelana-sdk` and `zelana-sequencer`.

### 1. Parsing Packets

```rust
use zelana_net::protocol::Packet;

let buffer = [0x01, ...]; // Raw bytes from UDP socket
match Packet::parse(&buffer)? {
    Packet::ClientHello { public_key } => {
        println!("Client connecting with key: {:?}", public_key);
    }
    Packet::AppData { nonce, ciphertext } => {
        // Ready to decrypt
    }
    _ => {}
}
```

### 2. Managing Encryption

```rust
use zelana_net::SessionKeys;

// Derive keys after handshake
let mut session = SessionKeys::derive(shared_secret, &client_pk, &server_pk);

// Encrypt (Automatically handles nonce increment)
let packet = session.encrypt(b"Hello Sequencer")?;

// Decrypt
let plaintext = session.decrypt(packet_nonce, packet_ciphertext)?;
```

## Architecture

* **crypto.rs:** Implementation of X25519 Handshake logic and ChaCha20-Poly1305 wrappers.
* **protocol.rs:** Zero-allocation packet parsers and serializers.
* **lib.rs:** Core constants and type exports.

## Testing

This crate includes unit tests for the cryptographic handshake and nonce generation to ensure compatibility and security.

```bash
cargo test -p zelana-net
```
