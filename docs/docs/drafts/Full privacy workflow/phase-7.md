## Phase 7 — Settlement on Solana (canonical finality)

### 7.1 SubmitBatch instruction
Submit to Solana program:
- `old_state_root`
- `new_state_root`
- `new_nullifier_root`
- `batch_hash`
- `proof_batch`
- committee signatures on `BatchHeader`

### 7.2 Solana program verifies
On-chain:
1. old_root matches current
2. committee approval (M-of-N) is valid
3. verify `proof_batch`
4. update stored roots + batch index + last batch_hash

If this succeeds:
- the batch is final and canonical
- there is **no revert model** (validity rollup)
