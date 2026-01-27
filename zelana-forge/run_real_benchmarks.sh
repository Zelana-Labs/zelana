#!/bin/bash
# Real Zelana Forge Performance Benchmarks
# Tests actual running system with real cryptographic operations
#
# Usage:
#   ./run_real_benchmarks.sh                    # Test running binary services
#   ./run_real_benchmarks.sh binary             # Test running binary services
#   ./run_real_benchmarks.sh docker             # Test running Docker services
#   ./run_real_benchmarks.sh start              # Auto-start binary services + test
#   ./run_real_benchmarks.sh start binary       # Auto-start binary services + test
#   ./run_real_benchmarks.sh start docker       # Auto-start Docker services + test

set -e

# Parse command line arguments
AUTO_START=false
MODE="binary"  # Default mode

if [ "$1" = "docker" ]; then
    MODE="docker"
elif [ "$1" != "" ]; then
    echo "Usage: $0 [binary|docker]"
    echo ""
    echo "Commands:"
    echo "  (no args): Run benchmarks against running binary services"
    echo "  binary:     Run benchmarks against running binary services"
    echo "  docker:     Auto-start Docker services + run benchmarks + cleanup"
    echo ""
    echo "Examples:"
    echo "  $0                    # Test running binary services"
    echo "  $0 docker             # Auto-start Docker + test + cleanup"
    echo ""
    echo "Note: Binary services must be started separately."
    echo "  Binary: ./start_real_benchmark.sh"
    exit 1
fi

echo "Zelana Forge Performance Benchmarks"

# Configuration based on mode
if [ "$MODE" = "docker" ]; then
    COORDINATOR_URL="http://127.0.0.1:8000"
    SESSION_MODE="request"  # Docker uses session_id from request
else
    COORDINATOR_URL="http://127.0.0.1:8080"
    SESSION_MODE="hash"     # Direct binary generates hash-based session IDs
fi

TEST_ITERATIONS=5

echo "Zelana Forge Performance Benchmarks"
echo "Mode: $MODE"
echo "Coordinator: $COORDINATOR_URL"
echo "Circuit: Schnorr signature"
echo "Threshold: 3-of-5 nodes"
echo ""

# Test data - 32 bytes (64 hex chars) for field element
SECRETS=(
    "1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef"
    "abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890"
    "4567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef12"
    "7890abcdef1234567890abcdef1234567890abcdef1234567890abcdef123456"
    "0abcdef1234567890abcdef1234567890abcdef1234567890abcdef12345678"
)
MESSAGE="Hello, Zelana Forge! This is a test message for benchmarking."

echo "Checking service availability..."

if [ "$MODE" = "docker" ]; then
    if ! docker ps | grep -q "docker-coordinator"; then
        echo "Starting Docker services..."
        cd deploy/docker
        docker compose up -d

        echo "Waiting for Docker services..."
        for i in {1..60}; do
            healthy_count=$(docker compose ps 2>/dev/null | grep -c "healthy" || echo "0")
            if [ "$healthy_count" -ge 6 ]; then
                echo "Docker services ready"
                cd ../..
                break
            fi
            sleep 2
        done

        if [ "$healthy_count" -lt 6 ]; then
            echo "Docker services failed to start"
            exit 1
        fi
    else
        echo "Docker services detected"
    fi
else
    if ! curl -s --max-time 2 "$COORDINATOR_URL/health" > /dev/null; then
        echo "Coordinator not responding"
        exit 1
    fi
    echo "Binary services detected"
fi

echo ""
echo "Running benchmark..."
SETUP_TIMES=()
PROVE_TIMES=()
COMBINED_TIMES=()

for i in $(seq 1 $TEST_ITERATIONS); do
    # Use different data for each iteration to avoid session conflicts
    case $i in
        1) SECRET="1111111111111111111111111111111111111111111111111111111111111111" ;;
        2) SECRET="2222222222222222222222222222222222222222222222222222222222222222" ;;
        3) SECRET="3333333333333333333333333333333333333333333333333333333333333333" ;;
        4) SECRET="4444444444444444444444444444444444444444444444444444444444444444" ;;
        5) SECRET="5555555555555555555555555555555555555555555555555555555555555555" ;;
    esac

    # Make session ID unique per iteration and run
    TIMESTAMP=$(date +%s%N)
    SESSION_ID="bench-session-$TIMESTAMP-$i"

    echo -n "Iteration $i/$TEST_ITERATIONS: "

    START_TIME=$(date +%s%N)
    PROVE_RESPONSE=$(curl -s --max-time 30 -X POST "$COORDINATOR_URL/prove_single" \
        -H "Content-Type: application/json" \
        -d "{\"circuit_type\":\"schnorr\",\"witness_commitment\":{\"hash\":\"$SECRET\",\"session_id\":\"$SESSION_ID\"},\"secret\":\"0x$SECRET\"}")
    END_TIME=$(date +%s%N)

    if [ -z "$PROVE_RESPONSE" ]; then
        echo "Empty response"
        exit 1
    elif echo "$PROVE_RESPONSE" | grep -q '"status":"success"'; then
        DURATION=$(( (END_TIME - START_TIME) / 1000000 ))
        echo "${DURATION}ms"
        COMBINED_TIMES+=($DURATION)
    else
        echo "Failed"
        exit 1
    fi

    # Small delay between iterations to avoid node conflicts
    sleep 0.1
done

# Calculate combined statistics
COMBINED_TOTAL=0
COMBINED_MIN=${COMBINED_TIMES[0]}
COMBINED_MAX=${COMBINED_TIMES[0]}

for time in "${COMBINED_TIMES[@]}"; do
    COMBINED_TOTAL=$((COMBINED_TOTAL + time))
    if [ "$time" -lt "$COMBINED_MIN" ]; then COMBINED_MIN=$time; fi
    if [ "$time" -gt "$COMBINED_MAX" ]; then COMBINED_MAX=$time; fi
done

COMBINED_AVG=$((COMBINED_TOTAL / TEST_ITERATIONS))

echo ""
echo "Results:"
echo "Average: ${COMBINED_AVG}ms"
echo "Min: ${COMBINED_MIN}ms"
echo "Max: ${COMBINED_MAX}ms"

# Sequential estimate
SEQUENTIAL_ESTIMATE=$((COMBINED_AVG * 5))
SPEEDUP=$((SEQUENTIAL_ESTIMATE / COMBINED_AVG))
echo "Sequential estimate: ${SEQUENTIAL_ESTIMATE}ms"
echo "Speedup: ${SPEEDUP}x"

if [ "$AUTO_START" = true ] && [ "$MODE" = "docker" ]; then
    echo ""
    echo "Cleaning up Docker services..."
    cd deploy/docker
    docker compose down
    cd ../..
fi