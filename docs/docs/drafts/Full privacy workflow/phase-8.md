
## Phase 8 — Wallet sync (how receivers learn updates)
### 8.1 Detect new finalized batch
Wallet watches Solana for:
- new `batch_index` / root update

### 8.2 Fetch DA blob (fast path: sequencer cache)
Wallet fetches `DA_blob` from:
- sequencer cache/indexer (fast)
- verifies `H(DA_blob) == batch_hash` from Solana

If mismatch:
- ignore that source and fetch from elsewhere

### 8.3 Scan/decrypt outputs
Wallet scans `encrypted_outputs`:
- attempt decrypt with viewing key
- if success: obtain note/state update
- verify inclusion under `new_state_root` (or store and prove later)
### 8.4 Mark spent notes
Wallet checks batch `nullifiers`:
- if any match wallet’s derived nullifiers → mark notes spent

Wallet’s local state updates:
- new unspent notes added
- spent notes removed
- private contract state updated (from decrypted logs)
