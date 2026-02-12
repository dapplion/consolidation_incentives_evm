# Gnosis Chain Beacon State Structure for Consolidation Incentives EVM Contract

## Research Findings

---

## 1. EIP-4788 on Gnosis Chain

**Yes, Gnosis Chain supports EIP-4788.** It was activated as part of the Dencun hardfork.

- **Contract address**: `0x000F3df6D732807Ef1319fB7B8bB8522d0Beac02` -- **same as Ethereum mainnet**
- **Verified on-chain**: https://gnosis.blockscout.com/address/0x000F3df6D732807Ef1319fB7B8bB8522d0Beac02
- **Not a precompile** -- it is a system-level "predeploy" smart contract written in EVM assembly
- **HISTORY_BUFFER_LENGTH**: 8191 (same as Ethereum)
- **Mechanism**: At the start of each execution block, the system calls the contract as `SYSTEM_ADDRESS` with the 32-byte `header.parent_beacon_block_root`. You query it by calling the contract with a timestamp to get back the corresponding beacon block root.

**Key point**: The contract stores **parent beacon block roots** (not state roots). To prove beacon state data, you must first prove the `state_root` field within the `BeaconBlockHeader`, then navigate the state tree.

---

## 2. Gnosis Chain Beacon State SSZ Layout (Electra / Pectra)

### 2a. Electra BeaconState Container (37 fields, 0-indexed)

The Gnosis Chain Pectra/Electra fork is live. The BeaconState container is **identical in structure** to Ethereum's -- only configuration parameter values differ.

```
Index  Field                              Type
-----  -----                              ----
0      genesis_time                       uint64
1      genesis_validators_root            Root
2      slot                               Slot
3      fork                               Fork
4      latest_block_header                BeaconBlockHeader
5      block_roots                        Vector[Root, SLOTS_PER_HISTORICAL_ROOT]
6      state_roots                        Vector[Root, SLOTS_PER_HISTORICAL_ROOT]
7      historical_roots                   List[Root, HISTORICAL_ROOTS_LIMIT]
8      eth1_data                          Eth1Data
9      eth1_data_votes                    List[Eth1Data, EPOCHS_PER_ETH1_VOTING_PERIOD * SLOTS_PER_EPOCH]
10     eth1_deposit_index                 uint64
11     validators                         List[Validator, VALIDATOR_REGISTRY_LIMIT]
12     balances                           List[Gwei, VALIDATOR_REGISTRY_LIMIT]
13     randao_mixes                       Vector[Bytes32, EPOCHS_PER_HISTORICAL_VECTOR]
14     slashings                          Vector[Gwei, EPOCHS_PER_SLASHINGS_VECTOR]
15     previous_epoch_participation       List[ParticipationFlags, VALIDATOR_REGISTRY_LIMIT]
16     current_epoch_participation        List[ParticipationFlags, VALIDATOR_REGISTRY_LIMIT]
17     justification_bits                 Bitvector[JUSTIFICATION_BITS_LENGTH]
18     previous_justified_checkpoint      Checkpoint
19     current_justified_checkpoint       Checkpoint
20     finalized_checkpoint               Checkpoint
21     inactivity_scores                  List[uint64, VALIDATOR_REGISTRY_LIMIT]
22     current_sync_committee             SyncCommittee
23     next_sync_committee                SyncCommittee
24     latest_execution_payload_header    ExecutionPayloadHeader
25     next_withdrawal_index              WithdrawalIndex
26     next_withdrawal_validator_index    ValidatorIndex
27     historical_summaries               List[HistoricalSummary, HISTORICAL_ROOTS_LIMIT]
--- Electra additions ---
28     deposit_requests_start_index       uint64
29     deposit_balance_to_consume         Gwei
30     exit_balance_to_consume            Gwei
31     earliest_exit_epoch                Epoch
32     consolidation_balance_to_consume   Gwei
33     earliest_consolidation_epoch       Epoch
34     pending_deposits                   List[PendingDeposit, PENDING_DEPOSITS_LIMIT]
35     pending_partial_withdrawals        List[PendingPartialWithdrawal, PENDING_PARTIAL_WITHDRAWALS_LIMIT]
36     pending_consolidations             List[PendingConsolidation, PENDING_CONSOLIDATIONS_LIMIT]
```

**Total: 37 fields** => next power of 2 = 64 => **tree depth = 6** (ceil(log2(37)) = 6)

### 2b. BeaconBlockHeader Container (5 fields)

```
Index  Field              Type
-----  -----              ----
0      slot               Slot
1      proposer_index     ValidatorIndex
2      parent_root        Root
3      state_root         Root
4      body_root          Root
```

**5 fields** => next power of 2 = 8 => **tree depth = 3**

**Critical property**: `hash_tree_root(BeaconBlock) == hash_tree_root(BeaconBlockHeader)` because the `body` field of `BeaconBlock` Merkleizes to the same root as `body_root`.

### 2c. Validator Container (8 fields)

```
Index  Field                        Type
-----  -----                        ----
0      pubkey                       BLSPubkey
1      withdrawal_credentials       Bytes32
2      effective_balance             Gwei
3      slashed                      boolean
4      activation_eligibility_epoch  Epoch
5      activation_epoch             Epoch
6      exit_epoch                   Epoch
7      withdrawable_epoch           Epoch
```

**8 fields** => next power of 2 = 8 => **tree depth = 3**

### 2d. PendingConsolidation Container (2 fields)

```
Index  Field          Type
-----  -----          ----
0      source_index   ValidatorIndex
1      target_index   ValidatorIndex
```

**2 fields** => next power of 2 = 2 => **tree depth = 1**

---

## 3. Generalized Index Computation

### 3a. Formula

For a Container with N fields, field at position p has generalized index:
```
GI = get_power_of_two_ceil(N) + p
```

For a List[T, LIMIT], the data subtree root is at child index 0 (left child), and the length mix-in is at child index 1 (right child). So for the data portion:
```
GI_data = GI_list * 2      (left child = data)
GI_len  = GI_list * 2 + 1  (right child = length)
```

The data subtree for `List[T, LIMIT]` has depth `ceil(log2(LIMIT))` if T is a basic type or fixed-size container that fits in one chunk, or `ceil(log2(LIMIT * chunks_per_element))` in general.

To concatenate generalized indices (compose paths):
```
concat_generalized_indices(i1, i2):
  return i1 * get_previous_power_of_two(i2) + (i2 - get_previous_power_of_two(i2))
```
This effectively concatenates the binary representations (stripping the leading 1 sentinel bit of i2).

### 3b. Key Generalized Indices for Electra BeaconState

**BeaconState has 37 fields => depth 6 => base GI = 64**

| Field | Index | GI in BeaconState |
|-------|-------|-------------------|
| `validators` | 11 | 64 + 11 = **75** |
| `balances` | 12 | 64 + 12 = **76** |
| `pending_consolidations` | 36 | 64 + 36 = **100** |
| `finalized_checkpoint` | 20 | 64 + 20 = **84** |

### 3c. Navigating from Beacon Block Root to State Root

The beacon block root = `hash_tree_root(BeaconBlockHeader)`.

**BeaconBlockHeader has 5 fields => depth 3 => base GI = 8**

`state_root` is at index 3 in BeaconBlockHeader:
```
GI_state_root_in_block = 8 + 3 = 11
```

So from the beacon block root, `state_root` has **generalized index 11**.

### 3d. Full Path: `state.validators[i].withdrawal_credentials`

Step by step:

1. **Beacon block root -> state_root**: GI = 11 (in BeaconBlockHeader)
2. **state_root -> validators list root**: GI = 75 (field 11 in Electra BeaconState)
3. **validators list root -> data subtree**: GI = 2 (left child of list node; right child = length)
4. **data subtree -> validator[i]**:
   - VALIDATOR_REGISTRY_LIMIT = 2^40
   - For a List of Containers, each Validator is a single subtree node at depth ceil(log2(2^40)) = 40
   - GI within data subtree = 2^40 + i
5. **validator[i] -> withdrawal_credentials**:
   - Validator has 8 fields => depth 3 => base GI = 8
   - withdrawal_credentials is field 1: GI = 8 + 1 = 9

**Composed full generalized index for `state.validators[i].withdrawal_credentials`**:
```
concat(11, 75, 2, 2^40 + i, 9)
```

Proof depth from beacon block root:
- 3 (block header) + 6 (beacon state) + 1 (list data/len) + 40 (validator registry) + 3 (validator fields) = **53 siblings** in the Merkle proof

### 3e. Full Path: `state.validators[i].activation_epoch`

Same as above but `activation_epoch` is field 5 in Validator:
```
GI within Validator = 8 + 5 = 13
```

Composed: `concat(11, 75, 2, 2^40 + i, 13)`

Proof depth: same **53 siblings**

### 3f. Full Path: `state.pending_consolidations[i].source_index`

1. **Beacon block root -> state_root**: GI = 11
2. **state_root -> pending_consolidations list root**: GI = 100 (field 36 in Electra BeaconState)
3. **pending_consolidations list root -> data subtree**: GI = 2 (left child)
4. **data subtree -> pending_consolidation[i]**:
   - PENDING_CONSOLIDATIONS_LIMIT = 2^18 = 262,144
   - PendingConsolidation is a Container. Its `hash_tree_root` produces a single 32-byte root that occupies one leaf in the list data tree.
   - Data tree depth = ceil(log2(262144)) = 18
   - GI within data subtree = 2^18 + i
5. **pending_consolidation[i] -> source_index**:
   - PendingConsolidation has 2 fields => depth 1
   - source_index is field 0: GI = 2 + 0 = 2 (left child)

Composed: `concat(11, 100, 2, 2^18 + i, 2)`

Proof depth from beacon block root:
- 3 (block header) + 6 (beacon state) + 1 (list data/len) + 18 (consolidations list) + 1 (consolidation fields) = **29 siblings**

---

## 4. Tree Depths Summary

### Validators List (`List[Validator, VALIDATOR_REGISTRY_LIMIT]`)
- VALIDATOR_REGISTRY_LIMIT = 2^40 = 1,099,511,627,776 (same on both Gnosis and Ethereum)
- Data tree depth = ceil(log2(2^40)) = **40**
- +1 for the list length mix-in node
- Total subtree depth from list root = **41**

### Pending Consolidations List (`List[PendingConsolidation, PENDING_CONSOLIDATIONS_LIMIT]`)
- PENDING_CONSOLIDATIONS_LIMIT = 2^18 = 262,144 (same on both Gnosis and Ethereum)
- Data tree depth = ceil(log2(2^18)) = **18**
- +1 for the list length mix-in node
- Total subtree depth from list root = **19**

### Validator Container Subtree
- 8 fields => tree depth = **3**

### PendingConsolidation Container Subtree
- 2 fields => tree depth = **1**

---

## 5. Gnosis-Specific Parameters

### Timing Parameters

| Parameter | Gnosis | Ethereum | Notes |
|-----------|--------|----------|-------|
| `SECONDS_PER_SLOT` | **5** | 12 | Gnosis is 2.4x faster |
| `SLOTS_PER_EPOCH` | **16** | 32 | Gnosis epochs are half as many slots |
| Epoch duration | **80 seconds** | 384 seconds (6.4 min) | 16 * 5 = 80s |
| `MIN_VALIDATOR_WITHDRAWABILITY_DELAY` | **256 epochs** | 256 epochs | Same epoch count, but ~5.7 hours on Gnosis vs ~27 hours on Ethereum |

### Key Preset/Config Values (Gnosis vs Ethereum)

| Parameter | Gnosis | Ethereum |
|-----------|--------|----------|
| `VALIDATOR_REGISTRY_LIMIT` | 2^40 (1,099,511,627,776) | 2^40 (same) |
| `PENDING_CONSOLIDATIONS_LIMIT` | 2^18 (262,144) | 2^18 (same) |
| `PENDING_DEPOSITS_LIMIT` | 2^27 | 2^27 (same) |
| `PENDING_PARTIAL_WITHDRAWALS_LIMIT` | 2^27 | 2^27 (same) |
| `MAX_VALIDATORS_PER_WITHDRAWALS_SWEEP` | **8,192** (2^13) | 16,384 (2^14) |
| `MAX_WITHDRAWALS_PER_PAYLOAD` | **8** (2^3) | 16 (2^4) |
| `MAX_EFFECTIVE_BALANCE` | 32 GNO (32e9 Gwei) | 32 ETH (32e9 Gwei) |
| `MAX_EFFECTIVE_BALANCE_ELECTRA` | 2,048 GNO (2048e9 Gwei) | 2,048 ETH (same) |
| `BASE_REWARD_FACTOR` | **25** | 64 |
| `CHURN_LIMIT_QUOTIENT` | **4,096** | 65,536 |
| `EPOCHS_PER_ETH1_VOTING_PERIOD` | **64** | 64 (same) |
| `ETH1_FOLLOW_DISTANCE` | **1,024** | 2,048 |

### Finality

- Finality is achieved after **2 epochs** (same as Ethereum in terms of epoch count)
- On Gnosis: 2 epochs = 2 * 80s = **160 seconds** (~2.67 minutes)
- On Ethereum: 2 epochs = 2 * 384s = 768 seconds (~12.8 minutes)
- The EIP-4788 HISTORY_BUFFER_LENGTH of 8191 provides: 8191 * 5 seconds = ~11.4 hours of coverage on Gnosis (vs ~27.3 hours on Ethereum)

---

## 6. Important Notes for Smart Contract Implementation

1. **Generalized indices changed in Electra**: The BeaconState grew from 28 fields (Deneb) to 37 fields (Electra), crossing the 32-field power-of-two boundary. This changes the tree depth from 5 to 6, and all top-level GIs shifted from `32 + field_index` to `64 + field_index`. Any hardcoded GIs must use the Electra values.

2. **SSZ list Merkleization**: Lists have a mix-in length node. The data is in the left subtree (GI * 2) and the length is in the right subtree (GI * 2 + 1). The data subtree is always padded to the maximum capacity.

3. **Same SSZ structure as Ethereum**: The Gnosis beacon state SSZ container structure is identical to Ethereum. Only configuration values (slot times, epoch sizes, some limits) differ. The VALIDATOR_REGISTRY_LIMIT and PENDING_CONSOLIDATIONS_LIMIT are the same, so **all generalized indices are identical** between Gnosis and Ethereum for Electra.

4. **Proof verification in Solidity**: A Merkle proof for `state.validators[i].withdrawal_credentials` will have 53 sibling hashes. For `state.pending_consolidations[i].source_index`, it will have 29 sibling hashes. These are from the beacon block root (which is what EIP-4788 provides).

---

## Sources

- [EIP-4788 Specification](https://eips.ethereum.org/EIPS/eip-4788)
- [EIP-4788 on Gnosis Blockscout](https://gnosis.blockscout.com/address/0x000F3df6D732807Ef1319fB7B8bB8522d0Beac02)
- [Gnosis Chain Specs Repository](https://github.com/gnosischain/specs)
- [Gnosis Electra Preset](https://raw.githubusercontent.com/gnosischain/specs/refs/heads/master/consensus/preset/gnosis/electra.yaml)
- [Gnosis Chain Configs Repository](https://github.com/gnosischain/configs)
- [Gnosis Mainnet Config](https://github.com/gnosischain/configs/blob/main/mainnet/config.yaml)
- [Ethereum Consensus Specs - Electra Beacon Chain](https://ethereum.github.io/consensus-specs/specs/electra/beacon-chain/)
- [Ethereum Consensus Specs - Phase 0 Beacon Chain](https://ethereum.github.io/consensus-specs/specs/phase0/beacon-chain/)
- [SSZ Merkle Proofs Spec](https://ethereum.github.io/consensus-specs/ssz/merkle-proofs/)
- [SSZ-QL Guide](https://www.mexc.com/news/ssz-ql-a-guide-to-querying-ethereums-beaconstate-using-offsets-proofs-and-g-indexes/168734)
- [Gnosis Pectra Hardfork Announcement](https://blog.validategnosis.com/p/gnosis-chain-pragueelectra-hard-fork)
- [Gnosis Pectra Upgrade Docs](https://docs.gnosischain.com/about/specs/hard-forks/pectra)
- [Gnosis Chain Documentation](https://docs.gnosischain.com/)
