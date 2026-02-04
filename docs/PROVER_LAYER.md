# Zelana Prover Layer

This document describes how to build and run the Zelana prover layer, which generates ZK proofs for batch settlement on Solana.

## Architecture Overview

```
                    -----------------------------------------------
                    -           COORDINATOR (Brain)               -
                    -                                             -
   Batch ---------->-  1. Slice batch into chunks                 -
   (100 txs)        -  2. Compute intermediate state roots        -
                    -  3. Dispatch chunks to workers in parallel  -
                    -  4. Collect proofs                          -
                    -  5. Submit to Solana (batched)              -
                    -----------------------------------------------
                              -         -         -         -
                              ▼         ▼         ▼         ▼
                         ---------- ---------- ---------- ----------
                         -Worker 1- -Worker 2- -Worker 3- -Worker 4-
                         ---------- ---------- ---------- ----------
                              -         -         -         -
                              ▼         ▼         ▼         ▼
                         -------------------------------------------
                         -              SOLANA (Verifier)          -
                         -------------------------------------------
```

## Prerequisites

### 1. Install Noir Compiler (nargo)

```bash
# Install noirup
curl -L https://raw.githubusercontent.com/noir-lang/noirup/main/install | bash

# Install nargo (version 0.30.0+)
noirup --version 0.30.0
```

### 2. Install Sunspot (ZK Prover for Solana)

Sunspot generates Groth16 proofs and compiles Solana verifier programs.

```bash
# Follow Sunspot installation from their repository
# https://github.com/sunspot-dev/sunspot
```

## Circuit Structure

### Directory Layout

```
zelana-forge/circuits/
--- zelana_batch/        # Main batch validity circuit
-   --- Nargo.toml
-   --- src/
-   -   --- main.nr      # Circuit entry point
-   --- target/          # Build artifacts
--- zelana_lib/          # Shared cryptographic primitives
-   --- src/
-       --- poseidon.nr  # MiMC-based hash (hash_2, hash_3, hash_4)
-       --- merkle.nr    # 32-level Sparse Merkle Tree
-       --- nullifier.nr # Nullifier computation
-       --- account.nr   # Account state management
-       --- signature.nr # Schnorr signature verification
--- ownership/           # Client-side ownership proof (~500ms in WASM)
--- batch_processor/     # Legacy batch processor
```

### Main Batch Circuit

The `zelana_batch` circuit proves validity of batches containing:
- Up to 8 transfers
- Up to 4 withdrawals  
- Up to 4 shielded transactions

**Public Inputs (7 x 32 bytes = 224 bytes):**
1. `pre_state_root` - State root before batch
2. `post_state_root` - State root after batch
3. `pre_shielded_root` - Shielded Merkle root before batch
4. `post_shielded_root` - Shielded Merkle root after batch
5. `withdrawal_root` - Merkle root of withdrawals
6. `batch_hash` - Hash of all transactions in batch
7. `batch_id` - Sequential batch identifier

## Building the Circuits

### 1. Compile Circuit

```bash
cd zelana-forge/circuits/zelana_batch

# Compile to ACIR (Arithmetic Circuit Intermediate Representation)
nargo compile
```

This generates:
- `target/zelana_batch.json` (~15 MB) - Compiled ACIR

### 2. Generate Witness (Optional - for testing)

```bash
# Create Prover.toml with test inputs
# Then generate witness
nargo execute witness_name
```

This generates:
- `target/witness_name.gz` - Compressed witness file

### 3. Setup Proving/Verification Keys (One-time)

```bash
sunspot setup \
  target/zelana_batch.json \
  target/zelana_batch.ccs \
  target/zelana_batch.pk \
  target/zelana_batch.vk
```

This generates:
- `target/zelana_batch.ccs` (~59 MB) - Circuit Constraint System
- `target/zelana_batch.pk` (~885 MB) - Proving Key
- `target/zelana_batch.vk` (~1.4 KB) - Verification Key

### 4. Generate Proof

```bash
sunspot prove \
  target/zelana_batch.json \
  target/witness_name.gz \
  target/zelana_batch.ccs \
  target/zelana_batch.pk
```

This generates:
- `target/zelana_batch.proof` (388 bytes) - Groth16 proof
- `target/zelana_batch.pw` (236 bytes) - Public witness

### 5. Deploy Verifier to Solana (One-time)

```bash
sunspot deploy
```

This generates and deploys:
- `target/zelana_batch.so` (~201 KB) - Compiled Solana verifier program

## Running the Prover Coordinator

### Configuration

Set environment variables:

```bash
# Coordinator settings
export PORT=8080
export WORKERS=http://localhost:3001,http://localhost:3002
export CHUNK_SIZE=25

# Solana settings
export SOLANA_RPC=https://api.devnet.solana.com
export PROGRAM_ID=EZzyLrTrC4uyU488jVAs4GKeCR1s9GmoFggeiDqwDeNK

# Development mode
export MOCK_PROVER=false      # Use real prover
export MOCK_SETTLEMENT=false  # Submit to real Solana
```

### Start Prover Workers

```bash
# Terminal 1: Start worker on port 3001
cd zelana-forge
cargo run -p prover-worker -- --port 3001

# Terminal 2: Start worker on port 3002
cargo run -p prover-worker -- --port 3002
```

### Start Coordinator

```bash
cd zelana-forge
cargo run -p prover-coordinator
```

## API Endpoints

### Core API (v2) - Used by Sequencer

```
POST /v2/batch/prove
  Request: { batch_id, transactions, pre_state, ... }
  Response: { job_id }

GET /v2/batch/:job_id/status (SSE)
  Stream: status updates

GET /v2/batch/:job_id/proof
  Response: { proof_bytes, public_witness }

DELETE /v2/batch/:job_id
  Cancel job
```

### Parallel Swarm API

```
POST /batch/submit
  Submit batch for parallel proving

GET /batch/:id/status
  Check batch status

GET /workers
  List worker status
```

## Proof Sizes

| Component | Size |
|-----------|------|
| Groth16 Proof | 388 bytes |
| Public Witness | 236 bytes (4-byte header + 8-byte padding + 7 x 32-byte inputs) |
| Total Instruction Data | 624 bytes |

## Deployed Verifier

- **Program ID**: `EZzyLrTrC4uyU488jVAs4GKeCR1s9GmoFggeiDqwDeNK`
- **Network**: Solana Devnet
- **Type**: Sunspot-generated Groth16 verifier

## Testing Verification

```bash
# Run verification test script
./zelana-forge/scripts/test-verification.sh
```

## Development Mode

For local development without real proofs:

```bash
# Set mock mode
export MOCK_PROVER=true
export MOCK_SETTLEMENT=true

# Run sequencer with mock settlement
cargo run -p zelana-core
```

The sequencer will accept transactions with mock proofs (`[1, 2, 3, 4]`) and simulate settlement.

## Ownership Prover (Client-Side)

For Split Proving, clients generate lightweight ownership proofs in ~500ms using WASM:

```typescript
import { initOwnershipProver, computeNullifier, computeCommitment } from '@zelana/sdk';

// Initialize WASM prover
await initOwnershipProver();

// Compute nullifier
const nullifier = await computeNullifier(spendingKeyHex, commitmentHex, position);

// Compute commitment (MiMC)
const commitment = await computeCommitment(ownerPkHex, amount, blindingHex);
```

The ownership circuit (`circuits/ownership/`) proves:
1. User knows the spending key
2. Nullifier is correctly derived
3. Commitment is correctly computed

The Swarm then adds Merkle membership proof and generates the final batch proof.
