-- Top Validators by Consolidation Count
--
-- Shows which validators (by withdrawal address) have consolidated the most.
-- Helps identify power users and validator operators utilizing consolidations.

SELECT 
    recipient AS withdrawal_address,
    COUNT(*) AS consolidation_count,
    SUM(rewardAmount) / POWER(10, 18) AS total_rewards_xdai,
    MIN(evt_block_time) AS first_claim,
    MAX(evt_block_time) AS last_claim
FROM 
    consolidation_incentives_gnosis.ConsolidationIncentives_evt_RewardClaimed
GROUP BY 
    recipient
ORDER BY 
    consolidation_count DESC
LIMIT 100;
