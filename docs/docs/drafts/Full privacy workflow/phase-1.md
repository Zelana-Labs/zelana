## Phase 1 — User creates a private transaction (client-side)

### 1.1 Fetch latest canonical root
Wallet reads from:
- sequencer cache/indexer **for speed**
- verifies against Solana `state_root` (truth)

### 1.2 Select inputs and build witnesses
Wallet selects unspent notes it owns and prepares:
- inclusion proofs (Merkle paths) for input notes (fetched from DA/indexer)
- secrets needed to spend them

### 1.3 Execute private logic locally
Wallet runs the private function (payments / private contract):
- computes output notes / new private state
- computes **nullifiers** for spent notes
- computes **new commitments** for new notes/state objects
- prepares **encrypted outputs** for recipients (encrypted to their viewing pubkeys)

### 1.4 Generate tx proof (or proof-ready artifact)
Wallet produces `proof_tx` via prover layer that attests:
- inputs existed under `old_state_root`
- nullifiers correctly derived (ownership + uniqueness)
- output commitments correctly formed
- private rules satisfied (no negative amounts, app invariants, etc.)
```rust
pub struct TxObject {
    pub proof_tx: Vec<u8>,
    pub nullifiers: Vec<[u8; 32]>,
    pub new_commitments: Vec<[u8; 32]>,
    pub encrypted_outputs: Vec<Vec<u8>>,
    pub tx_commitment: [u8; 32],
}
```
