# Zelana CLI

Command-line tool for Zelana L2 bridge operations.

## Quick Start

```bash
# Generate keypair
cargo run -p cli genkey

# Initialize bridge (first time only)
cargo run -p cli init

# Airdrop SOL and bridge to L2
cargo run -p cli airdrop 1000000000
```

## Commands

**`genkey [filename]`** - Generate new keypair (default: `id.json`)

**`airdrop <amount> [filename]`** - Airdrop SOL and bridge to L2

## Configuration

```bash
export SOLANA_RPC_URL="http://127.0.0.1:8899"
export BRIDGE_PROGRAM_ID="9HXapBN9otLGnQNGv1HRk91DGqMNvMAvQqohL7gPW1sd"
```

Keypairs stored in `~/.config/solana/zelana/` with 600 permissions.