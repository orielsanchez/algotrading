use anyhow::Result;
use algotrading::position_inertia::{PositionInertiaCalculator, InertiaDecision, InertiaConfig};
use algotrading::transaction_cost::TransactionCostCalculator;
use algotrading::security_types::SecurityType;
use algotrading::portfolio::Portfolio;

#[cfg(test)]
mod position_inertia_tests {
    use super::*;

    fn setup_test_inertia_calculator() -> PositionInertiaCalculator {
        let config = InertiaConfig {
            inertia_multiplier: 2.0, // Carver's 2x transaction cost threshold
            min_position_change_value: 100.0, // $100 minimum change
            max_position_change_pct: 0.50, // 50% max change per rebalance
            enable_position_inertia: true,
        };

        PositionInertiaCalculator::new(config)
    }

    #[test]
    fn test_position_inertia_threshold_calculation() -> Result<()> {
        let calculator = setup_test_inertia_calculator();
        
        // Test basic threshold calculation
        let transaction_cost = 50.0; // $50 transaction cost
        let expected_threshold = 50.0 * 2.0; // 2x transaction cost = $100
        
        let threshold = calculator.calculate_inertia_threshold(transaction_cost)?;
        assert_eq!(threshold, expected_threshold, 
                   "Inertia threshold should be 2x transaction cost");

        Ok(())
    }

    #[test]
    fn test_small_position_change_blocked_by_inertia() -> Result<()> {
        let calculator = setup_test_inertia_calculator();
        
        let current_position = 1000.0; // $1000 current position
        let target_position = 1050.0;  // $1050 target position
        let transaction_cost = 30.0;   // $30 transaction cost
        let signal_strength = 5.0;     // Moderate signal strength
        let price = 100.0;

        let decision = calculator.calculate_position_decision(
            current_position,
            target_position,
            transaction_cost,
            signal_strength,
            price
        )?;

        // Position change ($50) < inertia threshold ($60), so should hold
        assert_eq!(decision.action, InertiaDecision::Hold, 
                   "Small position changes should be blocked by inertia");
        assert_eq!(decision.recommended_position, current_position,
                   "Should recommend holding current position");
        assert!(decision.reason.contains("inertia"), 
                "Decision reason should mention inertia");

        Ok(())
    }

    #[test]
    fn test_large_position_change_overrides_inertia() -> Result<()> {
        let calculator = setup_test_inertia_calculator();
        
        let current_position = 1000.0; // $1000 current position
        let target_position = 1200.0;  // $1200 target position
        let transaction_cost = 30.0;   // $30 transaction cost
        let signal_strength = 15.0;    // Strong signal strength
        let price = 100.0;

        let decision = calculator.calculate_position_decision(
            current_position,
            target_position,
            transaction_cost,
            signal_strength,
            price
        )?;

        // Position change ($200) > inertia threshold ($60), so should rebalance
        assert_eq!(decision.action, InertiaDecision::Rebalance,
                   "Large position changes should override inertia");
        assert_eq!(decision.recommended_position, target_position,
                   "Should recommend target position");
        assert!(decision.reason.contains("threshold exceeded"),
                "Decision reason should mention threshold exceeded");

        Ok(())
    }

    #[test]
    fn test_strong_signal_overrides_inertia() -> Result<()> {
        let calculator = setup_test_inertia_calculator();
        
        let current_position = 1000.0; // $1000 current position
        let target_position = 1070.0;  // $1070 target position
        let transaction_cost = 40.0;   // $40 transaction cost
        let signal_strength = 18.0;    // Very strong signal (near +20 max)
        let price = 100.0;

        let decision = calculator.calculate_position_decision(
            current_position,
            target_position,
            transaction_cost,
            signal_strength,
            price
        )?;

        // Position change ($70) < inertia threshold ($80), but strong signal should override
        assert_eq!(decision.action, InertiaDecision::Rebalance,
                   "Strong signals should override inertia");
        assert_eq!(decision.recommended_position, target_position,
                   "Should recommend target position for strong signals");
        assert!(decision.reason.contains("strong signal"),
                "Decision reason should mention strong signal");

        Ok(())
    }

    #[test]
    fn test_negative_signal_triggers_position_reduction() -> Result<()> {
        let calculator = setup_test_inertia_calculator();
        
        let current_position = 1000.0; // $1000 current position
        let target_position = 0.0;     // $0 target position (exit)
        let transaction_cost = 50.0;   // $50 transaction cost
        let signal_strength = -15.0;   // Strong negative signal
        let price = 100.0;

        let decision = calculator.calculate_position_decision(
            current_position,
            target_position,
            transaction_cost,
            signal_strength,
            price
        )?;

        // Large position reduction with strong negative signal should execute
        assert_eq!(decision.action, InertiaDecision::Rebalance,
                   "Strong negative signals should trigger position reduction");
        assert_eq!(decision.recommended_position, target_position,
                   "Should recommend target position for negative signals");

        Ok(())
    }

    #[test]
    fn test_position_reversal_always_executes() -> Result<()> {
        let calculator = setup_test_inertia_calculator();
        
        let current_position = 1000.0;  // $1000 long position
        let target_position = -500.0;   // $500 short position
        let transaction_cost = 30.0;    // $30 transaction cost
        let signal_strength = -10.0;    // Negative signal
        let price = 100.0;

        let decision = calculator.calculate_position_decision(
            current_position,
            target_position,
            transaction_cost,
            signal_strength,
            price
        )?;

        // Position reversal (long to short) should always execute
        assert_eq!(decision.action, InertiaDecision::Rebalance,
                   "Position reversals should always execute");
        assert_eq!(decision.recommended_position, target_position,
                   "Should recommend target position for reversals");
        assert!(decision.reason.contains("reversal"),
                "Decision reason should mention reversal");

        Ok(())
    }

    #[test]
    fn test_minimum_position_change_threshold() -> Result<()> {
        let calculator = setup_test_inertia_calculator();
        
        let current_position = 1000.0; // $1000 current position
        let target_position = 1020.0;  // $1020 target position
        let transaction_cost = 5.0;    // $5 transaction cost (very low)
        let signal_strength = 8.0;     // Moderate signal
        let price = 100.0;

        let decision = calculator.calculate_position_decision(
            current_position,
            target_position,
            transaction_cost,
            signal_strength,
            price
        )?;

        // Position change ($20) < minimum threshold ($100), so should hold
        assert_eq!(decision.action, InertiaDecision::Hold,
                   "Position changes below minimum threshold should be blocked");
        assert!(decision.reason.contains("minimum change"),
                "Decision reason should mention minimum change threshold");

        Ok(())
    }

    #[test]
    fn test_maximum_position_change_limiting() -> Result<()> {
        let calculator = setup_test_inertia_calculator();
        
        let current_position = 1000.0; // $1000 current position
        let target_position = 2000.0;  // $2000 target position (100% increase)
        let transaction_cost = 30.0;   // $30 transaction cost
        let signal_strength = 20.0;    // Maximum signal strength
        let price = 100.0;

        let decision = calculator.calculate_position_decision(
            current_position,
            target_position,
            transaction_cost,
            signal_strength,
            price
        )?;

        // Should limit to 50% increase = $1500 position
        let expected_position = current_position * 1.5; // 50% increase
        assert_eq!(decision.action, InertiaDecision::Rebalance,
                   "Large position changes should be executed but limited");
        assert_eq!(decision.recommended_position, expected_position,
                   "Should limit position change to maximum percentage");
        assert!(decision.reason.contains("limited"),
                "Decision reason should mention position change limiting");

        Ok(())
    }

    #[test]
    fn test_disabled_inertia_always_rebalances() -> Result<()> {
        let mut config = InertiaConfig {
            inertia_multiplier: 2.0,
            min_position_change_value: 100.0,
            max_position_change_pct: 0.50,
            enable_position_inertia: false, // Disabled
        };

        let calculator = PositionInertiaCalculator::new(config);
        
        let current_position = 1000.0; // $1000 current position
        let target_position = 1010.0;  // $1010 target position (tiny change)
        let transaction_cost = 50.0;   // $50 transaction cost
        let signal_strength = 1.0;     // Weak signal
        let price = 100.0;

        let decision = calculator.calculate_position_decision(
            current_position,
            target_position,
            transaction_cost,
            signal_strength,
            price
        )?;

        // With inertia disabled, should always rebalance
        assert_eq!(decision.action, InertiaDecision::Rebalance,
                   "Disabled inertia should always rebalance");
        assert_eq!(decision.recommended_position, target_position,
                   "Should recommend target position when inertia disabled");

        Ok(())
    }

    #[test]
    fn test_portfolio_level_inertia_analysis() -> Result<()> {
        let calculator = setup_test_inertia_calculator();
        
        // Create mock portfolio positions
        let current_positions = vec![
            ("AAPL", 1000.0, 1050.0, 10.0), // symbol, current, target, tx_cost
            ("GOOGL", 2000.0, 2200.0, 20.0), // Should rebalance
            ("MSFT", 1500.0, 1520.0, 15.0),  // Should hold (inertia)
        ];

        let decisions = calculator.analyze_portfolio_rebalancing(current_positions)?;

        assert_eq!(decisions.len(), 3, "Should analyze all positions");
        
        // AAPL: $50 change vs $20 threshold = Hold
        assert_eq!(decisions[0].action, InertiaDecision::Hold);
        
        // GOOGL: $200 change vs $40 threshold = Rebalance
        assert_eq!(decisions[1].action, InertiaDecision::Rebalance);
        
        // MSFT: $20 change vs $30 threshold = Hold
        assert_eq!(decisions[2].action, InertiaDecision::Hold);

        Ok(())
    }

    #[test]
    fn test_cost_benefit_analysis() -> Result<()> {
        let calculator = setup_test_inertia_calculator();
        
        let current_position = 1000.0;
        let target_position = 1150.0;
        let transaction_cost = 40.0;
        let signal_strength = 12.0;
        let price = 100.0;

        let analysis = calculator.calculate_cost_benefit_analysis(
            current_position,
            target_position,
            transaction_cost,
            signal_strength,
            price
        )?;

        assert_eq!(analysis.position_change_value, 150.0);
        assert_eq!(analysis.transaction_cost, 40.0);
        assert_eq!(analysis.inertia_threshold, 80.0);
        assert_eq!(analysis.net_benefit, 110.0); // 150 - 40
        assert!(analysis.is_beneficial, "Should be beneficial");
        assert!(analysis.exceeds_threshold, "Should exceed threshold");

        Ok(())
    }
}