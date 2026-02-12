# Consolidation Incentives EVM Contract

## Project Overview

Smart contract for Gnosis Chain that incentivizes validator consolidations (EIP-7251). Uses EIP-4788 beacon block roots and SSZ Merkle proofs to verify on-chain that consolidations occurred.

## Key Technical Context

### Gnosis Chain Parameters
- Slot time: 5 seconds (vs Ethereum's 12)
- Slots per epoch: 16 (vs Ethereum's 32)
- Epoch duration: 80 seconds
- EIP-4788 contract: `0x000F3df6D732807Ef1319fB7B8bB8522d0Beac02` (same as Ethereum)

### SSZ Merkle Proof Constants (Electra)
- BeaconState has 37 fields -> tree depth 6 (base GI = 64)
- `validators` field GI in BeaconState: 75 (field index 11)
- `pending_consolidations` field GI in BeaconState: 100 (field index 36)
- `state_root` GI in BeaconBlockHeader: 11 (field index 3, depth 3)
- VALIDATOR_REGISTRY_LIMIT: 2^40 (data tree depth 40)
- PENDING_CONSOLIDATIONS_LIMIT: 2^18 (data tree depth 18)

### Proof Depths from Beacon Block Root
- `state.validators[i].withdrawal_credentials`: 53 sibling hashes
- `state.validators[i].activation_epoch`: 53 sibling hashes
- `state.pending_consolidations[i].source_index`: 29 sibling hashes

## Development

This is a Solidity smart contract project. Detailed research on the beacon state structure is in `docs/gnosis-beacon-state-research.md`.
