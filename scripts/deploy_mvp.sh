#!/bin/bash
# Zelana L2 MVP Deployment Script
# This script deploys and configures Zelana with real ZK proofs

set -e

echo "============================================"
echo "  Zelana L2 MVP Deployment"
echo "============================================"
echo ""

# Configuration
KEYS_DIR="./keys"
SURFPOOL_RPC="http://127.0.0.1:8899"
DOMAIN="zelana"

# Colors
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m' # No Color

info() { echo -e "${GREEN}[INFO]${NC} $1"; }
warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }
error() { echo -e "${RED}[ERROR]${NC} $1"; }

# Check dependencies
check_deps() {
    info "Checking dependencies..."
    
    if ! command -v solana &> /dev/null; then
        error "solana CLI not found. Install from https://docs.solana.com/cli/install-solana-cli-tools"
        exit 1
    fi
    
    if ! command -v cargo &> /dev/null; then
        error "cargo not found. Install Rust from https://rustup.rs"
        exit 1
    fi
    
    info "Dependencies OK"
}

# Step 1: Generate ZK keys
generate_keys() {
    info "Step 1: Generating ZK proving/verifying keys..."
    
    mkdir -p "$KEYS_DIR"
    
    if [ -f "$KEYS_DIR/proving.key" ] && [ -f "$KEYS_DIR/verifying.key" ]; then
        warn "Keys already exist. Use --force to regenerate."
        return 0
    fi
    
    cargo run --release --package prover --bin keygen -- \
        --pk-out "$KEYS_DIR/proving.key" \
        --vk-out "$KEYS_DIR/verifying.key" \
        --force
    
    info "Keys generated successfully"
}

# Step 2: Convert VK to Solana format
convert_vk() {
    info "Step 2: Converting VK to Solana format..."
    
    cargo run --release --package prover --bin convert_vk -- \
        --vk-in "$KEYS_DIR/verifying.key" \
        --vk-out "$KEYS_DIR/batch_vk.json"
    
    info "VK converted successfully"
}

# Step 3: Build programs
build_programs() {
    info "Step 3: Building on-chain programs..."
    
    # Build bridge program
    info "  Building bridge program..."
    (cd onchain-programs/bridge && cargo build-sbf)
    
    # Build verifier program
    info "  Building verifier program..."
    (cd onchain-programs/verifier && anchor build)
    
    info "Programs built successfully"
}

# Step 4: Deploy programs
deploy_programs() {
    info "Step 4: Deploying programs to Surfpool..."
    
    # Check Surfpool is running
    if ! solana cluster-version --url "$SURFPOOL_RPC" &> /dev/null; then
        error "Surfpool not running. Start with: surfpool start --reset"
        exit 1
    fi
    
    # Airdrop SOL if needed
    BALANCE=$(solana balance --url "$SURFPOOL_RPC" 2>/dev/null | awk '{print $1}')
    if (( $(echo "$BALANCE < 10" | bc -l) )); then
        info "  Airdropping SOL..."
        solana airdrop 100 --url "$SURFPOOL_RPC"
    fi
    
    # Deploy bridge
    info "  Deploying bridge program..."
    solana program deploy \
        --program-id scripts/keys/bridge-keypair.json \
        onchain-programs/bridge/target/deploy/bridge_z.so \
        --url "$SURFPOOL_RPC"
    
    # Deploy verifier
    info "  Deploying verifier program..."
    solana program deploy \
        --program-id scripts/keys/verifier-keypair.json \
        onchain-programs/verifier/target/deploy/onchain_verifier.so \
        --url "$SURFPOOL_RPC"
    
    info "Programs deployed successfully"
}

# Step 5: Initialize bridge
init_bridge() {
    info "Step 5: Initializing bridge..."
    
    cargo run --release --package zelana-scripts --bin init_bridge
    
    info "Bridge initialized"
}

# Step 6: Store VK on-chain
store_vk() {
    info "Step 6: Storing VK on-chain..."
    
    cargo run --release --package zelana-scripts --bin store_vk -- \
        --vk-file "$KEYS_DIR/batch_vk.json"
    
    info "VK stored successfully"
}

# Step 7: Print environment for sequencer
print_env() {
    echo ""
    echo "============================================"
    echo "  Deployment Complete!"
    echo "============================================"
    echo ""
    echo "To start the sequencer with real ZK proofs:"
    echo ""
    echo "export ZL_MOCK_PROVER=false"
    echo "export ZL_PROVING_KEY=$KEYS_DIR/proving.key"
    echo "export ZL_VERIFYING_KEY=$KEYS_DIR/verifying.key"
    echo "export ZL_SETTLEMENT_ENABLED=true"
    echo "export ZL_SEQUENCER_KEYPAIR=~/.config/solana/id.json"
    echo "export SOLANA_RPC_URL=$SURFPOOL_RPC"
    echo "export SOLANA_WS_URL=ws://127.0.0.1:8900/"
    echo ""
    echo "cargo run --release --package zelana-core"
    echo ""
    echo "============================================"
}

# Main
main() {
    case "${1:-all}" in
        keys)
            check_deps
            generate_keys
            convert_vk
            ;;
        build)
            check_deps
            build_programs
            ;;
        deploy)
            check_deps
            deploy_programs
            ;;
        init)
            check_deps
            init_bridge
            store_vk
            ;;
        all)
            check_deps
            generate_keys
            convert_vk
            build_programs
            deploy_programs
            init_bridge
            store_vk
            print_env
            ;;
        env)
            print_env
            ;;
        *)
            echo "Usage: $0 [keys|build|deploy|init|all|env]"
            echo ""
            echo "  keys   - Generate ZK proving/verifying keys"
            echo "  build  - Build on-chain programs"
            echo "  deploy - Deploy programs to Surfpool"
            echo "  init   - Initialize bridge and store VK"
            echo "  all    - Run all steps (default)"
            echo "  env    - Print environment variables for sequencer"
            exit 1
            ;;
    esac
}

main "$@"
