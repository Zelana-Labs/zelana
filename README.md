# Zelana

## Run sequencer
```
RUST_LOG=info cargo run -p core --release
```

Run a throughput bench:
```
cargo run -p core --example bench_throughput --release
```

Run a bridge test:
```
cargo run -p core --example bridge --release
```

Run the `full_lifecycle` example:
```
cargo run -p core --example full_lifecycle --release
```

Run the L2 transaction example:
```
cargo run -p core --example transaction --release
```



Run a service:
```
cargo run -p prover
```

Build a specific repo:
```
cargo build -p core
```