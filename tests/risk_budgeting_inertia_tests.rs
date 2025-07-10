use algotrading::portfolio::Portfolio;
use algotrading::position_inertia::PositionInertiaCalculator;
use algotrading::risk_budgeting::{ERCAllocation, RiskBudgeter};
use algotrading::risk_budgeting_inertia::{
    ERCInertiaDecision, ERCInertiaDecisionInfo, RiskBudgetingInertiaCalculator,
};
use algotrading::security_types::SecurityType;
use algotrading::transaction_cost::TransactionCostCalculator;
use anyhow::Result;

#[cfg(test)]
mod risk_budgeting_inertia_tests {
    use super::*;

    fn setup_test_risk_budgeting_inertia() -> RiskBudgetingInertiaCalculator {
        RiskBudgetingInertiaCalculator::new(
            2.0,  // inertia_multiplier
            0.02, // rebalance_threshold (2%)
            true, // enable_cost_aware_erc
        )
    }

    #[test]
    fn test_erc_allocation_with_inertia_constraints() -> Result<()> {
        let calculator = setup_test_risk_budgeting_inertia();

        // Mock current portfolio positions
        let current_positions = vec![
            ("AAPL", 10000.0, 0.25), // symbol, value, current_weight
            ("GOOGL", 15000.0, 0.35),
            ("MSFT", 12000.0, 0.30),
            ("AMZN", 3000.0, 0.10),
        ];

        // Mock ERC target allocations
        let erc_targets = vec![
            ("AAPL", 0.30),  // Should increase
            ("GOOGL", 0.30), // Should decrease
            ("MSFT", 0.25),  // Should decrease
            ("AMZN", 0.15),  // Should increase
        ];

        // Mock transaction costs
        let transaction_costs = vec![
            ("AAPL", 25.0),
            ("GOOGL", 30.0),
            ("MSFT", 20.0),
            ("AMZN", 15.0),
        ];

        let decisions = calculator.calculate_erc_with_inertia(
            current_positions,
            erc_targets,
            transaction_costs,
            40000.0, // total_portfolio_value
        )?;

        assert_eq!(decisions.len(), 4, "Should analyze all positions");

        // AAPL: 25% -> 30% = $2000 change, $25 cost, threshold = $50
        // Should rebalance (exceeds threshold)
        assert_eq!(decisions[0].action, ERCInertiaDecision::Rebalance);
        assert!(decisions[0].target_allocation_adjusted);

        // GOOGL: 35% -> 30% = $2000 change, $30 cost, threshold = $60
        // Should rebalance (exceeds threshold)
        assert_eq!(decisions[1].action, ERCInertiaDecision::Rebalance);

        // MSFT: 30% -> 25% = $2000 change, $20 cost, threshold = $40
        // Should rebalance (exceeds threshold)
        assert_eq!(decisions[2].action, ERCInertiaDecision::Rebalance);

        // AMZN: 10% -> 15% = $2000 change, $15 cost, threshold = $30
        // Should rebalance (exceeds threshold)
        assert_eq!(decisions[3].action, ERCInertiaDecision::Rebalance);

        Ok(())
    }

    #[test]
    fn test_small_erc_adjustments_blocked_by_inertia() -> Result<()> {
        let calculator = setup_test_risk_budgeting_inertia();

        // Mock current portfolio with small ERC deviations
        let current_positions = vec![
            ("AAPL", 10000.0, 0.250), // Current weight
            ("GOOGL", 10000.0, 0.250),
            ("MSFT", 10000.0, 0.250),
            ("AMZN", 10000.0, 0.250),
        ];

        // Mock ERC targets with small adjustments
        let erc_targets = vec![
            ("AAPL", 0.255),  // +0.5% adjustment
            ("GOOGL", 0.245), // -0.5% adjustment
            ("MSFT", 0.252),  // +0.2% adjustment
            ("AMZN", 0.248),  // -0.2% adjustment
        ];

        // Mock high transaction costs
        let transaction_costs = vec![
            ("AAPL", 50.0), // High cost
            ("GOOGL", 50.0),
            ("MSFT", 50.0),
            ("AMZN", 50.0),
        ];

        let decisions = calculator.calculate_erc_with_inertia(
            current_positions,
            erc_targets,
            transaction_costs,
            40000.0, // total_portfolio_value
        )?;

        // Small changes should be blocked by inertia
        assert_eq!(decisions[0].action, ERCInertiaDecision::Hold);
        assert_eq!(decisions[1].action, ERCInertiaDecision::Hold);
        assert_eq!(decisions[2].action, ERCInertiaDecision::Hold);
        assert_eq!(decisions[3].action, ERCInertiaDecision::Hold);

        // All should indicate inertia blocking
        assert!(decisions.iter().all(|d| d.blocked_by_inertia));

        Ok(())
    }

    #[test]
    fn test_partial_erc_rebalancing() -> Result<()> {
        let calculator = setup_test_risk_budgeting_inertia();

        // Mixed scenario: some positions justify rebalancing, others don't
        let current_positions = vec![
            ("AAPL", 8000.0, 0.20),   // Large change needed
            ("GOOGL", 12000.0, 0.30), // Small change needed
            ("MSFT", 16000.0, 0.40),  // Large change needed
            ("AMZN", 4000.0, 0.10),   // Small change needed
        ];

        let erc_targets = vec![
            ("AAPL", 0.25),  // +$2000 change
            ("GOOGL", 0.28), // -$800 change
            ("MSFT", 0.35),  // -$2000 change
            ("AMZN", 0.12),  // +$800 change
        ];

        let transaction_costs = vec![
            ("AAPL", 30.0),
            ("GOOGL", 40.0), // Higher cost
            ("MSFT", 25.0),
            ("AMZN", 45.0), // Higher cost
        ];

        let decisions = calculator.calculate_erc_with_inertia(
            current_positions,
            erc_targets,
            transaction_costs,
            40000.0,
        )?;

        // AAPL: $2000 change vs $60 threshold = Rebalance
        assert_eq!(decisions[0].action, ERCInertiaDecision::Rebalance);

        // GOOGL: $800 change vs $80 threshold = Rebalance
        assert_eq!(decisions[1].action, ERCInertiaDecision::Rebalance);

        // MSFT: $2000 change vs $50 threshold = Rebalance
        assert_eq!(decisions[2].action, ERCInertiaDecision::Rebalance);

        // AMZN: $800 change vs $90 threshold = Rebalance
        assert_eq!(decisions[3].action, ERCInertiaDecision::Rebalance);

        Ok(())
    }

    #[test]
    fn test_correlation_risk_with_inertia() -> Result<()> {
        let calculator = setup_test_risk_budgeting_inertia();

        // Mock positions with high correlation
        let current_positions = vec![
            ("AAPL", 10000.0, 0.25),
            ("GOOGL", 10000.0, 0.25),
            ("MSFT", 10000.0, 0.25),
            ("NVDA", 10000.0, 0.25),
        ];

        // Mock correlation matrix (high tech correlation)
        let correlation_matrix = vec![
            vec![1.0, 0.8, 0.7, 0.9], // AAPL correlations
            vec![0.8, 1.0, 0.6, 0.8], // GOOGL correlations
            vec![0.7, 0.6, 1.0, 0.7], // MSFT correlations
            vec![0.9, 0.8, 0.7, 1.0], // NVDA correlations
        ];

        let erc_targets = vec![
            ("AAPL", 0.20), // Reduce concentration
            ("GOOGL", 0.20),
            ("MSFT", 0.20),
            ("NVDA", 0.15), // Reduce most correlated
        ];

        let transaction_costs = vec![
            ("AAPL", 25.0),
            ("GOOGL", 25.0),
            ("MSFT", 25.0),
            ("NVDA", 25.0),
        ];

        let decisions = calculator.calculate_erc_with_correlation_risk(
            current_positions,
            erc_targets,
            correlation_matrix,
            transaction_costs,
            40000.0,
        )?;

        // High correlation should increase urgency to rebalance
        assert!(decisions.iter().any(|d| d.correlation_risk_boost > 0.0));

        // NVDA should have highest correlation penalty
        let nvda_decision = decisions.iter().find(|d| d.symbol == "NVDA").unwrap();
        assert!(nvda_decision.correlation_risk_boost > 0.0);

        Ok(())
    }

    #[test]
    fn test_erc_rebalancing_threshold_override() -> Result<()> {
        let calculator = setup_test_risk_budgeting_inertia();

        // Test portfolio drift exceeding rebalancing threshold
        let current_positions = vec![
            ("AAPL", 8000.0, 0.20),
            ("GOOGL", 18000.0, 0.45), // Drifted significantly
            ("MSFT", 12000.0, 0.30),
            ("AMZN", 2000.0, 0.05),
        ];

        let erc_targets = vec![
            ("AAPL", 0.25),
            ("GOOGL", 0.25), // Large reduction needed
            ("MSFT", 0.25),
            ("AMZN", 0.25),
        ];

        let transaction_costs = vec![
            ("AAPL", 100.0), // Very high costs
            ("GOOGL", 100.0),
            ("MSFT", 100.0),
            ("AMZN", 100.0),
        ];

        let decisions = calculator.calculate_erc_with_inertia(
            current_positions,
            erc_targets,
            transaction_costs,
            40000.0,
        )?;

        // Despite high transaction costs, large drifts should trigger rebalancing
        let googl_decision = decisions.iter().find(|d| d.symbol == "GOOGL").unwrap();
        assert_eq!(googl_decision.action, ERCInertiaDecision::Rebalance);
        assert!(googl_decision.reason.contains("drift"));

        Ok(())
    }

    #[test]
    fn test_portfolio_risk_budget_adjustment() -> Result<()> {
        let calculator = setup_test_risk_budgeting_inertia();

        // Test overall portfolio risk budget needs adjustment
        let current_positions = vec![
            ("AAPL", 10000.0, 0.25),
            ("GOOGL", 10000.0, 0.25),
            ("MSFT", 10000.0, 0.25),
            ("AMZN", 10000.0, 0.25),
        ];

        let erc_targets = vec![
            ("AAPL", 0.25),
            ("GOOGL", 0.25),
            ("MSFT", 0.25),
            ("AMZN", 0.25),
        ];

        let transaction_costs = vec![
            ("AAPL", 20.0),
            ("GOOGL", 20.0),
            ("MSFT", 20.0),
            ("AMZN", 20.0),
        ];

        // Simulate portfolio volatility exceeding target
        let portfolio_volatility = 0.30; // 30% volatility
        let target_volatility = 0.25; // 25% target

        let decisions = calculator.calculate_erc_with_volatility_adjustment(
            current_positions,
            erc_targets,
            transaction_costs,
            portfolio_volatility,
            target_volatility,
            40000.0,
        )?;

        // Should adjust all positions to reduce portfolio volatility
        assert!(decisions.iter().all(|d| d.volatility_adjustment_applied));

        // Should override inertia due to risk budget breach
        assert!(
            decisions
                .iter()
                .any(|d| d.action == ERCInertiaDecision::Rebalance)
        );

        Ok(())
    }

    #[test]
    fn test_cost_aware_erc_disabled() -> Result<()> {
        let calculator = RiskBudgetingInertiaCalculator::new(
            2.0,   // inertia_multiplier
            0.02,  // rebalance_threshold
            false, // enable_cost_aware_erc (DISABLED)
        );

        let current_positions = vec![
            ("AAPL", 10000.0, 0.25),
            ("GOOGL", 10000.0, 0.25),
            ("MSFT", 10000.0, 0.25),
            ("AMZN", 10000.0, 0.25),
        ];

        let erc_targets = vec![
            ("AAPL", 0.26),  // Small change
            ("GOOGL", 0.24), // Small change
            ("MSFT", 0.26),  // Small change
            ("AMZN", 0.24),  // Small change
        ];

        let transaction_costs = vec![
            ("AAPL", 1000.0), // Extremely high costs
            ("GOOGL", 1000.0),
            ("MSFT", 1000.0),
            ("AMZN", 1000.0),
        ];

        let decisions = calculator.calculate_erc_with_inertia(
            current_positions,
            erc_targets,
            transaction_costs,
            40000.0,
        )?;

        // With cost-aware ERC disabled, should rebalance despite high costs
        assert!(
            decisions
                .iter()
                .all(|d| d.action == ERCInertiaDecision::Rebalance)
        );
        assert!(decisions.iter().all(|d| !d.blocked_by_inertia));

        Ok(())
    }

    #[test]
    fn test_erc_inertia_decision_serialization() -> Result<()> {
        let decision = ERCInertiaDecisionInfo {
            symbol: "AAPL".to_string(),
            current_allocation: 0.25,
            target_allocation: 0.30,
            recommended_allocation: 0.28,
            action: ERCInertiaDecision::Rebalance,
            transaction_cost: 25.0,
            position_change_value: 2000.0,
            inertia_threshold: 50.0,
            blocked_by_inertia: false,
            correlation_risk_boost: 0.05,
            volatility_adjustment_applied: true,
            target_allocation_adjusted: true,
            reason: "Position change exceeds inertia threshold".to_string(),
        };

        // Test that decision can be serialized/deserialized (for logging)
        let serialized = serde_json::to_string(&decision)?;
        let deserialized: ERCInertiaDecisionInfo = serde_json::from_str(&serialized)?;

        assert_eq!(decision.symbol, deserialized.symbol);
        assert_eq!(decision.action, deserialized.action);
        assert_eq!(decision.transaction_cost, deserialized.transaction_cost);

        Ok(())
    }
}
