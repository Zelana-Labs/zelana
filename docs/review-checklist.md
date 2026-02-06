# Review Checklist

This checklist is for a deep review of the Zelana codebase. It is meant to be used with `.codebase-index/*` and the workspace manifests so that every crate and subsystem is covered.

## Scope and Inventory

- [ ] Confirm workspace members and excluded paths in `Cargo.toml` match the intended review scope.
- [ ] Enumerate crates from `.codebase-index/overview.md` and map to folder paths.
- [ ] Identify non-workspace components that still impact runtime or security: `forge/`, `onchain-programs/`, `rpc/`, `udp-client/`, `zelana-db/`.
- [ ] Identify SDKs and client libraries in `sdk/` that must stay compatible with core APIs.
- [ ] Verify that build scripts or generators in `scripts/` are reviewed for safety and correctness.

## Architecture and Data Flow

- [ ] Map the end-to-end data flow: client submission -> sequencer -> prover -> settlement -> on-chain verification.
- [ ] Identify all trust boundaries and explicitly list which components are assumed honest.
- [ ] Validate that the architecture diagrams and docs reflect the actual code paths.
- [ ] Check that cross-crate boundaries use stable, versioned interfaces.
- [ ] Confirm that critical flows do not bypass validation layers.

## API Surface and Contracts

- [ ] Inventory public APIs using `.codebase-index/api-surface.md` and confirm handler coverage.
- [ ] Validate input schemas and reject malformed or oversized inputs.
- [ ] Verify pagination, filtering, and error handling behavior for list endpoints.
- [ ] Confirm all APIs have deterministic output for identical inputs.
- [ ] Check compatibility guarantees for SDK consumers.

## Cryptography and Security

- [ ] Identify every cryptographic primitive and its implementation location.
- [ ] Validate domain separation is consistently applied across Rust and circuit code.
- [ ] Confirm key derivation, storage, and usage are safe and minimal.
- [ ] Check for constant-time operations where needed.
- [ ] Verify signature verification, nullifier checks, and commitment logic against spec.

## Circuit and Prover Correctness

- [ ] Map every circuit to its Rust witness builder and input normalization logic.
- [ ] Verify public inputs are computed exactly the same in Rust and Noir.
- [ ] Check for mismatched hash functions or parameterization between layers.
- [ ] Validate Merkle path verification and root updates are consistent with on-chain expectations.
- [ ] Confirm that proof verification and VK handling is stable and versioned.

## State Transitions and Integrity

- [ ] Confirm all state transitions are explicit and validated.
- [ ] Ensure account balances cannot go negative or overflow.
- [ ] Validate nonce handling and replay protection.
- [ ] Check batch execution logic for determinism and ordering guarantees.
- [ ] Verify withdrawal flow correctness and state persistence.

## Storage and Database Safety

- [ ] Inspect RocksDB key schema and column family usage.
- [ ] Check for atomicity in multi-step writes.
- [ ] Confirm recovery logic for interrupted writes or crashes.
- [ ] Validate any caching layer consistency and invalidation.
- [ ] Check database migrations and versioning practices.

## Networking and Transport

- [ ] Verify UDP server behavior for packet limits and malformed packets.
- [ ] Check retry strategies and timeouts for RPC or external services.
- [ ] Confirm that network serialization formats are stable and documented.
- [ ] Ensure no sensitive data is transmitted over untrusted channels without encryption.

## Concurrency and Fault Handling

- [ ] Review shared-state usage and locking for races or deadlocks.
- [ ] Confirm task supervision and cancellation policies.
- [ ] Validate retry loops and backoff settings.
- [ ] Ensure partial failures do not corrupt state.
- [ ] Verify that degraded modes are safe and explicit.

## Performance and Scalability

- [ ] Profile key hot paths: mempool, batch builder, prover pipeline.
- [ ] Check for unbounded growth in queues, caches, or logs.
- [ ] Validate that expensive crypto ops are bounded and cached when safe.
- [ ] Review worker parallelism and coordinator bottlenecks.
- [ ] Confirm any benchmarks are reproducible and up to date.

## Observability and Ops

- [ ] Confirm structured logging with consistent event keys.
- [ ] Identify missing metrics for critical pipelines.
- [ ] Validate health checks and readiness probes.
- [ ] Check configuration validation and safe defaults.
- [ ] Confirm secrets are not logged and are loaded from secure locations.

## Deployment and Infrastructure

- [ ] Verify Docker, compose, and K8s configs align with actual ports and env vars.
- [ ] Check for version drift between deploy configs and code requirements.
- [ ] Ensure required migrations or setup steps are documented.
- [ ] Validate that dev, staging, and prod modes are clearly separated.

## Tests and Verification

- [ ] Ensure unit tests cover cryptographic primitives.
- [ ] Ensure integration tests cover full proving and settlement paths.
- [ ] Add regression tests for any previously found bugs.
- [ ] Validate e2e test environment setup is deterministic.
- [ ] Check fuzzing or property tests for parsers and critical logic.

## Documentation and Spec Alignment

- [ ] Confirm READMEs reflect actual usage and CLI flags.
- [ ] Check that protocol assumptions are explicitly documented.
- [ ] Validate that any external spec references match implementation.

## Review Deliverables

- [ ] Findings log with severity and affected modules.
- [ ] Risk register with mitigation plan.
- [ ] Test gap list with priority ranking.
- [ ] Follow-up issues for refactors or tech debt.
