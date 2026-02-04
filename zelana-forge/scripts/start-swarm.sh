#!/bin/bash
# Zelana Prover Swarm Startup Script
# 
# Starts the complete prover swarm:
# - 4 prover workers (ports 9001-9004)
# - 1 prover coordinator (port 8080)
#
# Environment Variables:
#   MOCK_PROVER    - Set to "true" for mock proofs (fast), "false" for real proofs (default: false)
#   NUM_WORKERS    - Number of workers to start (default: 4)
#
# Usage:
#   ./start-swarm.sh                       # Start full swarm with real prover
#   MOCK_PROVER=true ./start-swarm.sh      # Start full swarm with mock prover (for demos)
#
# For production/video demos, use real prover (MOCK_PROVER=false)

set -e

# Colors for output
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

echo -e "${CYAN}╔════════════════════════════════════════════════════════════╗${NC}"
echo -e "${CYAN}║        Zelana Prover Swarm - Parallel Architecture         ║${NC}"
echo -e "${CYAN}╠════════════════════════════════════════════════════════════╣${NC}"
echo -e "${CYAN}║  Coordinator (Brain):  http://localhost:8080               ║${NC}"
echo -e "${CYAN}║  Workers (Muscle):     http://localhost:9001-9004          ║${NC}"
echo -e "${CYAN}╚════════════════════════════════════════════════════════════╝${NC}"
echo ""

cd "$(dirname "$0")/.."

# ============================================================================
# Configuration
# ============================================================================

NUM_WORKERS=${NUM_WORKERS:-4}
BASE_PORT=${BASE_PORT:-9001}
MOCK_PROVER=${MOCK_PROVER:-false}
MOCK_DELAY_MS=${MOCK_DELAY_MS:-500}

# Circuit paths
OWNERSHIP_CIRCUIT_PATH=circuits/ownership
BATCH_CIRCUIT_PATH=circuits/zelana_batch

# Coordinator settings
COORDINATOR_PORT=${COORDINATOR_PORT:-8080}
COORDINATOR_HOST=${COORDINATOR_HOST:-0.0.0.0}

# Build worker URLs
WORKER_URLS=""
for i in $(seq 1 $NUM_WORKERS); do
    PORT=$((BASE_PORT + i - 1))
    if [ -n "$WORKER_URLS" ]; then
        WORKER_URLS="$WORKER_URLS,"
    fi
    WORKER_URLS="${WORKER_URLS}http://localhost:$PORT"
done

# ============================================================================
# Pre-flight Checks
# ============================================================================

echo -e "${YELLOW}Pre-flight checks...${NC}"

# Build binaries if needed
if [ ! -f "target/release/prover-worker" ] || [ ! -f "target/release/prover-coordinator" ]; then
    echo -e "${YELLOW}Building binaries in release mode...${NC}"
    cargo build --release --bin prover-worker --bin prover-coordinator
fi

echo -e "  ${GREEN}[OK]${NC} Binaries ready"

# Check circuits (only warn, don't fail)
if [ "$MOCK_PROVER" = "false" ]; then
    if [ ! -f "$BATCH_CIRCUIT_PATH/target/zelana_batch.json" ]; then
        echo -e "  ${YELLOW}[WARN]${NC} Batch circuit not compiled at $BATCH_CIRCUIT_PATH"
    else
        echo -e "  ${GREEN}[OK]${NC} Batch circuit found"
    fi
fi

echo ""

# ============================================================================
# Configuration Summary
# ============================================================================

echo -e "${GREEN}Configuration:${NC}"
echo "  Mode:              $([ "$MOCK_PROVER" = "true" ] && echo "MOCK (fast)" || echo "REAL (ZK proofs)")"
echo "  Workers:           $NUM_WORKERS"
echo "  Worker Ports:      $BASE_PORT - $((BASE_PORT + NUM_WORKERS - 1))"
echo "  Coordinator Port:  $COORDINATOR_PORT"
echo ""

# ============================================================================
# Store PIDs for cleanup
# ============================================================================

PIDS=()
PIDFILE="/tmp/zelana-swarm.pids"

cleanup() {
    echo ""
    echo -e "${YELLOW}Shutting down swarm...${NC}"
    
    # Kill all processes
    for pid in "${PIDS[@]}"; do
        if kill -0 "$pid" 2>/dev/null; then
            kill "$pid" 2>/dev/null || true
        fi
    done
    
    # Clean up PID file
    rm -f "$PIDFILE"
    
    echo -e "${GREEN}Swarm stopped.${NC}"
    exit 0
}

trap cleanup SIGINT SIGTERM

# ============================================================================
# Start Workers
# ============================================================================

echo -e "${BLUE}Starting $NUM_WORKERS prover workers...${NC}"

for i in $(seq 1 $NUM_WORKERS); do
    PORT=$((BASE_PORT + i - 1))
    WORKER_ID=$i
    
    echo -e "  Starting Worker $WORKER_ID on port $PORT..."
    
    RUST_LOG=prover_worker=info \
    ./target/release/prover-worker \
        --worker-id="$WORKER_ID" \
        --port="$PORT" \
        --host="0.0.0.0" \
        --circuit-path="$BATCH_CIRCUIT_PATH" \
        --max-concurrent-jobs=2 \
        --mock-prover="$MOCK_PROVER" \
        --mock-delay-ms="$MOCK_DELAY_MS" \
        2>&1 | sed "s/^/[W$WORKER_ID] /" &
    
    PIDS+=($!)
    sleep 0.3
done

# Wait for workers to be ready
echo -e "${YELLOW}Waiting for workers to initialize...${NC}"
sleep 2

# Check worker health
echo -e "${BLUE}Checking worker health...${NC}"
HEALTHY_WORKERS=0
for i in $(seq 1 $NUM_WORKERS); do
    PORT=$((BASE_PORT + i - 1))
    if curl -s "http://localhost:$PORT/health" > /dev/null 2>&1; then
        echo -e "  ${GREEN}[OK]${NC} Worker $i on port $PORT"
        HEALTHY_WORKERS=$((HEALTHY_WORKERS + 1))
    else
        echo -e "  ${RED}[FAIL]${NC} Worker $i on port $PORT"
    fi
done

if [ "$HEALTHY_WORKERS" -lt 1 ]; then
    echo -e "${RED}ERROR: No healthy workers. Cannot start coordinator.${NC}"
    cleanup
    exit 1
fi

echo ""

# ============================================================================
# Start Coordinator
# ============================================================================

echo -e "${BLUE}Starting prover coordinator on port $COORDINATOR_PORT...${NC}"

RUST_LOG=prover_coordinator=info \
./target/release/prover-coordinator \
    --host="$COORDINATOR_HOST" \
    --port="$COORDINATOR_PORT" \
    --workers="$WORKER_URLS" \
    --ownership-circuit-path="$OWNERSHIP_CIRCUIT_PATH" \
    --circuit-target-path="$BATCH_CIRCUIT_PATH" \
    --mock-prover="$MOCK_PROVER" \
    --mock-settlement=true \
    --enable-core-api=true \
    2>&1 | sed "s/^/[COORD] /" &

PIDS+=($!)

# Wait for coordinator to be ready
sleep 2

# Check coordinator health
if curl -s "http://localhost:$COORDINATOR_PORT/health" > /dev/null 2>&1; then
    echo -e "  ${GREEN}[OK]${NC} Coordinator ready on port $COORDINATOR_PORT"
else
    echo -e "  ${YELLOW}[WAIT]${NC} Coordinator starting..."
    sleep 3
fi

echo ""

# ============================================================================
# Summary
# ============================================================================

echo -e "${GREEN}╔════════════════════════════════════════════════════════════╗${NC}"
echo -e "${GREEN}║              Swarm Started Successfully!                   ║${NC}"
echo -e "${GREEN}╠════════════════════════════════════════════════════════════╣${NC}"
echo -e "${GREEN}║                                                            ║${NC}"
echo -e "${GREEN}║  Coordinator API:  http://localhost:$COORDINATOR_PORT                    ║${NC}"
echo -e "${GREEN}║                                                            ║${NC}"
echo -e "${GREEN}║  Endpoints:                                                ║${NC}"
echo -e "${GREEN}║    GET  /health          - Health check                    ║${NC}"
echo -e "${GREEN}║    POST /core/prove      - Submit batch for proving        ║${NC}"
echo -e "${GREEN}║    GET  /core/status/:id - Check proof status              ║${NC}"
echo -e "${GREEN}║    POST /prove/ownership - Generate ownership proof        ║${NC}"
echo -e "${GREEN}║                                                            ║${NC}"
echo -e "${GREEN}║  Workers: $HEALTHY_WORKERS/$NUM_WORKERS healthy                                      ║${NC}"
echo -e "${GREEN}║                                                            ║${NC}"
echo -e "${GREEN}╚════════════════════════════════════════════════════════════╝${NC}"
echo ""
echo -e "${YELLOW}Press Ctrl+C to stop the swarm${NC}"
echo ""

# Save PIDs for external management
echo "${PIDS[@]}" > "$PIDFILE"

# Wait for all processes
wait
