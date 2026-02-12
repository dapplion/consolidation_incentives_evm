-- Total Rewards Distributed Over Time
-- 
-- Shows daily aggregation of consolidation reward claims on Gnosis Chain.
-- Requires the ConsolidationIncentives contract to be decoded on Dune.

SELECT 
    DATE_TRUNC('day', evt_block_time) AS day,
    COUNT(*) AS claims,
    SUM(rewardAmount) / POWER(10, 18) AS total_xdai,
    COUNT(DISTINCT recipient) AS unique_recipients
FROM 
    consolidation_incentives_gnosis.ConsolidationIncentives_evt_RewardClaimed
GROUP BY 
    DATE_TRUNC('day', evt_block_time)
ORDER BY 
    day ASC;
