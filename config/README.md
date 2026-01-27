# zelana-config

Shared configuration crate for all Zelana components.

## Overview

This crate provides a centralized configuration system that can be used by:
- `zelana-core` (sequencer)
- `zelana-cli` (command-line interface)
- `zelana-scripts` (testing scripts)
- Debug tools

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
zelana-config = { path = "../config" }
```

## Configuration Loading

Config is loaded from multiple sources with the following priority (highest to lowest):

```
1. Environment variables     (override everything)
2. ./config.toml             (current directory)
3. ~/.zelana/config.toml     (user home)
4. Built-in defaults         (fallback)
```

## Usage

### Quick Start

```rust
use zelana_config::{SOLANA, API, DATABASE, PIPELINE, BATCH, FEATURES};

// All config sections as lazy-loaded constants
SOLANA.bridge_program      // Pubkey
SOLANA.rpc_url             // &str
API.sequencer_url          // &str
DATABASE.path              // &str
PIPELINE.prover_mode       // ProverModeToml
BATCH.max_transactions     // usize
FEATURES.dev_mode          // bool
```

### Explicit Loading (with error handling)

For startup validation where you want to handle config errors:

```rust
use zelana_config::ZelanaConfig;

fn main() -> anyhow::Result<()> {
    let config = ZelanaConfig::load()?;
    println!("RPC: {}", config.solana.rpc_url);
    Ok(())
}
```

### Load from Specific File

```rust
use zelana_config::ZelanaConfig;
use std::path::Path;

let config = ZelanaConfig::load_from(Path::new("./custom-config.toml"))?;
```

### Testing

Inject custom config for tests:

```rust
use zelana_config::ZelanaConfig;

#[test]
fn test_with_custom_config() {
    let mut config = ZelanaConfig::default();
    config.solana.rpc_url = "http://localhost:8899".to_string();
    config.features.dev_mode = true;

    // Set as global for the test
    let _ = ZelanaConfig::set_global(config);
}
```

## Configuration File Format

Create a `config.toml` file:

```toml
[api]
sequencer = "127.0.0.1:8080"
# udp_port = 8081  # Optional

[database]
path = "./zelana-db"

[solana]
rpc_url = "http://127.0.0.1:8899"
ws_url = "ws://127.0.0.1:8900/"
bridge_program_id = "9HXapBN9otLGnQNGv1HRk91DGqMNvMAvQqohL7gPW1sd"
verifier_program_id = "7rsVijhQ1ipfc6uxzcs4R2gBtD9L5ZLubSc6vPKXgawo"
domain = "solana"

[pipeline]
prover_mode = "mock"  # "mock", "groth16", or "noir"
settlement_enabled = false
max_settlement_retries = 5
settlement_retry_base_ms = 5000
poll_interval_ms = 100

[batch]
max_transactions = 100
max_batch_age_secs = 60
max_shielded = 10
min_transactions = 1

[features]
dev_mode = false
fast_withdrawals = false
threshold_encryption = false
threshold_k = 2
threshold_n = 3
```

## Environment Variables

All config values can be overridden via environment variables:

| Variable | Description | Default |
|----------|-------------|---------|
| `ZL_CONFIG` | Path to custom config file | - |
| `ZL_API_HOST` | API server address | `127.0.0.1:8080` |
| `ZL_UDP_PORT` | UDP port | None |
| `ZL_DB_PATH` | Database path | `./zelana-db` |
| `SOLANA_RPC_URL` | Solana RPC URL | `http://127.0.0.1:8899` |
| `SOLANA_WS_URL` | Solana WebSocket URL | `ws://127.0.0.1:8900/` |
| `ZL_BRIDGE_PROGRAM` | Bridge program ID | `9HXapBN9...` |
| `ZL_VERIFIER_PROGRAM_ID` | Verifier program ID | `7rsVijhQ...` |
| `ZL_PROVER_MODE` | Prover mode | `mock` |
| `ZL_SETTLEMENT_ENABLED` | Enable settlement | `false` |
| `BATCH_MAX_TXS` | Max transactions | `100` |
| `DEV_MODE` | Enable dev mode | `false` |

## API Reference

### Constants (lazy-loaded)

```rust
use zelana_config::{SOLANA, API, DATABASE, PIPELINE, BATCH, FEATURES};
```

| Constant | Fields |
|----------|--------|
| `SOLANA` | `bridge_program`, `verifier_program`, `rpc_url`, `ws_url`, `domain` |
| `API` | `sequencer_url`, `udp_port` |
| `DATABASE` | `path` |
| `PIPELINE` | `prover_mode`, `settlement_enabled`, `max_settlement_retries`, ... |
| `BATCH` | `max_transactions`, `max_batch_age_secs`, `max_shielded`, `min_transactions` |
| `FEATURES` | `dev_mode`, `fast_withdrawals`, `threshold_encryption`, `threshold_k`, `threshold_n` |

### `ZelanaConfig` Methods

```rust
impl ZelanaConfig {
    pub fn load() -> Result<Self>;                    // Load with error handling
    pub fn load_from(path: &Path) -> Result<Self>;    // Load from specific file
    pub fn global() -> &'static ZelanaConfig;         // Get global instance
    pub fn set_global(config: Self) -> Result<(), Self>; // Set for testing
    pub fn generate_sample() -> String;               // Generate sample TOML
}
```
