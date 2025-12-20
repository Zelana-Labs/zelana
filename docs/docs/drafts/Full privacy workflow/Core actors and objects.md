## Actors
- **User wallet (client)**: executes privately + proves (with prover layer)
- **Relayer** (optional but recommended): hides IP/timing
- **Sequencer committee**: orders txs, commits ordering, threshold-decrypts
- **Prover layer**: build chunk proofs + aggregate proof
- **DA layer**: stores batch blobs (encrypted logs, commitments, nullifiers, etc.)
- **Solana program**: verifies final proof + updates roots (canonical truth)
- **Indexers** (optional): help wallets fetch/scan faster (untrusted)


## Core objects (what exists)
### Private tx object (opaque; contains no addresses/amounts)
`TxObj` includes:
- `proof_tx` (proof of correct private execution for this tx OR a proof-ready witness commitment)
- `nullifiers[]`
- `new_commitments[]`
- `encrypted_outputs[]` (ciphertexts for recipients)
- optional: `public_hooks` (bridge/fee hooks; minimize)
- `tx_commitment` (hash)
### Batch header (what committee signs)

`BatchHeader`:

- `old_state_root`
- `new_state_root`
- `new_nullifier_root`
- `batch_hash` (commitment to DA blob)
- `batch_index`