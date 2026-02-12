# Consolidation Incentives EVM

One-time consolidation incentives for Gnosis Chain validators — entirely on-chain, no consensus changes required.

## Overview

This system incentivizes validator consolidations (EIP-7251) by paying rewards to validators who consolidate. It verifies consolidations on-chain using:

- **EIP-4788** — Beacon block roots accessible from the EVM
- **SSZ Merkle proofs** — Cryptographic proof that a consolidation occurred

```
┌─────────────────────────────────────────────────────────────────┐
│                        Beacon Chain                             │
│  ┌─────────────┐    ┌─────────────┐    ┌─────────────────────┐ │
│  │  Validator  │───▶│Consolidation│───▶│ pending_consolidations│ │
│  │   (source)  │    │   Request   │    │     [i].source_index │ │
│  └─────────────┘    └─────────────┘    └──────────┬────────────┘ │
└──────────────────────────────────────────────────┼──────────────┘
                                                   │
                        EIP-4788                   │ Merkle Proof
                    ┌──────────────┐               │
                    │ Beacon Root  │◀──────────────┘
                    │   Oracle     │
                    └──────┬───────┘
                           │
┌──────────────────────────┼──────────────────────────────────────┐
│                          ▼              Gnosis Chain (EVM)      │
│  ┌─────────────────────────────────────────────────────────────┐│
│  │              ConsolidationIncentives.sol                    ││
│  │  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐ ││
│  │  │Verify Proofs│  │Check Epoch  │  │  Pay Reward to      │ ││
│  │  │ (3 proofs)  │─▶│ < MAX_EPOCH │─▶│  withdrawal_creds   │ ││
│  │  └─────────────┘  └─────────────┘  └─────────────────────┘ ││
│  └─────────────────────────────────────────────────────────────┘│
└─────────────────────────────────────────────────────────────────┘
```

## Why?

A consolidated validator set benefits the chain by:
- Reducing resource consumption
- Improving scalability  
- Enabling more expensive designs (e.g., SSF)

This is a **one-time cleanup** incentive, not permanent behavioral enforcement. Eligibility is locked to a pre-announcement snapshot epoch to prevent gaming.

## Components

### Smart Contracts (`contracts/`)

Foundry project with:

| Contract | Description |
|----------|-------------|
| `ConsolidationIncentives.sol` | Main contract — verifies proofs, pays rewards |
| `SSZMerkleVerifier.sol` | Library for SSZ Merkle proof verification |

**Key function:**
```solidity
function claimReward(
    uint64 beaconTimestamp,      // EIP-4788 lookup key
    uint64 consolidationIndex,   // Index in pending_consolidations
    uint64 sourceIndex,          // Validator index (source)
    uint64 activationEpoch,      // Must be < MAX_EPOCH
    bytes32 sourceCredentials,   // Withdrawal credentials
    bytes32[] proofConsolidation,   // 29 siblings
    bytes32[] proofCredentials,     // 53 siblings  
    bytes32[] proofActivationEpoch  // 53 siblings
) external
```

### Proof Service (`prover/`)

Rust workspace with:

| Crate | Description |
|-------|-------------|
| `proof-gen` | Core logic — SSZ types, gindex computation, proof generation |
| `service` | REST API + auto-submitter for permissionless claiming |
| `test-vectors` | Generates JSON test vectors for cross-validation |

**REST API:**
- `GET /health` — Health check
- `GET /status` — Sync status
- `GET /consolidations` — List detected consolidations
- `GET /metrics` — Prometheus metrics

## Quick Start

### Prerequisites

- [Foundry](https://book.getfoundry.sh/getting-started/installation)
- [Rust](https://rustup.rs/) (1.75+)

### Build & Test

```bash
# Contracts
cd contracts
forge build
forge test

# Prover
cd prover
cargo build
cargo test
```

### Deploy (Testnet)

```bash
cd contracts
forge script script/Deploy.s.sol --rpc-url $GNOSIS_RPC --broadcast
```

## Security Model

| Attack Vector | Defense |
|--------------|---------|
| Double-claiming | `rewarded[sourceIndex]` mapping + consensus-level exit_epoch invariant |
| Sybil farming | Eligibility locked to pre-announcement `MAX_EPOCH` |
| Reward theft | Payout derived from proven `withdrawal_credentials` |
| Reorg exploits | Require beacon timestamp sufficiently in the past |

## Technical Details

### Gnosis Chain Parameters

| Parameter | Value |
|-----------|-------|
| Slot time | 5 seconds |
| Slots per epoch | 16 |
| Epoch duration | 80 seconds |
| EIP-4788 Oracle | `0x000F3df6D732807Ef1319fB7B8bB8522d0Beac02` |

### Proof Depths (Electra)

| Proof Target | Siblings |
|-------------|----------|
| `pending_consolidations[i].source_index` | 29 |
| `validators[i].withdrawal_credentials` | 53 |
| `validators[i].activation_epoch` | 53 |

## Configuration

| Parameter | Description | Default |
|-----------|-------------|---------|
| `MAX_EPOCH` | Eligibility cutoff (set before announcement) | TBD |
| `rewardAmount` | xDAI per consolidation | 0.01 |
| `minClaimDelay` | Seconds before beacon timestamp is usable | ~2 epochs |

## License

MIT

## References

- [EIP-7251: Increase MAX_EFFECTIVE_BALANCE](https://eips.ethereum.org/EIPS/eip-7251)
- [EIP-4788: Beacon block root in the EVM](https://eips.ethereum.org/EIPS/eip-4788)
- [Research spec](https://github.com/dapplion/research/blob/main/consolidation_incentives_evm.md)
