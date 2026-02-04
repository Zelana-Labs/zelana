# Forge Parallel Swarm (Docker)

This directory contains the Docker-based **parallel swarm** setup: a coordinator plus multiple prover workers running in parallel.

## What You Get

- 1 coordinator on `:8080`
- 4 workers on `:9001-:9004`
- Optional dashboard on `:3000`

The swarm uses `docker-compose.swarm.yml` and defaults to mock proving for fast local testing.

## Quick Start

From `zelana-forge/deploy/docker`:

```bash
docker compose -f docker-compose.swarm.yml up -d
```

Tail logs:

```bash
docker compose -f docker-compose.swarm.yml logs -f
```

Stop and remove containers:

```bash
docker compose -f docker-compose.swarm.yml down
```

## Switching Between Mock and Real Proving

The compose file sets `MOCK_PROVER=true` for each worker. To run real proving:

1. Set `MOCK_PROVER=false` for all workers.
2. Ensure the Noir toolchain and circuits are available in the image.
3. Rebuild and restart the swarm.

## Common Environment Settings

- Coordinator:
  - `WORKERS`: Comma-separated worker URLs.
  - `CHUNK_SIZE`: How many items per worker job.
  - `PROOF_TIMEOUT_MS`: Timeout for job aggregation.
- Worker:
  - `MAX_CONCURRENT_JOBS`: Parallel jobs per worker.
  - `MOCK_DELAY_MS`: Simulated proving delay when mocking.

## Health Check Flow

1. Start the swarm.
2. Verify coordinator logs show connected workers.
3. Submit a prove request to the coordinator and confirm worker jobs complete.
