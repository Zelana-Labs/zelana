## Phase 5 — Proving pipeline (parallel + recursive)
### 5.1 Build a batch witness (off-chain)
A prover (or multiple) applies ordered txs to the state:

- starting from `old_state_root`
- updates state tree with new commitments
- updates nullifier accumulator with new nullifiers
- outputs:
    - `new_state_root`
    - `new_nullifier_root`
### 5.2 Chunk proving (parallel)
Split txs into chunks:
- chunk 1: tx 1..k
- chunk 2: tx k+1..2k  
    Each chunk prover generates `proof_chunk_i`.

### 5.3 Recursive aggregation
Aggregate `proof_chunk_i` into one:
- `proof_batch`

`proof_batch` attests:
- applying these tx effects in order transforms `old_root → new_root`
- nullifier updates are correct and no double spend
- all tx proofs were valid (or included as subproofs)
