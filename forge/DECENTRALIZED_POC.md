# Zelana Forge Decentralized Committee POC

## Overview

This document outlines a Proof of Concept (POC) implementation for transitioning Zelana Forge from a centralized coordinator model to a decentralized committee-based architecture. The goal is to enable scalable, secure distributed ZK proof generation with hundreds of nodes while maintaining sub-100ms performance.

## Current Architecture (Baseline)

```
-------------------    ------------------------    -------------------
-   Mobile Client ----▶-   Coordinator        ----▶-   Prover Nodes  -
-                 -    -   • Single API       -    -   • 5 Nodes      -
- • Witness Data  -    -   • Session Mgmt     -    -   • Shamir Shares-
- • Proof Request -    -   • Proof Assembly   -    -   • Threshold: 3 -
-   (<50ms)       -    -   • 5x Speedup       -    -                 -
-------------------    ------------------------    -------------------
```

**Performance:** 25-45ms proofs, 5x speedup
**Security:** 3-of-5 threshold cryptography
**Scalability:** Limited to ~10 nodes before coordination overhead

## Target Decentralized Architecture

```
-------------------    ------------------------    -------------------
-   Mobile Client ----▶-   Network Registry   ----▶-   Node Network  -
-                 -    -   • Committee        -    -   • 100+ Nodes   -
- • Witness Data  -    -   • Selection        -    -   • Geographic   -
- • Proof Request -    -   • Coordination     -    -   • Stake-based  -
-   (<200ms)      -    -   • Validation       -    -   • Committees   -
-------------------    ------------------------    -------------------
                             -
                             ▼
                    -------------------
                    -   Proof         -
                    -   Committees    -
                    -   • 7-21 Nodes  -
                    -   • 3-of-7      -
                    -   • Parallel    -
                    -   • <50ms       -
                    -------------------
```

## Core Components

### 1. Network Registry
**Purpose:** Decentralized node discovery and committee formation

**Responsibilities:**
- Node registration and reputation tracking
- Committee selection algorithms
- Stake-based weighting (proof-of-stake)
- Geographic distribution optimization
- Health monitoring and uptime tracking

**Data Structures:**
```rust
struct NetworkRegistry {
    nodes: HashMap<NodeId, NodeInfo>,
    committees: HashMap<CommitteeId, Committee>,
    stake_weights: HashMap<NodeId, u64>,
    geographic_zones: HashMap<ZoneId, Vec<NodeId>>,
}

struct NodeInfo {
    id: NodeId,
    public_key: PublicKey,
    endpoints: Vec<String>,
    stake_amount: u64,
    reputation_score: f64,
    geographic_zone: ZoneId,
    last_seen: Timestamp,
    capabilities: NodeCapabilities,
}
```

### 2. Committee Selection Engine
**Purpose:** Dynamic committee formation for each proof request

**Algorithms:**
1. **Random Selection:** Simple random sampling
2. **Stake-Weighted:** Higher stake = higher selection probability
3. **Geographic Distribution:** Ensure global distribution
4. **Reputation-Based:** Prefer high-uptime, high-reputation nodes

**Selection Process:**
```rust
fn select_committee(
    registry: &NetworkRegistry,
    committee_size: usize,
    client_location: Option<GeographicZone>
) -> Result<Committee, Error> {
    // 1. Filter healthy nodes
    let candidates = registry.get_healthy_nodes();

    // 2. Apply geographic constraints
    let distributed = apply_geographic_distribution(candidates, client_location);

    // 3. Weighted random selection
    let selected = stake_weighted_selection(distributed, committee_size);

    // 4. Form committee with threshold
    let threshold = calculate_threshold(committee_size);
    Ok(Committee::new(selected, threshold))
}
```

### 3. Committee Coordinator
**Purpose:** Temporary coordinator role within each committee

**Responsibilities:**
- Secret sharing distribution
- Proof fragment collection
- Result aggregation and validation
- Committee-internal consensus

**Lifecycle:**
1. Committee elected for proof request
2. One node becomes temporary coordinator
3. Coordinator manages proof generation
4. Committee dissolves after completion

### 4. Node Reputation System
**Purpose:** Ensure committee quality and prevent sybil attacks

**Metrics:**
- Uptime percentage
- Proof contribution success rate
- Response time consistency
- Stake amount and duration
- Geographic distribution compliance

**Scoring Formula:**
```
reputation = (uptime * 0.3) + (success_rate * 0.4) + (stake_weight * 0.2) + (geo_diversity * 0.1)
```

## Implementation Phases

### Phase 1: Core Committee Infrastructure (4 weeks)

#### 1.1 Network Registry Implementation
```rust
// New crate: prover-network-registry
mod network_registry;
mod committee_selection;
mod reputation_system;
```

**Key Components:**
- Node registration API
- Heartbeat monitoring
- Basic random committee selection
- SQLite/PostgreSQL persistence

#### 1.2 Committee Data Structures
```rust
struct Committee {
    id: CommitteeId,
    members: Vec<NodeId>,
    threshold: usize,
    coordinator: NodeId,
    created_at: Timestamp,
    expires_at: Timestamp,
}

struct CommitteeAssignment {
    committee_id: CommitteeId,
    node_id: NodeId,
    role: CommitteeRole, // Coordinator, Member
    share_index: usize,
}
```

#### 1.3 API Extensions
```rust
// New endpoints
POST /register-node    // Node joins network
POST /request-committee // Client requests committee
GET  /committee/{id}   // Get committee details
POST /submit-proof     // Committee submits completed proof
```

### Phase 2: Committee Selection Algorithms (3 weeks)

#### 2.1 Stake-Based Selection
```rust
impl CommitteeSelector for StakeWeightedSelector {
    fn select(&self, candidates: &[NodeId], size: usize) -> Vec<NodeId> {
        let total_stake: u64 = candidates.iter()
            .map(|id| self.registry.get_stake(id))
            .sum();

        let mut selected = Vec::new();
        let mut rng = thread_rng();

        while selected.len() < size {
            let target = rng.gen_range(0..total_stake);
            let mut cumulative = 0u64;

            for &node_id in candidates {
                cumulative += self.registry.get_stake(&node_id);
                if cumulative > target {
                    if !selected.contains(&node_id) {
                        selected.push(node_id);
                    }
                    break;
                }
            }
        }

        selected
    }
}
```

#### 2.2 Geographic Distribution
```rust
fn apply_geographic_distribution(
    candidates: Vec<NodeId>,
    client_zone: Option<ZoneId>,
    target_zones: usize
) -> Vec<NodeId> {
    let mut by_zone = HashMap::new();

    for node_id in candidates {
        let zone = self.registry.get_zone(&node_id);
        by_zone.entry(zone).or_insert(Vec::new()).push(node_id);
    }

    // Ensure representation from target_zones
    let mut selected = Vec::new();
    let zones: Vec<_> = by_zone.keys().collect();

    for zone in zones.iter().take(target_zones) {
        if let Some(nodes) = by_zone.get(zone) {
            selected.extend(nodes.iter().take(2)); // 2 nodes per zone
        }
    }

    selected
}
```

### Phase 3: Committee Protocol (4 weeks)

#### 3.1 Committee Formation Protocol
```
1. Client → Registry: Request committee
2. Registry → Committee: Form committee (7 nodes)
3. Registry → Client: Committee details + coordinator
4. Client → Coordinator: Proof request
5. Coordinator → Committee: Distribute shares
6. Committee → Coordinator: Generate fragments
7. Coordinator → Client: Aggregate proof
8. Registry: Update reputations
```

#### 3.2 Committee Communication
- **Intra-committee:** gRPC/WebSocket for low-latency communication
- **Coordinator election:** Temporary leader election within committee
- **Failure handling:** Automatic member replacement from registry

#### 3.3 Proof Generation Flow
```rust
async fn generate_distributed_proof(
    committee: &Committee,
    witness_commitment: &WitnessCommitment,
    circuit_type: CircuitType
) -> Result<BlindProof, Error> {

    // 1. Coordinator generates secret and shares
    let (secret, shares) = generate_shares(committee.members.len(), committee.threshold);

    // 2. Parallel share distribution
    let share_futures = committee.members.iter().enumerate().map(|(i, node_id)| {
        distribute_share(node_id, shares[i].clone())
    });
    try_join_all(share_futures).await?;

    // 3. Parallel commitment collection
    let commitment_futures = committee.members.iter().map(|node_id| {
        request_commitment(node_id, witness_commitment.clone())
    });
    let commitments = try_join_all(commitment_futures).await?;

    // 4. Challenge computation
    let challenge = compute_challenge(&commitments, witness_commitment);

    // 5. Parallel fragment collection
    let fragment_futures = committee.members.iter().enumerate().map(|(i, node_id)| {
        request_fragment(node_id, challenge, shares[i].clone())
    });
    let fragments = try_join_all(fragment_futures).await?;

    // 6. Result aggregation
    let proof = aggregate_fragments(&fragments, committee.threshold);

    Ok(proof)
}
```

### Phase 4: Security & Performance Optimization (3 weeks)

#### 4.1 Security Enhancements
- **Committee validation:** Cryptographic proof of committee formation
- **Share verification:** Zero-knowledge proofs of correct share handling
- **Reputation slashing:** Penalize misbehaving committee members
- **Sybil resistance:** Stake-based selection prevents fake nodes

#### 4.2 Performance Optimizations
- **Connection pooling:** Persistent connections within committees
- **Geographic routing:** Minimize network latency
- **Caching:** Reuse committees for related proof requests
- **Parallel aggregation:** Optimize Lagrange interpolation

## API Evolution

### Current API (Centralized)
```http
POST /prove_single
{
  "circuit_type": "schnorr",
  "witness_commitment": {...},
  "secret": "0x..."
}
```

### Decentralized API (Committee-Based)
```http
// Phase 1: Request committee
POST /request-committee
{
  "circuit_type": "schnorr",
  "estimated_complexity": "medium",
  "preferred_regions": ["us-east", "eu-west"]
}
Response: {"committee_id": "...", "coordinator_url": "..."}

// Phase 2: Generate proof
POST /prove
{
  "committee_id": "...",
  "witness_commitment": {...},
  "secret": "0x..."
}
```

## Security Analysis

### Threat Model Evolution

| Threat | Centralized | Decentralized | Mitigation |
|--------|-------------|----------------|------------|
| **Single Point Failure** | Coordinator | Committee election | Stake-based selection |
| **Sybil Attacks** | N/A | Fake nodes | Reputation + stake requirements |
| **Collusion** | 3-of-5 nodes | 3-of-7 committee | Larger network, dynamic committees |
| **Network Partition** | Full outage | Partial degradation | Geographic distribution |
| **DDoS** | Coordinator target | Distributed targets | Global node distribution |

### Cryptographic Security
- **Same threshold guarantees:** 3-of-7 provides better fault tolerance
- **Enhanced privacy:** Committee dissolution prevents correlation
- **Forward security:** Ephemeral committees per proof request

## Performance Projections

### Latency Breakdown
```
Committee Selection:    100-300ms (network consensus)
Share Distribution:      50-150ms (parallel to committee)
Proof Generation:        25-45ms (committee computation)
Result Aggregation:      10-50ms (coordinator overhead)
Total:                  185-545ms per proof
```

### Scalability Metrics
```
Network Size:     100 nodes → 1000 nodes → 10000 nodes
Committee Size:   7 nodes  → 13 nodes   → 21 nodes
Threshold:        3-of-7   → 7-of-13    → 11-of-21
Fault Tolerance:  57%      → 46%        → 48%
Proof Latency:    ~200ms   → ~300ms     → ~400ms
Throughput:       1000/s   → 5000/s     → 20000/s
```

### Performance Optimizations
- **Committee caching:** Reuse committees for 5-10 proof requests
- **Geographic optimization:** Select committee near client
- **Parallel committees:** Multiple proofs simultaneously
- **Hardware acceleration:** GPU-accelerated crypto operations

## Migration Strategy

### Gradual Rollout
1. **Phase 1:** Deploy network registry alongside existing coordinator
2. **Phase 2:** Enable committee selection for 10% of requests
3. **Phase 3:** Migrate 50% of traffic to committee system
4. **Phase 4:** Full decentralization, remove centralized coordinator

### Backward Compatibility
- Existing API remains functional during transition
- Gradual client migration to committee-based requests
- Rollback capability if issues arise

## Success Metrics

### Performance Targets
- **Latency:** <300ms end-to-end for 100-node network
- **Throughput:** 1000+ proofs/second with parallel committees
- **Uptime:** 99.9% availability with decentralized operation
- **Security:** Zero successful collusion attacks

### Decentralization Metrics
- **Network size:** 100+ active nodes
- **Geographic distribution:** 10+ regions represented
- **Committee diversity:** No single entity controls >10% of stake
- **Response time:** <500ms committee formation

## Implementation Timeline

### Month 1: Foundation
- [ ] Network registry implementation
- [ ] Basic committee selection
- [ ] Node reputation system
- [ ] API extensions

### Month 2: Committee Protocol
- [ ] Committee formation protocol
- [ ] Intra-committee communication
- [ ] Proof generation flow
- [ ] Error handling and recovery

### Month 3: Security & Optimization
- [ ] Stake-based selection
- [ ] Geographic distribution
- [ ] Security audits
- [ ] Performance benchmarking

### Month 4: Production Deployment
- [ ] Network bootstrapping
- [ ] Client SDK updates
- [ ] Monitoring and alerting
- [ ] Gradual traffic migration

## Risk Mitigation

### Technical Risks
- **Network partition:** Geographic distribution + backup coordinators
- **Committee failure:** Automatic reformation from larger network
- **Performance regression:** Performance budgets + rollback capability
- **Security vulnerabilities:** Formal verification + third-party audits

### Operational Risks
- **Low network participation:** Incentivize node operators
- **Stake concentration:** Maximum stake limits per entity
- **Client migration:** Backward compatibility + gradual rollout

This POC provides a clear roadmap for transforming Zelana Forge into a truly decentralized, scalable ZK proof network while maintaining the performance characteristics that make it suitable for mobile applications.</content>
<parameter name="filePath">DECENTRALIZED_POC.md