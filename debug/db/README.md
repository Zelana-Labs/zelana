# debug-db

Interactive TUI for monitoring your RocksDB database in real-time.

## Quick Start

```bash
cargo run -p debug-db
```

Set custom database path:
```bash
DB_PATH=/path/to/db cargo run -p debug-db
```

## Features

- Live updates (500ms refresh)
- Search across all data (`/`)
- Copy to clipboard (`C`)
- Thousand separators in balances

## Controls

| Key | Action |
|-----|--------|
| `Tab` | Switch panels |
| `↑/↓` | Scroll |
| `/` | Search |
| `C` | Copy selected item |
| `Q` | Quit |

**Search mode:** Type to filter, `Enter` to apply, `Esc` to cancel

## Layout

```
┌────────────────────┬────────────────────┐
│ Accounts           │ Transactions       │
│                    ├────────────────────┤
│                    │ Nullifiers         │
└────────────────────┴────────────────────┘
```

Accounts show address and balance (sorted by balance).
Transactions show ID and type (Transfer/Shielded/Deposit/Withdraw).
Nullifiers show spent hashes.