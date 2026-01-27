# Zelana Forge

A distributed zero-knowledge proof system with **blind proving** - no single node knows the complete secret, and provers never see the public witness until verification.

## Overview

```
                     BLIND DISTRIBUTED PROVING

    ┌──────────┐     Commitment Only      ┌────────────────┐
    │  Client  │ ─────────────────────────│  Coordinator   │
    │          │   (witness hidden!)      │                │
    │ witness  │                          │  Orchestrates  │
    │  salt    │                          │  No secrets    │
    └──────────┘                          └───────┬────────┘
         │                                        │
         │                           ┌────────────┼────────────┐
         │                           │            │            │
         │                      ┌────▼────┐  ┌────▼────┐  ┌────▼────┐
         │                      │ Node 1  │  │ Node 2  │  │ Node N  │
         │                      │ Share 1 │  │ Share 2 │  │ Share N │
         │                      │   (s₁)  │  │   (s₂)  │  │  (sₙ)   │
         │                      └─────────┘  └─────────┘  └─────────┘
         │                           │            │            │
         │                           └────────────┼────────────┘
         │                                        │
         │  Reveal for Verification               │
         └────────────────────────────────────────┘

    Privacy: Provers NEVER see the public witness - only commitment hash
    Security: Any t nodes can prove, but t-1 nodes learn NOTHING
```

## Features

- **Blind Proving**: Public witness hidden from provers until verification
- **Threshold Security**: t-of-n nodes required, t-1 nodes learn nothing
- **Pluggable Circuits**: Easy to add new ZK circuits
- **Interactive Dashboard**: Real-time cluster management and visualization
- **Production Ready**: Docker and Kubernetes deployment

## Quick Start

### Interactive Dashboard

```bash
# Launch dashboard + control server
./scripts/start-dashboard.sh

# Open browser: http://localhost:5173

# Click "Start Cluster" then run the 3-step workflow:
# Setup → Prove → Verify
```

### Docker Compose

```bash
cd deploy/docker
docker compose up --build

# Test with the dashboard or scripts
./scripts/test-local.sh
```

### Kubernetes Deployment

```bash
# Deploy to Kubernetes with dashboard
./scripts/deploy-k8s.sh --with-dashboard

# Port forward coordinator API
kubectl port-forward -n zelana-prover svc/coordinator 8080:8080

# Test API
curl http://localhost:8080/health

# Access Kubernetes dashboard
kubectl proxy
# Visit: http://localhost:8001/api/v1/namespaces/kubernetes-dashboard/services/https:kubernetes-dashboard:/proxy/
# Token: kubectl -n kubernetes-dashboard create token admin-user
```

## Available Circuits

| Circuit | Status | Statement |
|---------|--------|-----------|
| **Schnorr Signature** | Active | I know secret `s` such that `PK = g^s` |
| **Hash Preimage** | Active | I know preimage such that `H(preimage) = target` |
| Range Proof | Coming | My value is in range `[min, max]` |
| Merkle Membership | Coming | This leaf is in the Merkle tree |

## Performance

**Benchmark Results (Kubernetes, 5 nodes, 3-of-5 threshold):**
- **Average Proof Time**: 23ms
- **Speedup**: 5x parallel processing
- **Target**: <50ms end-to-end
- **Throughput**: ~43 proofs/second

**Tested Circuits:**
- Schnorr signatures: 19-31ms per proof
- Hash preimage proofs: Supported

### Adding New Circuits

1. Create handler: `dashboard/app/circuits/my-circuit.ts`
2. Register: Add to `dashboard/app/circuits/index.ts`
3. Backend: Add to `CircuitType` enum in `crates/prover-network/src/messages.rs`
4. Verify: Implement in `crates/prover-coordinator/src/main.rs`

## Project Structure

```
zelana-forge/
├── crates/
│   ├── prover-core/          # Core crypto (Shamir, Schnorr)
│   ├── prover-network/       # Message types & serialization
│   ├── prover-node/          # Prover node HTTP server
│   ├── prover-coordinator/   # Orchestration server
│   └── prover-control/       # Docker cluster control
│
├── dashboard/                # Next.js interactive dashboard
│   ├── app/
│   │   ├── components/       # UI components
│   │   ├── circuits/         # Circuit configuration system
│   │   └── utils/            # Client-side crypto
│   └── package.json
│
├── deploy/
│   ├── docker/               # Docker Compose setup
│   └── k8s/                  # Kubernetes manifests
│
├── scripts/                  # Automation scripts
└── docs/                     # Technical documentation
```

## Ports

| Service | Port (Docker) | Port (K8s) | URL |
|---------|---------------|------------|-----|
| Dashboard | 5173 | - | http://localhost:5173 |
| Control Server | 9000 | - | http://localhost:9000 |
| Coordinator | 8000 | 8080 | http://localhost:8080 |
| Nodes | 3001-3005 | 3000 | http://localhost:3000 |
| K8s Dashboard | - | 30080 | http://localhost:30080 |

## Security Properties

| Property | Description |
|----------|-------------|
| **Blind Proving** | Provers see commitment hash, not public witness |
| **Threshold Security** | Any t of n nodes can prove |
| **Information-Theoretic** | t-1 nodes learn nothing about secret |
| **Zero-Knowledge** | Verifier learns only proof validity |

## Protocol Flow

```
Phase 0: COMMIT       Phase 1: SHARE        Phase 2: PROVE        Phase 3: VERIFY
────────────────      ────────────────      ────────────────      ────────────────

Client:               Coordinator:          Nodes:                Client reveals:
Com = H(witness||    Split secret s →      Generate C_i, z_i     witness, salt
      salt)          Distribute shares     (blind to witness)
                                           ↓
Send Com only        Send (share_i, Com)   Aggregate via         Verify:
(witness hidden!)    to each node          Lagrange              H(w||s) == Com
                                                                 g^z == C · PK^c
```

## Documentation

- [Architecture](docs/ARCHITECTURE.md) - System design
- [Protocol](docs/PROTOCOL.md) - Cryptographic specification
- [State Machines](docs/STATE_MACHINES.md) - Component states
- [Diagrams](docs/DIAGRAMS.md) - Visual protocol flow
- [Privacy Design](docs/PRIVACY_DESIGN.md) - Blind proving details

## Development

```bash
# Build all crates
cargo build --workspace

# Run tests
cargo test --workspace

# Run with logging
RUST_LOG=debug cargo run -p prover-coordinator
```

## License

MIT OR Apache-2.0
