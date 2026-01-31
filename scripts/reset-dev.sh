#!/bin/bash
# Reset development environment for clean testing
# Usage: ./scripts/reset-dev.sh

set -e

echo "=== Zelana Development Reset Script ==="

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Find the project root (where this script is located)
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

echo ""
echo -e "${YELLOW}1. Clearing sequencer database...${NC}"
rm -rf "$PROJECT_ROOT/zelana-db" 2>/dev/null || true
rm -rf "$PROJECT_ROOT/zelana-dev-db" 2>/dev/null || true
rm -rf "$PROJECT_ROOT/core/zelana-db" 2>/dev/null || true
rm -rf "$PROJECT_ROOT/core/zelana-dev-db" 2>/dev/null || true
echo -e "${GREEN}   Database directories cleared${NC}"

echo ""
echo -e "${YELLOW}2. Database locations that were cleared:${NC}"
echo "   - $PROJECT_ROOT/zelana-db"
echo "   - $PROJECT_ROOT/zelana-dev-db"
echo "   - $PROJECT_ROOT/core/zelana-db"
echo "   - $PROJECT_ROOT/core/zelana-dev-db"

echo ""
echo -e "${YELLOW}3. To clear browser localStorage (for shielded notes):${NC}"
echo "   Open browser DevTools -> Application -> Local Storage -> Clear"
echo "   Or add ?clear-storage to the URL"

echo ""
echo -e "${GREEN}=== Reset Complete ===${NC}"
echo ""
echo "Next steps:"
echo "  1. Start the sequencer: cargo run -p zelana-core"
echo "  2. Start the website: cd ../zelana-website && bun run dev"
echo "  3. Connect wallet and deposit some zeSOL"
echo "  4. Test shielded transactions"
