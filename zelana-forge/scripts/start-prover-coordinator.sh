#!/bin/bash
# Zelana Prover-Coordinator Startup Script
# 
# Starts the prover-coordinator with real proof generation.
#

set -e

# Colors for output
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${GREEN}========================================${NC}"
echo -e "${GREEN}    Zelana Prover-Coordinator         ${NC}"
echo -e "${GREEN}========================================${NC}"

cd "$(dirname "$0")/.."

# ============================================================================
# Configuration
# ============================================================================

PORT=${PORT:-8080}
HOST=${HOST:-0.0.0.0}

# Circuit paths (relative to zelana-forge directory)
OWNERSHIP_CIRCUIT_PATH=circuits/ownership
BATCH_CIRCUIT_PATH=circuits/zelana_batch

# Mock settings
# - Batch proofs: false = real prover (takes ~5 minutes per batch)
# - Ownership proofs: false = real prover (fast, ~2-3 seconds)
MOCK_PROVER=${MOCK_PROVER:-false}
MOCK_OWNERSHIP_PROVER=${MOCK_OWNERSHIP_PROVER:-false}

# Solana Configuration (for settlement)
SOLANA_RPC=${SOLANA_RPC:-https://api.devnet.solana.com}
PROGRAM_ID=${PROGRAM_ID:-EZzyLrTrC4uyU488jVAs4GKeCR1s9GmoFggeiDqwDeNK}
MOCK_SETTLEMENT=${MOCK_SETTLEMENT:-true}

# ============================================================================
# Pre-flight Checks
# ============================================================================

echo ""
echo -e "${YELLOW}Pre-flight checks...${NC}"

# Check ownership circuit artifacts exist
if [ ! -f "$OWNERSHIP_CIRCUIT_PATH/target/ownership.pk" ]; then
    echo "WARNING: Ownership proving key not found at $OWNERSHIP_CIRCUIT_PATH/target/ownership.pk"
    echo "Run in circuits/ownership: nargo compile && sunspot compile && sunspot setup"
fi

if [ ! -f "$OWNERSHIP_CIRCUIT_PATH/target/ownership.vk" ]; then
    echo "WARNING: Ownership verifying key not found at $OWNERSHIP_CIRCUIT_PATH/target/ownership.vk"
fi

echo "  [OK] Circuit paths configured"

echo ""
echo -e "${GREEN}Starting prover-coordinator with:${NC}"
echo "  Port:                  $PORT"
echo "  Ownership Circuit:     $OWNERSHIP_CIRCUIT_PATH"
echo "  Batch Circuit:         $BATCH_CIRCUIT_PATH"
echo "  Mock Batch Prover:     $MOCK_PROVER"
echo "  Mock Ownership Prover: $MOCK_OWNERSHIP_PROVER"
echo "  Mock Settlement:       $MOCK_SETTLEMENT"
echo ""

# ============================================================================
# Start Prover-Coordinator
# ============================================================================

cargo run --bin prover-coordinator -- \
    --host="$HOST" \
    --port="$PORT" \
    --ownership-circuit-path="$OWNERSHIP_CIRCUIT_PATH" \
    --circuit-target-path="$BATCH_CIRCUIT_PATH" \
    --mock-prover="$MOCK_PROVER" \
    --mock-ownership-prover="$MOCK_OWNERSHIP_PROVER" \
    --mock-settlement="$MOCK_SETTLEMENT" \
    --solana-rpc="$SOLANA_RPC" \
    --program-id="$PROGRAM_ID" \
    --enable-core-api=true
