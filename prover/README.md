# Consolidation Incentives Proof Service

Rust proof service for generating and submitting SSZ Merkle proofs of validator consolidations on Gnosis Chain.

## Architecture

This is a Cargo workspace with three crates:

### 📦 Crates

#### `proof-gen` — Core Proof Generation Library

**Purpose:** Generate SSZ Merkle proofs from Gnosis beacon chain data.

**Key Components:**
- **SSZ Types:** Electra BeaconState structure (37 fields) with gnosis/minimal preset support
- **Sparse Merkle Proofs:** Efficient proof generation without allocating full 2^40 validator trees (see `sparse_proof.rs`)
- **StateProver:** High-level API for generating complete proof bundles
- **GindexCalculator:** Computes generalized indices for beacon state fields
- **BeaconClient:** HTTP client for Gnosis beacon API

**Features:**
- ✅ 47 tests passing (sparse proofs, state proofs, gindex computation)
- ✅ Cross-validated with ssz_rs built-in proofs
- ✅ Preset support via cargo features: `gnosis` (default), `minimal`

#### `service` — REST API + Auto-Submitter

**Purpose:** Continuous consolidation detection and automatic reward claim submission.

**Endpoints:**
| Endpoint | Description |
|----------|-------------|
| `GET /health` | Health check (degraded if >64 slots behind) |
| `GET /status` | Sync status (current slot/epoch, slots behind) |
| `GET /consolidations` | List detected consolidations with status |
| `GET /metrics` | Prometheus metrics |

**Prometheus Metrics:**
- `sync_current_slot`, `sync_slots_behind` — Sync status gauges
- `consolidations_detected_total`, `proofs_submitted_total`, `proofs_confirmed_total`, `proofs_failed_total` — Consolidation processing counters
- Individual status counters: `consolidations_by_status{status="detected|proof_built|submitted|confirmed|failed"}`

**Components:**
- **Scanner:** Polls beacon chain for new consolidations (deferred to production)
- **Submitter:** Submits claim transactions via alloy (deferred to production)
- **API:** Axum REST server with Prometheus metrics

**Status:** API fully functional (11 tests passing), scanner/submitter scaffolded.

#### `test-vectors` — Test Vector Generator

**Purpose:** Generate JSON test vectors for cross-language validation between Rust proof generation and Solidity verification.

**Output:** `contracts/test-vectors/test_vectors.json` (140KB)
- 4 valid claims (0x01/0x02 credentials, eligible epochs)
- 9 invalid claims (tampered proofs, wrong values, BLS credentials, swapped proofs)

**Why:** Ensures Rust-generated SSZ Merkle proofs verify correctly in Solidity.

#### `integration-tests` — End-to-End Tests

**Purpose:** Cross-validate the entire pipeline from proof generation to Solidity verification expectations.

**Coverage:** 12 tests validating:
- Test vector structure and format
- Proof lengths match gindex depths (29 for consolidations, 53 for validators)
- Hex encoding, credential prefixes, recipient addresses
- Gindex computation consistency
- Eligibility rules
- Invalid claim variety

## Prerequisites

- **Rust:** 1.75+ (`rustup install stable`)
- **Cargo:** Bundled with Rust

## Quick Start

### Build All Crates

```bash
cd prover
cargo build --release
```

### Run Tests

```bash
# All tests
cargo test --workspace

# Specific crate
cargo test -p proof-gen
cargo test -p service
cargo test -p integration-tests

# With output
cargo test --workspace -- --nocapture
```

### Generate Test Vectors

```bash
cargo run --bin generate-test-vectors

# Output: ../contracts/test-vectors/test_vectors.json
```

### Run Proof Service (Mock Mode)

```bash
cd crates/service
cargo run

# Check health
curl http://localhost:3000/health

# View metrics
curl http://localhost:3000/metrics
```

## Configuration

### Presets

The proof service supports two beacon chain presets via cargo features:

**Gnosis (default):**
```bash
cargo build --features gnosis  # or just: cargo build
```
- `PENDING_CONSOLIDATIONS_LIMIT = 2^18 = 262144`
- `VALIDATOR_REGISTRY_LIMIT = 2^40`
- `SLOTS_PER_EPOCH = 16`

**Minimal (for testing):**
```bash
cargo build --features minimal
```
- `PENDING_CONSOLIDATIONS_LIMIT = 2^6 = 64`
- `VALIDATOR_REGISTRY_LIMIT = 2^40`
- `SLOTS_PER_EPOCH = 8`

### Environment Variables (Production)

| Variable | Description | Example |
|----------|-------------|---------|
| `BEACON_URL` | Gnosis beacon API endpoint | `http://65.108.206.150:5052` |
| `RPC_URL` | Gnosis execution RPC | `https://rpc.gnosischain.com` |
| `CONTRACT_ADDRESS` | Deployed ConsolidationIncentives address | `0x...` |
| `PRIVATE_KEY` | Submitter private key | `0x...` |
| `PORT` | API server port | `3000` |

## Development

### Project Structure

```
prover/
├── Cargo.toml                       # Workspace manifest
├── crates/
│   ├── proof-gen/
│   │   ├── src/
│   │   │   ├── lib.rs              # Public API
│   │   │   ├── types.rs            # SSZ beacon state types
│   │   │   ├── sparse_proof.rs     # Low-level sparse Merkle proofs
│   │   │   ├── state_prover.rs     # High-level proof generation
│   │   │   ├── gindex.rs           # Generalized index computation
│   │   │   ├── beacon_client.rs    # Beacon API HTTP client
│   │   │   └── proof.rs            # ConsolidationProofBundle
│   │   └── Cargo.toml
│   ├── service/
│   │   ├── src/
│   │   │   ├── main.rs             # Entry point
│   │   │   ├── api.rs              # Axum REST handlers
│   │   │   ├── state.rs            # Shared AppState
│   │   │   ├── scanner.rs          # Beacon chain scanner (stub)
│   │   │   └── submitter.rs        # Transaction submitter (stub)
│   │   └── Cargo.toml
│   └── test-vectors/
│       ├── src/
│       │   └── main.rs             # Test vector generator
│       └── Cargo.toml
├── tests/
│   └── integration.rs              # Cross-crate integration tests
└── README.md                        # This file
```

### Key Dependencies

| Dependency | Version | Purpose |
|------------|---------|---------|
| `ssz_rs` | git | SSZ serialization + Merkle proof primitives |
| `alloy` | 1.6 | Ethereum/Gnosis chain interactions |
| `axum` | 0.8 | REST API framework |
| `tokio` | 1.45 | Async runtime |
| `reqwest` | 0.12 | HTTP client for beacon API |
| `serde` / `serde_json` | 1.0 | JSON serialization |
| `sha2` | 0.11 | SHA256 for Merkle hashing |
| `tracing` | 0.1 | Structured logging |

### Adding Tests

**Unit tests** go in each module:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_something() {
        // ...
    }
}
```

**Integration tests** go in `tests/`:
```rust
// tests/my_feature.rs
use proof_gen::*;

#[test]
fn test_cross_crate_integration() {
    // ...
}
```

### Debugging

Enable verbose logging:
```bash
RUST_LOG=debug cargo run
```

Pretty-print test output:
```bash
cargo test -- --nocapture --test-threads=1
```

## Production Deployment

### 1. Complete Integration (Deferred Items)

**Scanner** (`scanner.rs`):
- Implement full Electra BeaconState SSZ deserialization
- Poll `/eth/v2/debug/beacon/states/{slot}` for finalized states
- Extract `pending_consolidations` list
- Track processing state

**Submitter** (`submitter.rs`):
- Add alloy contract bindings via `sol!` macro
- Implement `submit_claim()` transaction building
- Gas estimation + nonce management
- Retry logic with exponential backoff

See `../REAL_CHAIN_TESTING.md` for real beacon node testing.

### 2. Build Release Binary

```bash
cargo build --release --features gnosis
# Binary: target/release/service
```

### 3. Deploy Service

**Docker:**
```dockerfile
FROM rust:1.75 as builder
WORKDIR /build
COPY . .
RUN cargo build --release --features gnosis

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /build/target/release/service /usr/local/bin/
ENTRYPOINT ["/usr/local/bin/service"]
```

**Systemd:**
```ini
[Unit]
Description=Consolidation Incentives Proof Service
After=network.target

[Service]
Type=simple
User=prover
Environment="BEACON_URL=http://localhost:5052"
Environment="RPC_URL=https://rpc.gnosischain.com"
Environment="CONTRACT_ADDRESS=0x..."
Environment="PRIVATE_KEY=0x..."
ExecStart=/usr/local/bin/service
Restart=on-failure

[Install]
WantedBy=multi-user.target
```

### 4. Monitoring

**Prometheus scrape config:**
```yaml
scrape_configs:
  - job_name: 'consolidation-prover'
    static_configs:
      - targets: ['localhost:3000']
    metrics_path: '/metrics'
```

**Grafana dashboards:**
- Sync status (slots behind)
- Consolidation processing pipeline (detected → submitted → confirmed)
- Transaction success/failure rates
- Gas costs

### 5. Operational Checklist

- [ ] Beacon node has `/debug/beacon/states` endpoint enabled
- [ ] RPC node is synced and stable
- [ ] Submitter wallet funded with xDAI for gas
- [ ] Contract address and ABI verified
- [ ] Prometheus scraping configured
- [ ] Alerts set up for:
  - Sync degradation (>64 slots behind)
  - Transaction failures
  - Low wallet balance

## Testing Strategy

### Unit Tests (47 in proof-gen)
- SSZ type serialization
- Gindex calculation
- Sparse Merkle proof generation
- StateProver proof composition
- Cross-validation with ssz_rs

### Integration Tests (12)
- Test vector format validation
- Proof length verification
- Cross-language compatibility
- Gindex depth checks

### Cross-Language Validation
1. Rust generates proofs → JSON test vectors
2. Solidity tests load vectors → verify proofs on-chain
3. 22 Solidity integration tests confirm compatibility

### Real Chain Testing (Deferred)
See `../REAL_CHAIN_TESTING.md` for testing against live Gnosis beacon data.

## Troubleshooting

### "ssz_rs not found" error
```bash
# Update Cargo.lock
cargo update
cargo clean
cargo build
```

### Stack overflow on large validator lists
We use sparse Merkle proofs to avoid this. If you see stack issues:
- Check that `StateProver` is being used (not `ssz_rs::Prove` directly)
- Verify the preset feature flag matches your data

### Beacon API 404 errors
- Ensure beacon node is fully synced
- Check `/eth/v2/debug/beacon/states/{slot}` is enabled (requires `--debug-http` flag)
- Try a recent finalized slot: `curl $BEACON_URL/eth/v1/beacon/states/head/finality_checkpoints`

### Transaction submission fails
- Check wallet has xDAI for gas
- Verify RPC URL is correct and synced
- Check contract address matches deployment
- Review gas estimation (may need manual override for complex proofs)

## License

MIT

## Contributing

This is part of the Gnosis Chain consolidation incentives program. For questions or contributions:
- Main repo: `../`
- Smart contracts: `../contracts/`
- Documentation: `../PLAN.md`, `../STATUS.md`
