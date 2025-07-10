use anyhow::Result;
use algotrading::transaction_cost::{TransactionCostCalculator, TransactionCostConfig, MarketImpactModel};
use algotrading::security_types::SecurityType;
use std::collections::HashMap;

#[cfg(test)]
mod transaction_cost_tests {
    use super::*;

    fn setup_test_calculator() -> TransactionCostCalculator {
        let mut bid_ask_spreads = HashMap::new();
        bid_ask_spreads.insert("AAPL".to_string(), 0.0005); // 0.05% spread
        bid_ask_spreads.insert("EURUSD".to_string(), 0.0020); // 0.20% spread
        bid_ask_spreads.insert("ES".to_string(), 0.0010); // 0.10% spread

        let mut commission_rates = HashMap::new();
        commission_rates.insert(SecurityType::Stock, 1.00); // $1.00 per trade
        commission_rates.insert(SecurityType::Forex, 0.50); // $0.50 per trade
        commission_rates.insert(SecurityType::Future, 2.50); // $2.50 per contract

        let config = TransactionCostConfig {
            bid_ask_spreads,
            commission_rates,
            market_impact_threshold: 0.01, // 1% of daily volume
            market_impact_coefficient: 0.5,
        };

        TransactionCostCalculator::new(config)
    }

    #[test]
    fn test_bid_ask_spread_cost_calculation() -> Result<()> {
        let calculator = setup_test_calculator();
        
        // Test stock spread cost
        let stock_cost = calculator.calculate_spread_cost("AAPL", &SecurityType::Stock, 100.0, 150.0)?;
        let expected_stock_cost = 150.0 * 100.0 * 0.0005; // price * quantity * spread
        assert!((stock_cost - expected_stock_cost).abs() < 0.01, 
                "Stock spread cost: expected {}, got {}", expected_stock_cost, stock_cost);

        // Test forex spread cost
        let forex_cost = calculator.calculate_spread_cost("EURUSD", &SecurityType::Forex, 10000.0, 1.0850)?;
        let expected_forex_cost = 1.0850 * 10000.0 * 0.0020;
        assert!((forex_cost - expected_forex_cost).abs() < 0.01,
                "Forex spread cost: expected {}, got {}", expected_forex_cost, forex_cost);

        // Test futures spread cost
        let futures_cost = calculator.calculate_spread_cost("ES", &SecurityType::Future, 1.0, 4200.0)?;
        let expected_futures_cost = 4200.0 * 1.0 * 0.0010;
        assert!((futures_cost - expected_futures_cost).abs() < 0.01,
                "Futures spread cost: expected {}, got {}", expected_futures_cost, futures_cost);

        Ok(())
    }

    #[test]
    fn test_commission_cost_calculation() -> Result<()> {
        let calculator = setup_test_calculator();

        // Test stock commission
        let stock_commission = calculator.calculate_commission_cost(&SecurityType::Stock, 100.0)?;
        assert_eq!(stock_commission, 1.00, "Stock commission should be $1.00");

        // Test forex commission
        let forex_commission = calculator.calculate_commission_cost(&SecurityType::Forex, 10000.0)?;
        assert_eq!(forex_commission, 0.50, "Forex commission should be $0.50");

        // Test futures commission (per contract)
        let futures_commission = calculator.calculate_commission_cost(&SecurityType::Future, 5.0)?;
        assert_eq!(futures_commission, 12.50, "5 futures contracts should cost $12.50");

        Ok(())
    }

    #[test]
    fn test_market_impact_scaling() -> Result<()> {
        let calculator = setup_test_calculator();

        // Test small position (no market impact)
        let small_position_cost = calculator.calculate_market_impact_cost(
            "AAPL", 66.0, 150.0, 1000000.0 // 66 shares * $150 = $9,900 = 0.99% of daily volume
        )?;
        assert_eq!(small_position_cost, 0.0, "Small positions should have no market impact");

        // Test large position (with market impact)
        let large_position_cost = calculator.calculate_market_impact_cost(
            "AAPL", 100.0, 150.0, 1000000.0 // 100 shares * $150 = $15,000 = 1.5% of daily volume
        )?;
        let position_value = 150.0 * 100.0; // $15,000
        let volume_pct = position_value / 1000000.0; // 1.5%
        let excess_pct = volume_pct - 0.01; // 1.5% - 1% = 0.5%
        let expected_impact = position_value * excess_pct * 0.5; // $15,000 * 0.5% * 0.5 = $37.50
        assert!((large_position_cost - expected_impact).abs() < 0.01,
                "Large position impact: expected {}, got {}", expected_impact, large_position_cost);

        Ok(())
    }

    #[test]
    fn test_total_transaction_cost_calculation() -> Result<()> {
        let calculator = setup_test_calculator();

        // Test complete transaction cost for stock (small position)
        let total_cost = calculator.calculate_total_cost(
            "AAPL", &SecurityType::Stock, 66.0, 150.0, 1000000.0 // 66 shares = 0.99% of daily volume
        )?;
        
        let expected_spread = 150.0 * 66.0 * 0.0005; // $4.95
        let expected_commission = 1.00;
        let expected_market_impact = 0.0; // Small position (under 1% threshold)
        let expected_total = expected_spread + expected_commission + expected_market_impact;
        
        assert!((total_cost - expected_total).abs() < 0.01,
                "Total transaction cost: expected {}, got {}", expected_total, total_cost);

        Ok(())
    }

    #[test]
    fn test_round_trip_cost_calculation() -> Result<()> {
        let calculator = setup_test_calculator();

        // Test round-trip cost (buy and sell)
        let round_trip_cost = calculator.calculate_round_trip_cost(
            "AAPL", &SecurityType::Stock, 100.0, 150.0, 1000000.0
        )?;

        let one_way_cost = calculator.calculate_total_cost(
            "AAPL", &SecurityType::Stock, 100.0, 150.0, 1000000.0
        )?;
        let expected_round_trip = one_way_cost * 2.0;

        assert!((round_trip_cost - expected_round_trip).abs() < 0.01,
                "Round-trip cost should be 2x one-way cost: expected {}, got {}", 
                expected_round_trip, round_trip_cost);

        Ok(())
    }

    #[test]
    fn test_unknown_symbol_fallback() -> Result<()> {
        let calculator = setup_test_calculator();

        // Test unknown symbol uses default spread
        let unknown_cost = calculator.calculate_spread_cost("UNKNOWN", &SecurityType::Stock, 100.0, 50.0)?;
        let expected_fallback = 50.0 * 100.0 * 0.0010; // Default 0.10% spread
        assert!((unknown_cost - expected_fallback).abs() < 0.01,
                "Unknown symbol should use default spread: expected {}, got {}", 
                expected_fallback, unknown_cost);

        Ok(())
    }

    #[test]
    fn test_zero_quantity_handling() -> Result<()> {
        let calculator = setup_test_calculator();

        let zero_cost = calculator.calculate_total_cost(
            "AAPL", &SecurityType::Stock, 0.0, 150.0, 1000000.0
        )?;
        
        assert_eq!(zero_cost, 0.0, "Zero quantity should have zero cost");

        Ok(())
    }

    #[test]
    fn test_negative_quantity_handling() -> Result<()> {
        let calculator = setup_test_calculator();

        // Negative quantity (short position) should have same cost as positive
        let negative_cost = calculator.calculate_total_cost(
            "AAPL", &SecurityType::Stock, -100.0, 150.0, 1000000.0
        )?;
        
        let positive_cost = calculator.calculate_total_cost(
            "AAPL", &SecurityType::Stock, 100.0, 150.0, 1000000.0
        )?;

        assert!((negative_cost - positive_cost).abs() < 0.01,
                "Negative quantity should have same cost as positive: expected {}, got {}", 
                positive_cost, negative_cost);

        Ok(())
    }
}