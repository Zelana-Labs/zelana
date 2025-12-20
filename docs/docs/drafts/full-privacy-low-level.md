---
sidebar_position: 2
---
# Low level workflow overview

## Actors
- **User wallet (client)**: executes privately + proves (with prover layer)
- **Relayer** (optional but recommended): hides IP/timing
- **Sequencer committee**: orders txs, commits ordering, threshold-decrypts
- **Prover layer**: build chunk proofs + aggregate proof
- **DA layer**: stores batch blobs (encrypted logs, commitments, nullifiers, etc.)
- **Solana program**: verifies final proof + updates roots (canonical truth)
- **Indexers** (optional): help wallets fetch/scan faster (untrusted)
---
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
---
## Phase 0 — Setup (once)
1. **Key systems**
	- Wallet has:
	    - spending key
	    - viewing key (for decrypting outputs)
	- Sequencer committee has:
	    - threshold encryption key shares (for mempool encryption)
	    - signing keys (for batch approvals)

1. **Rollup state on Solana**
	- program stores:
	    - `state_root`
	    - `nullifier_root`
	    - `batch_index`

---

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

---
## Phase 2 — Encrypted mempool submission (privacy vs sequencer)
### 2.1 Encrypt TxObject to committee
Wallet encrypts TxObject:
- `cipher = Encrypt(PK_committee, TxObj)`
- produces `cipher_commitment = Hash(cipher)`
### 2.2 Send to relayer (recommended)
Wallet → Relayer:
- sends `cipher` (and maybe fee token for relayer)

Relayer → committee mempool:

- forwards `cipher` to multiple sequencers    

**Sequencers see only ciphertexts at this stage.**

---

## Phase 3 — Decentralized sequencing (order without seeing contents)

### 3.1 Collect ciphertexts during a window
Committee collects ciphertexts for time window `W`.

### 3.2 Order ciphertexts blindly
Leader proposes an ordering of `cipher_commitment[]`.  
Committee members sign:
- `OrderCommit = Hash(ordered_cipher_commitments || window_id)`

Once threshold signatures gathered:
- ordering is fixed (can’t reorder based on contents)

---

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

---
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

---

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

---
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

---

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

---

## Failure modes (what happens if something goes wrong)
### Sequencer lies about data
- wallet detects hash mismatch vs Solana `batch_hash`
- fetch from another source
### Sequencer withholds data
- use another sequencer/cache/DA replica
- if DA is robust, withholding can’t trap users
### Malicious state transition attempt
- proof fails on Solana
- batch not accepted
- nothing reverts (it never finalized)

---
## Minimal “MVP” simplifications 
- Start with **no encrypted mempool**, just relayers + opaque TxObject (still private contents)
- Start with **single sequencer**
- Keep DA in one replicated store + anchor hash on Solana  
    Then iterate toward:
	- committee sequencing
	- threshold encryption
	- recursive proving