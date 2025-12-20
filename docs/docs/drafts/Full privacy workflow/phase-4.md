## Phase 4 — Decrypt-after-ordering (now reveal TxObject, still opaque)

### 4.1 Threshold decryption

Committee runs threshold decryption on ciphertexts **in the agreed order**:
- `TxObject_1, TxObject_2, ...`

Reminder: even now TxObj is still “opaque”:
- no addresses
- no amounts
- just commitments, nullifiers, encrypted outputs, proof artifact

### 4.2 Quick anti-DoS checks (optional)
Sequencers/provers may do cheap checks:

- tx format valid
- size limits
- nullifier duplicates within the same batch (fast reject)  
    (Full correctness is enforced by the final batch proof + on-chain verification.)
