#!/bin/bash
# Zelana Deployment and Testing Script
#
# This script helps you deploy and test the Zelana L2 system.
#
# Prerequisites:
#   - Rust toolchain installed
#   - Solana CLI installed
#   - Surfpool installed (cargo install surfpool)
#   - Anchor installed (for verifier program)
#
# Usage:
#   ./deploy.sh           # Full deployment and test
#   ./deploy.sh deploy    # Deploy programs only
#   ./deploy.sh init      # Initialize bridge and store VK
#   ./deploy.sh test      # Run e2e test (assumes programs deployed)

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Configuration
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(dirname "$SCRIPT_DIR")"
RPC_URL="http://127.0.0.1:8899"

# Program paths
BRIDGE_SO="$ROOT_DIR/onchain-programs/bridge/target/deploy/bridge_z.so"
BRIDGE_KEYPAIR="$SCRIPT_DIR/keys/bridge-keypair.json"
VERIFIER_SO="$ROOT_DIR/onchain-programs/verifier/target/deploy/onchain_verifier.so"
VERIFIER_KEYPAIR="$SCRIPT_DIR/keys/verifier-keypair.json"

# Program IDs (from freshly generated keypairs)
BRIDGE_PROGRAM_ID="8SE6gCijcFQixvDQqWu29mCm9AydN8hcwWh2e2Q6RQgE"
VERIFIER_PROGRAM_ID="8TveT3mvH59qLzZNwrTT6hBqDHEobW2XnCPb7xZLBYHd"

print_header() {
    echo ""
    echo -e "${BLUE}============================================${NC}"
    echo -e "${BLUE}  $1${NC}"
    echo -e "${BLUE}============================================${NC}"
    echo ""
}

print_success() {
    echo -e "${GREEN}✅ $1${NC}"
}

print_error() {
    echo -e "${RED}❌ $1${NC}"
}

print_info() {
    echo -e "${YELLOW}ℹ️  $1${NC}"
}

check_prerequisites() {
    print_header "Checking Prerequisites"
    
    # Check Solana CLI
    if ! command -v solana &> /dev/null; then
        print_error "Solana CLI not found. Install it from https://docs.solana.com/cli/install-solana-cli-tools"
        exit 1
    fi
    print_success "Solana CLI: $(solana --version)"
    
    # Check Surfpool
    if ! command -v surfpool &> /dev/null; then
        print_error "Surfpool not found. Install with: cargo install surfpool"
        exit 1
    fi
    print_success "Surfpool available"
    
    # Check program binaries
    if [ ! -f "$BRIDGE_SO" ]; then
        print_error "Bridge program not built: $BRIDGE_SO"
        print_info "Run: cd onchain-programs/bridge && cargo build-sbf"
        exit 1
    fi
    print_success "Bridge program binary exists"
    
    if [ ! -f "$VERIFIER_SO" ]; then
        print_error "Verifier program not built: $VERIFIER_SO"
        print_info "Run: cd onchain-programs/verifier && anchor build"
        exit 1
    fi
    print_success "Verifier program binary exists"
    
    # Check keypairs
    if [ ! -f "$BRIDGE_KEYPAIR" ]; then
        print_error "Bridge keypair not found: $BRIDGE_KEYPAIR"
        exit 1
    fi
    
    if [ ! -f "$VERIFIER_KEYPAIR" ]; then
        print_error "Verifier keypair not found: $VERIFIER_KEYPAIR"
        exit 1
    fi
    print_success "Program keypairs exist"
}

check_surfpool() {
    print_header "Checking Surfpool"
    
    # Try to connect to RPC
    if curl -s "$RPC_URL" -X POST -H "Content-Type: application/json" \
        -d '{"jsonrpc":"2.0","id":1,"method":"getHealth"}' 2>/dev/null | grep -q "ok"; then
        print_success "Surfpool is running at $RPC_URL"
        return 0
    else
        print_info "Surfpool not running. Please start it in another terminal:"
        echo ""
        echo " surfpool start"
        echo ""
        print_info "Press Enter when Surfpool is running, or Ctrl+C to exit"
        read -r
        
        # Check again
        if curl -s "$RPC_URL" -X POST -H "Content-Type: application/json" \
            -d '{"jsonrpc":"2.0","id":1,"method":"getHealth"}' 2>/dev/null | grep -q "ok"; then
            print_success "Surfpool is running"
            return 0
        else
            print_error "Still cannot connect to Surfpool at $RPC_URL"
            exit 1
        fi
    fi
}

fund_deployer() {
    print_header "Funding Deployer Account"
    
    DEPLOYER=$(solana address)
    print_info "Deployer: $DEPLOYER"
    
    BALANCE=$(solana balance --url "$RPC_URL" 2>/dev/null | cut -d' ' -f1)
    print_info "Current balance: $BALANCE SOL"
    
    if (( $(echo "$BALANCE < 10" | bc -l) )); then
        print_info "Requesting airdrop..."
        solana airdrop 100 --url "$RPC_URL"
        print_success "Airdrop received"
    else
        print_success "Sufficient balance for deployment"
    fi
}

deploy_programs() {
    print_header "Deploying Programs"
    
    # Deploy Bridge
    print_info "Deploying Bridge program..."
    print_info "  Program ID: $BRIDGE_PROGRAM_ID"
    
    solana program deploy \
        --program-id "$BRIDGE_KEYPAIR" \
        "$BRIDGE_SO" \
        --url "$RPC_URL" \
        --commitment confirmed
    
    print_success "Bridge program deployed"
    
    # Deploy Verifier
    print_info "Deploying Verifier program..."
    print_info "  Program ID: $VERIFIER_PROGRAM_ID"
    
    solana program deploy \
        --program-id "$VERIFIER_KEYPAIR" \
        "$VERIFIER_SO" \
        --url "$RPC_URL" \
        --commitment confirmed
    
    print_success "Verifier program deployed"
}

initialize_bridge() {
    print_header "Initializing Bridge"
    
    cd "$ROOT_DIR"
    cargo run --package zelana-scripts --bin init_bridge
}

store_vk() {
    print_header "Storing Batch Verifying Key"
    
    cd "$ROOT_DIR"
    cargo run --package zelana-scripts --bin store_vk
}

run_e2e_test() {
    print_header "Running End-to-End Test"
    
    print_info "Make sure the sequencer is running in another terminal:"
    echo ""
    echo "  cd $ROOT_DIR && cargo run --package core"
    echo ""
    print_info "Press Enter when ready, or Ctrl+C to exit"
    read -r
    
    cd "$ROOT_DIR"
    cargo run --package zelana-scripts --bin e2e_test
}

show_summary() {
    print_header "Deployment Summary"
    
    echo "Programs Deployed:"
    echo "  Bridge:   $BRIDGE_PROGRAM_ID"
    echo "  Verifier: $VERIFIER_PROGRAM_ID"
    echo ""
    echo "RPC URL: $RPC_URL"
    echo ""
    echo "Next Steps:"
    echo "  1. Start the sequencer:"
    echo "     cargo run --package core"
    echo ""
    echo "  2. Run individual tests:"
    echo "     cargo run --package zelana-scripts --bin deposit -- --amount 1.0"
    echo "     cargo run --package zelana-scripts --bin check_balance"
    echo "     cargo run --package zelana-scripts --bin transfer -- --to <PUBKEY> --amount 0.1"
    echo ""
    echo "  3. Run full e2e test:"
    echo "     cargo run --package zelana-scripts --bin e2e_test"
}

# Main script
case "${1:-full}" in
    deploy)
        check_prerequisites
        check_surfpool
        fund_deployer
        deploy_programs
        show_summary
        ;;
    init)
        check_surfpool
        initialize_bridge
        store_vk
        ;;
    test)
        run_e2e_test
        ;;
    full|*)
        check_prerequisites
        check_surfpool
        fund_deployer
        deploy_programs
        initialize_bridge
        store_vk
        show_summary
        print_info "Ready for testing! Run: $0 test"
        ;;
esac
