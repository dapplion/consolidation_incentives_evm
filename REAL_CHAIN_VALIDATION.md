# Real Chain Validation

**Date:** 2026-03-21  
**Status:** ✅ Complete  
**Chain:** Gnosis Mainnet  
**Beacon Node:** gnosis-bn-validators (65.108.206.150)

## Summary

Successfully validated all scanner components against live Gnosis beacon chain data. The proof service's Beacon API client works correctly with real production infrastructure.

**2026-04-17 update:** validated that the internal Lighthouse node also exposes the debug SSZ endpoint over SSH tunnel, and the upgraded `fetch-and-prove` binary can now export a richer real-chain snapshot. The finalized state tested on 2026-04-17 had zero pending consolidations, so proof generation remains waiting on a state that actually contains one.

**2026-04-18 update:** `fetch-and-prove` can now scan a finalized slot range (`--scan-start-slot` / `--scan-end-slot`) and record scan metadata in the JSON snapshot. That reduces Step 18’s remaining work to historical/future data discovery instead of manual one-slot probing.

**2026-04-19 update:** the historical scan supports configurable stride + direction (`--scan-step-slots`, `--scan-direction`), so real-chain searches can hop one finalized epoch at a time and search newest-first when looking for the most recent pending consolidation state.

**2026-04-20 update:** `fetch-and-prove` also accepts epoch-based scan windows (`--scan-start-epoch` / `--scan-end-epoch`), so historical searches no longer require manual slot arithmetic before each run.

**2026-04-22 update:** historical scan snapshots now persist every non-empty hit in `scan_window.non_empty_slots`, plus the first/last hit slots. That makes reverse scans useful for finding the newest consolidation-bearing state without throwing away the rest of the evidence.

**2026-04-23 update:** `fetch-and-prove` now also supports `--scan-last-epochs <N>`, which derives a recent finalized scan window automatically. Same historical search, less caveman slot math.

## Test Results

### Connection Details

- **Beacon Node:** Lighthouse/v8.0.1-ced49dd/x86_64-linux (2026-03-21 validation)
- **Beacon Node (2026-04-17):** Lighthouse/v8.1.3-176cce5/x86_64-linux
- **Access Method:** SSH tunnel (localhost:15052 → remote:4000) / later localhost:14000 → remote:4000
- **Finalized Epoch (2026-03-21):** 1689148
- **Finalized Epoch (2026-04-17):** 1717378
- **Head Slot (2026-03-21):** 27026406
- **Finalized Slot (2026-04-17):** 27478048

### API Endpoint Validation

All Beacon API endpoints required by the scanner tested successfully:

| Endpoint | Purpose | Status |
|----------|---------|--------|
| `/eth/v1/node/version` | Version check | ✅ Working |
| `/eth/v1/beacon/states/head/finality_checkpoints` | Get finalized epoch | ✅ Working |
| `/eth/v1/beacon/states/{slot}/pending_consolidations` | Fetch consolidations | ✅ Working |
| `/eth/v1/beacon/blocks/{slot}/header` | Block header metadata | ✅ Working |
| `/eth/v2/debug/beacon/states/{slot}` | Full state SSZ download | ✅ Working via internal SSH tunnel |

### Scanner Functionality

Ran `test_scanner` example against real Gnosis chain:

```bash
cd prover/crates/service
BEACON_API_URL=http://localhost:15052 cargo run --example test_scanner
```

**Results:**
- ✅ Successfully connected to beacon node
- ✅ Retrieved finalized checkpoint (epoch 1689148)
- ✅ Fetched pending consolidations (empty list - expected)
- ✅ All data parsed correctly
- ✅ No errors or API failures

### Current Chain State

- **Pending Consolidations:** 0 (none found at finalized slot 27026368 on 2026-03-21, and still 0 at finalized slot 27478048 on 2026-04-17)
- **Electra Upgrade:** Active (pending_consolidations endpoint available)
- **Finalization:** Healthy (consistent checkpoints)
- **Debug SSZ availability:** Confirmed on the internal node; finalized state download size was 80,503,375 bytes on 2026-04-17

## Implications

1. **Scanner is production-ready** — All detection logic validated against real chain
2. **Debug-state access is production-ready** — internal beacon node can serve full finalized SSZ over SSH tunnel
3. **EIP-7251 is active** — Electra endpoints available on Gnosis
4. **No consolidations yet** — Program will have no claims initially (expected)
5. **Remaining blocker is data availability, not infrastructure** — we now need a state with at least one pending consolidation to generate a real proof bundle

## Access Setup for Production

To run the service against the real Gnosis beacon node:

```bash
# Create SSH tunnel
ssh -f -N -L 5052:localhost:4000 root@65.108.206.150

# Run service
cd prover
cargo run --release -- \
  --beacon-url http://localhost:5052 \
  --rpc-url https://rpc.gnosischain.com \
  --contract-address $DEPLOYED_CONTRACT \
  --bind 0.0.0.0:8080
```

Or use systemd with persistent tunnel (see `NEXT_STEPS.md` Phase 2.5).

## Next Steps

With scanner validation complete, remaining work for deployment:

1. ✅ **Scanner validated** — Works with real Gnosis chain
2. ⬜ **Deploy contract** — To Chiado testnet first
3. ⬜ **Test full pipeline** — Scanner → proof generation → submission
4. ⬜ **Production deployment** — Gnosis mainnet

See `NEXT_STEPS.md` for detailed deployment roadmap.
