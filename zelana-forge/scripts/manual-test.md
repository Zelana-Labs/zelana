# Manual Testing of Zelana-Forge

This guide shows how to **run the coordinator and prover nodes** locally and execute basic tests.

---

## 1. Run the Servers

### Coordinator

```bash
cargo run -p prover-coordinator
```

### Prover Nodes

Run 3 nodes with unique IDs and ports:

```bash
cargo run -p prover-node -- --node-id 1 --port 3000
cargo run -p prover-node -- --node-id 2 --port 3001
cargo run -p prover-node -- --node-id 3 --port 3002
```

> Each node must have a unique `--node-id` and `--port`.

---

## 2. Run the Tests

### Test 1: Setup

Send the secret to the coordinator:

```bash
SECRET="0x0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"

curl -s -X POST http://127.0.0.1:8080/setup \
    -H "Content-Type: application/json" \
    -d "{\"secret\":\"$SECRET\"}" | jq '.' 2>/dev/null
```

---

### Test 2: Generate Proof

Send a message to generate a proof and save the response to a file:

```bash
PROVE_RESPONSE_FILE="prove_response.json"

curl -s -X POST http://127.0.0.1:8080/prove \
    -H "Content-Type: application/json" \
    -d '{"message":"Hello, Zelana!"}' | jq '.' 2>/dev/null > "$PROVE_RESPONSE_FILE"
```

> You can inspect the generated proof later in `prove_response.json`.

---

### Test 3: Verify Proof

Extract the proof from the file and verify it:

```bash
PROOF=$(jq -c '.data.proof' "$PROVE_RESPONSE_FILE")

curl -s -X POST http://127.0.0.1:8080/verify \
    -H "Content-Type: application/json" \
    -d "{\"proof\":$PROOF}" | jq '.' 2>/dev/null
```

---

### Notes

* Ensure the coordinator is running before starting the nodes.
* Make sure each node has a unique `--node-id` and port.
* `jq` is used to pretty-print JSON responses. Install with:

```bash
sudo apt install jq   # Debian/Ubuntu   # macOS
```