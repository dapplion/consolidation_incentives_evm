-- Program Health Metrics
--
-- Overall health metrics for the consolidation incentive program.
-- Shows total claims, rewards distributed, unique participants, and contract balance.

WITH stats AS (
    SELECT 
        COUNT(*) AS total_claims,
        SUM(rewardAmount) / POWER(10, 18) AS total_distributed_xdai,
        COUNT(DISTINCT recipient) AS unique_recipients,
        MIN(evt_block_time) AS program_start,
        MAX(evt_block_time) AS latest_claim
    FROM 
        consolidation_incentives_gnosis.ConsolidationIncentives_evt_RewardClaimed
),
balance AS (
    SELECT 
        balance / POWER(10, 18) AS current_balance_xdai
    FROM 
        gnosis.balances
    WHERE 
        address = {{contract_address}}  -- Replace with actual contract address
        AND day = (SELECT MAX(day) FROM gnosis.balances)
)
SELECT 
    stats.total_claims,
    stats.total_distributed_xdai,
    stats.unique_recipients,
    stats.program_start,
    stats.latest_claim,
    COALESCE(balance.current_balance_xdai, 0) AS remaining_balance_xdai,
    stats.total_distributed_xdai + COALESCE(balance.current_balance_xdai, 0) AS total_funded_xdai
FROM 
    stats
LEFT JOIN 
    balance ON TRUE;
