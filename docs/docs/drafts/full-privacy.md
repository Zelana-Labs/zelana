---
sidebar_position: 1
---
# Full privacy overview
## Goal

Private by default:

- Hide sender/receiver, amounts, transaction graph    
- Privacy **against the sequencer** (not just against L1 observers)
- Decentralized ordering + recoverability
- Hide smart-contract state + private execution
---

## Architecture in one picture

**Clients execute →
prover layer →
encrypted mempool →
decentralized sequencer ordering →
decrypt-after-ordering (obfuscated data) →
batch proof →
Solana verifies + stores roots (not only store roots in L1 also L2) →
users scan encrypted outputs**

Solana is the **finality + verifier + root anchor**, not the execution engine.
*(Note: we can still think about the where we want the finality at)*

---

## On-chain Solana program responsibilities

Keep it minimal (fast/cheap):

Store:
- `state_root` (Merkle root of all private state)
- `nullifier_root` (or accumulator commitment)
- `batch_index`
- `batch_commitment_hash` (commitment to DA data)

Verify on `SubmitBatch`:
1. `old_root == current_root`
2. **committee approval** (M-of-N sequencer signatures over batch header)
3. **ZK proof verifies** `old_root -> new_root` and correct nullifier update
4. update roots + batch commitment

That’s it. No plaintext state ever.

---

## Off-chain state model (what you actually prove)

Use **Zcash-style notes** (UTXO commitments) for best privacy and simplest circuits:

- **Commitments**: `C = Commit(note)` for new notes/state objects
    
- **Nullifiers**: `N = PRF(sk, note)` for spent notes (unlinkable)
    
- **Encrypted outputs**: ciphertexts addressed to receiver viewing keys
    
- **Private app state**: also committed into the same state tree (or separate subtree)
    

Global truth is only the root(s) on Solana. 
*(Note: maybe user can also just look at the sequencers state so we can have faster updates on the user side)*

---

## How users learn state updates (wallet logic)

Users do NOT get “pushed” balances.

They:

1. watch finalized batches (roots anchored on Solana)
2. fetch batch data from DA (or sequencer caches)
3. scan `encrypted_outputs[]`
4. decrypt what belongs to them (notes / state diffs)
5. mark spends by checking if _their_ nullifiers appear
6. maintain local wallet state (unspent notes + app state)

This keeps privacy and avoids storing global state.

---

## “Full privacy against the sequencer”
You need **encrypted mempool** + **decrypt-after-ordering**.
### Minimal viable (MVP)
- TX object is already “opaque”: `{proof, nullifiers, commitments, encrypted_outputs}`
- sequencer still sees those objects, but not addresses/amounts/state (because they’re not present)

### Real full privacy (target, decentralized order sequencing)
- Users submit **ciphertexts** to the mempool:
    - `cipher = Encrypt(PK_committee, tx_object)`
- Sequencers order ciphertexts **blindly**
- Order is committed (signed hash of ciphertext list)
- Only then the committee performs **threshold decryption** to reveal tx objects
- Prover builds batch + proof

This prevents:
- content-based censorship
- MEV/front-running on private actions
- early linkage via tx contents

You still add:
- **relayers + batching windows** to reduce IP/timing correlation.

---

## Decentralization plan (without drowning)
Don’t try to ship everything at once.
### Phase 1 (ship a working private rollup)
- Shielded notes + nullifiers + encrypted outputs
- Single sequencer, permissionless prover
- DA via committed hash + replicated storage
- Solana verifies batch proofs + updates roots
### Phase 2 (decentralize ordering)
- Sequencer committee (rotating leader)
- M-of-N signatures required to submit batch
- Decentralized prover layer
### Phase 3 (true “sequencer-blind” privacy)
- encrypted mempool with threshold encryption
- decrypt-after-ordering
- relayers + batching windows

---

## Data availability (DA) so users can always recover
Never rely on “sequencer database only.”

You need:
- a DA store for batch blobs (encrypted outputs + commitments + nullifiers + ordering)
- on Solana: store `batch_hash` + pointer/reference

For maximum safety: publish DA on Solana accounts (expensive).  
For maximum throughput: external DA + strong replication + hash anchored on Solana.

Sequencers are **caches/CDNs**, not the source of truth.

---

## Performance choices for “fastest”

- Batch aggressively (one proof per batch)
- Use a proof system with **cheap verification on Solana**
- Keep on-chain verification minimal (roots + verifier + committee signatures)
- Avoid on-chain storage of blobs unless needed; anchor hashes + pointers
---

## Build checklist (what to implement)

1. Note format + commitment scheme
2. Nullifier scheme + anti-double-spend logic
3. State tree + inclusion proofs
4. Client proving pipeline (private execution) or **prover  layer**
5. Batch prover (aggregates txs → new roots + batch proof)
6. Solana program: verify proof + update roots
7. DA publishing + batch hash anchoring
8. Wallet scanner (decrypt outputs, track nullifiers)
9. Committee sequencing (M-of-N approvals)
10. Encrypted mempool (threshold) + relayers + batching

---

## One-sentence summary

**Fastest full-privacy Solana rollup = client-side private execution (with prover layer) + ZK validity proof batches + Solana root verification + decentralized sequencer committee + encrypted mempool (decrypt-after-ordering) + DA anchored by batch commitments + wallets that scan/decrypt outputs and track nullifiers.**