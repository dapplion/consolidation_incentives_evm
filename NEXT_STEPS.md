# Next Steps for Production Deployment

**Current Status:** MVP complete — 138 tests passing, all core functionality implemented.

**Blocked on:** Production infrastructure (deployed contract + beacon node debug API access)

---

## Quick Reference

| What | Where | Status |
|------|-------|--------|
| Core contracts | `contracts/src/` | ✅ Complete (68 tests) |
| Proof generation | `prover/crates/proof-gen/` | ✅ Complete (59 tests) |
| REST API | `prover/crates/service/` | ✅ Complete (11 tests) |
| Test vectors | `contracts/test-vectors/` | ✅ Generated |
| Deployment script | `contracts/script/Deploy.s.sol` | ✅ Complete (6 tests) |
| Analytics | `dune/queries/` | ✅ Complete (5 queries) |
| **Scanner** | `prover/crates/service/src/scanner.rs` | 🔸 Stub (needs beacon node) |
| **Submitter** | `prover/crates/service/src/submitter.rs` | 🔸 Stub (needs contract ABI) |
| **Real chain testing** | `prover/crates/real-chain-test/` | 🔸 Blocked (no debug API) |

---

## Phase 1: Chiado Testnet Deployment

**Goal:** Validate the full system on testnet before mainnet.

### 1.1 Deploy Contract

```bash
cd contracts

# Set testnet parameters
export MAX_EPOCH=50000        # Recent testnet epoch
export REWARD_AMOUNT=10000000000000000  # 0.01 xDAI
export MIN_CLAIM_DELAY=160    # 2 epochs (80s each)
export INITIAL_FUNDING=1000000000000000000  # 1 xDAI
export PRIVATE_KEY=$TESTNET_DEPLOYER_KEY

# Deploy
forge script script/Deploy.s.sol \
  --rpc-url https://rpc.chiado.gnosis.gateway.fm \
  --broadcast \
  --verify \
  --etherscan-api-key $GNOSISSCAN_API_KEY

# Save the deployed address
export CONTRACT_ADDRESS=0x...
```

**Output:** `ConsolidationIncentives` proxy deployed and verified on Chiado.

### 1.2 Complete Submitter Integration

Now that we have a deployed contract:

```rust
// In prover/crates/service/src/submitter.rs

// 1. Generate contract bindings
sol! {
    #[sol(rpc)]
    ConsolidationIncentives,
    "../../contracts/out/ConsolidationIncentives.sol/ConsolidationIncentives.json"
}

// 2. Implement submit_claim()
pub async fn submit_claim(&self, proof: ConsolidationProofBundle) -> Result<TxHash> {
    let contract = ConsolidationIncentives::new(self.contract_address, &self.provider);
    
    let tx = contract.claimReward(
        proof.beacon_timestamp,
        proof.consolidation_index,
        proof.source_index,
        proof.activation_epoch,
        proof.source_credentials.into(),
        proof.proof_consolidation.into_iter().map(|p| p.into()).collect(),
        proof.proof_credentials.into_iter().map(|p| p.into()).collect(),
        proof.proof_activation_epoch.into_iter().map(|p| p.into()).collect(),
    );
    
    let pending_tx = tx.send().await?;
    let receipt = pending_tx.get_receipt().await?;
    Ok(receipt.transaction_hash)
}

// 3. Implement is_rewarded()
pub async fn is_rewarded(&self, source_index: u64) -> Result<bool> {
    let contract = ConsolidationIncentives::new(self.contract_address, &self.provider);
    Ok(contract.rewarded(source_index).call().await?._0)
}
```

**Test:**
```bash
cd prover
cargo test --package service -- submitter
```

### 1.3 Set Up Beacon Node Access

**Option A: SSH Tunnel to Gnosis Validator** (fastest)
```bash
# Connect to gnosis-bn-validators (65.108.206.150)
ssh root@65.108.206.150

# Check if debug API is enabled
curl -s http://localhost:5052/eth/v2/debug/beacon/states/finalized | head -c 100

# If enabled, create tunnel:
ssh -L 5052:localhost:5052 root@65.108.206.150

# In another terminal:
export BEACON_API_URL=http://localhost:5052
```

**Option B: Local Gnosis Node** (if debug API not available remotely)
```bash
# Use existing Chiado setup at /data/chiado/
cd /data/chiado

# Check status
docker compose ps

# Ensure beacon node has --debug-level=debug flag
# Access: http://localhost:5052
```

**Test:**
```bash
# Verify debug API access
curl -H "Accept: application/octet-stream" \
  http://localhost:5052/eth/v2/debug/beacon/states/finalized \
  | xxd | head
```

### 1.4 Complete Scanner Implementation

With beacon node access available:

```rust
// In prover/crates/service/src/scanner.rs

async fn scan_finalized_epoch(&self, epoch: u64) -> Result<Vec<PendingConsolidation>> {
    // 1. Get slot for epoch
    let slot = epoch * SLOTS_PER_EPOCH;
    
    // 2. Fetch full state
    let state_bytes = self.beacon_client.get_state_ssz(slot).await?;
    
    // 3. Deserialize (use ssz_rs)
    let state: ElectraBeaconState = SimpleSerialize::deserialize(&state_bytes)?;
    
    // 4. Extract pending_consolidations
    Ok(state.pending_consolidations.iter().cloned().collect())
}

async fn process_consolidation(&self, consolidation: &PendingConsolidation, epoch: u64) -> Result<()> {
    let source_index = consolidation.source_index;
    
    // Check if already rewarded
    if self.submitter.is_rewarded(source_index).await? {
        info!("Validator {} already rewarded", source_index);
        return Ok(());
    }
    
    // Generate proof
    let proof = self.proof_generator.generate_proof(&state, consolidation_index)?;
    
    // Submit claim
    let tx_hash = self.submitter.submit_claim(proof).await?;
    info!("Submitted claim for validator {} (tx: {})", source_index, tx_hash);
    
    Ok(())
}
```

**Test:**
```bash
cd prover
cargo test --package service -- scanner
```

### 1.5 Real Chain Testing

With both contract deployed and beacon access:

```bash
cd prover
cargo run --bin real-chain-test -- \
  --beacon-url http://localhost:5052 \
  --contract-address $CONTRACT_ADDRESS \
  --rpc-url https://rpc.chiado.gnosis.gateway.fm \
  --epoch finalized
```

**This will:**
1. Fetch finalized state from beacon node
2. Extract pending_consolidations
3. Generate proofs for each consolidation
4. Submit claims to the testnet contract
5. Verify rewards were paid

### 1.6 Run Full Service

```bash
cd prover
cargo build --release

./target/release/service \
  --beacon-url http://localhost:5052 \
  --contract-address $CONTRACT_ADDRESS \
  --rpc-url https://rpc.chiado.gnosis.gateway.fm \
  --private-key $SUBMITTER_PRIVATE_KEY \
  --bind 0.0.0.0:8080
```

**Monitor:**
- Logs: `journalctl -u consolidation-incentives -f`
- Metrics: `curl http://localhost:8080/metrics`
- Status: `curl http://localhost:8080/status`
- Consolidations: `curl http://localhost:8080/consolidations`

---

## Phase 2: Mainnet Deployment

Once Chiado is working perfectly:

### 2.1 Determine Parameters

**Critical decisions:**

1. **`MAX_EPOCH`** — Snapshot epoch before public announcement
   - Check current finalized epoch: `curl https://rpc.gnosischain.com/... | jq .finalized_epoch`
   - Subtract safety margin (e.g., -100 epochs = ~2 hours)
   - **This locks eligibility** — must be set before any public discussion

2. **`rewardAmount`** — Budget per consolidation
   - Estimate eligible validators: `SELECT COUNT(*) FROM validators WHERE activation_epoch < MAX_EPOCH`
   - Conservative budget: `rewardAmount * estimated_count * 1.2`
   - Example: 1000 validators × 0.01 xDAI × 1.2 = 12 xDAI

3. **`minClaimDelay`** — Finality safety
   - Gnosis finality: ~2 epochs = 160 seconds
   - Recommended: 320 seconds (4 epochs) for safety

### 2.2 Deploy via Multisig

**DO NOT use single EOA for mainnet!**

```bash
# Generate deployment data (don't broadcast yet)
forge script script/Deploy.s.sol \
  --rpc-url https://rpc.gnosischain.com \
  --private-key $DEPLOYER_KEY \
  > deployment_data.txt

# Submit to Gnosis Safe for approval
# Use Safe UI or CLI to propose the transaction
```

### 2.3 Verify Deployment

```bash
# Check proxy
cast call $CONTRACT_ADDRESS "maxEpoch()" --rpc-url https://rpc.gnosischain.com
cast call $CONTRACT_ADDRESS "rewardAmount()" --rpc-url https://rpc.gnosischain.com

# Check implementation
cast call $CONTRACT_ADDRESS "implementation()" --rpc-url https://rpc.gnosischain.com

# Fund contract
cast send $CONTRACT_ADDRESS --value 12ether --private-key $FUNDING_KEY
```

### 2.4 Submit to Dune

1. Go to https://dune.com/contracts/new
2. Submit contract address: `$CONTRACT_ADDRESS`
3. Network: Gnosis Chain
4. Wait for decoding (~24 hours)
5. Upload queries from `dune/queries/`
6. Create dashboard (see `dune/README.md`)

### 2.5 Deploy Proof Service

**Production setup (systemd + monitoring):**

```bash
# Build release binary
cd prover
cargo build --release

# Install binary
sudo cp target/release/service /usr/local/bin/consolidation-incentives-service

# Create service user
sudo useradd -r -s /bin/false consolidation-incentives

# Create systemd unit
sudo tee /etc/systemd/system/consolidation-incentives.service << EOF
[Unit]
Description=Consolidation Incentives Proof Service
After=network.target

[Service]
Type=simple
User=consolidation-incentives
Environment="BEACON_API_URL=http://localhost:5052"
Environment="CONTRACT_ADDRESS=$CONTRACT_ADDRESS"
Environment="RPC_URL=https://rpc.gnosischain.com"
Environment="PRIVATE_KEY_FILE=/etc/consolidation-incentives/key"
ExecStart=/usr/local/bin/consolidation-incentives-service \
  --beacon-url \$BEACON_API_URL \
  --contract-address \$CONTRACT_ADDRESS \
  --rpc-url \$RPC_URL \
  --private-key \$(cat \$PRIVATE_KEY_FILE) \
  --bind 0.0.0.0:8080
Restart=always
RestartSec=10

[Install]
WantedBy=multi-user.target
EOF

# Start service
sudo systemctl daemon-reload
sudo systemctl enable consolidation-incentives
sudo systemctl start consolidation-incentives

# Check status
sudo systemctl status consolidation-incentives
```

**Monitoring (Prometheus + Grafana):**

```yaml
# Add to prometheus.yml
scrape_configs:
  - job_name: 'consolidation-incentives'
    static_configs:
      - targets: ['localhost:8080']
```

### 2.6 Announce Program

Only after everything is deployed and running:

1. Write announcement post explaining the program
2. Include contract address and eligibility snapshot (MAX_EPOCH)
3. Link to dashboard and documentation
4. Post to Gnosis forums, Discord, Twitter

---

## Troubleshooting

### "Stack too deep" during contract compilation

**Already fixed** — `foundry.toml` has `via_ir = true`

### "Debug API not available" from beacon node

Check beacon node startup flags:
- Lighthouse: `--debug-level=debug`
- Nimbus: `--rest-api-debug=true`
- Prysm: `--enable-debug-rpc-endpoints`

### Proof verification fails on-chain

1. Check gindex constants match:
   ```bash
   cd prover
   cargo test gindex_computation
   ```

2. Verify proof lengths:
   ```bash
   # Should be 29, 53, 53
   echo $PROOF_CONSOLIDATION_LENGTH
   ```

3. Test with known-good test vectors:
   ```bash
   cd contracts
   forge test --match-test test_validClaim
   ```

### Gas costs too high

Current estimates (from tests):
- Deployment: ~3M gas
- claimReward(): ~200k gas

If too expensive, check:
- Solidity optimizer runs (currently 200)
- Proof verification loop unrolling

### Service crashes / restarts frequently

Check logs:
```bash
journalctl -u consolidation-incentives -n 100
```

Common causes:
- Beacon node connectivity issues
- RPC rate limiting
- Insufficient private key balance for gas

---

## Reference

| Document | Purpose |
|----------|---------|
| `PLAN.md` | Original 19-step implementation plan |
| `STATUS.md` | Current completion status |
| `contracts/DEPLOYMENT.md` | Detailed contract deployment guide |
| `REAL_CHAIN_TESTING.md` | Real chain testing options |
| `dune/README.md` | Analytics dashboard setup |
| `prover/README.md` | Proof service architecture |

---

**When ready to proceed:** Start with Phase 1 (Chiado testnet deployment) and work through each step methodically. The MVP is solid — production deployment is just infrastructure hookup.
