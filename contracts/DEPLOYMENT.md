# Deployment Guide: ConsolidationIncentives on Gnosis Chain

## Overview

This contract uses the **UUPS upgradeable proxy pattern** (ERC1967). You deploy:
1. **Implementation contract** — the logic
2. **Proxy contract** — delegates to implementation, holds state and funds

Always interact with the **proxy address**. The implementation is swappable via upgrades.

---

## Prerequisites

1. **Foundry installed**: `curl -L https://foundry.paradigm.xyz | bash && foundryup`
2. **Private key** with xDAI on Gnosis Chain for deployment gas
3. **RPC endpoint** — e.g., `https://rpc.gnosischain.com`

---

## Configuration

Set these environment variables before deploying:

| Variable | Description | Example |
|----------|-------------|---------|
| `MAX_EPOCH` | Eligibility cutoff epoch (only validators activated before this can claim) | `100000` |
| `REWARD_AMOUNT` | Reward per consolidation in wei | `1000000000000000000` (1 xDAI) |
| `MIN_CLAIM_DELAY` | Minimum seconds between beacon timestamp and claim (finality safety) | `960` (12 Gnosis epochs = 80s × 12) |
| `INITIAL_FUNDING` | Initial contract funding in wei (optional) | `10000000000000000000` (10 xDAI) |
| `PRIVATE_KEY` | Deployer private key (Foundry uses this via `--private-key`) | `0xabc...` |

**Recommended values for Gnosis Chain:**
- `MIN_CLAIM_DELAY`: 960 seconds (12 epochs, ~2 minutes) — ensures finality
- `REWARD_AMOUNT`: 1-10 xDAI depending on incentive budget
- `MAX_EPOCH`: Set to the epoch when the incentive program starts (e.g., current epoch + 100)

---

## Deployment Steps

### 1. Dry Run (Simulation)

Test the deployment locally without broadcasting:

```bash
cd contracts

export MAX_EPOCH=100000
export REWARD_AMOUNT=1000000000000000000  # 1 xDAI
export MIN_CLAIM_DELAY=960
export INITIAL_FUNDING=10000000000000000000  # 10 xDAI

forge script script/Deploy.s.sol \
  --rpc-url https://rpc.gnosischain.com \
  --private-key $PRIVATE_KEY
```

Review the output. If it succeeds, you'll see:
- Implementation address
- Proxy address
- Owner verification
- Configuration verification

### 2. Deploy to Gnosis Chain

Add `--broadcast` to actually deploy:

```bash
forge script script/Deploy.s.sol \
  --rpc-url https://rpc.gnosischain.com \
  --private-key $PRIVATE_KEY \
  --broadcast
```

**Save the output!** You'll need:
- **Proxy Address** — this is your contract address (use this in all interactions)
- **Implementation Address** — for verification and reference

### 3. Verify on Gnosisscan (Optional but Recommended)

Add `--verify` to auto-verify on Gnosisscan:

```bash
forge script script/Deploy.s.sol \
  --rpc-url https://rpc.gnosischain.com \
  --private-key $PRIVATE_KEY \
  --broadcast \
  --verify \
  --etherscan-api-key $GNOSISSCAN_API_KEY
```

Get API key from: https://gnosisscan.io/myapikey

Verification makes the contract readable on the block explorer.

---

## Post-Deployment

### Fund the Contract

The contract needs xDAI to pay rewards. Fund it via:

**During deployment:**
```bash
export INITIAL_FUNDING=10000000000000000000  # 10 xDAI
```

**After deployment:**
```bash
# Send xDAI directly to the proxy address
cast send <PROXY_ADDRESS> --value 10ether --rpc-url https://rpc.gnosischain.com --private-key $PRIVATE_KEY
```

### Verify Deployment

Check contract state:

```bash
PROXY=<your-proxy-address>

# Check owner
cast call $PROXY "owner()(address)" --rpc-url https://rpc.gnosischain.com

# Check configuration
cast call $PROXY "maxEpoch()(uint64)" --rpc-url https://rpc.gnosischain.com
cast call $PROXY "rewardAmount()(uint256)" --rpc-url https://rpc.gnosischain.com
cast call $PROXY "minClaimDelay()(uint256)" --rpc-url https://rpc.gnosischain.com

# Check balance
cast balance $PROXY --rpc-url https://rpc.gnosischain.com
```

### Test a Claim (Testnet First!)

Before deploying to mainnet, test on **Chiado testnet**:

```bash
# Chiado RPC
export RPC_URL=https://rpc.chiadochain.net

# Deploy with test values
export MAX_EPOCH=10000
export REWARD_AMOUNT=100000000000000000  # 0.1 xDAI
export MIN_CLAIM_DELAY=80  # 1 epoch on Chiado

forge script script/Deploy.s.sol \
  --rpc-url $RPC_URL \
  --private-key $PRIVATE_KEY \
  --broadcast
```

Then use the Rust proof service to generate and submit a test claim.

---

## Upgrading the Contract

The contract is upgradeable via UUPS. Only the owner can upgrade.

### 1. Deploy New Implementation

```bash
cd contracts

forge create src/ConsolidationIncentives.sol:ConsolidationIncentives \
  --rpc-url https://rpc.gnosischain.com \
  --private-key $PRIVATE_KEY
```

Save the new implementation address.

### 2. Upgrade the Proxy

```bash
NEW_IMPL=<new-implementation-address>
PROXY=<proxy-address>

# Encode the upgrade call
cast send $PROXY \
  "upgradeToAndCall(address,bytes)" \
  $NEW_IMPL \
  "0x" \
  --rpc-url https://rpc.gnosischain.com \
  --private-key $PRIVATE_KEY
```

The proxy now delegates to the new implementation. **State persists** (maxEpoch, rewardAmount, rewarded mapping, etc.).

---

## Admin Functions

### Withdraw Excess Funds

```bash
cast send $PROXY \
  "withdraw(address,uint256)" \
  <recipient-address> \
  <amount-in-wei> \
  --rpc-url https://rpc.gnosischain.com \
  --private-key $PRIVATE_KEY
```

### Transfer Ownership

```bash
cast send $PROXY \
  "transferOwnership(address)" \
  <new-owner-address> \
  --rpc-url https://rpc.gnosischain.com \
  --private-key $PRIVATE_KEY
```

---

## Security Checklist

Before deploying to mainnet:

- [ ] Deployed and tested on Chiado testnet
- [ ] Verified contract on Gnosisscan
- [ ] Reviewed `maxEpoch` value (is the cutoff correct?)
- [ ] Reviewed `rewardAmount` (is the budget sustainable?)
- [ ] Reviewed `minClaimDelay` (is finality guaranteed?)
- [ ] Contract funded with sufficient xDAI for expected claims
- [ ] Owner address is a secure multisig (not a single EOA)
- [ ] Emergency withdrawal tested
- [ ] Upgrade mechanism tested on testnet

---

## Example: Full Mainnet Deployment

```bash
# Environment
export RPC_URL=https://rpc.gnosischain.com
export PRIVATE_KEY=0x...  # Use a hardware wallet or secure key management
export GNOSISSCAN_API_KEY=...

# Configuration
export MAX_EPOCH=120000  # Example: epoch 120,000
export REWARD_AMOUNT=5000000000000000000  # 5 xDAI per consolidation
export MIN_CLAIM_DELAY=960  # 12 Gnosis epochs
export INITIAL_FUNDING=100000000000000000000  # 100 xDAI initial funding

# Deploy + Verify
cd contracts
forge script script/Deploy.s.sol \
  --rpc-url $RPC_URL \
  --private-key $PRIVATE_KEY \
  --broadcast \
  --verify

# Save output
# Proxy Address: 0x...
# Implementation Address: 0x...

# Verify deployment
cast call <PROXY> "owner()(address)" --rpc-url $RPC_URL
cast balance <PROXY> --rpc-url $RPC_URL
```

---

## Troubleshooting

### "Insufficient funds" error
- Deployer account needs xDAI for gas (≥0.01 xDAI for deployment)
- Contract needs xDAI for rewards (set `INITIAL_FUNDING` or fund after deployment)

### "Initialization failed"
- Check that `MAX_EPOCH > 0` and `REWARD_AMOUNT > 0`
- Verify RPC URL is correct and accessible

### "Verification failed"
- Ensure `--verify` uses correct Gnosisscan API key
- Check that constructor arguments match (Foundry auto-detects for UUPS proxies)
- Manually verify at https://gnosisscan.io/verifyContract

### "Only owner can upgrade"
- Ensure you're using the deployer's private key
- Check ownership: `cast call <PROXY> "owner()(address)"`
- Transfer ownership if needed: `cast send <PROXY> "transferOwnership(address)" <new-owner>`

---

## Reference

- **Gnosis Chain Docs**: https://docs.gnosischain.com
- **Foundry Book**: https://book.getfoundry.sh
- **EIP-4788 Beacon Roots**: https://eips.ethereum.org/EIPS/eip-4788
- **EIP-7251 Consolidations**: https://eips.ethereum.org/EIPS/eip-7251
- **Gnosisscan**: https://gnosisscan.io
