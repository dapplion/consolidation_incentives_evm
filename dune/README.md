# Dune Analytics Queries

This directory contains SQL queries for analyzing the Gnosis Consolidation Incentives program on Dune Analytics.

## Setup

1. **Deploy the contract** on Gnosis Chain
2. **Submit for decoding** on Dune Analytics:
   - Go to https://dune.com/contracts/new
   - Submit contract address and ABI
   - Wait for decoding (usually 24-48 hours)
3. **Create queries** using the decoded events

## Available Queries

### 1. Total Rewards (`total_rewards.sql`)

Daily aggregation showing:
- Number of claims per day
- Total xDAI distributed per day
- Unique recipients per day

**Use case:** Track daily program activity and spending.

### 2. Consolidations Over Time (`consolidations_over_time.sql`)

Cumulative view showing:
- Daily claim count
- Running total of all claims

**Use case:** Growth chart for program adoption.

### 3. Top Validators (`top_validators.sql`)

Leaderboard showing:
- Withdrawal addresses with most consolidations
- Total rewards earned by each
- First and last claim timestamps

**Use case:** Identify power users and validator operators.

### 4. Eligibility Distribution (`eligibility_distribution.sql`)

Cohort analysis showing:
- Validator index ranges (grouped by thousands)
- Consolidation counts per cohort
- Total rewards per cohort

**Use case:** Understand which validator cohorts are most active.

### 5. Program Health (`program_health.sql`)

Overall metrics showing:
- Total claims and rewards distributed
- Unique participants
- Program start and latest activity
- Current contract balance
- Total funding

**Use case:** Executive dashboard for program health.

**Note:** Requires manual update of `{{contract_address}}` parameter.

## Event Schema

The contract emits `RewardClaimed` events with the following structure:

```solidity
event RewardClaimed(
    uint64 indexed sourceIndex,
    address indexed recipient,
    uint256 rewardAmount
);
```

Dune decodes this as:

- `sourceIndex` (uint64): The source validator index
- `recipient` (address): The withdrawal address that received the reward
- `rewardAmount` (uint256): The amount of xDAI rewarded (in wei)
- `evt_block_time` (timestamp): Block timestamp
- `evt_tx_hash` (bytes32): Transaction hash
- `evt_index` (int): Log index within the transaction

## Creating Dashboards

After creating queries on Dune, combine them into a dashboard:

1. Create a new dashboard on Dune
2. Add visualizations for each query:
   - **total_rewards.sql** → Bar chart (daily claims + rewards)
   - **consolidations_over_time.sql** → Area chart (cumulative growth)
   - **top_validators.sql** → Table (leaderboard)
   - **eligibility_distribution.sql** → Bar chart (cohort distribution)
   - **program_health.sql** → Counter cards (key metrics)
3. Add filters (date range, address, etc.)
4. Publish and share the dashboard

## Example Dashboard Layout

```
┌─────────────────────────────────────────┐
│  Program Health (Big Numbers)           │
│  Total Claims | Distributed | Recipients│
└─────────────────────────────────────────┘

┌─────────────────┬───────────────────────┐
│ Cumulative      │  Daily Rewards        │
│ Consolidations  │  (Bar Chart)          │
│ (Area Chart)    │                       │
└─────────────────┴───────────────────────┘

┌─────────────────┬───────────────────────┐
│ Top Validators  │  Cohort Distribution  │
│ (Table)         │  (Bar Chart)          │
│                 │                       │
└─────────────────┴───────────────────────┘
```

## Notes

- Queries use `consolidation_incentives_gnosis.ConsolidationIncentives_evt_RewardClaimed`
- This table name is generated automatically when Dune decodes the contract
- The prefix `consolidation_incentives_gnosis` comes from the contract name and chain
- All xDAI amounts are divided by 10^18 to convert from wei to whole units
- Replace `{{contract_address}}` in `program_health.sql` with the actual deployed address
