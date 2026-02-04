#!/bin/bash
# Zelana Prover Workers Startup Script
# 
# Starts 4 prover workers on ports 9001-9004.
# Each worker can generate ZK proofs for batch transactions.
#
# Environment Variables:
#   MOCK_PROVER    - Set to "true" for mock proofs (fast), "false" for real proofs (default: false)
#   NUM_WORKERS    - Number of workers to start (default: 4)
#   CIRCUIT_PATH   - Path to circuit directory (default: circuits/zelana_batch)
#
# Usage:
#   ./start-prover-workers.sh              # Start 4 workers with real prover
#   MOCK_PROVER=true ./start-prover-workers.sh  # Start 4 workers with mock prover
#   NUM_WORKERS=2 ./start-prover-workers.sh     # Start only 2 workers

set -e

# Colors for output
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo -e "${GREEN}========================================${NC}"
echo -e "${GREEN}    Zelana Prover Workers             ${NC}"
echo -e "${GREEN}========================================${NC}"

cd "$(dirname "$0")/.."

# ============================================================================
# Configuration
# ============================================================================

# Number of workers (default: 4)
NUM_WORKERS=${NUM_WORKERS:-4}

# Base port for workers (workers will use 9001, 9002, 9003, 9004)
BASE_PORT=${BASE_PORT:-9001}

# Mock prover setting (default: false = real prover)
MOCK_PROVER=${MOCK_PROVER:-false}

# Mock delay in ms (only used if MOCK_PROVER=true)
MOCK_DELAY_MS=${MOCK_DELAY_MS:-500}

# Circuit path (relative to zelana-forge directory)
CIRCUIT_PATH=${CIRCUIT_PATH:-circuits/zelana_batch}

# Max concurrent jobs per worker (increase for faster throughput)
MAX_CONCURRENT_JOBS=${MAX_CONCURRENT_JOBS:-4}

# ============================================================================
# Pre-flight Checks
# ============================================================================

echo ""
echo -e "${YELLOW}Pre-flight checks...${NC}"

# Check if circuit exists (only if using real prover)
if [ "$MOCK_PROVER" = "false" ]; then
    if [ ! -d "$CIRCUIT_PATH" ]; then
        echo -e "${RED}ERROR: Circuit directory not found at $CIRCUIT_PATH${NC}"
        echo "Please compile the circuit first or set MOCK_PROVER=true"
        exit 1
    fi
    
    if [ ! -f "$CIRCUIT_PATH/target/zelana_batch.json" ]; then
        echo -e "${YELLOW}WARNING: Circuit not compiled. Run 'nargo compile' in $CIRCUIT_PATH${NC}"
    fi
fi

# Check if binary exists
if [ ! -f "target/release/prover-worker" ]; then
    echo -e "${YELLOW}Building prover-worker in release mode...${NC}"
    cargo build --release --bin prover-worker
fi

echo -e "  ${GREEN}[OK]${NC} Pre-flight checks passed"

# ============================================================================
# Start Workers
# ============================================================================

echo ""
echo -e "${GREEN}Starting $NUM_WORKERS prover workers...${NC}"
echo "  Mock Prover:         $MOCK_PROVER"
echo "  Circuit Path:        $CIRCUIT_PATH"
echo "  Max Concurrent Jobs: $MAX_CONCURRENT_JOBS"
echo ""

# Store PIDs for cleanup
PIDS=()

cleanup() {
    echo ""
    echo -e "${YELLOW}Shutting down workers...${NC}"
    for pid in "${PIDS[@]}"; do
        if kill -0 "$pid" 2>/dev/null; then
            kill "$pid" 2>/dev/null || true
        fi
    done
    echo -e "${GREEN}All workers stopped.${NC}"
    exit 0
}

trap cleanup SIGINT SIGTERM

# Start each worker
for i in $(seq 1 $NUM_WORKERS); do
    PORT=$((BASE_PORT + i - 1))
    WORKER_ID=$i
    
    echo -e "${BLUE}Starting Worker $WORKER_ID on port $PORT...${NC}"
    
    RUST_LOG=prover_worker=info \
    ./target/release/prover-worker \
        --worker-id="$WORKER_ID" \
        --port="$PORT" \
        --host="0.0.0.0" \
        --circuit-path="$CIRCUIT_PATH" \
        --max-concurrent-jobs="$MAX_CONCURRENT_JOBS" \
        --mock-prover="$MOCK_PROVER" \
        --mock-delay-ms="$MOCK_DELAY_MS" \
        2>&1 | sed "s/^/[Worker $WORKER_ID] /" &
    
    PIDS+=($!)
    
    # Small delay to stagger startup
    sleep 0.5
done

echo ""
echo -e "${GREEN}All $NUM_WORKERS workers started!${NC}"
echo ""
echo "Worker URLs:"
for i in $(seq 1 $NUM_WORKERS); do
    PORT=$((BASE_PORT + i - 1))
    echo "  - http://localhost:$PORT"
done
echo ""
echo -e "${YELLOW}Press Ctrl+C to stop all workers${NC}"
echo ""

# Wait for all workers
wait
