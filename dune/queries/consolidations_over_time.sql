-- Cumulative Consolidations Over Time
--
-- Shows the cumulative count of consolidation reward claims over time.
-- Useful for tracking adoption and growth of the consolidation incentive program.

WITH daily_claims AS (
    SELECT 
        DATE_TRUNC('day', evt_block_time) AS day,
        COUNT(*) AS daily_count
    FROM 
        consolidation_incentives_gnosis.ConsolidationIncentives_evt_RewardClaimed
    GROUP BY 
        DATE_TRUNC('day', evt_block_time)
)
SELECT 
    day,
    daily_count,
    SUM(daily_count) OVER (ORDER BY day ASC ROWS BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW) AS cumulative_claims
FROM 
    daily_claims
ORDER BY 
    day ASC;
