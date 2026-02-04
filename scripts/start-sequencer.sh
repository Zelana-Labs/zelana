#!/bin/bash
# Zelana Sequencer Startup Script
# 
# Starts the sequencer with real Noir prover and L1 settlement enabled.
# 
# Prerequisites:
# 1. Start prover-coordinator on port 8080:
#    cd zelana-forge && cargo run --bin prover-coordinator -- --port=8080
#
# 2. Ensure Solana keypair exists at ~/.config/solana/id.json
#    (Run: solana-keygen new --no-bip39-passphrase if needed)
#
# 3. Fund the keypair with devnet SOL:
#    solana airdrop 2 --url https://api.devnet.solana.com
#

set -e

# Colors for output
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${GREEN}========================================${NC}"
echo -e "${GREEN}       Zelana Sequencer Startup        ${NC}"
echo -e "${GREEN}========================================${NC}"

# Configuration

# API Configuration
export ZL_API_PORT=3001
export ZL_API_HOST=0.0.0.0

# Prover Mode: Use Noir prover via prover-coordinator
export ZL_PROVER_MODE=noir
export ZL_NOIR_COORDINATOR_URL=http://localhost:8080
export ZL_NOIR_PROOF_TIMEOUT_SECS=300

# Settlement Configuration
export ZL_SETTLEMENT_ENABLED=true
export ZL_SEQUENCER_KEYPAIR=~/.config/solana/id.json
export ZL_SETTLEMENT_RETRIES=5

# Solana Configuration
export SOLANA_RPC_URL=https://api.devnet.solana.com
export SOLANA_WS_URL=wss://api.devnet.solana.com

# Bridge program (deployed to devnet)
export ZL_BRIDGE_PROGRAM=8SE6gCijcFQixvDQqWu29mCm9AydN8hcwWh2e2Q6RQgE

# Verifier program (sunspot-generated, deployed to devnet)
export ZL_VERIFIER_PROGRAM_ID=EZzyLrTrC4uyU488jVAs4GKeCR1s9GmoFggeiDqwDeNK

# Domain identifier for L2
export ZL_DOMAIN=zelana

# Development mode (enables test endpoints)
export DEV_MODE=1

# Logging
export RUST_LOG=info,zelana_core=debug

# Pre-flight Checks

echo ""
echo -e "${YELLOW}Pre-flight checks...${NC}"

# Check keypair exists
if [ ! -f ~/.config/solana/id.json ]; then
    echo "ERROR: Solana keypair not found at ~/.config/solana/id.json"
    echo "Run: solana-keygen new --no-bip39-passphrase"
    exit 1
fi
echo "  [OK] Keypair found"

# Check prover-coordinator is running
if ! curl -s http://localhost:8080/health > /dev/null 2>&1; then
    echo "WARNING: Prover-coordinator not running on port 8080"
    echo "Start it with: cd zelana-forge && cargo run --bin prover-coordinator -- --port=8080"
else
    echo "  [OK] Prover-coordinator is running"
fi

echo ""
echo -e "${GREEN}Starting sequencer with:${NC}"
echo "  API Port:        $ZL_API_PORT"
echo "  Prover Mode:     $ZL_PROVER_MODE"
echo "  Coordinator:     $ZL_NOIR_COORDINATOR_URL"
echo "  Settlement:      $ZL_SETTLEMENT_ENABLED"
echo "  RPC URL:         $SOLANA_RPC_URL"
echo "  Bridge Program:  $ZL_BRIDGE_PROGRAM"
echo "  Verifier:        $ZL_VERIFIER_PROGRAM_ID"
echo ""

# Start Sequencer

cd "$(dirname "$0")/.."
cargo run --release -p zelana-core
