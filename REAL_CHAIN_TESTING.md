# Real Chain Testing Status

## Step 18: Real Gnosis Chain Proof Generation

### Progress (2026-02-13 → 2026-04-17)

**✅ Confirmed Working:**
- Connection to Gnosis public beacon endpoint: `https://rpc.gnosischain.com/beacon`
- Finality checkpoint fetching works against both public and internal nodes
- Block header fetching works against both public and internal nodes
- SSH access to internal beacon host `gnosis-bn-validators` (`65.108.206.150`) works
- Internal Lighthouse beacon API is available on `127.0.0.1:4000`
- Full beacon state SSZ fetching works via SSH tunnel to the internal node:
  ```bash
  ssh -o BatchMode=yes -N -L 14000:127.0.0.1:4000 root@65.108.206.150
  GNOSIS_BEACON_URL=http://127.0.0.1:14000 cargo run -p real-chain-test -- --state-id finalized
  ```
- Verified finalized-state debug SSZ download at slot `27478048` (`80,503,375` bytes)
- `fetch-and-prove` now produces a richer JSON snapshot with:
  - resolved state/slot metadata
  - pending consolidation count
  - validator activation epoch + withdrawal credential summaries
  - debug endpoint availability notes
  - optional historical scan metadata (`--scan-start-slot` / `--scan-end-slot`, `--scan-start-epoch` / `--scan-end-epoch`, or `--scan-last-epochs <N>`)
  - configurable scan stride + direction (`--scan-step-slots`, `--scan-direction`) so it can sweep finalized history efficiently and either stop at the earliest or latest non-empty `pending_consolidations` state
  - optional early-stop hit limit (`--scan-hit-limit <N>`) so long archaeology runs can stop once enough non-empty states have been collected
  - a `non_empty_slots` summary in the scan window so historical runs record every hit that was observed, not just the first one
  - epoch metadata (`start_epoch`, `end_epoch`, per-hit `epoch`, first/last non-empty epochs) so scan snapshots line up with beacon-history discussions without manual slot→epoch conversion
  - a live finalized-state watcher (`--watch-finalized`, with `--watch-poll-seconds` / `--watch-max-polls`) for catching the first non-empty state before historical retention eats the evidence
  - watch-mode deduplication: if finalized head has not advanced yet, the watcher skips the redundant `pending_consolidations` fetch instead of hammering the same finalized slot over and over
  - optional progress snapshots during `--watch-finalized` via `--watch-progress-output <file>`, so long-running live capture sessions leave inspectable JSON breadcrumbs after every poll instead of being silent until exit
  - optional append-only watch event logs via `--watch-event-log-output <file>`, so long-running capture sessions keep a poll-by-poll JSONL trail instead of overwriting history with a single latest snapshot
  - explicit watch progress state in those JSON snapshots (`status: polling|found_non_empty_state|max_polls_reached|error` plus `terminal: bool`), so cron/sidecar monitors can tell whether the watcher is still waiting, finished cleanly, or died noisily without reverse-engineering the counters
  - timestamped watch snapshots (`updated_at_unix`, `updated_at_rfc3339`) plus the current `finalized_root`, so external monitors can tell exactly when a progress file was refreshed and which finalized checkpoint it represents

**🔸 Still Blocked / Deferred:**
- The currently finalized real-chain state has **0 pending consolidations**, so there is nothing real to prove yet
- Historical archaeology against the internal Lighthouse node is more limited than it first looked: older finalized-slot headers remain queryable, but older finalized-slot **states** for `pending_consolidations` return 404. So the node retains enough history for block metadata, not enough for arbitrary historical state scans.
- Step 18's original “generate proofs for actual consolidations” sub-goal therefore still requires either:
  - a beacon node that retains historical states, or
  - capturing the needed state live when pending consolidations actually exist (the new watcher mode is built for exactly this)
- Step 19 still depends on obtaining at least one real consolidation proof bundle

**Options to Unblock:**

1. **Access internal beacon node via SSH tunnel:**
   ```bash
   ssh -L 14000:127.0.0.1:4000 root@65.108.206.150
   GNOSIS_BEACON_URL=http://127.0.0.1:14000 cargo run -p real-chain-test -- --state-id finalized
   ```

2. **Run local Gnosis beacon node:**
   - Requires syncing full Gnosis chain (time-intensive)
   - Would enable unlimited state access for testing

3. **Wait for consolidations on testnet:**
   - Chiado testnet might be more accessible
   - Could generate proofs from Chiado first, then validate mainnet compatibility

4. **Skip full state testing for now:**
   - Current synthetic test vectors (from `test-vectors` binary) are sufficient for contract validation
   - All 62 Solidity tests passing with synthetic SSZ proofs
   - Can defer real chain testing to deployment phase

### Recommendation

The SSH-tunneled internal node removes the old debug-endpoint blocker, so the remaining blocker is now purely **chain state availability**:
- Contract is already fully tested with synthetic but valid SSZ proofs
- Proof generation logic is validated via cross-checks against `ssz_rs` library
- Real proof generation can proceed as soon as we have a state with at least one pending consolidation
- Until then, deployment work can continue and Step 19 can stay staged behind that missing real proof bundle

### Created Artifacts

- `prover/crates/real-chain-test/`: Binary for fetching real Gnosis beacon data
- Successfully compiled and tested against public endpoints
- Ready to use once debug API access is available

## Step 19: Local Devnet Validation

**Status:** Deferred until deployment

This step requires:
1. Deployed contract (can use Foundry's Anvil fork)
2. Real beacon state data (currently blocked by Step 18)
3. Mock EIP-4788 oracle with real block roots

**Alternative approach:**
- Deploy to Chiado testnet first
- Use Chiado's smaller state for easier testing
- Validate end-to-end flow before mainnet deployment

## Next Steps for Production

1. Keep SSH tunnel workflow for internal beacon node access
2. Use `fetch-and-prove --scan-last-epochs <N> --scan-step-slots 16 --scan-direction reverse --scan-hit-limit <N>` against the internal node for a quick recent-history sweep, or fall back to `--scan-start-epoch <epoch> --scan-end-epoch <epoch>` / slot flags when you need a precise archaeology window. The emitted scan window now includes both slot and epoch breadcrumbs (`start_epoch`, `end_epoch`, `first_non_empty_epoch`, `last_non_empty_epoch`, plus per-hit epochs), and each hit records both the original `requested_slot` and the resolved `slot` used after missed-slot fallback.
3. When archaeology fails because history is pruned, switch to live capture instead of arguing with the node: `fetch-and-prove --state-id finalized --watch-finalized --watch-poll-seconds 80` (optionally add `--watch-max-polls <N>` if you want it to stop on its own). The watcher now skips duplicate finalized slots automatically, so shorter poll cadences no longer spam redundant state requests while finality is unchanged.
4. If you want observability during a long watch, add `--watch-progress-output /tmp/real-chain-watch.json` and tail that file from cron / another shell. It records refresh timestamps (`updated_at_unix`, `updated_at_rfc3339`), poll count, state checks, skipped duplicate-finality polls, the latest finalized slot+epoch+root, the current pending-consolidation count after each poll, and an explicit `status`/`terminal` pair plus optional `error` text so downstream automation knows whether the watcher is still polling, exited cleanly, or faceplanted on an API failure.
5. If you want a full audit trail instead of just the latest state, also add `--watch-event-log-output /tmp/real-chain-watch.jsonl`. Each poll/exit appends one JSON object per line using the same schema as the progress snapshot, which is much nicer for postmortems and dumb little shell parsers. Do not point it at the same file as `--watch-progress-output` unless you enjoy corrupting both views at once.
6. If the scan fails with `beacon header exists ... but beacon state is unavailable`, stop blaming the scanner — that's the node refusing historical state lookups, not a missed-slot issue. Use a non-pruning node or wait to capture the state live.
7. Generate a real proof bundle once such a state is found
8. Deploy contract to local Anvil fork with real beacon roots
9. Submit claims with real proofs to verify end-to-end flow
10. Deploy to Chiado testnet for live testing
11. Deploy to Gnosis mainnet
