#!/usr/bin/env bash
# Start control server and dashboard

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

# Colors
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
NC='\033[0m'

echo -e "${BLUE}╔══════════════════════════════════════════╗${NC}"
echo -e "${BLUE}║   Zelana Prover - Dashboard Setup       ║${NC}"
echo -e "${BLUE}╚══════════════════════════════════════════╝${NC}"
echo ""

PIDS=()

cleanup() {
    echo ""
    echo -e "${YELLOW}Shutting down services...${NC}"
    for pid in "${PIDS[@]}"; do
        kill "$pid" 2>/dev/null || true
    done
    echo -e "${GREEN} All services stopped${NC}"
}

trap cleanup EXIT INT TERM

# Build control server
echo -e "${BLUE}Building control server...${NC}"
cd "$PROJECT_ROOT"
cargo build --release -p prover-control
echo -e "${GREEN} Control server built${NC}"
echo ""

# Start control server
echo -e "${BLUE}Starting control server...${NC}"
"$PROJECT_ROOT/target/release/prover-control" > "/tmp/prover-control.log" 2>&1 &
CONTROL_PID=$!
PIDS+=($CONTROL_PID)
echo -e "${GREEN} Control server started (PID: $CONTROL_PID)${NC}"

# Wait for control server
sleep 2

# Install dashboard dependencies if needed
if [ ! -d "$PROJECT_ROOT/dashboard/node_modules" ]; then
    echo -e "${BLUE}Installing dashboard dependencies...${NC}"
    cd "$PROJECT_ROOT/dashboard"
    npm install
    echo -e "${GREEN} Dependencies installed${NC}"
fi

# Start dashboard (Next.js)
echo -e "${BLUE}Starting Next.js dashboard...${NC}"
cd "$PROJECT_ROOT/dashboard"
npm run dev -- --turbopack > "/tmp/prover-dashboard.log" 2>&1 &
DASHBOARD_PID=$!
PIDS+=($DASHBOARD_PID)
echo -e "${GREEN} Dashboard started (PID: $DASHBOARD_PID)${NC}"
echo ""

sleep 2

echo -e "${GREEN}╔══════════════════════════════════════════╗${NC}"
echo -e "${GREEN}║         Dashboard Ready!               ║${NC}"
echo -e "${GREEN}╚══════════════════════════════════════════╝${NC}"
echo ""
echo -e "${BLUE}Services:${NC}"
echo -e "  Dashboard:       ${GREEN}http://localhost:5173${NC}"
echo -e "  Control Server:  ${GREEN}http://localhost:9000${NC}"
echo ""
echo -e "${BLUE}Instructions:${NC}"
echo -e "  1. Open ${GREEN}http://localhost:5173${NC} in your browser"
echo -e "  2. Click ${GREEN}'▶ Start Cluster'${NC} to launch Docker containers"
echo -e "  3. Follow the workflow to generate proofs!"
echo ""
echo -e "${BLUE}Logs:${NC}"
echo -e "  Control Server: /tmp/prover-control.log"
echo -e "  Dashboard:      /tmp/prover-dashboard.log"
echo ""
echo -e "${YELLOW}Press Ctrl+C to stop all services${NC}"
echo ""

# Keep running
while true; do
    sleep 1
done
