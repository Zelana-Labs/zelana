<p align="center">
  <a href="https://zelana.org">
    <img alt="Anza" src="https://raw.githubusercontent.com/Zelana-Labs/media/refs/heads/main/logo-name-2.png" width="250" />
  </a>
</p>

## Prerequisites

Before running the sequencer, you must start a **local Solana test validator** and deploy the **bridge program**.
The sequencer listens to bridge events over a **WebSocket connection**, which requires the validator to be running.

### Start the test validator

```bash
surfpool start
```

### Set Solana CLI to localnet

```bash
solana config set --url http://127.0.0.1:8899
```

### Deploy the bridge program

```bash
solana program deploy ../../onchain-programs/bridge/target/deploy/bridge.so
```

After deployment:

1. Copy the deployed **program ID**
2. Update it in `lib.rs`
3. Re-deploy the program so the change is applied

---

## Run the Sequencer

```bash
RUST_LOG=info cargo run -p core --release
```

---

## Examples

### Throughput benchmark

```bash
cargo run -p core --example bench_throughput --release
```

### Bridge test

```bash
cargo run -p core --example bridge --release
```

### Full lifecycle example

```bash
cargo run -p core --example full_lifecycle --release
```

### L2 transaction example

```bash
cargo run -p core --example transaction --release
```

---

## Debugging

### Run a service

```bash
cargo run -p prover
```

### Build a specific crate

```bash
cargo build -p core
```

## License
Licensed under the Apache License, Version 2.0. 