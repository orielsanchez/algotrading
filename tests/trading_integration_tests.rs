use algotrading::config::RiskConfig;
use algotrading::orders::OrderSignal;
use algotrading::portfolio::{Portfolio, Position};
use algotrading::security_types::SecurityInfo;
use algotrading::trading_integration::TradingIntegrationLayer;
use anyhow::Result;
use std::collections::HashMap;

#[cfg(test)]
mod trading_integration_tests {
    use super::*;

    fn setup_test_risk_config() -> RiskConfig {
        RiskConfig {
            max_position_size: 0.5,
            max_portfolio_exposure: 1.0,
            stop_loss_percentage: 0.02,
            take_profit_percentage: 0.04,
            max_margin_utilization: 0.70,
            min_excess_liquidity: 10000.0,
            futures_position_limit: 10.0,
            margin_call_threshold: 0.30,
            margin_buffer_percentage: 0.05,
            enable_risk_budgeting: true,
            risk_budget_target_volatility: 0.25,
            risk_budget_rebalance_threshold: 0.02,
            max_correlation_exposure: 0.60,
            correlation_lookback_days: 63,
            min_positions_for_erc: 3,
            // Transaction cost configuration
            enable_transaction_cost_optimization: true,
            stock_commission: 1.00,
            futures_commission: 2.50,
            forex_commission: 0.50,
            max_acceptable_cost_bps: 50.0,
            // Position inertia configuration
            enable_position_inertia: true,
            inertia_multiplier: 2.0,
            min_position_change_value: 100.0,
            max_position_change_pct: 0.50,
        }
    }

    fn create_test_signal(
        symbol: &str,
        quantity: f64,
        price: f64,
        signal_strength: f64,
    ) -> OrderSignal {
        OrderSignal {
            symbol: symbol.to_string(),
            action: if quantity > 0.0 {
                "BUY".to_string()
            } else {
                "SELL".to_string()
            },
            quantity: quantity.abs(),
            price,
            order_type: "MARKET".to_string(),
            limit_price: None,
            reason: format!("Test signal strength:{:.1}", signal_strength),
            security_info: SecurityInfo::new_stock(
                symbol.to_string(),
                "SMART".to_string(),
                "USD".to_string(),
            ),
        }
    }

    fn setup_test_portfolio() -> Portfolio {
        let mut portfolio = Portfolio::new(50000.0); // $50k cash

        // Simulate adding positions through trades
        portfolio.update_position("AAPL", 100.0, 150.0);
        portfolio.update_position("GOOGL", 50.0, 2800.0);

        // Update current prices
        let mut prices = HashMap::new();
        prices.insert("AAPL".to_string(), 155.0);
        prices.insert("GOOGL".to_string(), 2850.0);
        portfolio.update_market_prices(&prices);

        portfolio
    }

    #[tokio::test]
    async fn test_signal_filtering_with_inertia_enabled() -> Result<()> {
        let risk_config = setup_test_risk_config();
        let integration_layer = TradingIntegrationLayer::new(&risk_config);
        let portfolio = setup_test_portfolio();

        let mut latest_prices = HashMap::new();
        latest_prices.insert("AAPL".to_string(), 155.0);
        latest_prices.insert("GOOGL".to_string(), 2850.0);
        latest_prices.insert("MSFT".to_string(), 380.0);

        // Create test signals
        let signals = vec![
            // Small position change for AAPL (should be blocked by inertia)
            create_test_signal("AAPL", 100.6, 155.0, 5.0), // ~$93 change vs ~$121 threshold
            // Large position change for GOOGL (should pass inertia)
            create_test_signal("GOOGL", 60.0, 2850.0, 12.0), // $28,500 change vs $100 threshold
            // New position for MSFT (should pass - no current position)
            create_test_signal("MSFT", 50.0, 380.0, 15.0),
        ];

        let (filtered_signals, filter_result) = integration_layer
            .filter_signals_with_cost_optimization(
                signals,
                &portfolio,
                &latest_prices,
                50.0, // 50 bps max cost
            )
            .await?;

        // Should filter out AAPL due to inertia and GOOGL due to high transaction cost
        assert_eq!(filter_result.original_signals, 3);
        assert_eq!(filter_result.inertia_filtered, 1); // AAPL blocked by inertia
        assert_eq!(filter_result.cost_filtered, 1); // GOOGL blocked by high cost
        assert_eq!(filter_result.final_signals, 1); // Only MSFT should pass

        // Verify AAPL and GOOGL were filtered out
        assert!(!filtered_signals.iter().any(|s| s.symbol == "AAPL"));
        assert!(!filtered_signals.iter().any(|s| s.symbol == "GOOGL"));

        // Verify only MSFT passed
        assert!(filtered_signals.iter().any(|s| s.symbol == "MSFT"));
        assert_eq!(filtered_signals.len(), 1);

        Ok(())
    }

    #[tokio::test]
    async fn test_signal_filtering_with_strong_signal_override() -> Result<()> {
        let risk_config = setup_test_risk_config();
        let integration_layer = TradingIntegrationLayer::new(&risk_config);
        let portfolio = setup_test_portfolio();

        let mut latest_prices = HashMap::new();
        latest_prices.insert("AAPL".to_string(), 155.0);

        // Create signal with strong signal strength (≥18.0 should override inertia)
        let signals = vec![
            create_test_signal("AAPL", 102.0, 155.0, 18.5), // Strong signal
        ];

        let (filtered_signals, filter_result) = integration_layer
            .filter_signals_with_cost_optimization(signals, &portfolio, &latest_prices, 50.0)
            .await?;

        // Strong signal should override inertia
        assert_eq!(filter_result.inertia_filtered, 0);
        assert_eq!(filtered_signals.len(), 1);
        assert_eq!(filtered_signals[0].symbol, "AAPL");

        Ok(())
    }

    #[tokio::test]
    async fn test_transaction_cost_filtering() -> Result<()> {
        let mut risk_config = setup_test_risk_config();
        risk_config.enable_position_inertia = false; // Disable inertia to focus on cost filtering

        let integration_layer = TradingIntegrationLayer::new(&risk_config);
        let portfolio = Portfolio::new(50000.0); // Empty portfolio with $50k cash

        let mut latest_prices = HashMap::new();
        latest_prices.insert("AAPL".to_string(), 155.0);
        latest_prices.insert("PENNY".to_string(), 0.10); // Penny stock with high relative costs

        let signals = vec![
            // Normal cost signal
            create_test_signal("AAPL", 100.0, 155.0, 10.0),
            // High relative cost signal (small position value)
            create_test_signal("PENNY", 100.0, 0.10, 10.0), // $10 position, $1+ commission = >1000 bps
        ];

        let (filtered_signals, filter_result) = integration_layer
            .filter_signals_with_cost_optimization(
                signals,
                &portfolio,
                &latest_prices,
                50.0, // 50 bps max cost
            )
            .await?;

        // Penny stock should be filtered out due to high relative cost
        assert_eq!(filter_result.cost_filtered, 1);
        assert_eq!(filtered_signals.len(), 1);
        assert_eq!(filtered_signals[0].symbol, "AAPL");

        Ok(())
    }

    #[tokio::test]
    async fn test_position_reversal_always_executes() -> Result<()> {
        let risk_config = setup_test_risk_config();
        let integration_layer = TradingIntegrationLayer::new(&risk_config);

        // Create portfolio with long AAPL position
        let mut portfolio = Portfolio::new(50000.0);
        portfolio.update_position("AAPL", 100.0, 150.0);

        let mut prices = HashMap::new();
        prices.insert("AAPL".to_string(), 155.0);
        portfolio.update_market_prices(&prices);

        let mut latest_prices = HashMap::new();
        latest_prices.insert("AAPL".to_string(), 155.0);

        // Create signal to go short (position reversal)
        let signals = vec![
            create_test_signal("AAPL", -50.0, 155.0, 5.0), // Weak signal but reversal
        ];

        let (filtered_signals, filter_result) = integration_layer
            .filter_signals_with_cost_optimization(signals, &portfolio, &latest_prices, 50.0)
            .await?;

        // Position reversal should always execute regardless of signal strength
        assert_eq!(filter_result.inertia_filtered, 0);
        assert_eq!(filtered_signals.len(), 1);
        assert_eq!(filtered_signals[0].symbol, "AAPL");

        Ok(())
    }

    #[tokio::test]
    async fn test_maximum_position_change_limiting() -> Result<()> {
        let mut risk_config = setup_test_risk_config();
        risk_config.enable_transaction_cost_optimization = false; // Disable cost filtering to test position limiting
        let integration_layer = TradingIntegrationLayer::new(&risk_config);
        let portfolio = setup_test_portfolio();

        let mut latest_prices = HashMap::new();
        latest_prices.insert("AAPL".to_string(), 155.0);

        // Create signal for very large position increase (should be limited)
        let signals = vec![
            create_test_signal("AAPL", 300.0, 155.0, 20.0), // 300 shares vs current 100 = 200% increase
        ];

        let (filtered_signals, filter_result) = integration_layer
            .filter_signals_with_cost_optimization(signals, &portfolio, &latest_prices, 50.0)
            .await?;

        // Signal should pass but be limited to 50% increase (150 shares total)
        assert_eq!(filtered_signals.len(), 1);
        let limited_signal = &filtered_signals[0];

        // Should be limited to 150 shares (50% increase from 100)
        assert!(limited_signal.quantity <= 150.0);
        assert!(limited_signal.quantity > 100.0);

        Ok(())
    }

    #[tokio::test]
    async fn test_disabled_optimization_passes_all_signals() -> Result<()> {
        let mut risk_config = setup_test_risk_config();
        risk_config.enable_position_inertia = false;
        risk_config.enable_transaction_cost_optimization = false;

        let integration_layer = TradingIntegrationLayer::new(&risk_config);
        let portfolio = setup_test_portfolio();

        let mut latest_prices = HashMap::new();
        latest_prices.insert("AAPL".to_string(), 155.0);
        latest_prices.insert("PENNY".to_string(), 0.01);

        let signals = vec![
            create_test_signal("AAPL", 101.0, 155.0, 1.0), // Tiny change, weak signal
            create_test_signal("PENNY", 1.0, 0.01, 1.0),   // High cost signal
        ];

        let (filtered_signals, filter_result) = integration_layer
            .filter_signals_with_cost_optimization(signals, &portfolio, &latest_prices, 50.0)
            .await?;

        // All signals should pass when optimization is disabled
        assert_eq!(filter_result.inertia_filtered, 0);
        assert_eq!(filter_result.cost_filtered, 0);
        assert_eq!(filtered_signals.len(), 2);

        Ok(())
    }

    #[tokio::test]
    async fn test_order_cost_validation() -> Result<()> {
        let risk_config = setup_test_risk_config();
        let integration_layer = TradingIntegrationLayer::new(&risk_config);

        // Test normal cost order
        let normal_signal = create_test_signal("AAPL", 100.0, 155.0, 10.0);
        let is_valid = integration_layer
            .validate_order_cost(&normal_signal, 155.0, 50.0)
            .await?;
        assert!(is_valid, "Normal cost order should be valid");

        // Test high cost order
        let high_cost_signal = create_test_signal("PENNY", 10.0, 0.01, 10.0);
        let is_valid_high_cost = integration_layer
            .validate_order_cost(&high_cost_signal, 0.01, 50.0)
            .await?;
        assert!(!is_valid_high_cost, "High cost order should be invalid");

        Ok(())
    }

    #[tokio::test]
    async fn test_transaction_cost_estimation() -> Result<()> {
        let risk_config = setup_test_risk_config();
        let integration_layer = TradingIntegrationLayer::new(&risk_config);

        let signal = create_test_signal("AAPL", 100.0, 155.0, 10.0);
        let estimated_cost = integration_layer
            .estimate_transaction_cost(&signal, &155.0)
            .await?;

        // Should include spread cost + commission
        assert!(estimated_cost > 0.0, "Estimated cost should be positive");
        assert!(
            estimated_cost > 1.0,
            "Should include at least $1 commission"
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_filter_result_metrics() -> Result<()> {
        let risk_config = setup_test_risk_config();
        let integration_layer = TradingIntegrationLayer::new(&risk_config);
        let portfolio = setup_test_portfolio();

        let mut latest_prices = HashMap::new();
        latest_prices.insert("AAPL".to_string(), 155.0);
        latest_prices.insert("GOOGL".to_string(), 2850.0);
        latest_prices.insert("MSFT".to_string(), 380.0);
        latest_prices.insert("PENNY".to_string(), 0.01);

        let signals = vec![
            create_test_signal("AAPL", 101.0, 155.0, 5.0), // Small change - inertia filter
            create_test_signal("GOOGL", 60.0, 2850.0, 12.0), // Pass both filters
            create_test_signal("MSFT", 50.0, 380.0, 15.0), // Pass both filters
            create_test_signal("PENNY", 100.0, 0.01, 10.0), // High cost - cost filter
        ];

        let (filtered_signals, filter_result) = integration_layer
            .filter_signals_with_cost_optimization(signals, &portfolio, &latest_prices, 50.0)
            .await?;

        // Validate filter result metrics
        assert_eq!(filter_result.original_signals, 4);
        assert_eq!(filter_result.inertia_filtered, 1); // PENNY blocked by inertia
        assert_eq!(filter_result.cost_filtered, 1); // GOOGL blocked by cost
        assert_eq!(filter_result.final_signals, 2); // AAPL, MSFT pass
        assert!(filter_result.total_estimated_costs > 0.0);

        // Validate actual filtered signals
        assert_eq!(filtered_signals.len(), 2);
        let symbols: Vec<String> = filtered_signals.iter().map(|s| s.symbol.clone()).collect();
        assert!(symbols.contains(&"AAPL".to_string()));
        assert!(symbols.contains(&"MSFT".to_string()));
        assert!(!symbols.contains(&"GOOGL".to_string()));
        assert!(!symbols.contains(&"PENNY".to_string()));

        Ok(())
    }
}
