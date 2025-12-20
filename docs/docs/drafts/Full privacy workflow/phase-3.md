
## Phase 3 — Decentralized sequencing (order without seeing contents)

### 3.1 Collect ciphertexts during a window
Committee collects ciphertexts for time window `W`.

### 3.2 Order ciphertexts blindly
Leader proposes an ordering of `cipher_commitment[]`.  
Committee members sign:
- `OrderCommit = Hash(ordered_cipher_commitments || window_id)`

Once threshold signatures gathered:
- ordering is fixed (can’t reorder based on contents)
