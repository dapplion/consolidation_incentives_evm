-- Consolidation Eligibility Distribution
--
-- Analyzes the distribution of consolidations by source validator activation epoch.
-- Shows which validator cohorts are most actively consolidating.
-- Note: This requires the activationEpoch field to be indexed or decoded.

SELECT 
    FLOOR(sourceIndex / 1000) * 1000 AS validator_cohort,
    COUNT(*) AS consolidations,
    SUM(rewardAmount) / POWER(10, 18) AS total_rewards_xdai
FROM 
    consolidation_incentives_gnosis.ConsolidationIncentives_evt_RewardClaimed
GROUP BY 
    FLOOR(sourceIndex / 1000) * 1000
ORDER BY 
    validator_cohort ASC;
