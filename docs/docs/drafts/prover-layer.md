---
sidebar_position: 3
---
# Prover Layer

## Purpose (what the prover layer actually is)

The **prover layer is an untrusted, permissionless compute service** that:

- accelerates ZK proof generation
- aggregates proofs for batching
- acts as a relayer + RPC/indexer
- **never owns secrets**
- **never executes private logic**
- **never decides state**

> The prover computes math.  
> The wallet owns secrets.  
> Solana decides truth.
---
## Core invariants (non-negotiable)
The prover layer:
- ❌ does NOT know private inputs (amounts, note secrets, keys)
- ❌ does NOT choose which notes are spent
- ❌ does NOT modify tx semantics
- ❌ does NOT submit invalid state (proofs are verified on-chain)

Everything it produces is **cryptographically checked** later.

---
## Inputs and outputs (strictly defined)
### Inputs the prover MAY receive
Depending on design:
1. **Proof-ready witness**
    - circuit witnesses
    - Merkle paths
    - note randomness
    - already bound to commitments/nullifiers
2. **Public artifacts**
    - commitments
    - nullifiers
    - tx ordering
    - previous state root

> These inputs are _mathematically opaque_ without private keys.
---
### Inputs the prover MUST NOT receive
- private keys
- plaintext amounts
- receiver identities
- wallet seed material
- decrypted outputs
---
### Outputs the prover produces
- `proof_tx` (per-tx proof) **or**
- `proof_chunk` (chunk-level proof)
- `proof_batch` (aggregated proof)
- optional: proof metadata (hashes, sizes)

All outputs are:
- deterministic
- verifiable
- useless without verification

---
## Full prover-layer workflow
### Phase 1 — Wallet prepares proof input (client-side)
1. Wallet fetches latest canonical roots
2. Wallet selects unspent notes
3. Wallet executes private logic locally
4. Wallet computes:
    - nullifiers
    - new commitments
    - encrypted outputs
5. Wallet builds **proof witness**
6. Wallet sends **proof input** to prover

At this point:
- execution is DONE
- prover cannot change semantics

---

### Phase 2 — Prover generates proof (off-chain)
1. Prover loads circuit
2. Prover injects witness
3. Prover runs ZK proving algorithm
4. Prover outputs:
    - `proof_tx` (or chunk proof)

This step can be:
- GPU accelerated
- parallelized
- hardware-optimized

Privacy is preserved because:
- witness is already commitment-bound
- no secrets can be derived
---
### Phase 3 — Proof return / relay
Two valid patterns:
#### Pattern A — Proof returned to wallet
- Prover → wallet: `proof_tx`
- Wallet submits tx to relayer/sequencer
#### Pattern B — Prover also acts as relayer
- Prover wraps:
    ```
    TxObject = {
      proof_tx,
      nullifiers,
      commitments,
      encrypted_outputs
    }
    ```
    
- Prover encrypts TxObject to sequencer committee
- Forwards to encrypted mempool

Both are valid.  
Pattern B gives better UX.

---

## Batch proving & aggregation (TPS accelerator)

### Chunk proving (parallel)
- Sequencer orders txs
- Prover splits txs into chunks
- Each chunk proven independently:
    ```
    proof_chunk_i
    ```
### Recursive aggregation
- Aggregator prover combines chunk proofs
- Produces:
    ```
    proof_batch
    ```
- `proof_batch` attests:
    - correct tx ordering
    - correct state transitions
    - correct nullifier updates

Only **one proof** goes to Solana.

---
## Prover as RPC / indexer (UX layer)
The prover layer can ALSO expose:
### Read APIs
- latest state root
- batch headers
- DA blob retrieval
- Merkle path queries
- encrypted output streams

### Important rule

> All data served by the prover must be verifiable against Solana anchors.

If it lies:
- wallets detect mismatch
- switch providers
- no trust loss

---
## Prover + encrypted mempool integration
If using encrypted mempool:
1. Wallet → prover:
    - encrypted TxObj
2. Prover:
    - forwards ciphertext
    - cannot decrypt
3. Sequencer:
    - orders ciphertexts
4. Committee decrypts after ordering
5. Prover receives ordered plaintext TxObjs
6. Prover builds batch proof

The prover never sees tx contents **before ordering**.

---
## Failure modes (safe by design)
### Prover malicious?
- Returns invalid proof → rejected on Solana
- Withholds proof → user switches prover
### Prover offline?
- User uses another prover
- No funds trapped
### Prover colludes with sequencer?
- Proof still enforces correctness
- Encrypted mempool prevents pre-order leakage

---

## Minimal prover API (example)

```
POST /prove/tx
  → input: proof_witness
  → output: proof_tx

POST /prove/chunk
  → input: [proof_tx...]
  → output: proof_chunk

POST /prove/batch
  → input: [proof_chunk...]
  → output: proof_batch

GET /state/root
GET /batch/:id
GET /da/:hash
```

Everything returned is verifiable.

---
## Final mental model (this is the one to keep)
> **The prover is a stateless math worker.  
> It accelerates proofs, not authority.  
> If it lies, the chain rejects it.**

---
## One-sentence summary
**The prover layer is a permissionless, untrusted compute + relay + RPC layer that accelerates proof generation and batching while wallets keep secrets, sequencers order blindly, and Solana verifies correctness.**

If you want next, I can:
- design the **exact Rust structs** passed wallet ↔ prover
- help you decide **what to prove per-tx vs per-batch**
- or map this prover layer onto your **Solana localnet setup**