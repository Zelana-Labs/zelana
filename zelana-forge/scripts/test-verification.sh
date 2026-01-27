#!/bin/bash
# Test script for verifying zelana_batch proof on Solana devnet
#
# Prerequisites:
# - Solana CLI configured with devnet keypair
# - Proof files in circuits/zelana_batch/target/
# - Verifier program deployed at EZzyLrTrC4uyU488jVAs4GKeCR1s9GmoFggeiDqwDeNK

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ZELANA_FORGE_DIR="${SCRIPT_DIR}/.."
CIRCUIT_TARGET="${ZELANA_FORGE_DIR}/circuits/zelana_batch/target"

# Configuration
VERIFIER_PROGRAM_ID="EZzyLrTrC4uyU488jVAs4GKeCR1s9GmoFggeiDqwDeNK"
RPC_URL="${SOLANA_RPC:-https://api.devnet.solana.com}"
KEYPAIR_PATH="${KEYPAIR_PATH:-~/.config/solana/id.json}"

echo "=== Zelana Forge Proof Verification Test ==="
echo ""
echo "Configuration:"
echo "  Verifier Program: ${VERIFIER_PROGRAM_ID}"
echo "  RPC URL: ${RPC_URL}"
echo "  Keypair: ${KEYPAIR_PATH}"
echo "  Circuit Target: ${CIRCUIT_TARGET}"
echo ""

# Check proof files exist
if [ ! -f "${CIRCUIT_TARGET}/zelana_batch.proof" ]; then
    echo "ERROR: Proof file not found: ${CIRCUIT_TARGET}/zelana_batch.proof"
    echo "Please generate a proof first using: cd circuits/zelana_batch && nargo execute && sunspot prove"
    exit 1
fi

if [ ! -f "${CIRCUIT_TARGET}/zelana_batch.pw" ]; then
    echo "ERROR: Public witness file not found: ${CIRCUIT_TARGET}/zelana_batch.pw"
    exit 1
fi

# Show proof info
PROOF_SIZE=$(stat -c%s "${CIRCUIT_TARGET}/zelana_batch.proof" 2>/dev/null || stat -f%z "${CIRCUIT_TARGET}/zelana_batch.proof")
PW_SIZE=$(stat -c%s "${CIRCUIT_TARGET}/zelana_batch.pw" 2>/dev/null || stat -f%z "${CIRCUIT_TARGET}/zelana_batch.pw")

echo "Proof files:"
echo "  zelana_batch.proof: ${PROOF_SIZE} bytes"
echo "  zelana_batch.pw: ${PW_SIZE} bytes"
echo ""

# Check keypair balance
echo "Checking keypair balance..."
BALANCE=$(solana balance --url "${RPC_URL}" --keypair "${KEYPAIR_PATH}" 2>/dev/null || echo "0 SOL")
echo "  Balance: ${BALANCE}"

if [ "${BALANCE}" == "0 SOL" ]; then
    echo ""
    echo "WARNING: No SOL balance. You may need to airdrop:"
    echo "  solana airdrop 1 --url ${RPC_URL} --keypair ${KEYPAIR_PATH}"
    echo ""
fi

# Check verifier program exists
echo ""
echo "Checking verifier program..."
if solana program show "${VERIFIER_PROGRAM_ID}" --url "${RPC_URL}" > /dev/null 2>&1; then
    echo "  Verifier program found"
else
    echo "ERROR: Verifier program not found at ${VERIFIER_PROGRAM_ID}"
    echo "Please deploy the verifier first using sunspot deploy"
    exit 1
fi

echo ""
echo "=== Building Rust Test Client ==="
echo ""

# Build the test binary
cd "${ZELANA_FORGE_DIR}"
cargo build --release -p prover-coordinator 2>&1 | tail -20

echo ""
echo "=== Running Verification Test ==="
echo ""

# Create a simple Rust test program
cat > /tmp/verify_proof.rs << 'EOF'
use std::path::PathBuf;
use std::env;

fn main() {
    let args: Vec<String> = env::args().collect();
    
    let proof_path = args.get(1).map(PathBuf::from).unwrap_or_else(|| {
        PathBuf::from("circuits/zelana_batch/target/zelana_batch.proof")
    });
    let pw_path = args.get(2).map(PathBuf::from).unwrap_or_else(|| {
        PathBuf::from("circuits/zelana_batch/target/zelana_batch.pw")
    });
    let program_id = args.get(3).map(|s| s.as_str()).unwrap_or("EZzyLrTrC4uyU488jVAs4GKeCR1s9GmoFggeiDqwDeNK");
    let rpc_url = args.get(4).map(|s| s.as_str()).unwrap_or("https://api.devnet.solana.com");
    
    println!("Loading proof from: {:?}", proof_path);
    println!("Loading public witness from: {:?}", pw_path);
    println!("Program ID: {}", program_id);
    println!("RPC URL: {}", rpc_url);
    
    let proof_bytes = std::fs::read(&proof_path).expect("Failed to read proof file");
    let pw_bytes = std::fs::read(&pw_path).expect("Failed to read public witness file");
    
    println!("Proof: {} bytes", proof_bytes.len());
    println!("Public Witness: {} bytes", pw_bytes.len());
    
    // Create instruction data
    let mut instruction_data = Vec::with_capacity(proof_bytes.len() + pw_bytes.len());
    instruction_data.extend_from_slice(&proof_bytes);
    instruction_data.extend_from_slice(&pw_bytes);
    
    println!("Instruction data: {} bytes", instruction_data.len());
    println!("Instruction data (hex): {}", hex::encode(&instruction_data[..32]));
    println!("...");
    
    // Output for manual verification with solana CLI
    println!("\n=== For manual verification ===");
    println!("solana program invoke {} \\", program_id);
    println!("  --data $(xxd -p -c 9999 {}) \\", proof_path.display());
    println!("  --url {}", rpc_url);
}
EOF

echo "Use the following command to verify the proof using the Rust client:"
echo ""
echo "  cargo run -p prover-coordinator --bin verify-proof -- \\"
echo "    --proof ${CIRCUIT_TARGET}/zelana_batch.proof \\"
echo "    --public-witness ${CIRCUIT_TARGET}/zelana_batch.pw \\"
echo "    --program-id ${VERIFIER_PROGRAM_ID} \\"
echo "    --rpc-url ${RPC_URL}"
echo ""

# Alternative: Use sunspot if available
if command -v sunspot &> /dev/null; then
    echo "=== Using sunspot verify (if supported) ==="
    echo ""
    echo "sunspot verify \\"
    echo "  ${CIRCUIT_TARGET}/zelana_batch.proof \\"
    echo "  ${CIRCUIT_TARGET}/zelana_batch.pw \\"
    echo "  ${CIRCUIT_TARGET}/zelana_batch.vk \\"
    echo "  --program-id ${VERIFIER_PROGRAM_ID} \\"
    echo "  --url ${RPC_URL}"
    echo ""
fi

echo "=== Manual Test with Raw Transaction ==="
echo ""

# Create hex-encoded instruction data
PROOF_HEX=$(xxd -p -c 9999 "${CIRCUIT_TARGET}/zelana_batch.proof")
PW_HEX=$(xxd -p -c 9999 "${CIRCUIT_TARGET}/zelana_batch.pw")
INSTRUCTION_DATA="${PROOF_HEX}${PW_HEX}"

echo "Instruction data length: $((${#INSTRUCTION_DATA} / 2)) bytes"
echo ""
echo "First 64 bytes of instruction data:"
echo "${INSTRUCTION_DATA:0:128}"
echo ""

echo "=== Done ==="
echo ""
echo "To verify manually, you can use the Solana CLI or run the coordinator with:"
echo ""
echo "  MOCK_SETTLEMENT=false \\"
echo "  PROGRAM_ID=${VERIFIER_PROGRAM_ID} \\"
echo "  SOLANA_RPC=${RPC_URL} \\"
echo "  CIRCUIT_TARGET_PATH=${CIRCUIT_TARGET} \\"
echo "  cargo run -p prover-coordinator"
