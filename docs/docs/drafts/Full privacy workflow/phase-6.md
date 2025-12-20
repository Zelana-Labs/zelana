## Phase 6 — Data availability publishing

### 6.1 Construct the DA blob
`DA_blob` contains what anyone needs to reconstruct:
- ordered list of `nullifiers`
- ordered list of `new_commitments`
- all `encrypted_outputs`
- ordering commitment / signatures
- (optional) per-tx commitments
- (optional) chunk proof commitments

### 6.2 Publish DA blob

Publish `DA_blob` to
- Solana accounts (strongest, expensive) OR
- external DA / replicated storage (cheaper)

Compute:
- `batch_hash = H(DA_blob)`
