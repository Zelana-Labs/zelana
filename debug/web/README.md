# Zelana Debug Dashboard

A modern web-based debug inspector for the Zelana L2 sequencer database.

## Features

- **Real-time Updates**: Auto-refreshes data every 2 seconds
- **All Column Families**: View all 12 RocksDB column families
- **Connection Status**: Monitor DB Reader, Sequencer, and Solana RPC status
- **Dark Theme**: Clean, modern dark interface
- **Search & Filter**: Find accounts, transactions, and more
- **Copy to Clipboard**: Easy copying of hashes and IDs

## Architecture

```
┌─────────────────────┐     ┌─────────────────────┐     ┌─────────────────┐
│   React Frontend    │────▶│   Bun.js Server     │────▶│  Rust DB Reader │
│   (Vite + React)    │     │   (Hono)            │     │  (RocksDB)      │
│   Port: 5173        │     │   Port: 3456        │     │  Port: 3457     │
└─────────────────────┘     └─────────────────────┘     └─────────────────┘
```

## Prerequisites

- [Bun](https://bun.sh/) v1.0+
- [Rust](https://rustup.rs/) (for db-reader)
- A Zelana database at `./zelana-db` (or set `DB_PATH`)

## Quick Start

### 1. Build the Rust DB Reader

```bash
cd /path/to/zelana
cargo build -p db-reader --release
```

### 2. Start the DB Reader Server

```bash
# From the zelana root directory
DB_PATH=./zelana-db cargo run -p db-reader --release
```

This starts a TCP server on port 3457 that reads from RocksDB.

### 3. Install Dependencies

```bash
cd debug/web
bun install
cd client
bun install
```

### 4. Start Development Servers

```bash
# From debug/web directory
bun run dev
```

Or start them separately:

```bash
# Terminal 1: Bun API server
bun run dev:server

# Terminal 2: Vite dev server
cd client && bun run dev
```

### 5. Open the Dashboard

Navigate to http://localhost:5173

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `PORT` | `3456` | Bun server port |
| `DB_READER_PORT` | `3457` | Rust DB reader port |
| `SEQUENCER_URL` | `http://127.0.0.1:8080` | Sequencer HTTP API |
| `SOLANA_RPC_URL` | `http://127.0.0.1:8899` | Solana RPC endpoint |
| `DB_PATH` | `./zelana-db` | Path to RocksDB database |

## Pages

| Page | Description |
|------|-------------|
| **Dashboard** | Overview with stats and recent transactions |
| **Accounts** | All L2 accounts with balances and nonces |
| **Transactions** | Transaction list with type/status filters |
| **Batches** | Batch list with settlement status |
| **Blocks** | Block headers with state roots |
| **Shielded** | Nullifiers, commitments, encrypted notes, tree metadata |
| **Bridge** | Processed deposits, pending withdrawals, indexer status |

## Production Build

```bash
cd debug/web/client
bun run build
```

Then serve with:

```bash
cd debug/web
bun run start
```

The server will serve the built frontend from `client/dist/`.

## Development

### Project Structure

```
debug/web/
├── package.json          # Root package
├── server/               # Bun.js backend
│   ├── index.ts          # Server entry point
│   ├── db/client.ts      # TCP client for DB reader
│   └── ws/handler.ts     # WebSocket handler
├── client/               # React frontend
│   ├── src/
│   │   ├── components/   # React components
│   │   ├── pages/        # Page components
│   │   ├── lib/          # API client & utilities
│   │   └── hooks/        # React hooks
│   └── public/           # Static assets
└── db-reader/            # Rust RocksDB server
    └── src/main.rs       # TCP server implementation
```

### Adding New Data Views

1. Add the endpoint in `server/index.ts`
2. Add the API method in `client/src/lib/api.ts`
3. Create the page component in `client/src/pages/`
4. Add the route in `client/src/App.tsx`
