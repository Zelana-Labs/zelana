#!/usr/bin/env bash
# Test the distributed prover system locally

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

echo "==> Starting local test cluster..."
echo ""

# Colors for output
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Function to check if command exists
command_exists() {
    command -v "$1" >/dev/null 2>&1
}

# Check prerequisites
echo "Checking prerequisites..."
if ! command_exists cargo; then
    echo -e "${RED}Error: cargo not found. Please install Rust.${NC}"
    exit 1
fi

if ! command_exists curl; then
    echo -e "${RED}Error: curl not found. Please install curl.${NC}"
    exit 1
fi

echo -e "${GREEN} Prerequisites satisfied${NC}"
echo ""

# Build the binaries
echo "==> Building binaries..."
cd "$PROJECT_ROOT/crates"
cargo build --release --workspace
echo -e "${GREEN} Build complete${NC}"
echo ""

# Start nodes in background
PIDS=()
PORTS=(3001 3002 3003 3004 3005)

echo "==> Starting 5 prover nodes..."
for i in {1..5}; do
    PORT=${PORTS[$i-1]}
    echo "  Starting node $i on port $PORT..."
    "$PROJECT_ROOT/crates/target/release/prover-node" \
        --node-id "$i" \
        --port "$PORT" \
        --host "127.0.0.1" \
        > "/tmp/prover-node-$i.log" 2>&1 &
    PIDS+=($!)
    sleep 0.5
done

echo -e "${GREEN} Nodes started${NC}"
echo ""

# Wait for nodes to be ready
echo "==> Waiting for nodes to be ready..."
for i in {1..5}; do
    PORT=${PORTS[$i-1]}
    MAX_RETRIES=30
    RETRY=0
    while [ $RETRY -lt $MAX_RETRIES ]; do
        if curl -s "http://127.0.0.1:$PORT/health" > /dev/null 2>&1; then
            echo -e "  ${GREEN} Node $i ready${NC}"
            break
        fi
        RETRY=$((RETRY + 1))
        if [ $RETRY -eq $MAX_RETRIES ]; then
            echo -e "  ${RED} Node $i failed to start${NC}"
            echo "  Check logs at /tmp/prover-node-$i.log"
            # Kill all processes
            for pid in "${PIDS[@]}"; do
                kill "$pid" 2>/dev/null || true
            done
            exit 1
        fi
        sleep 1
    done
done
echo ""

# Start coordinator
echo "==> Starting coordinator..."
NODE_URLS="http://127.0.0.1:3001,http://127.0.0.1:3002,http://127.0.0.1:3003,http://127.0.0.1:3004,http://127.0.0.1:3005"
"$PROJECT_ROOT/crates/target/release/prover-coordinator" \
    --threshold 3 \
    --nodes "$NODE_URLS" \
    --port 8080 \
    --host "127.0.0.1" \
    > "/tmp/prover-coordinator.log" 2>&1 &
COORDINATOR_PID=$!
PIDS+=($COORDINATOR_PID)

# Wait for coordinator to be ready
MAX_RETRIES=30
RETRY=0
while [ $RETRY -lt $MAX_RETRIES ]; do
    if curl -s "http://127.0.0.1:8080/health" > /dev/null 2>&1; then
        echo -e "${GREEN} Coordinator ready${NC}"
        break
    fi
    RETRY=$((RETRY + 1))
    if [ $RETRY -eq $MAX_RETRIES ]; then
        echo -e "${RED} Coordinator failed to start${NC}"
        echo "Check logs at /tmp/prover-coordinator.log"
        for pid in "${PIDS[@]}"; do
            kill "$pid" 2>/dev/null || true
        done
        exit 1
    fi
    sleep 1
done
echo ""

# Cleanup function
cleanup() {
    echo ""
    echo "==> Shutting down..."
    for pid in "${PIDS[@]}"; do
        kill "$pid" 2>/dev/null || true
    done
    echo -e "${GREEN} All processes stopped${NC}"
}

trap cleanup EXIT INT TERM

echo "==> System is ready!"
echo ""
echo "Coordinator: http://127.0.0.1:8080"
echo "Node 1:      http://127.0.0.1:3001"
echo "Node 2:      http://127.0.0.1:3002"
echo "Node 3:      http://127.0.0.1:3003"
echo "Node 4:      http://127.0.0.1:3004"
echo "Node 5:      http://127.0.0.1:3005"
echo ""
echo "Logs:"
echo "  Coordinator: /tmp/prover-coordinator.log"
echo "  Nodes:       /tmp/prover-node-*.log"
echo ""

# Run tests
echo "==> Running tests..."
echo ""

# Test 1: Setup
echo -e "${YELLOW}Test 1: Setup${NC}"
SECRET="0x0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
SETUP_RESPONSE=$(curl -s -X POST http://127.0.0.1:8080/setup \
    -H "Content-Type: application/json" \
    -d "{\"secret\":\"$SECRET\"}")

if echo "$SETUP_RESPONSE" | grep -q '"status":"success"'; then
    echo -e "${GREEN} Setup successful${NC}"
    echo "Response: $SETUP_RESPONSE" | jq '.' 2>/dev/null || echo "$SETUP_RESPONSE"
else
    echo -e "${RED} Setup failed${NC}"
    echo "Response: $SETUP_RESPONSE"
    exit 1
fi
echo ""

# Test 2: Generate proof
echo -e "${YELLOW}Test 2: Generate Proof${NC}"
PROVE_RESPONSE=$(curl -s -X POST http://127.0.0.1:8080/prove \
    -H "Content-Type: application/json" \
    -d '{"message":"Hello, Zelana!"}')

if echo "$PROVE_RESPONSE" | grep -q '"status":"success"'; then
    echo -e "${GREEN} Proof generation successful${NC}"
    echo "Response (truncated):"
    echo "$PROVE_RESPONSE" | jq '.status, .data.participants' 2>/dev/null || echo "$PROVE_RESPONSE"

    # Extract proof for verification
    PROOF=$(echo "$PROVE_RESPONSE" | jq '.data.proof' 2>/dev/null)
else
    echo -e "${RED} Proof generation failed${NC}"
    echo "Response: $PROVE_RESPONSE"
    exit 1
fi
echo ""

# Test 3: Verify proof
echo -e "${YELLOW}Test 3: Verify Proof${NC}"
VERIFY_RESPONSE=$(curl -s -X POST http://127.0.0.1:8080/verify \
    -H "Content-Type: application/json" \
    -d "{\"proof\":$PROOF}")

if echo "$VERIFY_RESPONSE" | grep -q '"valid":true'; then
    echo -e "${GREEN} Proof verification successful${NC}"
    echo "Response: $VERIFY_RESPONSE" | jq '.' 2>/dev/null || echo "$VERIFY_RESPONSE"
else
    echo -e "${RED} Proof verification failed${NC}"
    echo "Response: $VERIFY_RESPONSE"
    exit 1
fi
echo ""

# Success!
echo -e "${GREEN}==> All tests passed!${NC}"
echo ""
echo "Press Ctrl+C to stop the cluster, or it will run indefinitely..."
echo ""

# Keep running
while true; do
    sleep 1
done
